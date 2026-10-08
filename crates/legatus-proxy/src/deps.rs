//! What the proxy needs from outside, as one value (story 121 declares the first two fields).
use crate::config::typed::RegistryHandle;
use crate::engine::AdapterRegistry;
use crate::key::harness::HarnessTable;
use crate::key::hasher::{KeyHasher, KeySecret, SECRET_LEN_BYTES};
use crate::key::lazy_body::KeyProbe;
use crate::key::sources::WarnOnce;
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
    /// Story 127 adds this field: the adapter of each engine family. The default is an empty
    /// registry, so every node gets the unknown adapter.
    pub adapters: Arc<AdapterRegistry>,
    /// Story 126 adds these: the harness table, the keyed hash and the warn-once set of key reading.
    pub harnesses: Arc<HarnessTable>,
    pub key_hasher: Arc<KeyHasher>,
    pub key_warn: Arc<WarnOnce>,
    /// Test builds count the body parses made for a key here.
    pub key_probe: Option<KeyProbe>,
}

/// The secret of a test build: fixed, so keys are the same on every run.
const TEST_SECRET: [u8; SECRET_LEN_BYTES] = [0x5A; SECRET_LEN_BYTES];

impl RouterDeps {
    /// An empty fleet, no hooks and no trace.
    pub fn for_test(seams: Seams) -> RouterDeps {
        let key_warn = Arc::new(WarnOnce::new(seams.log.clone()));
        RouterDeps { seams, registry: Arc::new(RegistryHandle::empty()), hooks: Arc::new(NoHooks), trace: None, response_guard: Arc::new(NoGuard), adapters: Arc::new(AdapterRegistry::new()), harnesses: Arc::new(HarnessTable::default()), key_hasher: Arc::new(KeyHasher::new(&KeySecret::from_bytes(TEST_SECRET))), key_warn, key_probe: None }
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

    pub fn with_adapters(mut self, adapters: Arc<AdapterRegistry>) -> RouterDeps {
        self.adapters = adapters;
        self
    }

    pub fn with_harnesses(mut self, harnesses: Arc<HarnessTable>) -> RouterDeps {
        self.harnesses = harnesses;
        self
    }

    pub fn with_key_secret(mut self, secret: &KeySecret) -> RouterDeps {
        self.key_hasher = Arc::new(KeyHasher::new(secret));
        self
    }

    pub fn with_key_probe(mut self, probe: KeyProbe) -> RouterDeps {
        self.key_probe = Some(probe);
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
