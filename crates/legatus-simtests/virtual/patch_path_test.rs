//! Story 190 integration tests: patches in the request path with the real router, the model
//! rewrite, the registry loader and the reload hub, on paused time.
use super::reload_helpers::{harness, V1};
use bytes::Bytes;
use http::{Request, StatusCode};
use http_body_util::BodyExt;
use legatus_proxy::config::registry::ReloadResult;
use legatus_proxy::config::typed::{Registry, RegistryHandle};
use legatus_proxy::engine::patch::PatchChangeObserver;
use legatus_proxy::engine::AdapterRegistry;
use legatus_proxy::obs::log_sink::{DiscardSink, LogRecord};
use legatus_proxy::protocol::rewrite::peek_request;
use legatus_proxy::sim::SimPoints;
use legatus_proxy::time::WallTime;
use legatus_proxy::{build_router, RouterDeps, Seams, CHAT_COMPLETIONS_PATH};
use legatus_testkit::virt::{serve_duplex, FakeTransport, Script, SimWall};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

const PROMPT_MEMBERS: [&str; 3] = ["messages", "tools", "system"];

fn registry_with(patch_a: &str, patch_b: &str) -> String {
    format!(
        "version: 1\nsettings:\n  listen: 127.0.0.1:8080\nnodes:\n  a:\n    engine: {{ name: llama-server }}\n    model: model-a\n    endpoints: [ {{ protocol: openai-chat, base_url: \"http://a.invalid:1\" }} ]\n{patch_a}  b:\n    engine: {{ name: ollama }}\n    model: model-b\n    endpoints: [ {{ protocol: openai-chat, base_url: \"http://b.invalid:2\" }} ]\n{patch_b}aliases:\n  x: {{ nodes: [a] }}\n  y: {{ nodes: [b] }}\n"
    )
}

const PATCH_A: &str = "    patch: { set: { chat_template_kwargs: { enable_thinking: false }, max_tokens: 2048 } }\n";
const PATCH_B: &str = "    patch: { set: { reasoning_effort: low }, remove: [ temperature ] }\n";

fn deps(text: &str, fake: Arc<FakeTransport>) -> (RouterDeps, Arc<RegistryHandle>) {
    let handle = Arc::new(RegistryHandle::new(Arc::new(Registry::from_text(text).expect("valid registry"))));
    let seams = Seams { wall: Arc::new(SimWall::new(WallTime { unix_ms: 0 })), transport: fake, log: Arc::new(DiscardSink), sim: SimPoints::new() };
    (RouterDeps::for_test(seams).with_registry(handle.clone()).with_adapters(Arc::new(AdapterRegistry::standard())), handle)
}

fn fake(delay: Duration) -> Arc<FakeTransport> {
    Arc::new(FakeTransport::new(Script::Response { status: StatusCode::OK, chunks: vec![(delay, Ok(Bytes::from_static(b"{}")))] }))
}

fn post(body: &str) -> Request<axum::body::Body> {
    Request::builder().method("POST").uri(CHAT_COMPLETIONS_PATH).header("host", "proxy").header("content-type", "application/json").body(axum::body::Body::from(body.to_string())).unwrap()
}

fn body_for(alias: &str, turn: usize, extra: &str) -> String {
    format!("{{\"model\":\"{alias}\",{extra}\"tools\":[{{\"type\":\"function\",\"function\":{{\"name\":\"read\",\"parameters\":{{\"b\":1.0,\"a\":1e3}}}}}}],\"system\":\"be brief  \",\"messages\":[{{\"role\":\"user\",\"content\":\"turn {turn} caf\\u00e9\"}}],\"temperature\":0.9}}")
}

fn spans(body: &Bytes, key: &str) -> Vec<u8> {
    let peek = peek_request(body).unwrap();
    let (s, e) = peek.value_spans(key)[0];
    body[s..e].to_vec()
}

async fn send(client: &legatus_testkit::virt::DuplexClient, body: &str) -> StatusCode {
    let response = client.send(post(body)).await;
    let status = response.status();
    let _ = response.into_body().collect().await;
    status
}

