//! The Legatus proxy library. Later stories add the registry, pipeline and affinity.
#![deny(clippy::disallowed_types, clippy::disallowed_methods)]
pub mod config;
pub mod deps;
pub mod lifecycle;
pub mod net;
pub mod protocol;
pub mod obs;
pub mod sim;
pub mod sorted;
pub mod stream;
pub mod time;
pub mod upstream;

use crate::obs::log_sink::LogSink;
use crate::sim::SimPoints;
use crate::time::WallClock;
use crate::upstream::transport::UpstreamTransport;
use std::future::Future;
use std::sync::Arc;

pub use crate::deps::{build_router, RouterDeps};
pub use crate::protocol::paths::CHAT_COMPLETIONS_PATH;

/// Everything the proxy needs from the outside world (contract C02).
#[derive(Clone)]
pub struct Seams {
    pub wall: Arc<dyn WallClock>,
    pub transport: Arc<dyn UpstreamTransport>,
    pub log: Arc<dyn LogSink>,
    pub sim: SimPoints,
}

/// Start a background task on the current runtime. The caller keeps the handle.
pub fn spawn_named<F>(_name: &'static str, future: F) -> tokio::task::JoinHandle<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    tokio::spawn(future)
}
