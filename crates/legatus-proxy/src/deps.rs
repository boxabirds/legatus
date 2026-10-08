//! What the proxy needs from outside, as one value (story 121 declares the first two fields).
use crate::config::typed::RegistryHandle;
use crate::protocol::chat::{NoHooks, PipelineHooks, StepTrace};
use crate::protocol::chat::{run_pipeline, ParsedRequest};
use crate::protocol::body::{read_body_limited, BodyReadError};
use crate::protocol::errors::{refuse, RefusalDetail, RefusalKind};
use crate::protocol::paths::CHAT_COMPLETIONS_PATH;
use legatus_common::protocol::Protocol;
use crate::stream::guard::{NoGuard, ResponseGuard};
use crate::Seams;
use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::response::Response;
use axum::routing::post;
use axum::Router;
use std::sync::Arc;

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
    /// Story 145 adds this field: the guard of the reply path. `NoGuard` is inactive and free;
    /// story 135 supplies the real one.
    pub response_guard: Arc<dyn ResponseGuard>,
}

impl RouterDeps {
    /// An empty fleet, no hooks and no trace.
    pub fn for_test(seams: Seams) -> RouterDeps {
        RouterDeps { seams, registry: Arc::new(RegistryHandle::empty()), hooks: Arc::new(NoHooks), trace: None, response_guard: Arc::new(NoGuard) }
    }

    pub fn with_registry(mut self, registry: Arc<RegistryHandle>) -> RouterDeps {
        self.registry = registry;
        self
    }

    pub fn with_hooks(mut self, hooks: Arc<dyn PipelineHooks>) -> RouterDeps {
        self.hooks = hooks;
        self
    }

    pub fn with_response_guard(mut self, guard: Arc<dyn ResponseGuard>) -> RouterDeps {
        self.response_guard = guard;
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
    let limit = deps.registry.snapshot().settings.body_limit_bytes;
    let declared = parts.headers.get(http::header::CONTENT_LENGTH).and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok());
    let bytes = match read_body_limited(body, declared, limit).await {
        Ok(bytes) => bytes,
        Err(BodyReadError::TooLarge) => {
            let response = refuse(RefusalKind::BodyTooLarge, Protocol::OpenAiChat, &RefusalDetail::default());
            deps.hooks.write_event(None, &crate::protocol::chat::PipelineOutcome::Refused(RefusalKind::BodyTooLarge)).await;
            return response;
        }
        Err(BodyReadError::Aborted) => {
            // A body that cannot be read to its end is a client abort: no node is called and no
            // refusal body is written.
            deps.hooks.write_event(None, &crate::protocol::chat::PipelineOutcome::ClientClosed).await;
            let mut response = Response::new(Body::empty());
            *response.status_mut() = StatusCode::BAD_REQUEST;
            return response;
        }
    };
    run_pipeline(&deps, ParsedRequest { method: parts.method, uri: parts.uri, headers: parts.headers, body: bytes }).await
}