#[tokio::test(start_paused = true)]
async fn tc08_and_tc13_the_node_gets_its_own_model_name_the_patch_keys_and_equal_prompt_bytes_stream_or_not() {
    let f = fake(Duration::from_millis(1));
    let (deps, _) = deps(&registry_with(PATCH_A, PATCH_B), f.clone());
    let client = serve_duplex(build_router(deps)).await;
    for stream in ["\"stream\":true,", "\"stream\":false,", ""] {
        let sent = body_for("x", 1, stream);
        assert_eq!(send(&client, &sent).await, StatusCode::OK);
        let got = f.requests().pop().unwrap().body;
        let value: Value = serde_json::from_slice(&got).unwrap();
        assert_eq!(value["model"], "model-a", "the model rewrite ran");
        assert_eq!(value["chat_template_kwargs"]["enable_thinking"], false);
        assert_eq!(value["max_tokens"], 2048);
        let before = Bytes::from(sent);
        for key in PROMPT_MEMBERS {
            assert_eq!(spans(&got, key), spans(&before, key), "{key}");
        }
        if stream.contains("true") {
            assert_eq!(value["stream"], true);
        }
    }
}

#[tokio::test(start_paused = true)]
async fn tc10_three_conversations_of_five_turns_get_identical_patched_members_and_a_clashing_harness_value_is_replaced_every_turn() {
    let f = fake(Duration::from_millis(1));
    let (deps, _) = deps(&registry_with(PATCH_A, ""), f.clone());
    let client = serve_duplex(build_router(deps)).await;
    for conversation in 0..3 {
        for turn in 0..5 {
            let clash = "\"chat_template_kwargs\":{\"enable_thinking\":true,\"harness_own\":1},\"max_tokens\":7,";
            assert_eq!(send(&client, &body_for("x", conversation * 10 + turn, clash)).await, StatusCode::OK);
        }
    }
    let seen = f.requests();
    assert_eq!(seen.len(), 15);
    for r in &seen {
        let v: Value = serde_json::from_slice(&r.body).unwrap();
        assert_eq!(v["chat_template_kwargs"], serde_json::json!({"enable_thinking": false, "harness_own": 1}));
        assert_eq!(v["max_tokens"], 2048, "the patch replaces the smaller harness value");
    }
}

#[tokio::test(start_paused = true)]
async fn tc11_two_nodes_keep_their_own_patches_and_a_conversation_that_moves_carries_no_residue() {
    let f = fake(Duration::from_millis(1));
    let (deps, _) = deps(&registry_with(PATCH_A, PATCH_B), f.clone());
    let client = serve_duplex(build_router(deps)).await;
    for alias in ["x", "y", "x", "y"] {
        assert_eq!(send(&client, &body_for(alias, 1, "")).await, StatusCode::OK);
    }
    for (i, r) in f.requests().iter().enumerate() {
        let v: Value = serde_json::from_slice(&r.body).unwrap();
        if i % 2 == 0 {
            assert!(v.get("chat_template_kwargs").is_some() && v.get("reasoning_effort").is_none(), "request {i} to A: {v}");
            assert!(v.get("temperature").is_some(), "A does not remove temperature");
        } else {
            assert!(v.get("reasoning_effort").is_some() && v.get("chat_template_kwargs").is_none() && v.get("max_tokens").is_none(), "request {i} to B: {v}");
            assert!(v.get("temperature").is_none(), "B removes temperature");
        }
    }
}

