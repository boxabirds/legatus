//! Story 174 integration tests: the guard in the request path with the Ollama stub of story 162
//! and replies shaped like the real captured ones, on paused time.
use super::stub_helpers::*;
use bytes::Bytes;
use http::{Request, StatusCode};
use http_body_util::BodyExt;
use legatus_common::ids::NodeId;
use legatus_proxy::config::typed::{Registry, RegistryHandle};
use legatus_proxy::engine::AdapterRegistry;
use legatus_proxy::obs::log_sink::{LogRecord, TruncationFields};
use legatus_proxy::sim::SimPoints;
use legatus_proxy::time::WallTime;
use legatus_proxy::upstream::transport::UpstreamTransport;
use legatus_proxy::{build_router, RouterDeps, Seams, CHAT_COMPLETIONS_PATH};
use legatus_testkit::stubs::ollama::SerialOllamaStub;
use legatus_testkit::stubs::scenario::{StubKind, StubSpec};
use legatus_testkit::virt::{serve_duplex, FakeTransport, MemorySink, Script, SimWall};
use std::sync::Arc;
use std::time::Duration;

const ALIAS: &str = "local-coder";
const CONTEXT: u32 = 4096;
const BYTES_PER_TOKEN: usize = 3;
const SINK_CAPACITY: usize = 64;
const CANARY_PROMPT: &str = "CANARY-PROMPT-7f3a";

fn registry(ratio: Option<&str>) -> Registry {
    let settings = ratio.map(|r| format!("  ollama_truncation_limit_ratio: {r}\n")).unwrap_or_default();
    let text = format!(
        "version: 1\nsettings:\n  listen: 127.0.0.1:8080\n{settings}nodes:\n  o1:\n    engine: {{ name: ollama }}\n    model: qwen3\n    endpoints: [ {{ protocol: openai-chat, base_url: \"http://node.invalid\" }} ]\n    context: {{ per_slot: {CONTEXT}, on_overflow: silent_truncate }}\naliases:\n  {ALIAS}: {{ nodes: [o1] }}\n"
    );
    Registry::from_text(&text).expect("valid registry")
}

fn deps(transport: Arc<dyn UpstreamTransport>, sink: Arc<MemorySink>, ratio: Option<&str>) -> RouterDeps {
    let seams = Seams { wall: Arc::new(SimWall::new(WallTime { unix_ms: 0 })), transport, log: sink, sim: SimPoints::new() };
    RouterDeps::for_test(seams).with_registry(Arc::new(RegistryHandle::new(Arc::new(registry(ratio))))).with_adapters(Arc::new(AdapterRegistry::standard()))
}

/// A request body of about `tokens` estimated tokens (three bytes each). The node sees the model
/// name `qwen3` instead of the alias, which is six bytes shorter, so the estimate of the sent body
/// is two tokens below `tokens`.
fn body_of(tokens: usize, extra: &str) -> String {
    let prefix = format!("{{\"model\":\"{ALIAS}\",{extra}\"messages\":[{{\"role\":\"user\",\"content\":\"");
    let suffix = "\"}]}";
    let pad = (tokens * BYTES_PER_TOKEN).saturating_sub(prefix.len() + suffix.len());
    format!("{prefix}{}{suffix}", "x".repeat(pad))
}

fn post(body: &str) -> Request<axum::body::Body> {
    Request::builder().method("POST").uri(CHAT_COMPLETIONS_PATH).header("host", "proxy").header("content-type", "application/json").body(axum::body::Body::from(body.to_string())).unwrap()
}

