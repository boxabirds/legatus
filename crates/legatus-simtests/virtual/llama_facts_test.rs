//! Story 151 integration tests with the llama-server stub of story 162 on paused time: slot
//! discovery, load polling, a sleeping engine, the context error and the 500 pass-through.
use super::stub_helpers::*;
use async_trait::async_trait;
use bytes::Bytes;
use futures_util::StreamExt;
use http::{Request, StatusCode};
use http_body_util::BodyExt;
use legatus_common::engine::*;
use legatus_common::protocol::Protocol;
use legatus_proxy::config::typed::Registry;
use legatus_proxy::engine::llama_server::*;
use legatus_proxy::engine::EngineAdapter;
use legatus_proxy::obs::log_sink::DiscardSink;
use legatus_proxy::sim::SimPoints;
use legatus_proxy::time::WallTime;
use legatus_proxy::upstream::transport::{UpstreamError, UpstreamRequest, UpstreamResponse, UpstreamTransport};
use legatus_proxy::{build_router, Seams, CHAT_COMPLETIONS_PATH};
use legatus_testkit::fleet::{one_node_deps, FLEET_ALIAS};
use legatus_testkit::stubs::common::reply;
use legatus_testkit::stubs::llama::MultiSlotLlamaStub;
use legatus_testkit::stubs::scenario::{StubKind, StubSpec};
use legatus_testkit::virt::{serve_duplex, FakeTransport, Script, SimWall};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const NODE_URL: &str = "http://node.invalid";
const SLOTS_PATH: &str = "/slots";

/// Wraps a transport and records the path of every request, so a test can show what was asked.
struct Recording {
    inner: Arc<dyn UpstreamTransport>,
    paths: Mutex<Vec<String>>,
}

impl Recording {
    fn new(inner: Arc<dyn UpstreamTransport>) -> Arc<Recording> {
        Arc::new(Recording { inner, paths: Mutex::default() })
    }
    fn paths(&self) -> Vec<String> {
        self.paths.lock().unwrap().clone()
    }
}

#[async_trait]
impl UpstreamTransport for Recording {
    async fn send(&self, req: UpstreamRequest) -> Result<UpstreamResponse, UpstreamError> {
        self.paths.lock().unwrap().push(req.uri.path().to_string());
        self.inner.send(req).await
    }
}

fn spec(slots: u32) -> StubSpec {
    let mut s = StubSpec::new("llama", StubKind::MultiSlotLlama);
    s.speed.load_time_ms = 0;
    s.slots = slots;
    s
}

fn node(extra: &str) -> legatus_proxy::config::node::NodeSpec {
    let text = format!(
        "version: 1\nnodes:\n  n1:\n    engine: {{ name: llama-server }}\n    model: m\n    endpoints: [ {{ protocol: openai-chat, base_url: \"{NODE_URL}\" }} ]\n{extra}aliases:\n  a: {{ nodes: [n1] }}\n"
    );
    Registry::from_text(&text).expect("valid registry").nodes[0].clone()
}

async fn facts(stub: &Arc<Recording>, extra: &str, previous: Option<NodeFacts>) -> NodeFacts {
    discover(stub.as_ref(), &node(extra), previous).await.expect("facts")
}

#[tokio::test(start_paused = true)]
async fn tc03_slots_follow_the_engine_down_and_up_and_the_smaller_of_registry_and_engine_wins() {
    let four = Recording::new(Arc::new(MultiSlotLlamaStub::new(spec(4))));
    let auto = facts(&four, "", None).await;
    assert_eq!(auto.slots, EffectiveSlots { slots: 4, source: SlotsSource::Props });
    let two = Recording::new(Arc::new(MultiSlotLlamaStub::new(spec(2))));
    let lower = facts(&two, "    slots: 4\n", None).await;
    assert_eq!(lower.slots, EffectiveSlots { slots: 2, source: SlotsSource::Props }, "a registry count above the engine's never raises the cap");
    // The engine is restarted with 2 slots, then with 4: re-reading the entry follows both ways.
    let after_two = facts(&two, "", Some(auto)).await;
    assert_eq!(after_two.slots.slots, 2);
    let after_four = facts(&four, "", Some(after_two)).await;
    assert_eq!(after_four.slots.slots, 4);
    assert_eq!(facts(&four, "    slots: 2\n", None).await.slots, EffectiveSlots { slots: 2, source: SlotsSource::Registry });
    for recorded in [&four, &two] {
        assert!(recorded.paths().iter().all(|p| p == "/props"), "only the properties page is read for slot facts: {:?}", recorded.paths());
    }
}

