//! Serve a router over `tokio::io::duplex`: real HTTP parsing, no socket.
use axum::body::Body;
use axum::Router;
use http::{Request, Response};
use hyper::client::conn::http1::SendRequest;
use hyper_util::rt::TokioIo;
use hyper_util::service::TowerToHyperService;
use tokio::sync::Mutex;

/// In-memory pipe buffer between client and server.
const DUPLEX_BUFFER_BYTES: usize = 64 * 1024;

pub struct DuplexClient {
    sender: Mutex<SendRequest<Body>>,
}

impl DuplexClient {
    pub async fn send(&self, req: Request<Body>) -> Response<Body> {
        let mut sender = self.sender.lock().await;
        let response = sender.send_request(req).await.expect("send over the duplex connection");
        response.map(Body::new)
    }
}

pub async fn serve_duplex(router: Router) -> DuplexClient {
    let (client_io, server_io) = tokio::io::duplex(DUPLEX_BUFFER_BYTES);
    tokio::spawn(async move {
        let service = TowerToHyperService::new(router);
        let _ = hyper::server::conn::http1::Builder::new()
            .serve_connection(TokioIo::new(server_io), service)
            .await;
    });
    let (sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(client_io))
        .await
        .expect("handshake over the duplex connection");
    tokio::spawn(async move {
        let _ = connection.await;
    });
    DuplexClient { sender: Mutex::new(sender) }
}
