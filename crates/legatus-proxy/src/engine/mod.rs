//! One adapter per engine family (contract C60). The routing core calls this trait only; engine
//! names appear in this directory and in `config/` and nowhere else.
pub mod advisory;
pub mod estimate;
pub mod gufo;
pub mod guard;
pub mod llama_server;
pub mod mlx_lm;
pub mod ollama;
pub mod responses_gaps;
pub mod sglang;
pub mod vllm;
pub mod reuse;
pub mod unknown;
pub mod version;

/// The names the rest of the proxy uses, so it never names an engine.
pub use llama_server::{FactsRefresher as NodeFactsRefresher, NodeFactsStore as EngineFactsStore};

use crate::config::node::NodeSpec;
use legatus_common::engine::*;
use std::collections::HashMap;
use std::sync::Arc;

/// Where a place in the node is given back: when the client's reply ends, or when the node's ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaceRelease {
    AtClientEnd,
    AtNodeEnd,
}

/// What a health probe read from a reply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProbeReading {
    pub ok: bool,
    pub engine_version: Option<String>,
    pub model: Option<String>,
    pub answer_shape: AnswerShape,
}

#[derive(Clone, Copy)]
pub struct HealthProbe {
    pub path: &'static str,
    pub parse: fn(status: u16, body: &[u8]) -> ProbeReading,
}

/// The status of the models list that every OpenAI-compatible engine serves.
const MODELS_LIST_PATH: &str = "/v1/models";
const HTTP_OK: u16 = 200;

fn parse_models_list(status: u16, _body: &[u8]) -> ProbeReading {
    ProbeReading { ok: status == HTTP_OK, engine_version: None, model: None, answer_shape: AnswerShape::Other }
}

impl HealthProbe {
    pub fn models_list() -> HealthProbe {
        HealthProbe { path: MODELS_LIST_PATH, parse: parse_models_list }
    }
}

/// The 13 methods of an engine. Six are required; seven have a default that the adapters of the
/// families override where the engine differs.
pub trait EngineAdapter: Send + Sync {
    fn family(&self) -> EngineFamily;
    fn cap_source(&self) -> CapSource;
    fn load_signal(&self) -> LoadSignal;
    fn reuse_fields(&self, response: &UsageView<'_>, probe: ReuseProbeState) -> ReuseReading;
    fn overflow_behaviour(&self) -> OverflowBehaviour;
    fn allowed_flags(&self) -> &'static [FlagSpec];

    fn place_release(&self) -> PlaceRelease {
        PlaceRelease::AtClientEnd
    }
    fn health_probe(&self) -> HealthProbe {
        HealthProbe::models_list()
    }
    fn advisories(&self) -> &'static [AdvisoryCode] {
        &[]
    }
    fn reuse_noisy_per_request(&self) -> bool {
        false
    }
    fn reuse_measure_floor_tokens(&self) -> u32 {
        0
    }
    fn forbidden_added_fields(&self) -> &'static [&'static str] {
        &[]
    }
    fn drop_is_node_signal(&self) -> bool {
        true
    }
}

/// The family of an engine name as written in the registry. Matching is exact: a name that
/// differs by case, an empty name and a name without an adapter family are `Unknown`.
pub fn family_of(engine_name: &str) -> EngineFamily {
    match engine_name {
        "llama-server" => EngineFamily::LlamaServer,
        "ollama" => EngineFamily::Ollama,
        "mlx_lm" => EngineFamily::MlxLm,
        "vllm" => EngineFamily::Vllm,
        "sglang" => EngineFamily::Sglang,
        "gufo" => EngineFamily::Gufo,
        _ => EngineFamily::Unknown,
    }
}

/// The adapters that ship, built once (the admin node view asks them for notes).
pub fn standard_adapters() -> &'static AdapterRegistry {
    static STANDARD: std::sync::OnceLock<AdapterRegistry> = std::sync::OnceLock::new();
    STANDARD.get_or_init(AdapterRegistry::standard)
}

/// The adapters by family, built at start and shared read-only. The unknown adapter is always
/// present.
pub struct AdapterRegistry {
    by_family: HashMap<EngineFamily, Arc<dyn EngineAdapter>>,
    unknown: unknown::UnknownAdapter,
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        AdapterRegistry::new()
    }
}

impl AdapterRegistry {
    pub fn new() -> AdapterRegistry {
        AdapterRegistry { by_family: HashMap::new(), unknown: unknown::UnknownAdapter }
    }

    /// The adapters that ship: one per engine family that has one (story 151 adds llama-server).
    pub fn standard() -> AdapterRegistry {
        let mut registry = AdapterRegistry::new();
        registry.register(Arc::new(llama_server::LlamaServerAdapter));
        registry.register(Arc::new(ollama::OllamaAdapter));
        registry.register(Arc::new(mlx_lm::MlxLmAdapter));
        registry.register(Arc::new(vllm::VllmAdapter));
        registry.register(Arc::new(sglang::SglangAdapter));
        registry.register(Arc::new(gufo::GufoAdapter));
        registry
    }

    /// Replaces the slot of the family of the adapter.
    pub fn register(&mut self, adapter: Arc<dyn EngineAdapter>) {
        self.by_family.insert(adapter.family(), adapter);
    }

    /// The adapter of a node. Never fails: a name without a family and a family without a
    /// registered adapter give the unknown adapter.
    pub fn for_node(&self, n: &NodeSpec) -> &dyn EngineAdapter {
        self.for_family(family_of(n.engine.as_str()))
    }

    pub fn for_family(&self, family: EngineFamily) -> &dyn EngineAdapter {
        match self.by_family.get(&family) {
            Some(adapter) => adapter.as_ref(),
            None => &self.unknown,
        }
    }
}
