//! TC-07, TC-11, TC-12: the router over an in-memory connection.
use axum::body::Body;
use http::{Request, StatusCode};
use http_body_util::BodyExt;
use legatus_proxy::obs::log_sink::DiscardSink;
use legatus_proxy::sim::SimPoints;
use legatus_proxy::time::{Instant, WallTime};
use legatus_proxy::{Seams, CHAT_COMPLETIONS_PATH};
use legatus_testkit::fleet::{one_node_deps, FLEET_ALIAS, FLEET_NODE_MODEL, FLEET_REQUEST_BODY};

/// The router of the one-node fleet over the given seams.
fn build_router(seams: Seams) -> axum::Router {
    legatus_proxy::build_router(one_node_deps(seams))
}
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
        .body(Body::from(FLEET_REQUEST_BODY))
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

#[tokio::test(start_paused = true)]
async fn tc11_chunks_arrive_at_exact_virtual_instants() {
    let chunk = |delay_ms: u64, text: &'static str| (Duration::from_millis(delay_ms), Ok(bytes::Bytes::from_static(text.as_bytes())));
    let chunks = vec![chunk(7000, "one;"), chunk(2000, "two;"), chunk(2000, "three;")];
    let fake = Arc::new(FakeTransport::new(Script::Response { status: StatusCode::OK, chunks }));
    let client = serve_duplex(build_router(seams(fake.clone()))).await;
    let response = client.send(post()).await;
    assert_eq!(response.status(), StatusCode::OK);
    let (instants, bytes) = read_chunks(response.into_body()).await;
    // Real HTTP parsing over the in-memory pipe; chunks at 7000, 9000 and 11000 ms virtual.
    assert_eq!(instants, vec![7000, 9000, 11000]);
    assert_eq!(bytes, b"one;two;three;");
    // The node receives the body with only the model replaced by the node's own model name.
    assert_eq!(fake.requests()[0].body_len, FLEET_REQUEST_BODY.replace(FLEET_ALIAS, FLEET_NODE_MODEL).len());
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

const BIG_CHUNK_BYTES: usize = 256 * 1024;
const SLOW_READ: Duration = Duration::from_secs(1);

#[tokio::test(start_paused = true)]
async fn tc07_slow_reader_slows_the_node_through_the_router() {
    let chunks = (0..6).map(|_| (Duration::from_millis(10), Ok(bytes::Bytes::from(vec![b'x'; BIG_CHUNK_BYTES])))).collect();
    let fake = Arc::new(FakeTransport::new(Script::Response { status: StatusCode::OK, chunks }));
    let client = serve_duplex(build_router(seams(fake.clone()))).await;
    let started = Instant::now();
    let response = client.send(post()).await;
    let mut body = response.into_body();
    while let Some(frame) = body.frame().await {
        if frame.unwrap().into_data().is_ok() {
            tokio::time::sleep(SLOW_READ).await;
        }
    }
    let produced: Vec<u128> = fake.produced_at().iter().map(|i| (*i - started).as_millis()).collect();
    eprintln!("slow reader produced at: {produced:?}");
    assert_eq!(produced.len(), 6);
    // Once the pipe is full the node is held back: the last chunk is produced only after the reader made room.
    assert!(produced[5] >= 2 * SLOW_READ.as_millis(), "node was not slowed: {produced:?}");
    assert!(produced[0] <= 10, "the first chunk is not held: {produced:?}");
}
