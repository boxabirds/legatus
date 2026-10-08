//! Story 160 integration tests: the triggers owned here, through the real router, with zero node
//! calls for every refusal, and a pi rule stub reading each answer.
use axum::body::Body;
use bytes::Bytes;
use http::{Request, StatusCode};
use http_body_util::BodyExt;
use legatus_proxy::obs::log_sink::DiscardSink;
use legatus_proxy::protocol::errors::{forbidden_word_in, WordList};
use legatus_proxy::sim::SimPoints;
use legatus_proxy::time::WallTime;
use legatus_proxy::{build_router, Seams, CHAT_COMPLETIONS_PATH};
use legatus_testkit::fleet::{deps_from_text, one_node_deps, one_node_registry_text, FLEET_REQUEST_BODY};
use legatus_testkit::harness_rules::{pi_reaction, Reaction};
use legatus_testkit::virt::{serve_duplex, FakeTransport, MemorySink, Script, SimWall};
use std::sync::Arc;
use std::time::Duration;

const SINK_CAPACITY: usize = 16;
const CANARY: &str = "sk-ant-api03-Zk3QfN8vLm2PxTcR9wYbHd7sUe1AoJqGiV5nXtKzB4";

fn ok_script() -> Script {
    Script::Response { status: StatusCode::OK, chunks: vec![(Duration::ZERO, Ok(Bytes::from_static(b"{}")))] }
}

fn seams(fake: Arc<FakeTransport>) -> Seams {
    Seams { wall: Arc::new(SimWall::new(WallTime { unix_ms: 0 })), transport: fake, log: Arc::new(DiscardSink), sim: SimPoints::new() }
}

fn post(body: &str) -> Request<Body> {
    Request::builder().method("POST").uri(CHAT_COMPLETIONS_PATH).header("host", "proxy").body(Body::from(body.to_string())).unwrap()
}

