//! Story 153 integration tests: alias routing and the model rewrite through the real router over
//! real sockets, with the multi-slot llama-server stub of story 162 as the node.
use async_trait::async_trait;
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use legatus_proxy::obs::log_sink::DiscardSink;
use legatus_proxy::sim::SimPoints;
use legatus_proxy::time::WallTime;
use legatus_proxy::upstream::transport::{UpstreamError, UpstreamRequest, UpstreamResponse, UpstreamTransport};
use legatus_proxy::{Seams, CHAT_COMPLETIONS_PATH};
use legatus_testkit::fleet::deps_from_text;
use legatus_testkit::stubs::engine_for;
use legatus_testkit::stubs::scenario::{StubKind, StubSpec};
use legatus_testkit::virt::SimWall;
use std::sync::{Arc, Mutex};

const ALIAS: &str = "local-coder";
const NODE_MODEL: &str = "qwen3.8-27b-q4";

/// Wraps the stub and records every request that reaches it.
struct Recording {
    inner: Arc<dyn UpstreamTransport>,
    seen: Mutex<Vec<(Bytes, http::HeaderMap)>>,
}

#[async_trait]
impl UpstreamTransport for Recording {
    async fn send(&self, req: UpstreamRequest) -> Result<UpstreamResponse, UpstreamError> {
        self.seen.lock().unwrap().push((req.body.clone(), req.headers.clone()));
        self.inner.send(req).await
    }
}

async fn start() -> (std::net::SocketAddr, Arc<Recording>) {
    let mut spec = StubSpec::new("node", StubKind::MultiSlotLlama);
    spec.speed.load_time_ms = 0;
    let recording = Arc::new(Recording { inner: engine_for(&spec), seen: Mutex::new(Vec::new()) });
    let seams = Seams { wall: Arc::new(SimWall::new(WallTime { unix_ms: 0 })), transport: recording.clone(), log: Arc::new(DiscardSink), sim: SimPoints::new() };
    let registry = format!(
        "version: 1\nnodes:\n  strix:\n    engine: {{ name: llama-server }}\n    model: {NODE_MODEL}\n    endpoints: [ {{ protocol: openai-chat, base_url: \"http://strix.invalid:8081\" }} ]\n    slots: 2\naliases:\n  {ALIAS}: {{ nodes: [strix] }}\n"
    );
    let router = legatus_proxy::build_router(deps_from_text(seams, &registry));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (addr, recording)
}

async fn post(addr: std::net::SocketAddr, body: &str) -> (u16, String) {
    let client: Client<_, Full<Bytes>> = Client::builder(TokioExecutor::new()).build_http();
    let request = http::Request::builder().method("POST").uri(format!("http://{addr}{CHAT_COMPLETIONS_PATH}")).body(Full::new(Bytes::from(body.to_string()))).unwrap();
    let reply = client.request(request).await.unwrap();
    let status = reply.status().as_u16();
    let text = String::from_utf8_lossy(&reply.into_body().collect().await.unwrap().to_bytes()).to_string();
    (status, text)
}

fn harness_body(model: &str) -> String {
    format!("{{\"model\":\"{model}\",\"messages\":[{{\"role\":\"user\",\"content\":\"list the files\"}}],\"stream\":true,\"stream_options\":{{\"include_usage\":true}},\"prompt_tokens\":26,\"output_tokens\":3,\"zzz_unknown\":{{\"a\":[1,2,3]}}}}")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tc16_the_stub_gets_its_own_model_name_and_every_other_byte_as_sent_and_the_harness_sees_the_reply() {
    let (addr, node) = start().await;
    let sent = harness_body(ALIAS);
    let (status, reply) = post(addr, &sent).await;
    assert_eq!(status, 200);
    assert!(reply.contains("[DONE]"), "the harness sees the stub's reply: {reply}");
    let seen = node.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].0, Bytes::from(sent.replacen(ALIAS, NODE_MODEL, 1)), "only the model value differs");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tc17_an_unknown_alias_gets_the_unknown_model_error_and_the_stub_sees_nothing() {
    let (addr, node) = start().await;
    for wrong in ["local-coder ", "Local-Coder", "local", "qwen3.8-27b-q4"] {
        let (status, reply) = post(addr, &harness_body(wrong)).await;
        assert_eq!(status, 404, "{wrong:?}");
        assert!(reply.contains("\"code\":\"model_not_found\""), "{reply}");
        assert!(!reply.contains(wrong.trim()), "the value is not echoed: {reply}");
    }
    assert!(node.seen.lock().unwrap().is_empty(), "no request reached the node");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w1_a_misspelled_alias_is_refused_and_the_fixed_one_is_served() {
    let (addr, node) = start().await;
    assert_eq!(post(addr, &harness_body("local-codr")).await.0, 404);
    assert_eq!(post(addr, &harness_body(ALIAS)).await.0, 200);
    assert_eq!(node.seen.lock().unwrap().len(), 1);
}
