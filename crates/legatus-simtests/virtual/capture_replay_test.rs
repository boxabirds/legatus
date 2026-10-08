//! Story 120 tests: a replay through the router against the stub reply (virtual tier).
use bytes::Bytes;
use http::StatusCode;
use legatus_proxy::obs::log_sink::DiscardSink;
use legatus_proxy::sim::SimPoints;
use legatus_proxy::time::WallTime;
use legatus_proxy::{build_router, Seams, CHAT_COMPLETIONS_PATH};
use legatus_testkit::capture::{Capture, CapturedRequest};
use legatus_testkit::fleet::one_node_deps;
use legatus_testkit::harness::{replay, script_for, HarnessKind, ReplayError};
use legatus_testkit::virt::{FakeTransport, Script, SimWall};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

const NODE_REPLY: &str = "data: {\"model\":\"qwen-node\",\"choices\":[]}\n\ndata: [DONE]\n\n";

fn seams(transport: Arc<FakeTransport>) -> Seams {
    Seams { wall: Arc::new(SimWall::new(WallTime { unix_ms: 0 })), transport, log: Arc::new(DiscardSink), sim: SimPoints::new() }
}

fn ok_reply() -> Script {
    Script::Response { status: StatusCode::OK, chunks: vec![(Duration::from_millis(1), Ok(Bytes::from_static(NODE_REPLY.as_bytes())))] }
}

fn recorded(count: usize) -> Capture {
    let request = |i: usize| CapturedRequest {
        method: "POST".into(),
        path: CHAT_COMPLETIONS_PATH.into(),
        headers: vec![("content-type".into(), "application/json".into()), ("x-session-affinity".into(), "tok-session-abc".into())],
        body: json!({"model": "local-coder", "messages": [{"role": "user", "content": format!("filler {i}")}], "stream": true}),
        response_status: Some(200),
    };
    Capture::new("pi", "1.0.3", "t", (0..count).map(request).collect())
}

#[tokio::test(start_paused = true)]
async fn tc11_a_replay_through_the_router_equals_the_stub_reply_for_every_request() {
    let via = Arc::new(FakeTransport::new(ok_reply()));
    let direct = FakeTransport::new(ok_reply());
    let router = build_router(one_node_deps(seams(via.clone())));
    let report = replay(&recorded(3), &script_for(HarnessKind::Pi), router, &direct).await.unwrap();
    assert_eq!(report.requests.len(), 3);
    assert!(report.check().is_ok());
    assert!(report.requests.iter().all(|r| r.router_status == 200 && r.first_diff.is_none()));
    assert_eq!(via.requests().len(), 3);
}

#[tokio::test(start_paused = true)]
async fn tc11_a_difference_is_reported_with_the_request_index_and_the_first_offset_only() {
    let via = Arc::new(FakeTransport::new(ok_reply()));
    let other = Script::Response { status: StatusCode::OK, chunks: vec![(Duration::from_millis(1), Ok(Bytes::from_static(b"data: {\"model\":\"qwen-nodX\"")))] };
    let direct = FakeTransport::new(other);
    let router = build_router(one_node_deps(seams(via)));
    let report = replay(&recorded(1), &script_for(HarnessKind::Pi), router, &direct).await.unwrap();
    let error = report.check().unwrap_err();
    assert_eq!(error, ReplayError::Mismatch { index: 0, offset: Some(24) });
}
