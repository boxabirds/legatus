//! The one executable of the proxy, named `legatus`.
//! Story 121 adds `main_entry` with registry loading; until then the binary serves the skeleton route.
#![deny(clippy::disallowed_types, clippy::disallowed_methods)]
use legatus_proxy::config::validate::WarningSink;
use legatus_proxy::lifecycle::exit::{EXIT_BIND_FAILED, EXIT_REGISTRY_INVALID};
use legatus_proxy::lifecycle::start::{load_registry, AdminWarnings, LoadReporter};
use legatus_proxy::net::hyper_transport::HyperTransport;
use legatus_proxy::net::wall_system::SystemWallClock;
use legatus_proxy::obs::log_sink::DiscardSink;
use legatus_proxy::sim::SimPoints;
use legatus_proxy::{build_router, Seams};
use std::sync::{Arc, Mutex};

/// Default listen address until the registry (story 125) supplies one.
const DEFAULT_LISTEN: &str = "127.0.0.1:8080";
const LISTEN_ENV: &str = "LEGATUS_LISTEN";
const REGISTRY_FLAG: &str = "--registry";

/// The registry path from `--registry <path>`.
fn registry_path() -> Option<std::path::PathBuf> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == REGISTRY_FLAG {
            return args.next().map(std::path::PathBuf::from);
        }
    }
    None
}

#[tokio::main]
async fn main() {
    let Some(path) = registry_path() else {
        eprintln!("usage: legatus --registry <path>");
        std::process::exit(i32::from(EXIT_REGISTRY_INVALID));
    };
    let log = Arc::new(DiscardSink);
    let reporter = LoadReporter { out: Mutex::new(Box::new(std::io::stderr())), log: log.clone(), state: Arc::new(AdminWarnings::default()) };
    let sink: &dyn WarningSink = &reporter;
    let loaded = match load_registry(&path, &mut std::io::stderr(), sink) {
        Ok(loaded) => loaded,
        Err(code) => std::process::exit(i32::from(code)),
    };
    eprintln!("registry loaded: {} ({} warnings)", path.display(), loaded.warnings.len());
    let listen = std::env::var(LISTEN_ENV).unwrap_or_else(|_| DEFAULT_LISTEN.to_string());
    let seams = Seams {
        wall: Arc::new(SystemWallClock),
        transport: Arc::new(HyperTransport::new()),
        log,
        sim: SimPoints::new(),
    };
    let listener = match tokio::net::TcpListener::bind(&listen).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("cannot bind {listen}: {e}");
            std::process::exit(i32::from(EXIT_BIND_FAILED));
        }
    };
    eprintln!("legatus listening on {listen}");
    if let Err(e) = axum::serve(listener, build_router(seams)).await {
        eprintln!("server error: {e}");
        std::process::exit(1);
    }
}
