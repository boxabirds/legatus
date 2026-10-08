//! The llama-server adapter (contract C61). Slot count from the properties page, load from the
//! metrics page only, reuse from `timings.cache_n`, the context of one slot, the engine's own
//! errors passed on unchanged, and the idle limit of pooled connections.
//!
//! This engine has a status page that lists slots. The proxy never asks for it: it cannot tell
//! whether a node sleeps, and that page wakes a sleeping engine. Nothing in this file names it.
use crate::config::diff::RegistryDiff;
use crate::config::node::{NodeSpec, Slots};
use crate::config::reload::ReloadObserver;
use crate::config::typed::Registry;
use crate::engine::reuse::extract_reuse;
use crate::engine::EngineAdapter;
use crate::upstream::transport::{UpstreamRequest, UpstreamTransport};
use bytes::Bytes;
use futures_util::StreamExt;
use http::{HeaderMap, Method};
use legatus_common::engine::*;
use legatus_common::ids::NodeId;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Used when the engine does not say and the registry says `auto`. PROPOSED, from spike S3;
/// shown as unverified.
pub const LLAMA_SERVER_DEFAULT_SLOTS: u32 = 4;
/// How often the load is read (`node_probe_interval_s`, 2 s PROPOSED).
pub const NODE_PROBE_INTERVAL: Duration = Duration::from_secs(2);
/// The idle time up to which a pooled connection is reused (`upstream_idle_reuse_max_s`). Below
/// the 5 s at which the engine closes an idle connection (spike S3).
pub const UPSTREAM_IDLE_REUSE_MAX: Duration = crate::net::hyper_transport::DEFAULT_IDLE_REUSE_MAX;

const PROPS_PATH: &str = "/props";
const METRICS_PATH: &str = "/metrics";
const HTTP_OK: u16 = 200;
const HTTP_BAD_REQUEST: u16 = 400;
const HTTP_SERVER_ERROR: u16 = 500;
/// The error code the engine gives for a prompt longer than the context of one slot.
pub const EXCEED_CONTEXT_CODE: &str = "exceed_context_size_error";
const METRIC_PREFIX: &str = "llamacpp:";
const METRIC_PROCESSING: &str = "requests_processing";
const METRIC_DEFERRED: &str = "requests_deferred";
/// Flags of this engine that a registry may declare.
const ALLOWED_FLAGS: [FlagSpec; 6] = [
    FlagSpec { flag: EngineFlag::Jinja },
    FlagSpec { flag: EngineFlag::Np },
    FlagSpec { flag: EngineFlag::CtxCheckpoints },
    FlagSpec { flag: EngineFlag::CheckpointMinStep },
    FlagSpec { flag: EngineFlag::CacheRamMib },
    FlagSpec { flag: EngineFlag::KvUnified },
];

pub struct LlamaServerAdapter;

impl EngineAdapter for LlamaServerAdapter {
    fn family(&self) -> EngineFamily {
        EngineFamily::LlamaServer
    }
    fn cap_source(&self) -> CapSource {
        CapSource::PropsTotalSlots
    }
    fn load_signal(&self) -> LoadSignal {
        LoadSignal::MetricsEndpoint
    }
    /// The count of reused tokens is `timings.cache_n`. The stored length (`tokens_cached`) is the
    /// size of the slot and has no reader. The probe state does not matter for a known engine.
    fn reuse_fields(&self, response: &UsageView<'_>, _probe: ReuseProbeState) -> ReuseReading {
        extract_reuse(ReuseFieldName::TimingsCacheN, response)
    }
    fn overflow_behaviour(&self) -> OverflowBehaviour {
        OverflowBehaviour::Error400
    }
    fn allowed_flags(&self) -> &'static [FlagSpec] {
        &ALLOWED_FLAGS
    }
}

// ---- slots -------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropsError {
    MissingField,
    NotAnInteger,
    Zero,
}

