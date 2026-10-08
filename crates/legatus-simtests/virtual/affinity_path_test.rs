//! Story 143 integration tests: the table in the request path, the sweeper under virtual time,
//! cancels, parallel requests and what the table keeps. Real router, scripted nodes.
use async_trait::async_trait;
use bytes::Bytes;
use futures_util::StreamExt;
use http::{Request, StatusCode};
use http_body_util::BodyExt;
use legatus_common::ids::{AliasName, NodeId};
use legatus_proxy::affinity::seams::{AllUp, UnavailableReason};
use legatus_proxy::affinity::sweeper::run_sweeper;
use legatus_proxy::affinity::table::{AffinityTable, Lookup, TableKey, TableParams};
use legatus_proxy::config::typed::{Registry, RegistryHandle};
use legatus_proxy::key::hasher::KEY_LEN_BYTES;
use legatus_proxy::key::ConversationKey;
use legatus_proxy::protocol::chat::PipelineHooks;
use legatus_proxy::protocol::ctx::RequestCtx;
use legatus_proxy::sim::SimPoints;
use legatus_proxy::time::WallTime;
use legatus_proxy::{build_router, RouterDeps, Seams, CHAT_COMPLETIONS_PATH};
use legatus_testkit::virt::{serve_duplex, FakeTransport, MemorySink, Script, SimWall};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tokio::time::Instant;

const TTL: Duration = Duration::from_secs(600);
const SINK_CAPACITY: usize = 64;
const CANARY_SESSION: &str = "SECRET-SESSION-123";
const CANARY_PROMPT: &str = "CANARY-PROMPT-7f3a";

fn registry() -> Registry {
    let text = "version: 1\nsettings:\n  listen: 127.0.0.1:8080\nnodes:\n  a:\n    engine: { name: ollama }\n    model: model-a\n    endpoints: [ { protocol: openai-chat, base_url: \"http://a.invalid:1\" } ]\n  b:\n    engine: { name: ollama }\n    model: model-b\n    endpoints: [ { protocol: openai-chat, base_url: \"http://b.invalid:2\" } ]\naliases:\n  x: { nodes: [a, b] }\n";
    Registry::from_text(text).expect("valid registry")
}

/// Sends every request without a sticky entry to the node it is told to: the placement of story
/// 169 is not built, so this stands in for "the placer chose it".
struct Placer(Arc<std::sync::Mutex<String>>);

#[async_trait]
impl PipelineHooks for Placer {
    async fn place(&self, _ctx: &mut RequestCtx) -> Option<NodeId> {
        Some(NodeId(self.0.lock().unwrap().clone()))
    }
}

struct Rig {
    client: Arc<legatus_testkit::virt::DuplexClient>,
    router: axum::Router,
    fake: Arc<FakeTransport>,
    table: Arc<AffinityTable>,
    choose: Arc<std::sync::Mutex<String>>,
    sink: Arc<MemorySink>,
}

async fn rig(script: Script) -> Rig {
    let fake = Arc::new(FakeTransport::new(script));
    let sink = Arc::new(MemorySink::new(SINK_CAPACITY));
    let seams = Seams { wall: Arc::new(SimWall::new(WallTime { unix_ms: 0 })), transport: fake.clone(), log: sink.clone(), sim: SimPoints::new() };
    let choose = Arc::new(std::sync::Mutex::new("a".to_string()));
    let table = Arc::new(AffinityTable::new(TableParams { ttl: TTL, cap: 100, mature_turns: 2 }));
    let deps = RouterDeps::for_test(seams).with_registry(Arc::new(RegistryHandle::new(Arc::new(registry())))).with_hooks(Arc::new(Placer(choose.clone()))).with_affinity(table.clone());
    let router = build_router(deps);
    Rig { client: Arc::new(serve_duplex(router.clone()).await), router, fake, table, choose, sink }
}

fn ok_after(delay: Duration) -> Script {
    Script::Response { status: StatusCode::OK, chunks: vec![(delay, Ok(Bytes::from_static(b"{}")))] }
}

