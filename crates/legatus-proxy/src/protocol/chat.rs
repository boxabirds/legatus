//! The chat path: the sixteen request steps in one order, with hooks for later stories (contract C20).
use crate::config::alias::AliasSpec;
use crate::config::node::{EndpointProtocol, NodeSpec};
use crate::config::typed::Registry;
use crate::deps::RouterDeps;
use crate::protocol::ctx::RequestCtx;
use crate::engine::patch::{apply_patch, patch_applies_to};
use legatus_common::patch::NodePatch;
use crate::engine::guard::{context_decision, estimate_sent, GuardDecision, TruncationTap};
use crate::protocol::errors::{refuse, RefusalDetail, RefusalKind};
use crate::stream::tap::ResponseTap;
use legatus_common::engine::OverflowBehaviour;
use crate::protocol::headers::forward_headers;
use crate::protocol::paths::{recognise, RouteKind};
use crate::protocol::route::{route_by_model, Refusal};
use crate::protocol::rewrite::{peek_request, rewrite_model, RequestPeek, RewriteError};
use crate::upstream::send::send_to_node;
use crate::upstream::transport::{UpstreamError, UpstreamRequest};
use async_trait::async_trait;
use axum::http::header::HOST;
use axum::http::{HeaderValue, Uri};
use axum::response::Response;
use bytes::Bytes;
use crate::protocol::observe::EndObserver;
use crate::protocol::stream::EndFlags;
use crate::stream::tap::{copy_response, ResponseHead, StreamEnd};
use legatus_common::ids::{AliasName, NodeId};
use legatus_common::protocol::Protocol;
use tokio::time::Instant;

/// A parsed inbound request: method, URI, headers and the whole body.
pub struct ParsedRequest {
    pub method: http::Method,
    pub uri: http::Uri,
    pub headers: http::HeaderMap,
    pub body: Bytes,
}

/// What a hook decides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HookOutcome {
    Continue,
    Refuse(RefusalKind),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PipelineStep {
    AcceptAndAuth,
    RecogniseProtocol,
    ReadBody,
    AliasLookup,
    ComputeKey,
    TableLookup,
    Place,
    Hold,
    HoldRefuse,
    PatchAndRewrite,
    ContextCheck,
    Send,
    CopyResponse,
    CacheFeedback,
    TableUpdate,
    WriteEvent,
}

pub const PIPELINE_STEP_COUNT: usize = 16;

/// The one order of the steps. A unit test asserts it.
pub const STEP_ORDER: [PipelineStep; PIPELINE_STEP_COUNT] = [
    PipelineStep::AcceptAndAuth,
    PipelineStep::RecogniseProtocol,
    PipelineStep::ReadBody,
    PipelineStep::AliasLookup,
    PipelineStep::ComputeKey,
    PipelineStep::TableLookup,
    PipelineStep::Place,
    PipelineStep::Hold,
    PipelineStep::HoldRefuse,
    PipelineStep::PatchAndRewrite,
    PipelineStep::ContextCheck,
    PipelineStep::Send,
    PipelineStep::CopyResponse,
    PipelineStep::CacheFeedback,
    PipelineStep::TableUpdate,
    PipelineStep::WriteEvent,
];

/// How a request ended, for the event step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PipelineOutcome {
    /// A node answered with this status.
    Served(u16),
    Refused(RefusalKind),
    /// The client went away before the request was complete.
    ClientClosed,
}