async fn answer(client: &legatus_testkit::virt::DuplexClient, body: &str) -> (u16, serde_json::Value, String) {
    let response = client.send(post(body)).await;
    let status = response.status().as_u16();
    let text = String::from_utf8(response.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    (status, serde_json::from_str(&text).unwrap_or(serde_json::Value::Null), text)
}

/// A refusal that must stop: right status and code, no forbidden word, pi stops, no node call.
fn assert_stop(status: u16, json: &serde_json::Value, text: &str, want_status: u16, want_code: &str) {
    assert_eq!(status, want_status, "{text}");
    assert_eq!(json["error"]["code"], want_code, "{text}");
    assert_eq!(json["error"]["type"], "invalid_request_error");
    assert_eq!(forbidden_word_in(WordList::Stop, text), None, "{text}");
    assert_eq!(pi_reaction(status, json["error"]["message"].as_str().unwrap()), Reaction::Stop, "pi must stop: {text}");
}

#[tokio::test(start_paused = true)]
async fn tc07_an_unknown_alias_gets_404_model_not_found_and_pi_stops() {
    let fake = Arc::new(FakeTransport::new(ok_script()));
    let client = serve_duplex(build_router(one_node_deps(seams(fake.clone())))).await;
    let (status, json, text) = answer(&client, "{\"model\":\"nope\",\"messages\":[]}").await;
    assert_stop(status, &json, &text, 404, "model_not_found");
    assert!(fake.requests().is_empty());
}

#[tokio::test(start_paused = true)]
async fn tc08_no_model_gets_400_model_missing() {
    let fake = Arc::new(FakeTransport::new(ok_script()));
    let client = serve_duplex(build_router(one_node_deps(seams(fake.clone())))).await;
    let (status, json, text) = answer(&client, "{\"messages\":[]}").await;
    assert_stop(status, &json, &text, 400, "model_missing");
    assert!(fake.requests().is_empty());
}

#[tokio::test(start_paused = true)]
async fn tc09_a_numeric_model_gets_400_in_the_protocol_shape() {
    let fake = Arc::new(FakeTransport::new(ok_script()));
    let client = serve_duplex(build_router(one_node_deps(seams(fake.clone())))).await;
    let (status, json, text) = answer(&client, "{\"model\":42}").await;
    assert_stop(status, &json, &text, 400, "invalid_body");
    assert!(fake.requests().is_empty());
}

#[tokio::test(start_paused = true)]
async fn tc10_tc11_invalid_json_and_a_json_array_get_400_invalid_body_and_are_not_forwarded() {
    let fake = Arc::new(FakeTransport::new(ok_script()));
    let client = serve_duplex(build_router(one_node_deps(seams(fake.clone())))).await;
    for body in ["{\"model\":\"local-coder\"", "[{\"model\":\"local-coder\"}]", "", "plain text"] {
        let (status, json, text) = answer(&client, body).await;
        assert_stop(status, &json, &text, 400, "invalid_body");
    }
    assert!(fake.requests().is_empty());
}

#[tokio::test(start_paused = true)]
async fn tc12_an_empty_messages_list_passes_to_the_node_unchanged() {
    let fake = Arc::new(FakeTransport::new(ok_script()));
    let client = serve_duplex(build_router(one_node_deps(seams(fake.clone())))).await;
    let response = client.send(post("{\"model\":\"local-coder\",\"messages\":[]}")).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(fake.requests().len(), 1);
    assert_eq!(fake.requests()[0].body, Bytes::from("{\"model\":\"qwen-node\",\"messages\":[]}"));
}

#[tokio::test(start_paused = true)]
async fn tc15_an_alias_with_no_chat_node_gets_404_protocol_not_served() {
    let text = "version: 1\nnodes:\n  m:\n    engine: { name: anthropic-hosted }\n    model: claude-x\n    endpoints: [ { protocol: anthropic-messages, base_url: \"https://api.anthropic.com\" } ]\naliases:\n  frontier: { nodes: [m] }\n";
    let fake = Arc::new(FakeTransport::new(ok_script()));
    let client = serve_duplex(build_router(deps_from_text(seams(fake.clone()), text))).await;
    let (status, json, body) = answer(&client, "{\"model\":\"frontier\"}").await;
    assert_stop(status, &json, &body, 404, "protocol_not_served");
    assert!(fake.requests().is_empty());
}

#[tokio::test(start_paused = true)]
async fn tc20_a_canary_in_the_body_a_header_and_the_path_appears_in_no_refusal_and_no_record() {
    let sink = Arc::new(MemorySink::new(SINK_CAPACITY));
    let fake = Arc::new(FakeTransport::new(ok_script()));
    let mut s = seams(fake);
    s.log = sink.clone();
    let client = serve_duplex(build_router(one_node_deps(s))).await;
    let mut texts = Vec::new();
    for body in [format!("{{\"model\":\"{CANARY}\"}}"), format!("{{\"model\":7,\"x\":\"{CANARY}\"}}"), format!("not json {CANARY}"), format!("{{\"x\":\"{CANARY}\"}}")] {
        let request = Request::builder().method("POST").uri(CHAT_COMPLETIONS_PATH).header("authorization", format!("Bearer {CANARY}")).header("x-canary", CANARY).body(Body::from(body)).unwrap();
        let response = client.send(request).await;
        texts.push(String::from_utf8(response.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap());
    }
    let wrong_path = Request::builder().method("POST").uri(format!("/v1/{CANARY}")).body(Body::from("{}")).unwrap();
    texts.push(format!("{}", client.send(wrong_path).await.status()));
    for text in texts {
        assert!(!text.contains("sk-ant"), "{text}");
    }
    assert!(sink.take().is_empty());
}

#[tokio::test(start_paused = true)]
async fn tc22_a_node_that_does_not_answer_is_502_node_connect_failed_and_pi_retries() {
    let fake = Arc::new(FakeTransport::new(Script::Connect));
    let client = serve_duplex(build_router(one_node_deps(seams(fake)))).await;
    let (status, json, _) = answer(&client, FLEET_REQUEST_BODY).await;
    assert_eq!(status, 502);
    assert_eq!(json["error"]["code"], "node_connect_failed");
    assert_eq!(json["error"]["type"], "bad_gateway");
    assert_eq!(pi_reaction(status, json["error"]["message"].as_str().unwrap()), Reaction::Retry);
}

#[tokio::test(start_paused = true)]
async fn tc22_the_starting_refusal_is_503_unavailable_and_pi_retries() {
    use legatus_proxy::lifecycle::start::{start_gate, GateState};
    let fake = Arc::new(FakeTransport::new(ok_script()));
    let (gate, hooks) = start_gate();
    gate.send(GateState::Failed).unwrap();
    let client = serve_duplex(build_router(deps_from_text(seams(fake), &one_node_registry_text("http://node.invalid")).with_hooks(Arc::new(hooks)))).await;
    let (status, json, text) = answer(&client, FLEET_REQUEST_BODY).await;
    assert_eq!((status, json["error"]["code"].as_str().unwrap(), json["error"]["type"].as_str().unwrap()), (503, "starting", "unavailable"));
    assert_eq!(pi_reaction(status, json["error"]["message"].as_str().unwrap()), Reaction::Retry, "{text}");
}
