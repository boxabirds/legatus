//! Story 121 integration tests of the chat path with the in-memory transport and paused time.
use async_trait::async_trait;
use axum::body::Body;
use bytes::Bytes;
use http::{Request, StatusCode};
use http_body_util::BodyExt;
use legatus_common::ids::NodeId;
use legatus_common::protocol::Protocol;
use legatus_proxy::lifecycle::start::{start_gate, GateState};
use legatus_proxy::obs::log_sink::DiscardSink;
use legatus_proxy::protocol::chat::{HookOutcome, PipelineHooks, PipelineOutcome, PipelineStep, StepTrace, STEP_ORDER};
use legatus_proxy::protocol::ctx::RequestCtx;
use legatus_proxy::protocol::errors::RefusalKind;
use legatus_proxy::protocol::rewrite::{splice_top_level, RequestPeek, TopLevelEdit};
use legatus_proxy::sim::SimPoints;
use legatus_proxy::time::WallTime;
use legatus_proxy::{build_router, RouterDeps, Seams, CHAT_COMPLETIONS_PATH};
use legatus_testkit::fleet::{deps_from_text, one_node_deps, one_node_registry_text, FLEET_NODE_MODEL, FLEET_NODE_URL, FLEET_REQUEST_BODY};
use legatus_testkit::virt::{serve_duplex, FakeTransport, MemorySink, Script, SimWall};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const SINK_CAPACITY: usize = 64;
const CANARY_SECRET: &str = "sk-ant-api03-Zk3QfN8vLm2PxTcR9wYbHd7sUe1AoJqGiV5nXtKzB4";
const CANARY_PROMPT: &str = "Summarise the confidential merger memo for Halvorsen Biotech";
const NODE_REPLY: &str = "data: {\"model\":\"qwen-node\",\"choices\":[]}\n\ndata: [DONE]\n\n";

fn seams(transport: Arc<FakeTransport>) -> Seams {
    Seams { wall: Arc::new(SimWall::new(WallTime { unix_ms: 0 })), transport, log: Arc::new(DiscardSink), sim: SimPoints::new() }
}

fn ok_reply() -> Script {
    Script::Response { status: StatusCode::OK, chunks: vec![(Duration::from_millis(1), Ok(Bytes::from_static(NODE_REPLY.as_bytes())))] }
}

fn post(body: &str) -> Request<Body> {
    Request::builder().method("POST").uri(CHAT_COMPLETIONS_PATH).header("host", "proxy").header("content-type", "application/json").body(Body::from(body.to_string())).unwrap()
}

async fn read_all(response: http::Response<Body>) -> (StatusCode, Vec<u8>) {
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes().to_vec();
    (status, bytes)
}

fn code_of(body: &[u8]) -> String {
    let v: serde_json::Value = serde_json::from_slice(body).unwrap();
    v["error"]["code"].as_str().unwrap_or_default().to_string()
}

#[tokio::test(start_paused = true)]
async fn the_node_receives_its_own_model_name_and_only_that_changed() {
    let fake = Arc::new(FakeTransport::new(ok_reply()));
    let client = serve_duplex(build_router(one_node_deps(seams(fake.clone())))).await;
    let sent = "{\"model\":\"local-coder\",\"messages\":[{\"role\":\"user\",\"content\":\"hi\"}],\"stream\":true,\"stream_options\":{\"include_usage\":true},\"x\":[1, 2]}";
    let (status, body) = read_all(client.send(post(sent)).await).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, NODE_REPLY.as_bytes());
    let requests = fake.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].body, Bytes::from(sent.replace("local-coder", FLEET_NODE_MODEL)));
    assert_eq!(requests[0].uri.to_string(), format!("{FLEET_NODE_URL}{CHAT_COMPLETIONS_PATH}"));
    assert_eq!(requests[0].headers["host"], "node.invalid");
}

#[tokio::test(start_paused = true)]
async fn tc12_accept_encoding_gzip_reaches_the_node_and_the_reply_bytes_are_equal() {
    let fake = Arc::new(FakeTransport::new(ok_reply()));
    let client = serve_duplex(build_router(one_node_deps(seams(fake.clone())))).await;
    let request = Request::builder()
        .method("POST")
        .uri(CHAT_COMPLETIONS_PATH)
        .header("host", "proxy")
        .header("accept-encoding", "gzip, deflate")
        .header("connection", "keep-alive, x-hop")
        .header("x-hop", "private")
        .header("x-keep", "me")
        .body(Body::from(FLEET_REQUEST_BODY))
        .unwrap();
    let (status, body) = read_all(client.send(request).await).await;
    assert_eq!((status, body), (StatusCode::OK, NODE_REPLY.as_bytes().to_vec()));
    let headers = &fake.requests()[0].headers;
    assert_eq!(headers["accept-encoding"], "gzip, deflate");
    assert_eq!(headers["x-keep"], "me");
    assert!(!headers.contains_key("x-hop") && !headers.contains_key("connection"));
}

