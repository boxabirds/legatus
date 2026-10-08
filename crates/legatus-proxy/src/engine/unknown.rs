//! The adapter of an engine that has none: one request at a time, own in-flight count only, and
//! no trusted reuse signal until the probe found one.
use crate::engine::reuse::extract_reuse;
use crate::engine::EngineAdapter;
use legatus_common::engine::*;

pub struct UnknownAdapter;

impl EngineAdapter for UnknownAdapter {
    fn family(&self) -> EngineFamily {
        EngineFamily::Unknown
    }
    fn cap_source(&self) -> CapSource {
        CapSource::FixedOne
    }
    fn load_signal(&self) -> LoadSignal {
        LoadSignal::OwnInFlightOnly
    }
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
        &[]
    }
}