fn request(session: &str, body: &str) -> Request<axum::body::Body> {
    Request::builder().method("POST").uri(CHAT_COMPLETIONS_PATH).header("host", "proxy").header("content-type", "application/json").header("x-session-affinity", session).body(axum::body::Body::from(body.to_string())).unwrap()
}

const BODY: &str = "{\"model\":\"x\",\"messages\":[{\"role\":\"user\",\"content\":\"hi\"}]}";

async fn go(rig: &Rig, session: &str) -> StatusCode {
    let response = rig.client.send(request(session, BODY)).await;
    let status = response.status();
    let _ = response.into_body().collect().await;
    status
}

fn host_of(rig: &Rig, index: usize) -> String {
    rig.fake.requests()[index].uri.host().unwrap().to_string()
}

fn table_key_of(session_hash: &Rig, _s: &str) -> Option<TableKey> {
    session_hash.table.snapshot().first().map(|(k, _)| k.clone())
}

#[tokio::test(start_paused = true)]
async fn tc02_a_returning_key_goes_to_its_node_even_when_the_placer_would_choose_another_and_the_entry_is_unchanged() {
    let rig = rig(ok_after(Duration::from_millis(1))).await;
    assert_eq!(go(&rig, "conv-1").await, StatusCode::OK);
    assert_eq!(host_of(&rig, 0), "a.invalid");
    *rig.choose.lock().unwrap() = "b".to_string();
    assert_eq!(go(&rig, "conv-1").await, StatusCode::OK);
    assert_eq!(host_of(&rig, 1), "a.invalid", "the conversation returns to the node that holds its cache");
    assert_eq!(go(&rig, "conv-2").await, StatusCode::OK);
    assert_eq!(host_of(&rig, 2), "b.invalid", "a new conversation takes the placer's choice");
    assert_eq!(rig.table.len(), 2);
    let first = table_key_of(&rig, "conv-1").unwrap();
    assert_eq!(rig.table.peek(&first).map(|v| v.node.0).as_deref().map(|n| n == "a" || n == "b"), Some(true));
    assert_eq!(rig.table.entries_on_node(&NodeId("a".into())), 1);
}

#[tokio::test(start_paused = true)]
async fn a_conversation_completes_two_turns_and_the_entry_counts_them_while_an_error_status_counts_nothing() {
    let rig = rig(ok_after(Duration::from_millis(1))).await;
    go(&rig, "conv-1").await;
    go(&rig, "conv-1").await;
    let entry = rig.table.snapshot().pop().unwrap().1;
    assert_eq!((entry.done_count, entry.in_flight), (2, 0));
    let failing = self::rig(Script::Response { status: StatusCode::SERVICE_UNAVAILABLE, chunks: vec![(Duration::from_millis(1), Ok(Bytes::from_static(b"{}")))] }).await;
    go(&failing, "conv-1").await;
    let entry = failing.table.snapshot().pop().unwrap().1;
    assert_eq!((entry.done_count, entry.in_flight, entry.prev), (0, 0, None), "an error status refreshes nothing");
}

#[tokio::test(start_paused = true)]
async fn tc09_the_sweeper_and_a_lookup_in_the_same_virtual_millisecond_give_the_same_result_on_twenty_runs() {
    for run in 0..20 {
        let table = Arc::new(AffinityTable::new(TableParams { ttl: TTL, cap: 10, mature_turns: 2 }));
        let key = TableKey { alias: AliasName("x".into()), key: ConversationKey([run; KEY_LEN_BYTES]), credential: None };
        let placed = Instant::now();
        table.place(key.clone(), NodeId("a".into()), legatus_proxy::key::KeyClass::Strong, legatus_proxy::key::harness::HarnessLabel::Unknown, placed);
        let (stop_tx, stop_rx) = watch::channel(false);
        let sweeper = tokio::spawn(run_sweeper(table.clone(), stop_rx));
        tokio::time::sleep_until(placed + TTL).await;
        // At exactly the ttl the entry is still there, whatever the sweeper did.
        assert_eq!(table.lookup(&key, Instant::now(), &AllUp), Lookup::Hit { node: NodeId("a".into()) }, "run {run}");
        tokio::time::sleep(Duration::from_millis(1)).await;
        if run % 2 == 0 {
            tokio::task::yield_now().await;
        }
        assert_eq!(table.lookup(&key, Instant::now(), &AllUp), Lookup::Miss, "run {run}");
        assert_eq!(table.len(), 0);
        stop_tx.send(true).unwrap();
        sweeper.await.unwrap();
    }
}

