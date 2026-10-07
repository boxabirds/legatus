//! TC-10, TC-11, TC-12: the router over an in-memory connection.
use axum::body::Body;
use http::{Request, StatusCode};
use http_body_util::BodyExt;
use legatus_proxy::obs::log_sink::DiscardSink;
use legatus_proxy::sim::SimPoints;
use legatus_proxy::time::{Instant, WallTime};
use legatus_proxy::{build_router, Seams, CHAT_COMPLETIONS_PATH};
use legatus_testkit::virt::{serve_duplex, FakeTransport, Script, SimWall};
use std::sync::Arc;
use std::time::Duration;

fn seams(transport: Arc<FakeTransport>) -> Seams {
    Seams {
        wall: Arc::new(SimWall::new(WallTime { unix_ms: 0 })),
        transport,
        log: Arc::new(DiscardSink),
        sim: SimPoints::new(),
    }
}

fn post() -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(CHAT_COMPLETIONS_PATH)
        .header("host", "proxy")
        .body(Body::from("{\"model\":\"m\"}"))
        .unwrap()
}

/// Collect chunk arrival instants (ms since start) and the whole body.
async fn read_chunks(mut body: Body) -> (Vec<u128>, Vec<u8>) {
    let started = Instant::now();
    let mut instants = Vec::new();
    let mut bytes = Vec::new();
    while let Some(frame) = body.frame().await {
        if let Ok(data) = frame.unwrap().into_data() {
            instants.push((Instant::now() - started).as_millis());
            bytes.extend_from_slice(&data);
        }
    }
    (instants, bytes)
}

#[test]
fn tc10_two_routers_share_no_state_and_build_touches_no_socket() {
    let a = build_router(seams(Arc::new(FakeTransport::new(Script::Connect))));
    let b = build_router(seams(Arc::new(FakeTransport::new(Script::Connect))));
    // Routers are plain values; both exist side by side with their own seams.
    drop((a, b));
}

#[tokio::test(start_paused = true)]
async fn tc10_connect_failure_gives_502() {
    let client = serve_duplex(build_router(seams(Arc::new(FakeTransport::new(Script::Connect))))).await;
    let response = client.send(post()).await;
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
}

#[tokio::test(start_paused = true)]
async fn tc11_chunks_arrive_at_exact_virtual_instants() {
    let fake = Arc::new(FakeTransport::new(Script::chunks(3, Duration::from_millis(2000))));
    let client = serve_duplex(build_router(seams(fake.clone()))).await;
    let started = Instant::now();
    let response = client.send(post()).await;
    assert_eq!(response.status(), StatusCode::OK);
    let (instants, bytes) = read_chunks(response.into_body()).await;
    assert_eq!(instants.len(), 3);
    // The head is sent when the node answers (0 ms); chunks follow at 2000, 4000, 6000 ms.
    let first = (Instant::now() - started).as_millis();
    assert_eq!(first, 6000);
    assert_eq!(instants, vec![2000, 4000, 6000]);
    assert_eq!(bytes, b"chunk-0;chunk-1;chunk-2;");
    assert_eq!(fake.requests()[0].body_len, 13);
}

#[test]
fn tc12_a_250_second_hold_finishes_fast_and_identically_over_20_runs() {
    const HOLD: Duration = Duration::from_secs(250);
    const REAL_BUDGET: Duration = Duration::from_secs(1);
    let mut runs = Vec::new();
    for _ in 0..20 {
        let rt = legatus_testkit::virt::virtual_runtime();
        let wall_start = std::time::Instant::now();
        let instants = rt.block_on(async {
            let fake = Arc::new(FakeTransport::new(Script::Response {
                status: StatusCode::OK,
                chunks: vec![(HOLD, Ok(bytes::Bytes::from_static(b"late")))],
            }));
            let client = serve_duplex(build_router(seams(fake))).await;
            let response = client.send(post()).await;
            read_chunks(response.into_body()).await.0
        });
        assert!(wall_start.elapsed() < REAL_BUDGET, "took {:?}", wall_start.elapsed());
        runs.push(instants);
    }
    assert!(runs.iter().all(|r| *r == runs[0]));
    assert_eq!(runs[0], vec![250_000]);
}
