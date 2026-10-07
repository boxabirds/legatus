//! What the proxy needs from outside, as one value (story 121 declares the first two fields).
use crate::config::typed::RegistryHandle;
use crate::protocol::chat::{NoHooks, PipelineHooks, StepTrace};
use crate::protocol::chat::{run_pipeline, ParsedRequest};
use crate::protocol::paths::CHAT_COMPLETIONS_PATH;
use crate::Seams;
use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::response::Response;
use axum::routing::post;
use axum::Router;
use std::sync::Arc;

/// Largest chat body read when the settings give none (the catalogue default of body_limit_bytes).
const BODY_READ_LIMIT_FALLBACK: usize = 32 * 1024 * 1024;

/// A story that needs another field writes "story N adds field X (default Y) to the `RouterDeps`
/// of story 121" and adds the field, its default in `for_test` and a `with_<field>()` builder.
#[derive(Clone)]
pub struct RouterDeps {
    pub seams: Seams,
    pub registry: Arc<RegistryHandle>,
    /// The hooks of the later stories; `NoHooks` runs the pipeline alone.
    pub hooks: Arc<dyn PipelineHooks>,
    /// Test builds record the steps here.
    pub trace: Option<StepTrace>,
}

impl RouterDeps {
    /// An empty fleet, no hooks and no trace.
    pub fn for_test(seams: Seams) -> RouterDeps {
        RouterDeps { seams, registry: Arc::new(RegistryHandle::empty()), hooks: Arc::new(NoHooks), trace: None }
    }

    pub fn with_registry(mut self, registry: Arc<RegistryHandle>) -> RouterDeps {
        self.registry = registry;
        self
    }

    pub fn with_hooks(mut self, hooks: Arc<dyn PipelineHooks>) -> RouterDeps {
        self.hooks = hooks;
        self
    }

    pub fn with_trace(mut self, trace: StepTrace) -> RouterDeps {
        self.trace = Some(trace);
        self
    }
}

/// The router: the chat path, matched exactly. It never binds a socket.
pub fn build_router(deps: RouterDeps) -> Router {
    Router::new().route(CHAT_COMPLETIONS_PATH, post(handle_chat)).with_state(deps)
}

async fn handle_chat(State(deps): State<RouterDeps>, request: Request<Body>) -> Response {
    let (parts, body) = request.into_parts();
    let limit = usize::try_from(deps.registry.snapshot().settings.body_limit_bytes).unwrap_or(BODY_READ_LIMIT_FALLBACK);
    let bytes = match axum::body::to_bytes(body, limit).await {
        Ok(bytes) => bytes,
        Err(_) => {
            // A body that cannot be read to its end is a client abort (or over the limit, which
            // story 160 answers with 413): no node is called and no refusal body is written.
            deps.hooks.write_event(None, &crate::protocol::chat::PipelineOutcome::ClientClosed).await;
            let mut response = Response::new(Body::empty());
            *response.status_mut() = StatusCode::BAD_REQUEST;
            return response;
        }
    };
    run_pipeline(&deps, ParsedRequest { method: parts.method, uri: parts.uri, headers: parts.headers, body: bytes }).await
}