#[tokio::test(start_paused = true)]
async fn tc13_an_unknown_alias_is_404_model_not_found_and_no_node_is_called() {
    let fake = Arc::new(FakeTransport::new(ok_reply()));
    let client = serve_duplex(build_router(one_node_deps(seams(fake.clone())))).await;
    let (status, body) = read_all(client.send(post(&format!("{{\"model\":\"{CANARY_SECRET}\",\"messages\":[]}}"))).await).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(code_of(&body), "model_not_found");
    assert!(!String::from_utf8_lossy(&body).contains("sk-ant"), "the model value is not echoed");
    assert!(fake.requests().is_empty());
}

#[tokio::test(start_paused = true)]
async fn tc05_tc06_a_bad_body_is_400_with_a_fixed_text_and_no_node_is_called() {
    let fake = Arc::new(FakeTransport::new(ok_reply()));
    let client = serve_duplex(build_router(one_node_deps(seams(fake.clone())))).await;
    for (body, code) in [("{\"messages\":[]}", "model_missing"), ("{\"model\":7}", "invalid_body"), ("[1]", "invalid_body"), ("not json", "invalid_body"), ("", "invalid_body"), ("{\"model\":\"a\",\"model\":\"b\"}", "invalid_body")] {
        let (status, bytes) = read_all(client.send(post(body)).await).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body:?}");
        assert_eq!(code_of(&bytes), code, "{body:?}");
    }
    assert!(fake.requests().is_empty());
}

#[tokio::test(start_paused = true)]
async fn tc14_a_reply_that_names_a_different_model_passes_unchanged() {
    let reply = "{\"id\":\"1\",\"model\":\"some-other-model-name\",\"choices\":[]}";
    let fake = Arc::new(FakeTransport::new(Script::Response { status: StatusCode::OK, chunks: vec![(Duration::ZERO, Ok(Bytes::from_static(reply.as_bytes())))] }));
    let client = serve_duplex(build_router(one_node_deps(seams(fake)))).await;
    let (_, body) = read_all(client.send(post(FLEET_REQUEST_BODY)).await).await;
    assert_eq!(body, reply.as_bytes());
}

/// Records which hooks ran, in order, and what the event step saw.
#[derive(Default)]
struct Recording {
    calls: Mutex<Vec<&'static str>>,
    outcomes: Mutex<Vec<PipelineOutcome>>,
    refuse_at: Option<&'static str>,
}

impl Recording {
    fn note(&self, name: &'static str) -> HookOutcome {
        self.calls.lock().unwrap().push(name);
        match self.refuse_at {
            Some(at) if at == name => HookOutcome::Refuse(RefusalKind::Starting),
            _ => HookOutcome::Continue,
        }
    }
    fn calls(&self) -> Vec<&'static str> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl PipelineHooks for Recording {
    async fn accept_and_auth(&self, _p: Protocol, _h: &http::HeaderMap) -> HookOutcome {
        self.note("accept_and_auth")
    }
    async fn after_alias(&self, _c: &mut RequestCtx) -> HookOutcome {
        self.note("after_alias")
    }
    async fn compute_key(&self, _c: &mut RequestCtx, _h: &http::HeaderMap) -> HookOutcome {
        self.note("compute_key")
    }
    async fn table_lookup(&self, _c: &mut RequestCtx) -> HookOutcome {
        self.note("table_lookup")
    }
    async fn place(&self, _c: &mut RequestCtx) -> Option<NodeId> {
        self.note("place");
        None
    }
    async fn hold(&self, _c: &mut RequestCtx) -> HookOutcome {
        self.note("hold")
    }
    async fn hold_refuse(&self, _c: &mut RequestCtx) -> HookOutcome {
        self.note("hold_refuse")
    }
    async fn patch_body(&self, _c: &mut RequestCtx, _b: &mut Bytes, _p: &RequestPeek) -> HookOutcome {
        self.note("patch_body")
    }
    async fn context_check(&self, _c: &mut RequestCtx, _b: &Bytes) -> HookOutcome {
        self.note("context_check")
    }
    async fn cache_feedback(&self, _c: &RequestCtx, _s: u16) {
        self.note("cache_feedback");
    }
    async fn table_update(&self, _c: &RequestCtx, _s: u16) {
        self.note("table_update");
    }
    async fn write_event(&self, _c: Option<&RequestCtx>, outcome: &PipelineOutcome) {
        self.note("write_event");
        self.outcomes.lock().unwrap().push(*outcome);
    }
}

