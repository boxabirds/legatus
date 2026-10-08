//! Story 151 TC-15 (PRX-TEST-083) over real sockets: a node that closes an idle connection after
//! 5 s, and the proxy transport that reuses a pooled connection only when it was idle at most
//! 4 s. No request may fail on a connection the node already closed.
use bytes::Bytes;
use http::Method;
use http_body_util::Full;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::{TokioIo, TokioTimer};
use legatus_proxy::engine::llama_server::IdlePolicy;
use legatus_proxy::net::hyper_transport::HyperTransport;
use legatus_proxy::upstream::transport::{UpstreamRequest, UpstreamTransport};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// The node closes a connection that has been idle this long (spike S3: 5 s).
const NODE_IDLE_CLOSE: Duration = Duration::from_secs(5);

/// A node that counts the connections it accepts and closes idle ones after `NODE_IDLE_CLOSE`.
async fn counting_node() -> (SocketAddr, Arc<AtomicUsize>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let accepted = Arc::new(AtomicUsize::new(0));
    let count = accepted.clone();
    tokio::spawn(async move {
        loop {
            let Ok((socket, _)) = listener.accept().await else { continue };
            count.fetch_add(1, Ordering::SeqCst);
            tokio::spawn(async move {
                let service = service_fn(|_req: Request<hyper::body::Incoming>| async { Ok::<_, Infallible>(Response::new(Full::new(Bytes::from_static(b"{\"status\":\"ok\"}")))) });
                let _ = hyper::server::conn::http1::Builder::new().timer(TokioTimer::new()).header_read_timeout(NODE_IDLE_CLOSE).serve_connection(TokioIo::new(socket), service).await;
            });
        }
    });
    (addr, accepted)
}

fn get(addr: SocketAddr) -> UpstreamRequest {
    UpstreamRequest { method: Method::GET, uri: format!("http://{addr}/health").parse().unwrap(), headers: http::HeaderMap::new(), body: Bytes::new() }
}

async fn ok(transport: &HyperTransport, addr: SocketAddr) {
    let reply = transport.send(get(addr)).await.expect("no request fails on a closed idle connection");
    assert_eq!(reply.status, 200);
    use futures_util::StreamExt;
    let mut body = reply.body;
    while body.next().await.is_some() {}
}

/// Two requests with `idle` between them; the number of connections the node accepted.
async fn connections_after_idle(idle: Duration) -> usize {
    let (addr, accepted) = counting_node().await;
    let transport = HyperTransport::with_idle_timeout(IdlePolicy::default().max_idle);
    ok(&transport, addr).await;
    tokio::time::sleep(idle).await;
    ok(&transport, addr).await;
    accepted.load(Ordering::SeqCst)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tc15_a_connection_idle_for_3_seconds_is_reused() {
    assert_eq!(connections_after_idle(Duration::from_secs(3)).await, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tc15_a_connection_idle_for_4_5_seconds_is_not_reused_though_the_node_has_not_closed_it_yet() {
    assert_eq!(connections_after_idle(Duration::from_millis(4500)).await, 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tc15_a_connection_idle_for_6_seconds_gets_a_new_connection_and_the_request_succeeds() {
    assert_eq!(connections_after_idle(Duration::from_secs(6)).await, 2);
}

#[test]
fn the_transport_default_and_the_policy_are_one_value() {
    assert_eq!(HyperTransport::new().idle_timeout(), IdlePolicy::default().max_idle);
}
