//! The vLLM adapter. All facts here are read from source and documents, none from a running
//! engine (ASSUMPTION until captures exist, story 146). The cached-token field exists only when
//! the engine runs with prompt token details switched on, so reuse is unknown until the registry
//! declares it or the probe finds it.
use crate::engine::reuse::extract_reuse;
use crate::engine::EngineAdapter;
use legatus_common::engine::*;

/// Prompts shorter than this are left out of the reuse measure: the smallest block size in the
/// notes (PROPOSED).
pub const VLLM_REUSE_MEASURE_FLOOR_TOKENS: u32 = 528;
pub const VLLM_DEFAULT_SLOTS: u32 = 1;
const ADVISORIES: [AdvisoryCode; 1] = [AdvisoryCode::VllmHybridNoReuseSignal];
const FLAGS: [FlagSpec; 1] = [FlagSpec { flag: EngineFlag::PrefixCaching }];

pub struct VllmAdapter;

impl EngineAdapter for VllmAdapter {
    fn family(&self) -> EngineFamily {
        EngineFamily::Vllm
    }
    fn cap_source(&self) -> CapSource {
        CapSource::DeclaredSlots { default: VLLM_DEFAULT_SLOTS }
    }
    fn load_signal(&self) -> LoadSignal {
        LoadSignal::OwnInFlightOnly
    }
    /// Only a field the registry declared or the probe found is read. A zero is a reading.
    fn reuse_fields(&self, response: &UsageView<'_>, probe: ReuseProbeState) -> ReuseReading {
        match probe {
            ReuseProbeState::Unprobed => ReuseReading::Unknown(UnknownReason::NotYetProbed),
            ReuseProbeState::Probed { field: None } => ReuseReading::Unknown(UnknownReason::NoneFound),
            ReuseProbeState::Probed { field: Some(field) } => extract_reuse(field, response),
        }
    }
    fn overflow_behaviour(&self) -> OverflowBehaviour {
        OverflowBehaviour::Unknown
    }
    fn allowed_flags(&self) -> &'static [FlagSpec] {
        &FLAGS
    }
    fn advisories(&self) -> &'static [AdvisoryCode] {
        &ADVISORIES
    }
    fn reuse_noisy_per_request(&self) -> bool {
        true
    }
    fn reuse_measure_floor_tokens(&self) -> u32 {
        VLLM_REUSE_MEASURE_FLOOR_TOKENS
    }
}