/// One defaulted method per hook step. A later story overrides the method of its step.
#[async_trait]
pub trait PipelineHooks: Send + Sync {
    /// Step 1: StartGate release (story 119), client token (story 163).
    async fn accept_and_auth(&self, _protocol: Protocol, _headers: &http::HeaderMap) -> HookOutcome {
        HookOutcome::Continue
    }
    /// Step 4, after the alias is found.
    async fn after_alias(&self, _ctx: &mut RequestCtx) -> HookOutcome {
        HookOutcome::Continue
    }
    /// Step 5 (story 126).
    async fn compute_key(&self, _ctx: &mut RequestCtx, _headers: &http::HeaderMap) -> HookOutcome {
        HookOutcome::Continue
    }
    /// Step 6 (story 143).
    async fn table_lookup(&self, _ctx: &mut RequestCtx) -> HookOutcome {
        HookOutcome::Continue
    }
    /// Step 7 (story 169): `Some(node)` replaces the default rule.
    async fn place(&self, _ctx: &mut RequestCtx) -> Option<NodeId> {
        None
    }
    /// Step 8 (story 178).
    async fn hold(&self, _ctx: &mut RequestCtx) -> HookOutcome {
        HookOutcome::Continue
    }
    /// Step 9 (story 187).
    async fn hold_refuse(&self, _ctx: &mut RequestCtx) -> HookOutcome {
        HookOutcome::Continue
    }
    /// Step 10, after the model is rewritten (story 190). `peek` is a fresh peek of `body`.
    async fn patch_body(&self, _ctx: &mut RequestCtx, _body: &mut Bytes, _peek: &RequestPeek) -> HookOutcome {
        HookOutcome::Continue
    }
    /// Step 11 (story 174).
    async fn context_check(&self, _ctx: &mut RequestCtx, _body: &Bytes) -> HookOutcome {
        HookOutcome::Continue
    }
    /// Step 14 (stories 171, 194).
    async fn cache_feedback(&self, _ctx: &RequestCtx, _status: u16) {}
    /// Step 15 (stories 143, 169).
    async fn table_update(&self, _ctx: &RequestCtx, _status: u16) {}
    /// Step 16 (story 122): runs for every request, refused or served.
    async fn write_event(&self, _ctx: Option<&RequestCtx>, _outcome: &PipelineOutcome) {}
    /// Called once when the reply body has ended, however it ended (stories 122, 171, 184). It is
    /// not async: the reply stream calls it from its end or its drop.
    fn reply_end(&self, _summary: &ReplySummary) {}
}

/// How a relayed reply ended, for the hooks that record it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplySummary {
    pub alias: AliasName,
    pub node: NodeId,
    pub status: u16,
    pub end: StreamEnd,
    pub flags: EndFlags,
}

/// The hooks of a proxy that has none: every default.
pub struct NoHooks;

#[async_trait]
impl PipelineHooks for NoHooks {}

/// Records the steps that ran, in order. Used by tests through `RouterDeps::with_trace`.
#[derive(Clone, Default)]
pub struct StepTrace(std::sync::Arc<std::sync::Mutex<Vec<PipelineStep>>>);

impl StepTrace {
    pub fn new() -> StepTrace {
        StepTrace::default()
    }
    fn record(&self, step: PipelineStep) {
        if let Ok(mut steps) = self.0.lock() {
            steps.push(step);
        }
    }
    pub fn steps(&self) -> Vec<PipelineStep> {
        self.0.lock().map(|s| s.clone()).unwrap_or_default()
    }
}

fn enter(deps: &RouterDeps, step: PipelineStep) {
    if let Some(trace) = &deps.trace {
        trace.record(step);
    }
}

/// The node a request goes to when no placer is installed: the first node of the alias, in the
/// order the alias lists them, that has a chat endpoint.
pub fn default_node<'a>(alias: &'a AliasSpec, registry: &Registry) -> Option<&'a NodeId> {
    alias.nodes.iter().find(|id| registry.node(id).is_some_and(serves_chat))
}

fn serves_chat(node: &NodeSpec) -> bool {
    node.endpoints.iter().any(|e| e.protocol == EndpointProtocol::OpenaiChat)
}

fn rewrite_refusal(error: RewriteError) -> RefusalKind {
    match error {
        RewriteError::ModelMissing => RefusalKind::ModelMissing,
        RewriteError::InvalidJson | RewriteError::NotObject | RewriteError::ModelNotString | RewriteError::DuplicateModel => RefusalKind::InvalidBody,
    }
}

/// The node URI for the request path: the node's chat base address plus the path of the request.
fn node_uri(node: &NodeSpec, inbound: &Uri) -> Option<Uri> {
    let endpoint = node.endpoints.iter().find(|e| e.protocol == EndpointProtocol::OpenaiChat)?;
    let path = inbound.path_and_query().map(|p| p.as_str()).unwrap_or("/");
    format!("{}{}", endpoint.base_url.trim_end_matches('/'), path).parse().ok()
}

