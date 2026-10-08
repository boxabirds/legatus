//! Story 126 integration tests through the real router: the key a request gets, the same key on a
//! retry, no raw value in any output, the request bytes unchanged, and the body read lazily.
use async_trait::async_trait;
use bytes::Bytes;
use http::{Request, StatusCode};
use http_body_util::BodyExt;
use legatus_proxy::key::hasher::ConversationKey;
use legatus_proxy::key::lazy_body::KeyProbe;
use legatus_proxy::config::typed::{Registry, RegistryHandle};
use legatus_proxy::obs::log_sink::LogRecord;
use legatus_proxy::protocol::chat::{HookOutcome, PipelineHooks};
use legatus_proxy::protocol::ctx::RequestCtx;
use legatus_proxy::sim::SimPoints;
use legatus_proxy::time::WallTime;
use legatus_proxy::{build_router, RouterDeps, Seams, CHAT_COMPLETIONS_PATH};
use legatus_testkit::virt::{serve_duplex, FakeTransport, MemorySink, Script, SimWall};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const SINK_CAPACITY: usize = 64;
const CANARY_SESSION: &str = "SECRET-SESSION-123";
const LONG_HEADER_BYTES: usize = 10 * 1024;

/// Remembers the key and the source of each request, as the affinity table will see them.
#[derive(Default)]
struct Seen(Mutex<Vec<(Option<ConversationKey>, String)>>);

#[async_trait]
impl PipelineHooks for Seen {
    async fn compute_key(&self, ctx: &mut RequestCtx, _headers: &http::HeaderMap) -> HookOutcome {
        self.0.lock().unwrap().push((ctx.key, ctx.key_source.label()));
        HookOutcome::Continue
    }
}

fn registry(alias_extra: &str) -> Registry {
    let text = format!("version: 1\nsettings:\n  listen: 127.0.0.1:8080\nnodes:\n  a:\n    engine: {{ name: ollama }}\n    model: model-a\n    endpoints: [ {{ protocol: openai-chat, base_url: \"http://a.invalid:1\" }} ]\naliases:\n  x: {{ nodes: [a]{alias_extra} }}\n");
    Registry::from_text(&text).expect("valid registry")
}

struct Rig {
    client: legatus_testkit::virt::DuplexClient,
    seen: Arc<Seen>,
    sink: Arc<MemorySink>,
    fake: Arc<FakeTransport>,
    probe: KeyProbe,
}

async fn rig(script: Script, alias_extra: &str) -> Rig {
    let fake = Arc::new(FakeTransport::new(script));
    let sink = Arc::new(MemorySink::new(SINK_CAPACITY));
    let seams = Seams { wall: Arc::new(SimWall::new(WallTime { unix_ms: 0 })), transport: fake.clone(), log: sink.clone(), sim: SimPoints::new() };
    let seen = Arc::new(Seen::default());
    let probe = KeyProbe::default();
    let deps = RouterDeps::for_test(seams).with_registry(Arc::new(RegistryHandle::new(Arc::new(registry(alias_extra))))).with_hooks(seen.clone()).with_key_probe(probe.clone());
    Rig { client: serve_duplex(build_router(deps)).await, seen, sink, fake, probe }
}

fn ok() -> Script {
    Script::Response { status: StatusCode::OK, chunks: vec![(Duration::from_millis(1), Ok(Bytes::from_static(b"{}")))] }
}

fn request(headers: &[(&str, &str)], body: &str) -> Request<axum::body::Body> {
    let mut b = Request::builder().method("POST").uri(CHAT_COMPLETIONS_PATH).header("host", "proxy").header("content-type", "application/json");
    for (n, v) in headers {
        b = b.header(*n, *v);
    }
    b.body(axum::body::Body::from(body.to_string())).unwrap()
}

const BODY: &str = "{\"model\":\"x\",\"messages\":[{\"role\":\"user\",\"content\":\"hello\"}],\"prompt_cache_key\":\"pck-1\"}";

async fn status_of(rig: &Rig, headers: &[(&str, &str)], body: &str) -> StatusCode {
    let response = rig.client.send(request(headers, body)).await;
    let status = response.status();
    let _ = response.into_body().collect().await;
    status
}