struct Down;

#[async_trait]
impl UpstreamTransport for Down {
    async fn send(&self, _: UpstreamRequest) -> Result<UpstreamResponse, UpstreamError> {
        Err(UpstreamError::Connect)
    }
}

#[tokio::test(start_paused = true)]
async fn an_unreachable_engine_keeps_the_previous_slots_or_falls_back_to_the_unverified_default() {
    let up = Recording::new(Arc::new(MultiSlotLlamaStub::new(spec(2))));
    let known = facts(&up, "", None).await;
    let down: Arc<Recording> = Recording::new(Arc::new(Down));
    assert_eq!(facts(&down, "", Some(known)).await.slots, known.slots, "the previous value is kept");
    assert_eq!(facts(&down, "", None).await.slots, EffectiveSlots { slots: LLAMA_SERVER_DEFAULT_SLOTS, source: SlotsSource::DefaultUnverified });
    assert_eq!(facts(&down, "    slots: 3\n", None).await.slots, EffectiveSlots { slots: 3, source: SlotsSource::Registry });
}

#[tokio::test(start_paused = true)]
async fn tc05_a_node_run_with_np_4_and_c_16384_shows_a_slot_context_of_4096_and_4_slots() {
    let mut s = spec(4);
    s.ctx_total = 16384;
    let stub = MultiSlotLlamaStub::new(s);
    assert_eq!(stub.slot_ctx(), 4096);
    let recorded = Recording::new(Arc::new(stub));
    let f = facts(&recorded, "    engine_flags: { np: 4, ctx_size: 16384 }\n", None).await;
    assert_eq!((f.slots.slots, f.slot_context.tokens), (4, Some(4096)));
}

