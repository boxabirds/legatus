//! TC-13 and TC-14 of story 160 over real sockets: the body limit with a declared length, with a
//! chunked body, at the limit, one byte over, and a 2 MB first message at the default limit.
use bytes::Bytes;
use http::StatusCode;
use legatus_proxy::obs::log_sink::DiscardSink;
use legatus_proxy::sim::SimPoints;
use legatus_proxy::time::WallTime;
use legatus_proxy::Seams;
use legatus_testkit::fleet::{deps_from_text, one_node_registry_text};
use legatus_testkit::virt::{FakeTransport, Script, SimWall};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const READ_LIMIT: Duration = Duration::from_secs(10);
const LIMIT: usize = 400;
const PREFIX: &str = "{\"model\":\"local-coder\",\"pad\":\"";
const SUFFIX: &str = "\"}";
const TWO_MB: usize = 2 * 1024 * 1024;

fn body_of_len(len: usize) -> String {
    let pad = len - PREFIX.len() - SUFFIX.len();
    format!("{PREFIX}{}{SUFFIX}", "p".repeat(pad))
}

async fn start(limit: Option<usize>) -> (std::net::SocketAddr, Arc<FakeTransport>) {
    let fake = Arc::new(FakeTransport::new(Script::Response { status: StatusCode::OK, chunks: vec![(Duration::ZERO, Ok(Bytes::from_static(b"{}")))] }));
    let seams = Seams { wall: Arc::new(SimWall::new(WallTime { unix_ms: 0 })), transport: fake.clone(), log: Arc::new(DiscardSink), sim: SimPoints::new() };
    let mut text = one_node_registry_text("http://node.invalid");
    if let Some(limit) = limit {
        text = text.replace("nodes:\n", &format!("settings:\n  listen: 127.0.0.1:1\n  body_limit_bytes: {limit}\nnodes:\n"));
    }
    let router = legatus_proxy::build_router(deps_from_text(seams, &text));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (addr, fake)
}

async fn exchange(addr: std::net::SocketAddr, head: &str, body: &[u8]) -> String {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream.write_all(head.as_bytes()).await.unwrap();
    // The server may answer 413 and close before the whole body is sent; a write error is fine then.
    let _ = stream.write_all(body).await;
    let mut out = Vec::new();
    let _ = tokio::time::timeout(READ_LIMIT, stream.read_to_end(&mut out)).await.expect("the connection closes");
    String::from_utf8_lossy(&out).to_string()
}

fn declared(len: usize) -> String {
    format!("POST /v1/chat/completions HTTP/1.1\r\nHost: proxy\r\nConnection: close\r\nContent-Length: {len}\r\n\r\n")
}

fn chunked_head() -> &'static str {
    "POST /v1/chat/completions HTTP/1.1\r\nHost: proxy\r\nConnection: close\r\nTransfer-Encoding: chunked\r\n\r\n"
}

fn as_chunks(body: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for part in body.as_bytes().chunks(100) {
        out.extend_from_slice(format!("{:x}\r\n", part.len()).as_bytes());
        out.extend_from_slice(part);
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(b"0\r\n\r\n");
    out
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tc14_a_body_exactly_at_the_limit_passes_and_one_byte_over_gets_413() {
    let (addr, fake) = start(Some(LIMIT)).await;
    let at = body_of_len(LIMIT);
    let reply = exchange(addr, &declared(at.len()), at.as_bytes()).await;
    assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
    assert_eq!(fake.requests().len(), 1);
    let over = body_of_len(LIMIT + 1);
    let reply = exchange(addr, &declared(over.len()), over.as_bytes()).await;
    assert!(reply.starts_with("HTTP/1.1 413"), "{reply}");
    assert!(reply.contains("\"code\":\"body_too_large\""), "{reply}");
    assert!(reply.to_lowercase().contains("connection: close"), "{reply}");
    assert_eq!(fake.requests().len(), 1, "the refused body reached no node");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tc13_a_chunked_body_over_the_limit_gets_413_and_at_the_limit_passes() {
    let (addr, fake) = start(Some(LIMIT)).await;
    let over = body_of_len(LIMIT + 1);
    let reply = exchange(addr, chunked_head(), &as_chunks(&over)).await;
    assert!(reply.starts_with("HTTP/1.1 413"), "{reply}");
    let at = body_of_len(LIMIT);
    let reply = exchange(addr, chunked_head(), &as_chunks(&at)).await;
    assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
    assert_eq!(fake.requests().len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tc13_a_declared_length_over_the_limit_is_refused_without_the_body_being_sent() {
    let (addr, fake) = start(Some(LIMIT)).await;
    let reply = exchange(addr, &declared(10 * 1024 * 1024), b"").await;
    assert!(reply.starts_with("HTTP/1.1 413"), "{reply}");
    assert!(fake.requests().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tc14_a_two_megabyte_first_message_passes_at_the_default_limit() {
    let (addr, fake) = start(None).await;
    let body = body_of_len(TWO_MB);
    let reply = exchange(addr, &declared(body.len()), body.as_bytes()).await;
    assert!(reply.starts_with("HTTP/1.1 200"), "{}", &reply[..reply.len().min(200)]);
    assert_eq!(fake.requests()[0].body_len, TWO_MB - "local-coder".len() + "qwen-node".len());
}