macro_rules! hook {
    ($outcome:expr) => {
        if let HookOutcome::Refuse(kind) = $outcome {
            return Err(Refused::from(kind));
        }
    };
}

/// A refusal with the facts its message needs (the limit and the size of an over-long prompt).
struct Refused {
    kind: RefusalKind,
    detail: RefusalDetail,
}

impl From<RefusalKind> for Refused {
    fn from(kind: RefusalKind) -> Refused {
        Refused { kind, detail: RefusalDetail::default() }
    }
}

/// Run the sixteen steps for one request. The event step runs for a refused request too.
pub async fn run_pipeline(deps: &RouterDeps, req: ParsedRequest) -> Response {
    let mut ctx: Option<RequestCtx> = None;
    let result = run_steps(deps, req, &mut ctx).await;
    enter(deps, PipelineStep::WriteEvent);
    let (response, outcome) = match result {
        Ok(response) => {
            let status = response.status().as_u16();
            (response, PipelineOutcome::Served(status))
        }
        Err(Refused { kind, detail }) => {
            let protocol = ctx.as_ref().map(|c| c.protocol).unwrap_or(Protocol::OpenAiChat);
            (refuse(kind, protocol, &detail), PipelineOutcome::Refused(kind))
        }
    };
    deps.hooks.write_event(ctx.as_ref(), &outcome).await;
    response
}