async fn read(response: http::Response<axum::body::Body>) -> (StatusCode, String) {
    let status = response.status();
    (status, String::from_utf8(response.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap())
}

fn stub() -> Arc<dyn UpstreamTransport> {
    let mut spec = StubSpec::new("ollama", StubKind::SerialOllama);
    spec.ctx_total = CONTEXT;
    spec.model = "qwen3".to_string();
    spec.speed.load_time_ms = 0;
    Arc::new(SerialOllamaStub::new(spec))
}

#[tokio::test(start_paused = true)]
async fn tc07_a_prompt_over_the_limit_gets_the_context_error_and_the_node_sees_nothing_a_smaller_one_passes() {
    let recording = Recording::new(stub());
    let sink = Arc::new(MemorySink::new(SINK_CAPACITY));
    // The limit ratio 0.5 gives a limit of 2048 tokens; about 3000 is over it.
    let client = serve_duplex(build_router(deps(recording.clone(), sink, Some("0.5")))).await;
    let over = body_of(3000, "");
    let (status, text) = read(client.send(post(&over)).await).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(text.contains("2048") && text.contains("2998") && text.to_lowercase().contains("context"), "{text}");
    assert!(recording.paths().is_empty(), "nothing reached the node: {:?}", recording.paths());
    let under = body_of(1500, "");
    let (status, _) = read(client.send(post(&under)).await).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(recording.paths(), vec![CHAT_COMPLETIONS_PATH.to_string()]);
}

#[tokio::test(start_paused = true)]
async fn tc07_the_same_limit_holds_on_the_messages_shape_and_the_default_ratio_is_the_whole_context() {
    let sink = Arc::new(MemorySink::new(SINK_CAPACITY));
    let fake = Arc::new(FakeTransport::new(Script::Response { status: StatusCode::OK, chunks: vec![(Duration::from_millis(1), Ok(Bytes::from_static(b"{}")))] }));
    let client = serve_duplex(build_router(deps(fake.clone(), sink, None))).await;
    // Default ratio 1.0: 3000 tokens pass under a 4096 context, 4200 do not.
    assert_eq!(read(client.send(post(&body_of(3000, ""))).await).await.0, StatusCode::OK);
    let (status, text) = read(client.send(post(&body_of(4200, ""))).await).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(text.contains("4096"), "{text}");
    assert_eq!(fake.requests().len(), 1);
    // The equal case passes: exactly 4096 tokens.
    assert_eq!(read(client.send(post(&body_of(4096, ""))).await).await.0, StatusCode::OK);
    // The Messages route is not served by the router yet (story 131), so the Messages refusal
    // shape is covered by the unit test of the refusal text.
}

const OLLAMA_REPLY: &str = "{\"id\":\"chatcmpl-1\",\"object\":\"chat.completion\",\"model\":\"qwen3\",\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":PROMPT,\"prompt_tokens_details\":{\"cached_tokens\":0},\"completion_tokens\":4,\"total_tokens\":0}}";

fn reply_with(prompt_tokens: u32) -> String {
    OLLAMA_REPLY.replace("PROMPT", &prompt_tokens.to_string())
}

#[tokio::test(start_paused = true)]
async fn tc12_a_cut_the_guard_let_through_is_recorded_once_with_counts_and_the_answer_is_unchanged() {
    // Default ratio: a 3000-token prompt passes. The engine cut it to 2050 and says so.
    let answer = reply_with(2050);
    let fake = Arc::new(FakeTransport::new(Script::Response { status: StatusCode::OK, chunks: vec![(Duration::from_millis(1), Ok(Bytes::from(answer.clone())))] }));
    let sink = Arc::new(MemorySink::new(SINK_CAPACITY));
    let client = serve_duplex(build_router(deps(fake, sink.clone(), None))).await;
    let (status, text) = read(client.send(post(&body_of(3000, ""))).await).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(text, answer, "the answer is delivered unchanged");
    let records = sink.take();
    let found: Vec<&TruncationFields> = records
        .iter()
        .filter_map(|r| match r {
            LogRecord::System(s) => s.truncation.as_ref(),
            _ => None,
        })
        .collect();
    assert_eq!(found.len(), 1, "one event");
    assert_eq!(found[0].node, NodeId("o1".to_string()));
    assert_eq!((found[0].prompt_tokens_sent, found[0].prompt_tokens_seen), (2998, 2050));
    assert!(!format!("{found:?}").contains("xxx"), "no prompt text in the event");
}

#[tokio::test(start_paused = true)]
async fn tc12_no_event_when_the_count_is_missing_equal_or_within_a_quarter() {
    for answer in ["{\"choices\":[]}".to_string(), reply_with(3000), reply_with(2300), reply_with(3600)] {
        let fake = Arc::new(FakeTransport::new(Script::Response { status: StatusCode::OK, chunks: vec![(Duration::from_millis(1), Ok(Bytes::from(answer.clone())))] }));
        let sink = Arc::new(MemorySink::new(SINK_CAPACITY));
        let client = serve_duplex(build_router(deps(fake, sink.clone(), None))).await;
        let (status, text) = read(client.send(post(&body_of(3000, ""))).await).await;
        assert_eq!((status, text.as_str()), (StatusCode::OK, answer.as_str()));
        let events = sink.take().iter().filter(|r| matches!(r, LogRecord::System(s) if s.truncation.is_some())).count();
        assert_eq!(events, 0, "{answer}");
    }
}

#[tokio::test(start_paused = true)]
async fn tc13_a_first_byte_after_39_9_seconds_and_a_model_reload_fail_nothing() {
    let late = Duration::from_millis(39_900);
    let fake = Arc::new(FakeTransport::new(Script::Response { status: StatusCode::OK, chunks: vec![(late, Ok(Bytes::from(reply_with(100))))] }));
    let sink = Arc::new(MemorySink::new(SINK_CAPACITY));
    let client = serve_duplex(build_router(deps(fake.clone(), sink.clone(), None))).await;
    let started = tokio::time::Instant::now();
    let (status, _) = read(client.send(post(&body_of(100, ""))).await).await;
    assert_eq!(status, StatusCode::OK);
    assert!(started.elapsed() >= late, "{:?}", started.elapsed());
    // Ten minutes of silence before the first byte is still not a failure: no timer exists.
    let very_late = Duration::from_secs(600);
    let slow = Arc::new(FakeTransport::new(Script::Response { status: StatusCode::OK, chunks: vec![(very_late, Ok(Bytes::from(reply_with(100))))] }));
    let slow_client = serve_duplex(build_router(deps(slow, sink, None))).await;
    assert_eq!(read(slow_client.send(post(&body_of(100, ""))).await).await.0, StatusCode::OK);
    // The node still serves the next request.
    assert_eq!(read(client.send(post(&body_of(100, ""))).await).await.0, StatusCode::OK);
    assert_eq!(fake.requests().len(), 2);
}

#[tokio::test(start_paused = true)]
async fn tc14_a_keep_alive_from_the_harness_reaches_the_node_and_none_is_added() {
    let fake = Arc::new(FakeTransport::new(Script::Response { status: StatusCode::OK, chunks: vec![(Duration::from_millis(1), Ok(Bytes::from(reply_with(100))))] }));
    let sink = Arc::new(MemorySink::new(SINK_CAPACITY));
    let client = serve_duplex(build_router(deps(fake.clone(), sink, None))).await;
    let with = body_of(100, "\"keep_alive\":\"30m\",");
    let without = body_of(100, "");
    assert_eq!(read(client.send(post(&with)).await).await.0, StatusCode::OK);
    assert_eq!(read(client.send(post(&without)).await).await.0, StatusCode::OK);
    let seen = fake.requests();
    assert_eq!(seen[0].body, Bytes::from(with.replace(ALIAS, "qwen3")), "identical bytes apart from the model name");
    assert!(String::from_utf8_lossy(&seen[0].body).contains("\"keep_alive\":\"30m\""));
    assert!(!String::from_utf8_lossy(&seen[1].body).contains("keep_alive"), "none is added");
    let _ = CANARY_PROMPT;
}
