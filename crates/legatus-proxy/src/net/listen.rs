//! The one place the proxy opens a listening socket (the virtual tier forbids sockets elsewhere).
use std::net::SocketAddr;

pub type Listener = tokio::net::TcpListener;

/// Bind the listen address. The error text is the operating system's.
pub async fn bind(addr: SocketAddr) -> std::io::Result<Listener> {
    Listener::bind(addr).await
}

/// Serve the router on the listener until `shutdown` completes.
pub async fn serve(listener: Listener, router: axum::Router, shutdown: impl std::future::Future<Output = ()> + Send + 'static) -> std::io::Result<()> {
    axum::serve(listener, router).with_graceful_shutdown(shutdown).await
}