async fn run_steps(deps: &RouterDeps, req: ParsedRequest, ctx_slot: &mut Option<RequestCtx>) -> Result<Response, Refused> {
    let started = Instant::now();
    // Steps 1 to 3: accept, recognise, body (the body is already read).
    enter(deps, PipelineStep::AcceptAndAuth);
    let Some((protocol, route)) = recognise(req.uri.path()) else {
        return Err(RefusalKind::ModelNotFound.into());
    };
    hook!(deps.hooks.accept_and_auth(protocol, &req.headers).await);
    enter(deps, PipelineStep::RecogniseProtocol);
    enter(deps, PipelineStep::ReadBody);
    // Step 4: alias lookup on one snapshot of the registry.
    enter(deps, PipelineStep::AliasLookup);
    let registry = deps.registry.snapshot();
    let peek = peek_request(&req.body).map_err(rewrite_refusal)?;
    let model = peek.model().unwrap_or_default().to_string();
    let alias = route_by_model(&registry.aliases, &model).map_err(|Refusal::ModelNotFound| RefusalKind::ModelNotFound)?;
    let chat_nodes: Vec<&NodeId> = alias.nodes.iter().filter(|id| registry.node(id).is_some_and(serves_chat)).collect();
    if chat_nodes.is_empty() {
        return Err(RefusalKind::ProtocolNotServed.into());
    }
    let ctx = ctx_slot.insert(RequestCtx { protocol, route, model, stream: peek.stream().unwrap_or(false), peek, alias: alias.name.clone(), node: None, started });
    hook!(deps.hooks.after_alias(ctx).await);
    enter(deps, PipelineStep::ComputeKey);
    hook!(deps.hooks.compute_key(ctx, &req.headers).await);
    enter(deps, PipelineStep::TableLookup);
    hook!(deps.hooks.table_lookup(ctx).await);
    // Step 7: place. A placer replaces the default rule.
    enter(deps, PipelineStep::Place);
    let node_id = match deps.hooks.place(ctx).await {
        Some(id) => id,
        None => default_node(alias, &registry).cloned().ok_or(RefusalKind::ModelNotFound)?,
    };
    ctx.node = Some(node_id.clone());
    enter(deps, PipelineStep::Hold);
    hook!(deps.hooks.hold(ctx).await);
    enter(deps, PipelineStep::HoldRefuse);
    hook!(deps.hooks.hold_refuse(ctx).await);
    // Step 10: rewrite the model, peek again, then the patch hook works on the fresh peek.
    enter(deps, PipelineStep::PatchAndRewrite);
    let node = registry.node(&node_id).ok_or(RefusalKind::ModelNotFound)?;
    let mut body = rewrite_model(&req.body, &ctx.peek, alias.node_model(&node_id));
    let mut fresh = peek_request(&body).map_err(rewrite_refusal)?;
    // The node patch is applied after the model rewrite, on a fresh reading of the rewritten body,
    // for chat and Messages requests only (story 190).
    if patch_applies_to(ctx.route) {
        if let Some(patch) = node.patch.as_deref().and_then(|text| NodePatch::from_json(text).ok()) {
            body = apply_patch(&body, &fresh, &patch).map_err(|_| RefusalKind::InvalidBody)?;
            fresh = peek_request(&body).map_err(rewrite_refusal)?;
        }
    }
    hook!(deps.hooks.patch_body(ctx, &mut body, &fresh).await);
    enter(deps, PipelineStep::ContextCheck);
    hook!(deps.hooks.context_check(ctx, &body).await);
    // An engine that silently cuts a prompt that is too long is guarded here: an over-long prompt
    // is refused and never reaches the node (story 174).
    let adapter = deps.adapters.for_node(node);
    let guarded = adapter.overflow_behaviour() == OverflowBehaviour::SilentTruncate;
    if let GuardDecision::Refuse { limit_tokens, estimate_tokens } = context_decision(adapter, node, body.len(), &registry.settings) {
        let detail = RefusalDetail { max_tokens: Some(limit_tokens), used_tokens: Some(estimate_tokens), retry_after_s: None };
        return Err(Refused { kind: RefusalKind::ContextLengthExceeded, detail });
    }
    let sent_estimate = estimate_sent(&registry.settings, body.len());
    // Step 12: the single send.
    enter(deps, PipelineStep::Send);
    let uri = node_uri(node, &req.uri).ok_or(RefusalKind::NodeConnectFailed)?;
    let mut headers = forward_headers(&req.headers);
    if let Some(host) = uri.authority().and_then(|a| HeaderValue::from_str(a.as_str()).ok()) {
        headers.insert(HOST, host);
    }
    let upstream = UpstreamRequest { method: req.method, uri, headers, body };
    let reply = match send_to_node(deps.seams.transport.as_ref(), &node_id, upstream).await {
        Ok(reply) => reply,
        Err(UpstreamError::Connect | UpstreamError::Reset) => return Err(RefusalKind::NodeConnectFailed.into()),
    };
    // Step 13: relay the reply through the pull-through copy (story 145).
    enter(deps, PipelineStep::CopyResponse);
    let status = reply.status.as_u16();
    let head = ResponseHead { status: reply.status, headers: reply.headers, protocol: ctx.protocol };
    let hooks = deps.hooks.clone();
    let (alias_name, reply_node) = (ctx.alias.clone(), node_id.clone());
    let observer = EndObserver::new(ctx.protocol).with_report(move |end, flags| {
        hooks.reply_end(&ReplySummary { alias: alias_name.clone(), node: reply_node.clone(), status, end, flags });
    });
    let mut taps: Vec<Box<dyn ResponseTap>> = vec![Box::new(observer)];
    if guarded {
        taps.push(Box::new(TruncationTap::new(node_id.clone(), sent_estimate, registry.settings.truncation_report_ratio, deps.seams.log.clone(), ctx.protocol, ctx.stream)));
    }
    let (reply_status, reply_headers, reply_body) = copy_response(head, reply.body, deps.response_guard.clone(), taps);
    let mut response = Response::new(reply_body);
    *response.status_mut() = reply_status;
    *response.headers_mut() = reply_headers;
    enter(deps, PipelineStep::CacheFeedback);
    deps.hooks.cache_feedback(ctx, status).await;
    enter(deps, PipelineStep::TableUpdate);
    deps.hooks.table_update(ctx, status).await;
    Ok(response)
}

/// Is this the chat route? (The router only registers it, but tests ask.)
pub fn is_chat(route: RouteKind) -> bool {
    route == RouteKind::Chat
}
