//! The one executable of the proxy, named `legatus`.
//! Story 121 adds `main_entry` with registry loading; until then the binary serves the skeleton route.
#![deny(clippy::disallowed_types, clippy::disallowed_methods)]
use legatus_proxy::net::hyper_transport::HyperTransport;
use legatus_proxy::net::wall_system::SystemWallClock;
use legatus_proxy::obs::log_sink::DiscardSink;
use legatus_proxy::sim::SimPoints;
use legatus_proxy::{build_router, Seams};
use std::sync::Arc;

/// Default listen address until the registry (story 125) supplies one.
const DEFAULT_LISTEN: &str = "127.0.0.1:8080";
const LISTEN_ENV: &str = "LEGATUS_LISTEN";

#[tokio::main]
async fn main() {
    let listen = std::env::var(LISTEN_ENV).unwrap_or_else(|_| DEFAULT_LISTEN.to_string());
    let seams = Seams {
        wall: Arc::new(SystemWallClock),
        transport: Arc::new(HyperTransport::new()),
        log: Arc::new(DiscardSink),
        sim: SimPoints::new(),
    };
    let listener = match tokio::net::TcpListener::bind(&listen).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("cannot bind {listen}: {e}");
            std::process::exit(3);
        }
    };
    eprintln!("legatus listening on {listen}");
    if let Err(e) = axum::serve(listener, build_router(seams)).await {
        eprintln!("server error: {e}");
        std::process::exit(1);
    }
}