#[tokio::test(start_paused = true)]
async fn tc15_a_served_request_runs_sixteen_steps_in_order_and_the_hooks_in_the_documented_order() {
    let fake = Arc::new(FakeTransport::new(ok_reply()));
    let trace = StepTrace::new();
    let hooks = Arc::new(Recording::default());
    let deps = one_node_deps(seams(fake)).with_trace(trace.clone()).with_hooks(hooks.clone());
    let client = serve_duplex(build_router(deps)).await;
    let (status, _) = read_all(client.send(post(FLEET_REQUEST_BODY)).await).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(trace.steps(), STEP_ORDER.to_vec());
    assert_eq!(
        hooks.calls(),
        vec!["accept_and_auth", "after_alias", "compute_key", "table_lookup", "place", "hold", "hold_refuse", "patch_body", "context_check", "cache_feedback", "table_update", "write_event"]
    );
    assert_eq!(*hooks.outcomes.lock().unwrap(), vec![PipelineOutcome::Served(200)]);
}

#[tokio::test(start_paused = true)]
async fn tc15_tc24_a_hook_refusal_stops_the_later_steps_but_the_event_step_still_runs() {
    let fake = Arc::new(FakeTransport::new(ok_reply()));
    let trace = StepTrace::new();
    let hooks = Arc::new(Recording { refuse_at: Some("hold"), ..Recording::default() });
    let deps = one_node_deps(seams(fake.clone())).with_trace(trace.clone()).with_hooks(hooks.clone());
    let client = serve_duplex(build_router(deps)).await;
    let (status, body) = read_all(client.send(post(FLEET_REQUEST_BODY)).await).await;
    assert_eq!((status, code_of(&body).as_str()), (StatusCode::SERVICE_UNAVAILABLE, "starting"));
    assert_eq!(hooks.calls(), vec!["accept_and_auth", "after_alias", "compute_key", "table_lookup", "place", "hold", "write_event"]);
    assert_eq!(*hooks.outcomes.lock().unwrap(), vec![PipelineOutcome::Refused(RefusalKind::Starting)]);
    assert_eq!(trace.steps().last(), Some(&PipelineStep::WriteEvent));
    assert!(!trace.steps().contains(&PipelineStep::Send));
    assert!(fake.requests().is_empty(), "no node was called");
}

#[tokio::test(start_paused = true)]
async fn tc20_an_alias_with_only_a_messages_node_offers_no_chat_node() {
    let text = "version: 1\nnodes:\n  m:\n    engine: { name: anthropic-hosted }\n    model: claude-x\n    endpoints: [ { protocol: anthropic-messages, base_url: \"https://api.anthropic.com\" } ]\naliases:\n  frontier: { nodes: [m] }\n";
    let fake = Arc::new(FakeTransport::new(ok_reply()));
    let client = serve_duplex(build_router(deps_from_text(seams(fake.clone()), text))).await;
    let (status, body) = read_all(client.send(post("{\"model\":\"frontier\"}")).await).await;
    assert_eq!((status, code_of(&body).as_str()), (StatusCode::NOT_FOUND, "model_not_found"));
    assert!(fake.requests().is_empty());
}

