//! TC-06 of story 145 over real sockets: a reader that takes a chunk at a time slows the node. The
//! transport window here is the kernel socket buffers, so the bound is a constant in megabytes,
//! never an exact order.
use bytes::Bytes;
use http::StatusCode;
use legatus_proxy::obs::log_sink::DiscardSink;
use legatus_proxy::sim::SimPoints;
use legatus_proxy::time::WallTime;
use legatus_proxy::Seams;
use legatus_testkit::fleet::{one_node_deps, FLEET_REQUEST_BODY};
use legatus_testkit::virt::{FakeTransport, Script, SimWall};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const CHUNK_BYTES: usize = 1024 * 1024;
const CHUNKS: usize = 48;
/// What the kernel buffers and the HTTP layer may hold between the node and a stopped reader.
const WINDOW_BYTES: usize = 16 * 1024 * 1024;
const PAUSE: Duration = Duration::from_millis(15);

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tc06_a_slow_reader_over_a_real_socket_keeps_the_node_within_the_transport_window() {
    let chunks: Vec<_> = (0..CHUNKS).map(|_| (Duration::ZERO, Ok(Bytes::from(vec![b'x'; CHUNK_BYTES])))).collect();
    let fake = Arc::new(FakeTransport::new(Script::Response { status: StatusCode::OK, chunks }));
    let seams = Seams { wall: Arc::new(SimWall::new(WallTime { unix_ms: 0 })), transport: fake.clone(), log: Arc::new(DiscardSink), sim: SimPoints::new() };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = legatus_proxy::build_router(one_node_deps(seams));
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!("POST /v1/chat/completions HTTP/1.1\r\nHost: proxy\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{FLEET_REQUEST_BODY}", FLEET_REQUEST_BODY.len());
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut buf = vec![0u8; CHUNK_BYTES];
    let mut read = 0usize;
    let mut worst = 0usize;
    let mut checked_early = false;
    loop {
        let n = stream.read(&mut buf).await.unwrap();
        if n == 0 {
            break;
        }
        read += n;
        tokio::time::sleep(PAUSE).await;
        let produced = fake.produced_at().len() * CHUNK_BYTES;
        worst = worst.max(produced.saturating_sub(read));
        if !checked_early && read >= 2 * CHUNK_BYTES {
            checked_early = true;
            assert!(fake.produced_at().len() < CHUNKS, "the node did not run to the end while the reader had taken only {read} bytes");
        }
    }
    assert!(read >= CHUNKS * CHUNK_BYTES, "the whole body arrived: {read}");
    assert!(worst <= WINDOW_BYTES, "the node ran {worst} bytes ahead of the reader");
    assert!(checked_early);
}