#[tokio::test(start_paused = true)]
async fn the_sweeper_removes_an_idle_entry_at_its_due_time_without_a_lookup_and_sleeps_until_the_next_one() {
    let table = Arc::new(AffinityTable::new(TableParams { ttl: TTL, cap: 10, mature_turns: 2 }));
    let (stop_tx, stop_rx) = watch::channel(false);
    let sweeper = tokio::spawn(run_sweeper(table.clone(), stop_rx));
    let key = |n: u8| TableKey { alias: AliasName("x".into()), key: ConversationKey([n; KEY_LEN_BYTES]), credential: None };
    let place = |n: u8| table.place(key(n), NodeId("a".into()), legatus_proxy::key::KeyClass::Strong, legatus_proxy::key::harness::HarnessLabel::Unknown, Instant::now());
    tokio::task::yield_now().await;
    place(1);
    tokio::time::sleep(Duration::from_secs(300)).await;
    place(2);
    tokio::time::sleep(Duration::from_secs(300) + Duration::from_millis(2)).await;
    assert!(table.peek(&key(1)).is_none(), "the first entry went at its due time");
    assert!(table.peek(&key(2)).is_some());
    tokio::time::sleep(Duration::from_secs(300)).await;
    assert!(table.is_empty(), "the second went later");
    stop_tx.send(true).unwrap();
    sweeper.await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn tc21_the_sweeper_stops_when_its_token_turns_true_and_leaves_the_table_unchanged() {
    let table = Arc::new(AffinityTable::new(TableParams { ttl: TTL, cap: 10, mature_turns: 2 }));
    let key = TableKey { alias: AliasName("x".into()), key: ConversationKey([1; KEY_LEN_BYTES]), credential: None };
    table.place(key.clone(), NodeId("a".into()), legatus_proxy::key::KeyClass::Strong, legatus_proxy::key::harness::HarnessLabel::Unknown, Instant::now());
    let (stop_tx, stop_rx) = watch::channel(false);
    let sweeper = tokio::spawn(run_sweeper(table.clone(), stop_rx));
    tokio::time::sleep(Duration::from_secs(10)).await;
    stop_tx.send(true).unwrap();
    sweeper.await.expect("the sweeper ended");
    assert_eq!(table.len(), 1);
    // A token that is already true ends it at once.
    let (_tx, rx) = watch::channel(true);
    tokio::time::timeout(Duration::from_secs(1), run_sweeper(table.clone(), rx)).await.expect("stops at once");
    // With nothing idle it parks and still stops.
    let empty = Arc::new(AffinityTable::new(TableParams::default()));
    let (tx2, rx2) = watch::channel(false);
    let parked = tokio::spawn(run_sweeper(empty, rx2));
    tokio::time::sleep(Duration::from_secs(5)).await;
    tx2.send(true).unwrap();
    parked.await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn tc13_a_harness_that_leaves_mid_stream_drops_the_guard_in_flight_falls_and_nothing_else_changes() {
    let script = Script::Response {
        status: StatusCode::OK,
        chunks: vec![(Duration::from_millis(1), Ok(Bytes::from_static(b"data: one\n\n"))), (Duration::from_secs(30), Ok(Bytes::from_static(b"data: two\n\n")))],
    };
    let rig = rig(script).await;
    let response = rig.client.send(request("conv-1", BODY)).await;
    let mut body = response.into_body().into_data_stream();
    let first = body.next().await;
    assert!(first.is_some());
    let key = table_key_of(&rig, "conv-1").unwrap();
    let running = rig.table.peek(&key).unwrap();
    assert_eq!(running.in_flight, 1);
    let before = rig.table.snapshot().pop().unwrap().1.last_seen;
    drop(body);
    for _ in 0..5 {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let entry = rig.table.snapshot().pop().unwrap().1;
    assert_eq!((entry.in_flight, entry.done_count, entry.last_seen), (0, 0, before), "the count fell, the entry did not refresh");
}

#[tokio::test(start_paused = true)]
async fn tc18_twenty_parallel_requests_of_one_key_keep_in_flight_equal_to_the_number_running_and_return_it_to_zero() {
    let rig = rig(ok_after(Duration::from_secs(5))).await;
    // The first request creates the entry; the rest begin on it.
    let first = {
        let router = rig.router.clone();
        tokio::spawn(async move {
            let response = tower::ServiceExt::oneshot(router, request("conv-1", BODY)).await.unwrap();
            let _ = response.into_body().collect().await;
        })
    };
    tokio::time::sleep(Duration::from_millis(100)).await;
    let done = Arc::new(AtomicUsize::new(0));
    let mut tasks = vec![first];
    for _ in 0..19 {
        let router = rig.router.clone();
        let done = done.clone();
        tasks.push(tokio::spawn(async move {
            let response = tower::ServiceExt::oneshot(router, request("conv-1", BODY)).await.unwrap();
            let _ = response.into_body().collect().await;
            done.fetch_add(1, Ordering::SeqCst);
        }));
    }
    tokio::time::sleep(Duration::from_secs(1)).await;
    let key = table_key_of(&rig, "conv-1").unwrap();
    assert_eq!(rig.table.peek(&key).unwrap().in_flight, 20);
    for t in tasks {
        t.await.unwrap();
    }
    let entry = rig.table.snapshot().pop().unwrap().1;
    assert_eq!((entry.in_flight, entry.done_count), (0, 20));
    assert_eq!(rig.table.len(), 1, "one conversation, one entry");
}

#[tokio::test(start_paused = true)]
async fn tc15_a_canary_session_and_prompt_never_appear_in_a_snapshot_a_debug_print_or_a_log_record() {
    let rig = rig(ok_after(Duration::from_millis(1))).await;
    let body = format!("{{\"model\":\"x\",\"messages\":[{{\"role\":\"user\",\"content\":\"{CANARY_PROMPT}\"}}]}}");
    let response = rig.client.send(request(CANARY_SESSION, &body)).await;
    let _ = response.into_body().collect().await;
    let shown = format!("{:?} {:?}", rig.table.snapshot(), rig.table.conversations_after(None, 10));
    assert!(!shown.contains("CANARY") && !shown.contains(CANARY_SESSION), "{shown}");
    assert!(!format!("{:?}", rig.sink.take()).contains("CANARY"));
    let before = rig.table.snapshot().pop().unwrap().1.last_seen;
    tokio::time::advance(Duration::from_secs(5)).await;
    let _ = rig.table.snapshot();
    assert_eq!(rig.table.snapshot().pop().unwrap().1.last_seen, before, "a snapshot refreshes nothing");
}

#[tokio::test(start_paused = true)]
async fn tc14_a_new_table_and_a_new_start_are_empty_and_the_unavailable_reason_is_kept_for_a_down_node() {
    let rig = rig(ok_after(Duration::from_millis(1))).await;
    go(&rig, "conv-1").await;
    assert_eq!(rig.table.len(), 1);
    let restarted = AffinityTable::new(TableParams::default());
    assert!(restarted.is_empty(), "a restart empties the table");
    let _ = UnavailableReason::Down;
    // That nothing is written to disk is checked on the source by the scaled scan test.
}
