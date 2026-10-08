//! Story 183 integration tests with the mlx_lm stub of story 162 and scripted replies for the
//! engines that have no stub (vLLM, SGLang, gufo: their shapes are ASSUMPTIONS until captured).
use super::stub_helpers::*;
use bytes::Bytes;
use futures_util::StreamExt;
use http::{Request, StatusCode};
use http_body_util::BodyExt;
use legatus_common::engine::*;
use legatus_common::protocol::Protocol;
use legatus_proxy::config::typed::{Registry, RegistryHandle};
use legatus_proxy::engine::{standard_adapters, AdapterRegistry, EngineAdapter};
use legatus_proxy::obs::log_sink::DiscardSink;
use legatus_proxy::sim::SimPoints;
use legatus_proxy::time::WallTime;
use legatus_proxy::upstream::transport::{UpstreamRequest, UpstreamTransport};
use legatus_proxy::{build_router, RouterDeps, Seams, CHAT_COMPLETIONS_PATH};
use legatus_testkit::stubs::mlx::BatchingMlxStub;
use legatus_testkit::stubs::scenario::{StubKind, StubSpec};
use legatus_testkit::virt::{serve_duplex, FakeTransport, Script, SimWall};
use std::sync::Arc;
use std::time::Duration;

const ALIAS: &str = "local-coder";
const DETAILS: ReuseFieldName = ReuseFieldName::PromptTokensDetailsCachedTokens;

fn registry(engine: &str, extra: &str) -> Registry {
    let text = format!(
        "version: 1\nnodes:\n  n1:\n    engine: {{ name: {engine} }}\n    model: m\n    endpoints: [ {{ protocol: openai-chat, base_url: \"http://node.invalid\" }} ]\n{extra}aliases:\n  {ALIAS}: {{ nodes: [n1] }}\n"
    );
    Registry::from_text(&text).expect("valid registry")
}

fn deps(engine: &str, transport: Arc<dyn UpstreamTransport>) -> RouterDeps {
    let seams = Seams { wall: Arc::new(SimWall::new(WallTime { unix_ms: 0 })), transport, log: Arc::new(DiscardSink), sim: SimPoints::new() };
    RouterDeps::for_test(seams).with_registry(Arc::new(RegistryHandle::new(Arc::new(registry(engine, ""))))).with_adapters(Arc::new(AdapterRegistry::standard()))
}

fn post(body: &str) -> Request<axum::body::Body> {
    Request::builder().method("POST").uri(CHAT_COMPLETIONS_PATH).header("host", "proxy").header("content-type", "application/json").body(axum::body::Body::from(body.to_string())).unwrap()
}

fn reply(body: String) -> Arc<FakeTransport> {
    Arc::new(FakeTransport::new(Script::Response { status: StatusCode::OK, chunks: vec![(Duration::from_millis(1), Ok(Bytes::from(body)))] }))
}

