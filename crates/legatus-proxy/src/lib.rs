//! The Legatus proxy library. Later stories add the registry, pipeline and affinity.
#![deny(clippy::disallowed_types, clippy::disallowed_methods)]
pub mod net;
pub mod obs;
pub mod sim;
pub mod sorted;
pub mod time;
pub mod upstream;

use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::response::Response;
use axum::routing::post;
use axum::Router;
use futures_util::TryStreamExt;
use std::future::Future;
use std::sync::Arc;

use crate::obs::log_sink::LogSink;
use crate::sim::SimPoints;
use crate::time::WallClock;
use crate::upstream::transport::{UpstreamError, UpstreamRequest, UpstreamTransport};

/// Chat completions path (story 121 owns the full routing).
pub const CHAT_COMPLETIONS_PATH: &str = "/v1/chat/completions";
/// Placeholder node address of the skeleton route; story 121 picks a real node.
const SKELETON_NODE_URI: &str = "http://node.invalid/v1/chat/completions";
/// Largest request body the skeleton route reads (story 160 sets the real limit).
const SKELETON_BODY_LIMIT_BYTES: usize = 64 * 1024 * 1024;

/// Everything the proxy needs from the outside world (contract C02).
#[derive(Clone)]
pub struct Seams {
    pub wall: Arc<dyn WallClock>,
    pub transport: Arc<dyn UpstreamTransport>,
    pub log: Arc<dyn LogSink>,
    pub sim: SimPoints,
}

/// Skeleton router with one route; story 121 replaces the parameter with its `RouterDeps`.
pub fn build_router(seams: Seams) -> Router {
    Router::new().route(CHAT_COMPLETIONS_PATH, post(forward_chat)).with_state(seams)
}

/// Start a background task on the current runtime. The caller keeps the handle.
pub fn spawn_named<F>(_name: &'static str, future: F) -> tokio::task::JoinHandle<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    tokio::spawn(future)
}

async fn forward_chat(State(seams): State<Seams>, request: Request<Body>) -> Response {
    let (parts, body) = request.into_parts();
    let Ok(bytes) = axum::body::to_bytes(body, SKELETON_BODY_LIMIT_BYTES).await else {
        return status_response(StatusCode::BAD_REQUEST);
    };
    let upstream = UpstreamRequest {
        method: parts.method,
        uri: SKELETON_NODE_URI.parse().unwrap_or_default(),
        headers: parts.headers,
        body: bytes,
    };
    match seams.transport.send(upstream).await {
        Err(UpstreamError::Connect | UpstreamError::Reset) => status_response(StatusCode::BAD_GATEWAY),
        Ok(reply) => {
            let stream = reply.body.map_err(|e| std::io::Error::other(e.to_string()));
            let mut response = Response::new(Body::from_stream(stream));
            *response.status_mut() = reply.status;
            *response.headers_mut() = reply.headers;
            response
        }
    }
}

fn status_response(status: StatusCode) -> Response {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = status;
    response
}