async fn run_six(stub: &Arc<MultiSlotLlamaStub>) -> Vec<tokio::task::JoinHandle<()>> {
    let mut keep = Vec::new();
    for _ in 0..6 {
        let stub = stub.clone();
        keep.push(tokio::spawn(async move {
            let reply = stub.send(chat(2600, 3)).await.ok().unwrap();
            let mut body = reply.body;
            while body.next().await.is_some() {}
        }));
    }
    for _ in 0..5 {
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    keep
}

#[tokio::test(start_paused = true)]
async fn tc07_with_metrics_on_the_poller_shows_the_stubs_counters_as_six_requests_run_on_four_slots() {
    let mut s = spec(4);
    s.metrics_on = true;
    s.ctx_total = 16384;
    let stub = Arc::new(MultiSlotLlamaStub::new(s));
    let recorded = Recording::new(stub.clone());
    let poller = LoadPoller::new(NODE_URL);
    assert_eq!(poller.poll_once(recorded.as_ref()).await, LoadReading::Metrics(MetricsReading { processing: 0, deferred: 0 }));
    let running = run_six(&stub).await;
    assert_eq!(poller.poll_once(recorded.as_ref()).await, LoadReading::Metrics(MetricsReading { processing: 4, deferred: 2 }));
    for r in running {
        r.await.unwrap();
    }
    assert_eq!(poller.poll_once(recorded.as_ref()).await, LoadReading::Metrics(MetricsReading { processing: 0, deferred: 0 }), "refreshed after the run");
    assert!(recorded.paths().iter().all(|p| p == "/metrics"));
    let cell = NodeLoadCell::default();
    assert_eq!(cell.get(), LoadReading::OwnCountOnly);
    cell.store(LoadReading::Metrics(MetricsReading { processing: 1, deferred: 0 }));
    assert_eq!(cell.get(), LoadReading::Metrics(MetricsReading { processing: 1, deferred: 0 }));
}

/// A stub whose metrics page can be switched off and on, like an engine restarted without the flag.
struct Switch {
    on: Arc<dyn UpstreamTransport>,
    off: Arc<dyn UpstreamTransport>,
    use_on: Mutex<bool>,
}

#[async_trait]
impl UpstreamTransport for Switch {
    async fn send(&self, req: UpstreamRequest) -> Result<UpstreamResponse, UpstreamError> {
        if *self.use_on.lock().unwrap() { self.on.send(req).await } else { self.off.send(req).await }
    }
}

#[tokio::test(start_paused = true)]
async fn tc08_metrics_off_gives_own_count_only_and_no_slots_call_and_metrics_back_gives_metrics_again() {
    let mut on = spec(4);
    on.metrics_on = true;
    let switch = Arc::new(Switch { on: Arc::new(MultiSlotLlamaStub::new(on)), off: Arc::new(MultiSlotLlamaStub::new(spec(4))), use_on: Mutex::new(false) });
    let recorded = Recording::new(switch.clone());
    let poller = LoadPoller::new(NODE_URL);
    assert_eq!(poller.poll_once(recorded.as_ref()).await, LoadReading::OwnCountOnly);
    *switch.use_on.lock().unwrap() = true;
    assert_eq!(poller.poll_once(recorded.as_ref()).await, LoadReading::Metrics(MetricsReading { processing: 0, deferred: 0 }));
    *switch.use_on.lock().unwrap() = false;
    assert_eq!(poller.poll_once(recorded.as_ref()).await, LoadReading::OwnCountOnly);
    assert!(!recorded.paths().iter().any(|p| p == SLOTS_PATH), "{:?}", recorded.paths());
}

struct Garbage(&'static str, u16);

#[async_trait]
impl UpstreamTransport for Garbage {
    async fn send(&self, _: UpstreamRequest) -> Result<UpstreamResponse, UpstreamError> {
        Ok(reply(self.1, self.0))
    }
}

#[tokio::test(start_paused = true)]
async fn a_closed_connection_a_timeout_and_an_unreadable_page_all_give_own_count_only() {
    let poller = LoadPoller::new(NODE_URL);
    assert_eq!(poller.poll_once(&Down).await, LoadReading::OwnCountOnly);
    assert_eq!(poller.poll_once(&Garbage("<html>not metrics</html>", 200)).await, LoadReading::OwnCountOnly);
    assert_eq!(poller.poll_once(&Garbage("llamacpp:requests_processing 1\n", 200)).await, LoadReading::OwnCountOnly, "one counter is not enough");
    assert_eq!(poller.poll_once(&Garbage("llamacpp:requests_processing 1\nllamacpp:requests_deferred 0\n", 503)).await, LoadReading::OwnCountOnly);
    // A page that never answers: the poll gives up after one interval and does not hang.
    struct Hang;
    #[async_trait]
    impl UpstreamTransport for Hang {
        async fn send(&self, _: UpstreamRequest) -> Result<UpstreamResponse, UpstreamError> {
            std::future::pending().await
        }
    }
    let started = tokio::time::Instant::now();
    assert_eq!(poller.poll_once(&Hang).await, LoadReading::OwnCountOnly);
    assert_eq!(started.elapsed(), poller.interval);
}

async fn asleep_stub(metrics_on: bool) -> (Arc<MultiSlotLlamaStub>, Arc<Recording>) {
    let mut s = spec(4);
    s.sleep_on = true;
    s.metrics_on = metrics_on;
    s.speed.load_time_ms = 3000;
    let stub = Arc::new(MultiSlotLlamaStub::new(s));
    stub.force_sleep();
    let recorded = Recording::new(stub.clone());
    (stub, recorded)
}

#[tokio::test(start_paused = true)]
async fn tc09_ten_polls_leave_a_sleeping_engine_asleep_and_the_first_request_wakes_it() {
    let (stub, recorded) = asleep_stub(true).await;
    let poller = LoadPoller::new(NODE_URL);
    for _ in 0..10 {
        assert!(matches!(poller.poll_once(recorded.as_ref()).await, LoadReading::Metrics(_)));
        assert!(read_props(recorded.as_ref(), NODE_URL, Duration::from_secs(1)).await.is_some());
    }
    assert!(stub.is_asleep(), "polls do not wake the engine");
    assert!(!recorded.paths().iter().any(|p| p == SLOTS_PATH));
    let started = tokio::time::Instant::now();
    let answer = recorded.send(chat(100, 3)).await.ok().unwrap();
    assert_eq!(answer.status, StatusCode::OK);
    assert!(started.elapsed() >= Duration::from_secs(3), "the first request pays the load time");
    assert!(!stub.is_asleep());
}

#[tokio::test(start_paused = true)]
async fn tc20_metrics_off_and_asleep_together_fall_back_to_own_count_and_never_ask_for_the_slots_page() {
    let (stub, recorded) = asleep_stub(false).await;
    let poller = LoadPoller::new(NODE_URL);
    for _ in 0..10 {
        assert_eq!(poller.poll_once(recorded.as_ref()).await, LoadReading::OwnCountOnly);
    }
    assert!(stub.is_asleep());
    assert!(!recorded.paths().iter().any(|p| p == SLOTS_PATH));
}

fn seams(transport: Arc<dyn UpstreamTransport>) -> Seams {
    Seams { wall: Arc::new(SimWall::new(WallTime { unix_ms: 0 })), transport, log: Arc::new(DiscardSink), sim: SimPoints::new() }
}

fn post(body: &str) -> Request<axum::body::Body> {
    Request::builder().method("POST").uri(CHAT_COMPLETIONS_PATH).header("host", "proxy").header("content-type", "application/json").body(axum::body::Body::from(body.to_string())).unwrap()
}

#[tokio::test(start_paused = true)]
async fn tc16_a_6850_token_request_gets_the_engines_400_unchanged_and_the_node_stays_available() {
    let mut s = spec(4);
    s.ctx_total = 16384;
    let stub = Arc::new(MultiSlotLlamaStub::new(s));
    let body = format!("{{\"model\":\"{FLEET_ALIAS}\",\"messages\":[],\"prompt_tokens\":6850}}");
    // What the engine says to the same body when asked directly.
    let direct = stub.send(UpstreamRequest { method: http::Method::POST, uri: format!("{NODE_URL}{CHAT_COMPLETIONS_PATH}").parse().unwrap(), headers: http::HeaderMap::new(), body: Bytes::from(body.clone()) }).await.ok().unwrap();
    let direct_status = direct.status;
    let direct_headers = direct.headers.clone();
    let mut direct_bytes = Vec::new();
    let mut stream = direct.body;
    while let Some(chunk) = stream.next().await {
        direct_bytes.extend_from_slice(&chunk.unwrap());
    }
    let client = serve_duplex(build_router(one_node_deps(seams(stub.clone())))).await;
    let response = client.send(post(&body)).await;
    assert_eq!(response.status(), direct_status);
    assert_eq!(direct_status, StatusCode::BAD_REQUEST);
    for (name, value) in &direct_headers {
        assert_eq!(response.headers().get(name), Some(value), "{name}");
    }
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(bytes.as_ref(), direct_bytes.as_slice(), "the body is the engine's, byte for byte");
    let code = error_type_of(&bytes);
    assert_eq!(classify_error(400, code.as_deref()), ErrorClass::RequestFault);
    // The node still serves a request that fits.
    let ok = client.send(post(&format!("{{\"model\":\"{FLEET_ALIAS}\",\"messages\":[],\"prompt_tokens\":100,\"output_tokens\":2}}"))).await;
    assert_eq!(ok.status(), StatusCode::OK);
}

const ENGINE_500: &str = "{\"error\":{\"code\":500,\"type\":\"server_error\",\"message\":\"slot failed\"}}";

#[tokio::test(start_paused = true)]
async fn tc17_a_500_for_a_valid_json_body_reaches_the_harness_unchanged_and_the_node_stays_available() {
    let fake = Arc::new(FakeTransport::new(Script::Response { status: StatusCode::INTERNAL_SERVER_ERROR, chunks: vec![(Duration::from_millis(1), Ok(Bytes::from_static(ENGINE_500.as_bytes())))] }));
    let client = serve_duplex(build_router(one_node_deps(seams(fake.clone())))).await;
    let response = client.send(post(&format!("{{\"model\":\"{FLEET_ALIAS}\",\"messages\":[]}}"))).await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(bytes.as_ref(), ENGINE_500.as_bytes());
    assert_eq!(classify_error(500, error_type_of(&bytes).as_deref()), ErrorClass::RequestFault);
    let again = client.send(post(&format!("{{\"model\":\"{FLEET_ALIAS}\",\"messages\":[]}}"))).await;
    assert_eq!(again.status(), StatusCode::INTERNAL_SERVER_ERROR, "the node is still routed to");
    assert_eq!(fake.requests().len(), 2);
}

const REAL_STREAM_CAPTURE: &str = include_str!("../../../specs/proxy/evidence/captures/llama-stream-timings-20261008T005552735Z.json");

fn sse_of(chunk: &serde_json::Value) -> String {
    format!("data: {chunk}\n\ndata: [DONE]\n\n")
}

#[tokio::test(start_paused = true)]
async fn tc12_the_real_streamed_capture_gives_reuse_from_its_terminal_chunk_through_the_request_path() {
    let capture: serde_json::Value = serde_json::from_str(REAL_STREAM_CAPTURE).unwrap();
    let requests = capture["requests"].as_array().unwrap();
    let adapter = LlamaServerAdapter;
    for (index, expected) in [(2usize, 0u64), (3, 411)] {
        let sse = sse_of(&requests[index]["body"]);
        let fake = Arc::new(FakeTransport::new(Script::Response { status: StatusCode::OK, chunks: vec![(Duration::from_millis(1), Ok(Bytes::from(sse.clone())))] }));
        let client = serve_duplex(build_router(one_node_deps(seams(fake)))).await;
        let response = client.send(post(&format!("{{\"model\":\"{FLEET_ALIAS}\",\"messages\":[],\"stream\":true}}"))).await;
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let text = String::from_utf8(bytes.to_vec()).unwrap();
        let last = text.lines().rev().find(|l| l.starts_with("data: {")).expect("a terminal chunk");
        let view = UsageView { json_tail: last.as_bytes(), protocol: Protocol::OpenAiChat, stream: true };
        assert_eq!(adapter.reuse_fields(&view, ReuseProbeState::Unprobed), ReuseReading::Reused { cached_tokens: expected, field: ReuseFieldName::TimingsCacheN }, "request {index}");
    }
    // A streamed turn from an engine that sends no timings reads as unknown, never as poor.
    let without = r#"data: {"choices":[],"usage":{"prompt_tokens":10,"completion_tokens":2}}"#;
    let view = UsageView { json_tail: without.as_bytes(), protocol: Protocol::OpenAiChat, stream: true };
    assert_eq!(adapter.reuse_fields(&view, ReuseProbeState::Unprobed), ReuseReading::Unknown(UnknownReason::FieldAbsent));
}

fn fleet(nodes: &[(&str, &str, &str)]) -> Registry {
    let mut text = String::from("version: 1\nnodes:\n");
    for (name, engine, extra) in nodes {
        text += &format!("  {name}:\n    engine: {{ name: {engine} }}\n    model: m\n    endpoints: [ {{ protocol: openai-chat, base_url: \"{NODE_URL}\" }} ]\n{extra}");
    }
    text += &format!("aliases:\n  a: {{ nodes: [{}] }}\n", nodes.iter().map(|n| n.0).collect::<Vec<_>>().join(", "));
    Registry::from_text(&text).expect("valid registry")
}

async fn settle() {
    for _ in 0..10 {
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
}

#[tokio::test(start_paused = true)]
async fn the_facts_are_read_at_join_re_read_on_a_changed_entry_dropped_on_removal_and_read_on_return() {
    use legatus_proxy::config::diff::diff_registries;
    use legatus_proxy::config::reload::ReloadObserver;
    use legatus_proxy::engine::llama_server::{FactsRefresher, NodeFactsStore};
    let recorded = Recording::new(Arc::new(MultiSlotLlamaStub::new(spec(4))));
    let store = Arc::new(NodeFactsStore::default());
    let refresher = Arc::new(FactsRefresher::new(recorded.clone(), store.clone()));

    let first = fleet(&[("n1", "llama-server", ""), ("gone", "llama-server", ""), ("o1", "ollama", "")]);
    refresher.spawn_join(&first);
    settle().await;
    let id = |n: &str| legatus_common::ids::NodeId(n.to_string());
    assert_eq!(store.get(&id("n1")).unwrap().slots, EffectiveSlots { slots: 4, source: SlotsSource::Props });
    assert!(store.get(&id("gone")).is_some());
    assert!(store.get(&id("o1")).is_none(), "another engine is not read by this adapter");
    assert_eq!(recorded.paths().len(), 2, "one properties read per llama-server node: {:?}", recorded.paths());

    let second = fleet(&[("n1", "llama-server", "    slots: 2\n"), ("o1", "ollama", ""), ("fresh", "llama-server", "")]);
    let diff = diff_registries(&first, &second);
    refresher.on_reload(&first, &second, &diff);
    settle().await;
    assert_eq!(store.get(&id("n1")).unwrap().slots, EffectiveSlots { slots: 2, source: SlotsSource::Registry }, "the changed entry is re-read");
    assert_eq!(store.get(&id("fresh")).unwrap().slots.slots, 4, "an added node is read");
    assert!(store.get(&id("gone")).is_none(), "a removed node is dropped");

    // A node returns from down: story 175 calls refresh_node.
    let before = recorded.paths().len();
    refresher.refresh_node(second.node(&id("n1")).unwrap()).await;
    assert_eq!(recorded.paths().len(), before + 1);
}
