//! The notes shown with a node and the reuse-gate mapping that decides one of them.
use crate::config::node::CacheKind;
use crate::engine::EngineAdapter;
use legatus_common::engine::*;

pub const TEXT_MLX_MEMORY_GROWTH: &str = "This engine can grow its memory use over a long run. Watch it.";
pub const TEXT_VLLM_HYBRID_NO_REUSE_SIGNAL: &str = "This engine does not report reused tokens unless prompt token details are switched on, and a hybrid model needs that to be measured.";

/// What the advisories look at in a node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdvisoryFacts {
    pub cache_kind: CacheKind,
    pub responses: bool,
    pub declared_prompt_tokens_details: Option<bool>,
}

/// The advisory of a code, when it applies to a node with these facts and this reuse probe state.
/// A memory-growth note applies to every node whose adapter lists it. The vLLM hybrid note applies
/// only to a hybrid model whose reuse is not a probed field.
pub fn advisory_active(code: AdvisoryCode, facts: &AdvisoryFacts, probe: ReuseProbeState) -> Option<Advisory> {
    match code {
        AdvisoryCode::MlxMemoryGrowth => Some(Advisory { code, text: TEXT_MLX_MEMORY_GROWTH }),
        AdvisoryCode::VllmHybridNoReuseSignal => {
            let has_field = matches!(probe, ReuseProbeState::Probed { field: Some(_) });
            (facts.cache_kind == CacheKind::Hybrid && !has_field).then_some(Advisory { code, text: TEXT_VLLM_HYBRID_NO_REUSE_SIGNAL })
        }
    }
}

/// The advisories of a node: those its adapter lists that apply.
pub fn node_advisories(adapter: &dyn EngineAdapter, facts: &AdvisoryFacts, probe: ReuseProbeState) -> Vec<Advisory> {
    adapter.advisories().iter().filter_map(|code| advisory_active(*code, facts, probe)).collect()
}

/// The probe state of a node from its declaration and its probe result. A declared flag counts
/// as a field. A probe that ran beats a declaration: a measurement that contradicts it wins.
pub fn reuse_state_for_node(declared_details: Option<bool>, probed: ReuseProbeState) -> ReuseProbeState {
    match probed {
        ReuseProbeState::Probed { .. } => probed,
        ReuseProbeState::Unprobed if declared_details == Some(true) => ReuseProbeState::Probed { field: Some(ReuseFieldName::PromptTokensDetailsCachedTokens) },
        ReuseProbeState::Unprobed => ReuseProbeState::Unprobed,
    }
}

/// Prompts below the floor of an adapter are left out of the reuse measure.
pub fn counts_for_reuse_measure(adapter: &dyn EngineAdapter, prompt_tokens: u32) -> bool {
    prompt_tokens >= adapter.reuse_measure_floor_tokens()
}

/// The keys of a patch that the adapter forbids the proxy to add (`bad_patch` at load, story 190).
pub fn check_patch_keys<'a>(adapter: &dyn EngineAdapter, keys: &[&'a str]) -> Vec<&'a str> {
    keys.iter().copied().filter(|k| adapter.forbidden_added_fields().contains(k)).collect()
}