#[tokio::test(start_paused = true)]
async fn tc12_a_patch_changed_by_reload_applies_next_an_in_flight_request_keeps_the_old_one_and_one_event_names_the_node() {
    let first = registry_with(PATCH_A, "");
    let h = harness(&first);
    h.hub.register(Arc::new(PatchChangeObserver::new(h.sink.clone())));
    let f = fake(Duration::from_secs(5));
    let seams = Seams { wall: Arc::new(SimWall::new(WallTime { unix_ms: 0 })), transport: f.clone(), log: Arc::new(DiscardSink), sim: SimPoints::new() };
    let router_deps = RouterDeps::for_test(seams).with_registry(h.handle.clone()).with_adapters(Arc::new(AdapterRegistry::standard()));
    let client = Arc::new(serve_duplex(build_router(router_deps)).await);
    let in_flight = {
        let client = client.clone();
        tokio::spawn(async move { send(&client, &body_for("x", 1, "")).await })
    };
    tokio::time::sleep(Duration::from_millis(100)).await;
    h.source.set_text(&registry_with("    patch: { set: { max_tokens: 64 } }\n", ""));
    assert!(matches!(h.reloader.reload_once().await, ReloadResult::Loaded { .. }));
    assert_eq!(in_flight.await.unwrap(), StatusCode::OK);
    assert_eq!(send(&client, &body_for("x", 2, "")).await, StatusCode::OK);
    let seen = f.requests();
    let old: Value = serde_json::from_slice(&seen[0].body).unwrap();
    let new: Value = serde_json::from_slice(&seen[1].body).unwrap();
    assert_eq!((old["max_tokens"].as_i64(), old.get("chat_template_kwargs").is_some()), (Some(2048), true), "in flight keeps the old patch");
    assert_eq!((new["max_tokens"].as_i64(), new.get("chat_template_kwargs").is_some()), (Some(64), false), "the next request has the new one");
    let events: Vec<_> = h.sink.take().into_iter().filter_map(|r| match r {
        LogRecord::System(s) => s.patch_changed,
        _ => None,
    }).collect();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, "a");
    // A reload that changes nothing about the patch writes no event.
    h.source.set_text(&registry_with("    patch: { set: { max_tokens: 64 } }\n", "    # b changed in a way that is not its patch\n"));
    h.reloader.reload_once().await;
    assert!(h.sink.take().iter().all(|r| !matches!(r, LogRecord::System(s) if s.patch_changed.is_some())));
}

#[tokio::test(start_paused = true)]
async fn tc18_a_registry_with_a_bad_patch_loads_nothing_lists_every_mistake_and_a_reload_keeps_the_old_table() {
    let bad = registry_with("    patch: { set: { messages: 1, top_p: 1 }, remove: [ top_p, model ] }\n", "");
    let errors = Registry::from_text(&bad).err().expect("must not load");
    let lines: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
    assert!(lines.iter().any(|l| l.contains("bad_patch at nodes.a.patch.set.messages")), "{lines:?}");
    assert!(lines.iter().any(|l| l.contains("bad_patch at nodes.a.patch.remove.model")), "{lines:?}");
    assert!(lines.iter().any(|l| l.contains("bad_patch at nodes.a.patch.top_p")), "{lines:?}");
    assert!(lines.iter().all(|l| !l.contains("CANARY")));
    let sglang = "version: 1\nsettings:\n  listen: 127.0.0.1:8080\nnodes:\n  s:\n    engine: { name: sglang }\n    model: m\n    endpoints: [ { protocol: openai-chat, base_url: \"http://s.invalid\" } ]\n    patch: { set: { session_id: abc } }\naliases:\n  x: { nodes: [s] }\n";
    assert!(Registry::from_text(sglang).err().unwrap().iter().any(|e| e.to_string().contains("bad_patch at nodes.s.patch.set.session_id")));
    // The same patch on a llama-server node loads.
    assert!(Registry::from_text(&sglang.replace("sglang", "llama-server")).is_ok());
    let h = harness(V1);
    h.source.set_text(&bad);
    assert!(matches!(h.reloader.reload_once().await, ReloadResult::Rejected { .. }));
    assert_eq!(h.handle.snapshot().generation, 1, "the old table stays");
    // A shape that is not a map is named too.
    let shape = registry_with("    patch: { set: [ 1 ] }\n", "");
    assert!(Registry::from_text(&shape).err().unwrap().iter().any(|e| e.to_string().contains("bad_patch at nodes.a.patch")));
}