/// The `total_slots` of the properties body.
pub fn parse_props(body: &[u8]) -> Result<u32, PropsError> {
    let value: serde_json::Value = serde_json::from_slice(body).map_err(|_| PropsError::MissingField)?;
    let slots = value.get("total_slots").ok_or(PropsError::MissingField)?;
    match slots.as_u64().and_then(|n| u32::try_from(n).ok()) {
        None => Err(PropsError::NotAnInteger),
        Some(0) => Err(PropsError::Zero),
        Some(n) => Ok(n),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotsSource {
    Registry,
    Props,
    DefaultUnverified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectiveSlots {
    pub slots: u32,
    pub source: SlotsSource,
}

/// The smaller of the registry count and the engine count. A registry count above the engine's
/// never raises the cap. `auto` uses the engine; with no readable engine count it is the
/// default, shown as unverified.
pub fn effective_slots(decl: Slots, props: Option<u32>) -> EffectiveSlots {
    match (decl, props) {
        (Slots::Auto, Some(n)) => EffectiveSlots { slots: n, source: SlotsSource::Props },
        (Slots::Count(r), Some(n)) if r <= n => EffectiveSlots { slots: r, source: SlotsSource::Registry },
        (Slots::Count(_), Some(n)) => EffectiveSlots { slots: n, source: SlotsSource::Props },
        (Slots::Count(r), None) => EffectiveSlots { slots: r, source: SlotsSource::Registry },
        (Slots::Auto, None) => EffectiveSlots { slots: LLAMA_SERVER_DEFAULT_SLOTS, source: SlotsSource::DefaultUnverified },
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotContextSource {
    Declared,
    TotalOverParallel,
    WholeContext,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlotContext {
    pub tokens: Option<u32>,
    pub source: Option<SlotContextSource>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotContextError {
    ZeroParallel,
}

/// The context of one slot. A declared per-slot size wins. Otherwise, with an explicit parallel
/// count and no unified cache, the total is divided (rounded down: `-c 16384 -np 4` is 4096). With a
/// unified cache or no parallel count each slot may use the whole total. Nothing known gives none.
pub fn slot_context(declared_per_slot: Option<u32>, ctx_total: Option<u32>, np: Option<u32>, kv_unified: Option<bool>) -> Result<SlotContext, SlotContextError> {
    if np == Some(0) {
        return Err(SlotContextError::ZeroParallel);
    }
    if let Some(tokens) = declared_per_slot {
        return Ok(SlotContext { tokens: Some(tokens), source: Some(SlotContextSource::Declared) });
    }
    let Some(total) = ctx_total else { return Ok(SlotContext { tokens: None, source: None }) };
    Ok(match (np, kv_unified.unwrap_or(false)) {
        (Some(n), false) => SlotContext { tokens: Some(total / n), source: Some(SlotContextSource::TotalOverParallel) },
        _ => SlotContext { tokens: Some(total), source: Some(SlotContextSource::WholeContext) },
    })
}

// ---- load --------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MetricsReading {
    pub processing: u32,
    pub deferred: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetricsError {
    MissingCounter,
    NotANumber,
}

fn counter(text: &str, name: &str) -> Result<u32, MetricsError> {
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let (Some(key), Some(value)) = (parts.next(), parts.next()) else { continue };
        if key.strip_prefix(METRIC_PREFIX).unwrap_or(key) == name {
            return value.parse::<f64>().ok().filter(|v| *v >= 0.0 && v.fract() == 0.0 && *v <= f64::from(u32::MAX)).map(|v| v as u32).ok_or(MetricsError::NotANumber);
        }
    }
    Err(MetricsError::MissingCounter)
}

/// The two gauges, with or without the `llamacpp:` prefix; comment lines are ignored.
pub fn parse_metrics(text: &str) -> Result<MetricsReading, MetricsError> {
    Ok(MetricsReading { processing: counter(text, METRIC_PROCESSING)?, deferred: counter(text, METRIC_DEFERRED)? })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadReading {
    Metrics(MetricsReading),
    OwnCountOnly,
}

/// The last load reading of a node, with its source shown to the admin read.
pub struct NodeLoadCell(Mutex<LoadReading>);

impl Default for NodeLoadCell {
    fn default() -> Self {
        NodeLoadCell(Mutex::new(LoadReading::OwnCountOnly))
    }
}

impl NodeLoadCell {
    pub fn store(&self, reading: LoadReading) {
        if let Ok(mut held) = self.0.lock() {
            *held = reading;
        }
    }
    pub fn get(&self) -> LoadReading {
        self.0.lock().map(|held| *held).unwrap_or(LoadReading::OwnCountOnly)
    }
}

fn get_request(base_url: &str, path: &str) -> Option<UpstreamRequest> {
    let uri = format!("{}{path}", base_url.trim_end_matches('/')).parse().ok()?;
    Some(UpstreamRequest { method: Method::GET, uri, headers: HeaderMap::new(), body: Bytes::new() })
}

/// Fetch a page: its status and whole body, or none when the node cannot be reached in time.
async fn fetch(t: &dyn UpstreamTransport, base_url: &str, path: &str, limit: Duration) -> Option<(u16, Vec<u8>)> {
    let request = get_request(base_url, path)?;
    let work = async {
        let reply = t.send(request).await.ok()?;
        let status = reply.status.as_u16();
        let mut body = Vec::new();
        let mut stream = reply.body;
        while let Some(chunk) = stream.next().await {
            body.extend_from_slice(&chunk.ok()?);
        }
        Some((status, body))
    };
    tokio::time::timeout(limit, work).await.ok().flatten()
}

/// Reads the load of one node from its metrics page. It never asks for any other status page.
pub struct LoadPoller {
    pub base_url: String,
    pub interval: Duration,
}

impl LoadPoller {
    pub fn new(base_url: &str) -> LoadPoller {
        LoadPoller { base_url: base_url.to_string(), interval: NODE_PROBE_INTERVAL }
    }

    /// A 404 (metrics off), a timeout, a closed connection or an unreadable body gives
    /// `OwnCountOnly`; the next 200 returns to `Metrics`.
    pub async fn poll_once(&self, t: &dyn UpstreamTransport) -> LoadReading {
        let Some((status, body)) = fetch(t, &self.base_url, METRICS_PATH, self.interval).await else { return LoadReading::OwnCountOnly };
        if status != HTTP_OK {
            return LoadReading::OwnCountOnly;
        }
        match std::str::from_utf8(&body).ok().and_then(|text| parse_metrics(text).ok()) {
            Some(reading) => LoadReading::Metrics(reading),
            None => LoadReading::OwnCountOnly,
        }
    }
}

// ---- node facts --------------------------------------------------------------------------

/// What the admin read shows of a llama-server node besides its registry entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeFacts {
    pub slots: EffectiveSlots,
    pub slot_context: SlotContext,
}

/// The engine count of a node: its properties page when it answers and holds a usable count.
pub async fn read_props(t: &dyn UpstreamTransport, base_url: &str, limit: Duration) -> Option<u32> {
    let (status, body) = fetch(t, base_url, PROPS_PATH, limit).await?;
    (status == HTTP_OK).then(|| parse_props(&body).ok()).flatten()
}

/// Read the facts of a node at join, on a registry entry change and on return from down. An
/// unreadable properties page keeps the previous value when there is one.
pub async fn discover(t: &dyn UpstreamTransport, node: &NodeSpec, previous: Option<NodeFacts>) -> Result<NodeFacts, SlotContextError> {
    let base_url = node.endpoints.first().map(|e| e.base_url.as_str()).unwrap_or_default();
    let declared = node.slots.unwrap_or(Slots::Auto);
    let slots = match read_props(t, base_url, NODE_PROBE_INTERVAL).await {
        Some(n) => effective_slots(declared, Some(n)),
        None => previous.map(|p| p.slots).unwrap_or_else(|| effective_slots(declared, None)),
    };
    let slot_context = slot_context(node.context_per_slot, node.engine_flags.ctx_size, node.engine_flags.np, node.engine_flags.kv_unified)?;
    Ok(NodeFacts { slots, slot_context })
}

// ---- connections and errors --------------------------------------------------------------

/// A pooled connection is reused when it has been idle at most `max_idle`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IdlePolicy {
    pub max_idle: Duration,
}

impl Default for IdlePolicy {
    fn default() -> Self {
        IdlePolicy { max_idle: UPSTREAM_IDLE_REUSE_MAX }
    }
}

impl IdlePolicy {
    pub fn reusable(&self, idle: Duration) -> bool {
        idle <= self.max_idle
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorClass {
    /// The request was at fault; the node is fine.
    RequestFault,
    /// Health logic decides (stories 175 and 182).
    NotClassified,
}

/// A 400 for a prompt over the slot context and a 500 with a JSON error are faults of the request.
/// `json_error_code` is the code of the JSON error body, when the body has one.
pub fn classify_error(status: u16, json_error_code: Option<&str>) -> ErrorClass {
    match (status, json_error_code) {
        (HTTP_BAD_REQUEST, Some(EXCEED_CONTEXT_CODE)) => ErrorClass::RequestFault,
        (HTTP_SERVER_ERROR, Some(_)) => ErrorClass::RequestFault,
        _ => ErrorClass::NotClassified,
    }
}

/// The `type` of the JSON error object in an engine error body, when the body has one. Only the
/// code is kept for logs; the body itself is passed on unchanged and never logged.
pub fn error_type_of(body: &[u8]) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    value.get("error")?.get("type")?.as_str().map(str::to_string)
}

// ---- keeping the facts current -----------------------------------------------------------

/// The facts of every llama-server node, as the admin read serves them.
#[derive(Default)]
pub struct NodeFactsStore(Mutex<HashMap<NodeId, NodeFacts>>);

impl NodeFactsStore {
    pub fn get(&self, node: &NodeId) -> Option<NodeFacts> {
        self.0.lock().ok().and_then(|held| held.get(node).copied())
    }
    pub fn set(&self, node: NodeId, facts: NodeFacts) {
        if let Ok(mut held) = self.0.lock() {
            held.insert(node, facts);
        }
    }
    pub fn remove(&self, node: &NodeId) {
        if let Ok(mut held) = self.0.lock() {
            held.remove(node);
        }
    }
}

fn is_llama_server(node: &NodeSpec) -> bool {
    crate::engine::family_of(node.engine.as_str()) == EngineFamily::LlamaServer
}

/// Re-reads the facts of a node at join, on a registry entry change and on return from down.
pub struct FactsRefresher {
    transport: Arc<dyn UpstreamTransport>,
    store: Arc<NodeFactsStore>,
}

impl FactsRefresher {
    pub fn new(transport: Arc<dyn UpstreamTransport>, store: Arc<NodeFactsStore>) -> FactsRefresher {
        FactsRefresher { transport, store }
    }

    /// Read one node now. Use this at join and when the node returns from down (story 175).
    /// A node of another engine is ignored.
    pub async fn refresh_node(&self, node: &NodeSpec) {
        if !is_llama_server(node) {
            return;
        }
        let previous = self.store.get(&node.name);
        if let Ok(facts) = discover(self.transport.as_ref(), node, previous).await {
            self.store.set(node.name.clone(), facts);
        }
    }

    /// Read every llama-server node of a registry, each in its own task (the start of the proxy).
    pub fn spawn_join(self: &Arc<Self>, registry: &Registry) {
        for node in registry.nodes.iter().filter(|n| is_llama_server(n)) {
            self.spawn_refresh(node.clone());
        }
    }

    fn spawn_refresh(self: &Arc<Self>, node: NodeSpec) {
        if tokio::runtime::Handle::try_current().is_err() {
            return;
        }
        let me = self.clone();
        tokio::spawn(async move { me.refresh_node(&node).await });
    }
}

impl ReloadObserver for Arc<FactsRefresher> {
    fn on_reload(&self, _old: &Registry, new: &Registry, diff: &RegistryDiff) {
        for removed in &diff.nodes_removed {
            self.store.remove(removed);
        }
        let touched = diff.nodes_added.iter().chain(diff.nodes_changed.iter().map(|c| &c.name));
        for name in touched {
            if let Some(node) = new.node(name) {
                if is_llama_server(node) {
                    self.spawn_refresh(node.clone());
                }
            }
        }
    }
}
