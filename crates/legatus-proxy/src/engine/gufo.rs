//! The gufo adapter (facts from documents, ASSUMPTION until captures exist). It gives
//! llama-compatible timings in the terminal chunk and reuses an exact prefix.
use crate::engine::reuse::extract_reuse;
use crate::engine::EngineAdapter;
use legatus_common::engine::*;

pub const GUFO_DEFAULT_SLOTS: u32 = 1;

pub struct GufoAdapter;

impl EngineAdapter for GufoAdapter {
    fn family(&self) -> EngineFamily {
        EngineFamily::Gufo
    }
    fn cap_source(&self) -> CapSource {
        CapSource::DeclaredSlots { default: GUFO_DEFAULT_SLOTS }
    }
    fn load_signal(&self) -> LoadSignal {
        LoadSignal::OwnInFlightOnly
    }
    fn reuse_fields(&self, response: &UsageView<'_>, _probe: ReuseProbeState) -> ReuseReading {
        extract_reuse(ReuseFieldName::TimingsCacheN, response)
    }
    fn overflow_behaviour(&self) -> OverflowBehaviour {
        OverflowBehaviour::Unknown
    }
    fn allowed_flags(&self) -> &'static [FlagSpec] {
        &[]
    }
}
