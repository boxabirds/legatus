//! TC-15: the router on a multi-thread runtime over real sockets; chunk lateness stays in a band.
use axum::body::Body;
use http::{Request, StatusCode};
use http_body_util::BodyExt;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use legatus_proxy::obs::log_sink::DiscardSink;
use legatus_proxy::sim::SimPoints;
use legatus_proxy::time::WallTime;
use legatus_proxy::{build_router, Seams, CHAT_COMPLETIONS_PATH};
use legatus_testkit::virt::{FakeTransport, Script, SimWall};
use std::sync::Arc;
use std::time::{Duration, Instant};

const CHUNK_GAP: Duration = Duration::from_millis(100);
const CHUNKS: usize = 3;
/// PROPOSED band (spike s2 saw up to 11 ms): real scheduling may delay a chunk by at most this much.
const LATENESS_BAND: Duration = Duration::from_millis(40);

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tc15_chunks_over_real_sockets_arrive_within_the_lateness_band() {
    let seams = Seams {
        wall: Arc::new(SimWall::new(WallTime { unix_ms: 0 })),
        transport: Arc::new(FakeTransport::new(Script::chunks(CHUNKS, CHUNK_GAP))),
        log: Arc::new(DiscardSink),
        sim: SimPoints::new(),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, build_router(seams)).await.unwrap();
    });

    let client: Client<_, Body> = Client::builder(TokioExecutor::new()).build_http();
    let request = Request::builder()
        .method("POST")
        .uri(format!("http://{addr}{CHAT_COMPLETIONS_PATH}"))
        .body(Body::from("{}"))
        .unwrap();
    let started = Instant::now();
    let response = client.request(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut body = response.into_body();
    let mut late = Vec::new();
    let mut index = 0u32;
    while let Some(frame) = body.frame().await {
        if frame.unwrap().into_data().is_ok() {
            index += 1;
            let due = CHUNK_GAP * index;
            let at = started.elapsed();
            late.push(at.saturating_sub(due));
            assert!(at >= due, "chunk {index} arrived early: {at:?} < {due:?}");
        }
    }
    assert_eq!(index as usize, CHUNKS);
    for l in &late {
        assert!(*l <= LATENESS_BAND, "lateness {l:?} beyond {LATENESS_BAND:?}: {late:?}");
    }
    eprintln!("lateness per chunk: {late:?}");
}