#[tokio::test(start_paused = true)]
async fn tc10_a_request_retried_after_a_503_gets_the_same_key_and_the_same_source() {
    let rig = rig(Script::Response { status: StatusCode::SERVICE_UNAVAILABLE, chunks: vec![(Duration::from_millis(1), Ok(Bytes::from_static(b"{}")))] }, "").await;
    let headers = [("x-session-affinity", "conv-1")];
    assert_eq!(status_of(&rig, &headers, BODY).await, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(status_of(&rig, &headers, BODY).await, StatusCode::SERVICE_UNAVAILABLE);
    let seen = rig.seen.0.lock().unwrap().clone();
    assert_eq!(seen.len(), 2);
    assert!(seen[0].0.is_some());
    assert_eq!(seen[0], seen[1]);
    assert_eq!(seen[0].1, "header:x-session-affinity");
    // Another conversation gets another key.
    status_of(&rig, &[("x-session-affinity", "conv-2")], BODY).await;
    assert_ne!(rig.seen.0.lock().unwrap()[2].0, seen[0].0);
}

#[tokio::test(start_paused = true)]
async fn tc18_a_canary_session_id_reaches_no_event_and_the_body_the_node_gets_is_what_was_sent() {
    let rig = rig(ok(), "").await;
    let status = status_of(&rig, &[("x-session-affinity", CANARY_SESSION)], BODY).await;
    assert_eq!(status, StatusCode::OK);
    let events = format!("{:?}", rig.sink.take());
    assert!(!events.contains(CANARY_SESSION), "{events}");
    let seen = rig.seen.0.lock().unwrap().clone();
    assert!(!format!("{seen:?}").contains(CANARY_SESSION), "the key and its source hold no raw value");
    let got = rig.fake.requests().pop().unwrap();
    assert_eq!(got.body, Bytes::from(BODY.replace("\"x\"", "\"model-a\"")), "only the model name changed");
    // The harness's own header is forwarded as sent (it is the harness's header), the key adds none.
    assert_eq!(got.headers.get("x-session-affinity").unwrap(), CANARY_SESSION);
    assert!(got.headers.keys().all(|k| !k.as_str().contains("key")), "{:?}", got.headers.keys().collect::<Vec<_>>());
}

#[tokio::test(start_paused = true)]
async fn tc14_a_header_key_causes_zero_body_parses_and_a_body_source_causes_one() {
    let rig_header = rig(ok(), "").await;
    status_of(&rig_header, &[("x-session-affinity", "c")], BODY).await;
    assert_eq!(rig_header.probe.parses(), 0);
    let rig_body = rig(ok(), ", affinity: { key_headers: [x-nobody-sends-this], hash_fallback: false }").await;
    status_of(&rig_body, &[], BODY).await;
    assert_eq!(rig_body.probe.parses(), 0, "a header map never reads the body");
    // A route map with a body field parses the body once.
    let text = "version: 1\nsettings:\n  listen: 127.0.0.1:8080\nnodes:\n  a:\n    engine: { name: ollama }\n    model: model-a\n    endpoints: [ { protocol: openai-chat, base_url: \"http://a.invalid:1\" } ]\naliases:\n  x: { nodes: [a] }\nroutes:\n  - path: /v1/chat/completions\n    alias: x\n    key_map: [ { header: session-id }, { body_field: prompt_cache_key } ]\n";
    let fake = Arc::new(FakeTransport::new(ok()));
    let seams = Seams { wall: Arc::new(SimWall::new(WallTime { unix_ms: 0 })), transport: fake, log: Arc::new(MemorySink::new(SINK_CAPACITY)), sim: SimPoints::new() };
    let seen = Arc::new(Seen::default());
    let probe = KeyProbe::default();
    let deps = RouterDeps::for_test(seams).with_registry(Arc::new(RegistryHandle::new(Arc::new(Registry::from_text(text).unwrap())))).with_hooks(seen.clone()).with_key_probe(probe.clone());
    let client = serve_duplex(build_router(deps)).await;
    let r = client.send(request(&[("session-id", "S")], BODY)).await;
    let _ = r.into_body().collect().await;
    assert_eq!(probe.parses(), 0, "the header won");
    let r = client.send(request(&[], BODY)).await;
    let _ = r.into_body().collect().await;
    assert_eq!(probe.parses(), 1);
    let s = seen.0.lock().unwrap().clone();
    assert_eq!(s[1].1, "body_field:prompt_cache_key");
    assert!(s[1].0.is_some());
}

#[tokio::test(start_paused = true)]
async fn tc03_a_ten_kib_session_header_is_served_with_a_normal_status_and_no_key_header_case_refuses() {
    let rig = rig(ok(), "").await;
    let long = "s".repeat(LONG_HEADER_BYTES);
    assert_eq!(status_of(&rig, &[("x-session-affinity", &long)], BODY).await, StatusCode::OK);
    for headers in [vec![("x-session-affinity", "")], vec![("x-session-affinity", "  ")], vec![("x-session-id", "a"), ("x-session-id", "b")], vec![("X-SESSION-AFFINITY", "u")], vec![]] {
        assert_eq!(status_of(&rig, &headers, BODY).await, StatusCode::OK, "{headers:?}");
    }
    let sources: Vec<String> = rig.seen.0.lock().unwrap().iter().map(|s| s.1.clone()).collect();
    assert_eq!(sources[0], "header:x-session-affinity");
    assert_eq!(sources[1], "credential", "no header: the walk hands over to the credential source, which is story 180");
}

#[tokio::test(start_paused = true)]
async fn the_warning_for_a_header_the_map_names_is_written_once_for_many_requests() {
    let rig = rig(ok(), ", affinity: { key_headers: [x-not-sent], hash_fallback: true }").await;
    for _ in 0..4 {
        status_of(&rig, &[], BODY).await;
    }
    let warnings = rig.sink.take().into_iter().filter(|r| matches!(r, LogRecord::System(s) if s.code == Some("key_header_missing"))).count();
    assert_eq!(warnings, 1);
}