async fn answer_of(client: &legatus_testkit::virt::DuplexClient, body: &str) -> (StatusCode, String) {
    let response = client.send(post(body)).await;
    let status = response.status();
    (status, String::from_utf8(response.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap())
}

fn reading(adapter: &dyn EngineAdapter, text: &str, probe: ReuseProbeState) -> ReuseReading {
    adapter.reuse_fields(&UsageView { json_tail: text.as_bytes(), protocol: Protocol::OpenAiChat, stream: false }, probe)
}

fn mlx_spec() -> StubSpec {
    let mut spec = StubSpec::new("mlx", StubKind::BatchingMlx);
    spec.speed.load_time_ms = 0;
    spec
}

#[tokio::test(start_paused = true)]
async fn tc03_the_mlx_stub_sends_headers_at_once_while_it_is_still_prefilling() {
    let stub = BatchingMlxStub::new(mlx_spec());
    let body = "{\"messages\":[],\"prompt_tokens\":3000,\"output_tokens\":4}";
    let request = UpstreamRequest { method: http::Method::POST, uri: format!("http://node.invalid{CHAT_COMPLETIONS_PATH}").parse().unwrap(), headers: http::HeaderMap::new(), body: Bytes::from(body) };
    let started = tokio::time::Instant::now();
    let reply = stub.send(request).await.ok().unwrap();
    assert_eq!(started.elapsed(), Duration::ZERO, "the head is there at once");
    let mut stream = reply.body;
    assert!(stream.next().await.is_some());
    assert!(started.elapsed() > Duration::from_millis(100), "the first token comes after the prefill: {:?}", started.elapsed());
    // The part of TC-03 that holds the second request back at cap 1 is the admission counter
    // (story 124), which does not exist yet; the adapter fact it will read is checked in TC-01.
}

#[tokio::test(start_paused = true)]
async fn tc06_a_dropped_connection_on_an_invalid_request_closes_the_harness_connection_and_the_node_serves_the_next_valid_one() {
    let stub: Arc<dyn UpstreamTransport> = Arc::new(BatchingMlxStub::new(mlx_spec()));
    let recording = Recording::new(stub);
    let client = serve_duplex(build_router(deps("mlx_lm", recording.clone()))).await;
    let invalid = format!("{{\"model\":\"{ALIAS}\",\"prompt\":\"no messages\"}}");
    let (status, text) = answer_of(&client, &invalid).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{text}");
    assert!(text.contains("node_connect_failed"), "{text}");
    assert!(!standard_adapters().for_family(EngineFamily::MlxLm).drop_is_node_signal(), "a drop here is a fault of the request");
    let valid = format!("{{\"model\":\"{ALIAS}\",\"messages\":[],\"prompt_tokens\":10,\"output_tokens\":2}}");
    assert_eq!(answer_of(&client, &valid).await.0, StatusCode::OK, "the node is still served");
    assert_eq!(recording.paths().len(), 2);
    // Marking the node healthy or failed from this drop is story 182 and 175; not built yet.
}

#[tokio::test(start_paused = true)]
async fn tc08_the_mlx_stub_keeps_the_last_ten_conversations_and_readings_are_zero_or_unknown_never_invented() {
    let stub = Arc::new(BatchingMlxStub::new(mlx_spec()));
    for i in 0..12 {
        let body = format!("{{\"messages\":[],\"prompt_tokens\":10,\"output_tokens\":1,\"cache_key\":\"conv-{i}\"}}");
        let request = UpstreamRequest { method: http::Method::POST, uri: format!("http://node.invalid{CHAT_COMPLETIONS_PATH}").parse().unwrap(), headers: http::HeaderMap::new(), body: Bytes::from(body) };
        let reply = stub.send(request).await.ok().unwrap();
        let mut stream = reply.body;
        while stream.next().await.is_some() {}
    }
    let keys = stub.cache_keys();
    assert_eq!(keys.len(), 10);
    assert_eq!((keys[0].as_str(), keys[9].as_str()), ("conv-2", "conv-11"), "the two oldest were evicted");
    // The stub does not report cached tokens, so the readings are scripted like the engine's.
    let a = standard_adapters().for_family(EngineFamily::MlxLm);
    assert_eq!(reading(a, "{\"usage\":{\"prompt_tokens_details\":{\"cached_tokens\":0}}}", ReuseProbeState::Unprobed), ReuseReading::Reused { cached_tokens: 0, field: DETAILS });
    assert_eq!(reading(a, "{\"usage\":{\"prompt_tokens\":10}}", ReuseProbeState::Unprobed), ReuseReading::Unknown(UnknownReason::FieldAbsent));
}

#[tokio::test(start_paused = true)]
async fn tc10_without_the_declaration_twenty_vllm_turns_all_read_unknown_not_yet_probed() {
    let answer = "{\"usage\":{\"prompt_tokens\":900,\"completion_tokens\":4}}".to_string();
    let client = serve_duplex(build_router(deps("vllm", reply(answer.clone())))).await;
    let adapter = standard_adapters().for_family(EngineFamily::Vllm);
    for turn in 0..20 {
        let (status, text) = answer_of(&client, &format!("{{\"model\":\"{ALIAS}\",\"messages\":[{{\"role\":\"user\",\"content\":\"turn {turn}\"}}]}}")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(reading(adapter, &text, ReuseProbeState::Unprobed), ReuseReading::Unknown(UnknownReason::NotYetProbed), "turn {turn}");
    }
    // Neither a poor mark nor an affinity-mode event can exist here: the affinity code is story 194.
}

#[tokio::test(start_paused = true)]
async fn tc11_with_the_flag_on_twenty_turns_of_silent_zero_read_reused_zero() {
    let answer = "{\"usage\":{\"prompt_tokens\":900,\"prompt_tokens_details\":{\"cached_tokens\":0},\"completion_tokens\":4}}".to_string();
    let client = serve_duplex(build_router(deps("vllm", reply(answer)))).await;
    let adapter = standard_adapters().for_family(EngineFamily::Vllm);
    let probe = legatus_proxy::engine::advisory::reuse_state_for_node(Some(true), ReuseProbeState::Unprobed);
    for turn in 0..20 {
        let (_, text) = answer_of(&client, &format!("{{\"model\":\"{ALIAS}\",\"messages\":[{{\"role\":\"user\",\"content\":\"turn {turn}\"}}]}}")).await;
        assert_eq!(reading(adapter, &text, probe), ReuseReading::Reused { cached_tokens: 0, field: DETAILS }, "turn {turn}");
    }
}

#[tokio::test(start_paused = true)]
async fn tc16_five_requests_to_an_sglang_node_arrive_byte_equal_with_no_session_field_added() {
    let fake = reply("{}".to_string());
    let client = serve_duplex(build_router(deps("sglang", fake.clone()))).await;
    let mut sent = Vec::new();
    for i in 0..5 {
        let body = format!("{{\"model\":\"{ALIAS}\",\"messages\":[{{\"role\":\"user\",\"content\":\"hello {i}\"}}],\"stream\":false}}");
        assert_eq!(answer_of(&client, &body).await.0, StatusCode::OK);
        sent.push(body);
    }
    let own = format!("{{\"model\":\"{ALIAS}\",\"session_id\":\"harness-own\",\"messages\":[]}}");
    assert_eq!(answer_of(&client, &own).await.0, StatusCode::OK);
    let seen = fake.requests();
    assert_eq!(seen.len(), 6);
    for (i, body) in sent.iter().enumerate() {
        assert_eq!(seen[i].body, Bytes::from(body.replace(ALIAS, "m")), "only the model name changed");
        assert!(!String::from_utf8_lossy(&seen[i].body).contains("session_id"));
        assert!(seen[i].headers.keys().all(|k| !k.as_str().contains("session")), "no session header either: {:?}", seen[i].headers.keys().collect::<Vec<_>>());
    }
    assert_eq!(seen[5].body, Bytes::from(own.replace(ALIAS, "m")), "a session_id the harness sent passes unchanged");
}

/// A saw-tooth like the one the notes describe: reuse is the prompt rounded down to the grid.
const GRID: u32 = 256;

fn single_readings(turns: u32) -> Vec<u32> {
    (0..turns).map(|t| ((600 + t * 97) / GRID) * GRID).collect()
}

#[test]
fn tc18_single_sglang_readings_saw_tooth_and_a_window_of_fifty_is_steadier_per_request() {
    assert!(legatus_proxy::engine::sglang::SglangAdapter.reuse_noisy_per_request());
    let readings = single_readings(400);
    let jumps = readings.windows(2).filter(|w| w[0] != w[1]).count();
    assert!(jumps > 20, "single readings step: {jumps}");
    let single_swing = readings.windows(2).map(|w| w[0].abs_diff(w[1])).max().unwrap();
    assert_eq!(single_swing, GRID, "one step is a whole grid cell");
    let window = 50usize;
    let means: Vec<f64> = readings.windows(window).map(|w| f64::from(w.iter().sum::<u32>()) / window as f64).collect();
    let window_swing = means.windows(2).map(|m| (m[1] - m[0]).abs()).fold(0.0, f64::max);
    assert!(window_swing < f64::from(single_swing), "window {window_swing} against single {single_swing}");
    // The live window is story 194; this checks the property it relies on.
}

#[tokio::test(start_paused = true)]
async fn tc19_a_gufo_reply_after_an_edited_middle_message_reads_exactly_the_engines_cache_n() {
    let answer = "data: {\"choices\":[],\"timings\":{\"cache_n\":40,\"prompt_n\":260}}\n\ndata: [DONE]\n\n".to_string();
    let client = serve_duplex(build_router(deps("gufo", reply(answer)))).await;
    let adapter = standard_adapters().for_family(EngineFamily::Gufo);
    let edited = format!("{{\"model\":\"{ALIAS}\",\"stream\":true,\"messages\":[{{\"role\":\"user\",\"content\":\"a\"}},{{\"role\":\"assistant\",\"content\":\"EDITED\"}},{{\"role\":\"user\",\"content\":\"c\"}}]}}");
    let (status, text) = answer_of(&client, &edited).await;
    assert_eq!(status, StatusCode::OK);
    let last = text.lines().rev().find(|l| l.starts_with("data: {")).unwrap();
    assert_eq!(reading(adapter, last, ReuseProbeState::Unprobed), ReuseReading::Reused { cached_tokens: 40, field: ReuseFieldName::TimingsCacheN });
}