#[tokio::test(start_paused = true)]
async fn tc21_a_connect_failure_before_the_head_is_502_with_one_send_call() {
    let fake = Arc::new(FakeTransport::new(Script::Connect));
    let client = serve_duplex(build_router(one_node_deps(seams(fake.clone())))).await;
    let (status, body) = read_all(client.send(post(FLEET_REQUEST_BODY)).await).await;
    assert_eq!((status, code_of(&body).as_str()), (StatusCode::BAD_GATEWAY, "node_connect_failed"));
    assert_eq!(fake.requests().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn tc24_a_node_500_passes_unchanged_with_one_send_call() {
    let reply = "{\"error\":{\"message\":\"slot busy\",\"type\":\"server_error\"}}";
    let fake = Arc::new(FakeTransport::new(Script::Response { status: StatusCode::INTERNAL_SERVER_ERROR, chunks: vec![(Duration::ZERO, Ok(Bytes::from_static(reply.as_bytes())))] }));
    let client = serve_duplex(build_router(one_node_deps(seams(fake.clone())))).await;
    let (status, body) = read_all(client.send(post(FLEET_REQUEST_BODY)).await).await;
    assert_eq!((status, body), (StatusCode::INTERNAL_SERVER_ERROR, reply.as_bytes().to_vec()));
    assert_eq!(fake.requests().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn tc23_a_canary_in_the_body_and_a_secret_in_a_header_appear_in_no_record_and_no_refusal() {
    let sink = Arc::new(MemorySink::new(SINK_CAPACITY));
    let fake = Arc::new(FakeTransport::new(ok_reply()));
    let mut s = seams(fake);
    s.log = sink.clone();
    let client = serve_duplex(build_router(one_node_deps(s))).await;
    let served = Request::builder()
        .method("POST")
        .uri(CHAT_COMPLETIONS_PATH)
        .header("host", "proxy")
        .header("authorization", format!("Bearer {CANARY_SECRET}"))
        .body(Body::from(format!("{{\"model\":\"local-coder\",\"messages\":[{{\"role\":\"user\",\"content\":\"{CANARY_PROMPT}\"}}]}}")))
        .unwrap();
    let (_, served_body) = read_all(client.send(served).await).await;
    let refused = Request::builder().method("POST").uri(CHAT_COMPLETIONS_PATH).header("authorization", format!("Bearer {CANARY_SECRET}")).body(Body::from(format!("{{\"model\":\"{CANARY_PROMPT}\"}}"))).unwrap();
    let (_, refused_body) = read_all(client.send(refused).await).await;
    for text in [format!("{:?}", sink.take().len()), String::from_utf8_lossy(&refused_body).to_string(), String::from_utf8_lossy(&served_body).to_string()] {
        assert!(!text.contains("sk-ant") && !text.contains("Halvorsen"), "{text}");
    }
}

#[tokio::test(start_paused = true)]
async fn tc29_without_a_placer_the_first_chat_node_in_alias_order_is_chosen_and_a_messages_node_is_skipped() {
    let text = "version: 1\nnodes:\n  msg:\n    engine: { name: anthropic-hosted }\n    model: claude-x\n    endpoints: [ { protocol: anthropic-messages, base_url: \"https://api.anthropic.com\" } ]\n  first:\n    engine: { name: ollama }\n    model: model-first\n    endpoints: [ { protocol: openai-chat, base_url: \"http://first.invalid:1\" } ]\n  second:\n    engine: { name: ollama }\n    model: model-second\n    endpoints: [ { protocol: openai-chat, base_url: \"http://second.invalid:2\" } ]\naliases:\n  pool: { nodes: [msg, first, second] }\n";
    let fake = Arc::new(FakeTransport::new(ok_reply()));
    let client = serve_duplex(build_router(deps_from_text(seams(fake.clone()), text))).await;
    let (status, _) = read_all(client.send(post("{\"model\":\"pool\",\"x\":1}")).await).await;
    assert_eq!(status, StatusCode::OK);
    let requests = fake.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].uri.to_string(), "http://first.invalid:1/v1/chat/completions");
    assert_eq!(requests[0].body, Bytes::from("{\"model\":\"model-first\",\"x\":1}"));
}

/// A placer: always the second node of the pool.
struct SecondNode;

#[async_trait]
impl PipelineHooks for SecondNode {
    async fn place(&self, _ctx: &mut RequestCtx) -> Option<NodeId> {
        Some(NodeId("second".into()))
    }
}

#[tokio::test(start_paused = true)]
async fn tc29_an_installed_placer_replaces_the_default_rule() {
    let text = "version: 1\nnodes:\n  first:\n    engine: { name: ollama }\n    model: model-first\n    endpoints: [ { protocol: openai-chat, base_url: \"http://first.invalid:1\" } ]\n  second:\n    engine: { name: ollama }\n    model: model-second\n    endpoints: [ { protocol: openai-chat, base_url: \"http://second.invalid:2\" } ]\naliases:\n  pool: { nodes: [first, second] }\n";
    let fake = Arc::new(FakeTransport::new(ok_reply()));
    let client = serve_duplex(build_router(deps_from_text(seams(fake.clone()), text).with_hooks(Arc::new(SecondNode)))).await;
    read_all(client.send(post("{\"model\":\"pool\"}")).await).await;
    assert_eq!(fake.requests()[0].uri.to_string(), "http://second.invalid:2/v1/chat/completions");
    assert_eq!(fake.requests()[0].body, Bytes::from("{\"model\":\"model-second\"}"));
}

/// A patch hook: inserts `max_tokens` with the splice, on the peek it is given.
struct PatchHook {
    saw_model: Mutex<Option<String>>,
}

#[async_trait]
impl PipelineHooks for PatchHook {
    async fn patch_body(&self, _ctx: &mut RequestCtx, body: &mut Bytes, peek: &RequestPeek) -> HookOutcome {
        *self.saw_model.lock().unwrap() = peek.model().map(str::to_string);
        let edits = [TopLevelEdit::Insert { key: "max_tokens".into(), raw_value: Bytes::from_static(b"64") }, TopLevelEdit::Remove { key: "drop_me".into() }];
        *body = splice_top_level(body, peek, &edits);
        HookOutcome::Continue
    }
}

#[tokio::test(start_paused = true)]
async fn tc30_the_patch_hook_sees_a_fresh_peek_of_the_rewritten_body_for_a_longer_and_a_shorter_node_name() {
    for node_model in ["m", "a-considerably-longer-node-model-name-than-the-alias"] {
        let text = one_node_registry_text(FLEET_NODE_URL).replace(FLEET_NODE_MODEL, node_model);
        let fake = Arc::new(FakeTransport::new(ok_reply()));
        let hook = Arc::new(PatchHook { saw_model: Mutex::new(None) });
        let client = serve_duplex(build_router(deps_from_text(seams(fake.clone()), &text).with_hooks(hook.clone()))).await;
        read_all(client.send(post("{\"model\":\"local-coder\",\"drop_me\":[1,2],\"messages\":[]}")).await).await;
        assert_eq!(hook.saw_model.lock().unwrap().as_deref(), Some(node_model), "the peek describes the rewritten body");
        let received = fake.requests()[0].body.clone();
        assert_eq!(received, Bytes::from(format!("{{\"model\":\"{node_model}\",\"messages\":[],\"max_tokens\":64}}")));
        serde_json::from_slice::<serde_json::Value>(&received).unwrap();
    }
}

#[tokio::test(start_paused = true)]
async fn tc26_a_request_received_while_the_registry_loads_waits_and_is_served_when_the_load_is_valid() {
    let fake = Arc::new(FakeTransport::new(ok_reply()));
    let (gate, hooks) = start_gate();
    let client = Arc::new(serve_duplex(build_router(one_node_deps(seams(fake.clone())).with_hooks(Arc::new(hooks)))).await);
    let waiting = {
        let client = client.clone();
        tokio::spawn(async move { read_all(client.send(post(FLEET_REQUEST_BODY)).await).await })
    };
    tokio::time::sleep(Duration::from_secs(250)).await;
    assert!(!waiting.is_finished(), "the request waits, however long the load takes");
    assert!(fake.requests().is_empty());
    gate.send(GateState::Ready).unwrap();
    let (status, _) = waiting.await.unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(fake.requests().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn tc26_a_first_load_that_fails_answers_the_held_request_with_503_starting() {
    let fake = Arc::new(FakeTransport::new(ok_reply()));
    let (gate, hooks) = start_gate();
    let client = Arc::new(serve_duplex(build_router(one_node_deps(seams(fake.clone())).with_hooks(Arc::new(hooks)))).await);
    let waiting = {
        let client = client.clone();
        tokio::spawn(async move { read_all(client.send(post(FLEET_REQUEST_BODY)).await).await })
    };
    tokio::task::yield_now().await;
    gate.send(GateState::Failed).unwrap();
    let (status, body) = waiting.await.unwrap();
    assert_eq!((status, code_of(&body).as_str()), (StatusCode::SERVICE_UNAVAILABLE, "starting"));
    assert!(fake.requests().is_empty());
}

#[tokio::test(start_paused = true)]
async fn only_the_exact_chat_path_is_routed() {
    let fake = Arc::new(FakeTransport::new(ok_reply()));
    let client = serve_duplex(build_router(one_node_deps(seams(fake.clone())))).await;
    for path in ["/v1/chat/completions/", "/v1/chat/completion", "/V1/chat/completions", "/v1/messages"] {
        let request = Request::builder().method("POST").uri(path).body(Body::from(FLEET_REQUEST_BODY)).unwrap();
        assert_eq!(client.send(request).await.status(), StatusCode::NOT_FOUND, "{path}");
    }
    assert!(fake.requests().is_empty());
    let get = Request::builder().method("GET").uri(CHAT_COMPLETIONS_PATH).body(Body::empty()).unwrap();
    assert_eq!(client.send(get).await.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[test]
fn tc28_router_deps_for_test_defaults_to_an_empty_fleet_no_hooks_and_no_trace() {
    let fake = Arc::new(FakeTransport::new(Script::Connect));
    let deps = RouterDeps::for_test(seams(fake));
    assert!(deps.trace.is_none());
    let registry = deps.registry.snapshot();
    assert_eq!((registry.generation, registry.nodes.len(), registry.aliases.len()), (0, 0, 0));
}
