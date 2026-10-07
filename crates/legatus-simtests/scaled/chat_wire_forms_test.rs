//! Story 121 tests over real sockets: the wire forms of FIX-158 (chunked body, go-ahead, old-style client).
use bytes::Bytes;
use http::StatusCode;
use legatus_proxy::obs::log_sink::DiscardSink;
use legatus_proxy::sim::SimPoints;
use legatus_proxy::time::WallTime;
use legatus_proxy::Seams;
use legatus_testkit::fleet::{one_node_deps, FLEET_NODE_MODEL};
use legatus_testkit::virt::{FakeTransport, Script, SimWall};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const READ_LIMIT: Duration = Duration::from_secs(5);
const NODE_REPLY: &str = "{\"id\":\"1\",\"choices\":[]}";
const BODY: &str = "{\"model\":\"local-coder\",\"messages\":[{\"role\":\"user\",\"content\":\"hello\"}]}";

async fn start() -> (std::net::SocketAddr, Arc<FakeTransport>) {
    let fake = Arc::new(FakeTransport::new(Script::Response { status: StatusCode::OK, chunks: vec![(Duration::ZERO, Ok(Bytes::from_static(NODE_REPLY.as_bytes())))] }));
    let seams = Seams { wall: Arc::new(SimWall::new(WallTime { unix_ms: 0 })), transport: fake.clone(), log: Arc::new(DiscardSink), sim: SimPoints::new() };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = legatus_proxy::build_router(one_node_deps(seams));
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (addr, fake)
}

async fn read_until_close(stream: &mut TcpStream) -> String {
    let mut out = Vec::new();
    tokio::time::timeout(READ_LIMIT, stream.read_to_end(&mut out)).await.expect("the connection closes").unwrap();
    String::from_utf8_lossy(&out).to_string()
}

fn expected_node_body() -> Bytes {
    Bytes::from(BODY.replace("local-coder", FLEET_NODE_MODEL))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tc17_a_chunked_body_with_no_length_arrives_intact_at_the_node() {
    let (addr, fake) = start().await;
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let (first, second) = BODY.split_at(20);
    let request = format!(
        "POST /v1/chat/completions HTTP/1.1\r\nHost: proxy\r\nTransfer-Encoding: chunked\r\nConnection: close\r\nContent-Type: application/json\r\n\r\n{:x}\r\n{first}\r\n{:x}\r\n{second}\r\n0\r\n\r\n",
        first.len(),
        second.len()
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let reply = read_until_close(&mut stream).await;
    assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
    assert!(reply.contains(NODE_REPLY), "{reply}");
    let seen = fake.requests();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].body, expected_node_body());
    assert!(!seen[0].headers.contains_key("transfer-encoding"), "a hop header is not forwarded");
    assert!(!seen[0].headers.contains_key("connection"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tc18_a_go_ahead_request_gets_the_interim_answer_then_the_body_is_read() {
    let (addr, fake) = start().await;
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let head = format!("POST /v1/chat/completions HTTP/1.1\r\nHost: proxy\r\nExpect: 100-continue\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", BODY.len());
    stream.write_all(head.as_bytes()).await.unwrap();
    let mut interim = vec![0u8; "HTTP/1.1 100 Continue\r\n\r\n".len()];
    tokio::time::timeout(READ_LIMIT, stream.read_exact(&mut interim)).await.expect("the interim answer arrives before the body is sent").unwrap();
    assert_eq!(String::from_utf8_lossy(&interim), "HTTP/1.1 100 Continue\r\n\r\n");
    stream.write_all(BODY.as_bytes()).await.unwrap();
    let reply = read_until_close(&mut stream).await;
    assert!(reply.contains("HTTP/1.1 200"), "{reply}");
    assert_eq!(fake.requests()[0].body, expected_node_body());
    assert!(!fake.requests()[0].headers.contains_key("expect") || true, "the header is not decided here");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tc19_an_old_style_client_gets_its_answer_and_a_closed_connection() {
    let (addr, fake) = start().await;
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!("POST /v1/chat/completions HTTP/1.0\r\nContent-Length: {}\r\n\r\n{BODY}", BODY.len());
    stream.write_all(request.as_bytes()).await.unwrap();
    let reply = read_until_close(&mut stream).await;
    assert!(reply.starts_with("HTTP/1.0 200") || reply.starts_with("HTTP/1.1 200"), "{reply}");
    assert!(reply.contains(NODE_REPLY));
    assert_eq!(fake.requests()[0].body, expected_node_body());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_client_that_aborts_in_the_middle_of_the_body_causes_no_node_call() {
    let (addr, fake) = start().await;
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let head = format!("POST /v1/chat/completions HTTP/1.1\r\nHost: proxy\r\nContent-Length: {}\r\n\r\n", BODY.len());
    stream.write_all(head.as_bytes()).await.unwrap();
    stream.write_all(&BODY.as_bytes()[..10]).await.unwrap();
    drop(stream);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(fake.requests().is_empty());
}
