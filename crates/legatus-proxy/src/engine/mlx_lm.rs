//! The mlx_lm adapter. The engine admits every request and sends headers at once, so an early
//! header says nothing about capacity; it keeps prefilling after the client leaves, so a place
//! is held until the node ends the request; a dropped connection on an invalid request is a
//! fault of the request, not of the node.
use crate::engine::reuse::extract_reuse;
use crate::engine::{EngineAdapter, PlaceRelease};
use legatus_common::engine::*;

pub const MLX_LM_DEFAULT_SLOTS: u32 = 1;
const ADVISORIES: [AdvisoryCode; 1] = [AdvisoryCode::MlxMemoryGrowth];

pub struct MlxLmAdapter;

impl EngineAdapter for MlxLmAdapter {
    fn family(&self) -> EngineFamily {
        EngineFamily::MlxLm
    }
    fn cap_source(&self) -> CapSource {
        CapSource::DeclaredSlots { default: MLX_LM_DEFAULT_SLOTS }
    }
    fn load_signal(&self) -> LoadSignal {
        LoadSignal::OwnInFlightOnly
    }
    fn reuse_fields(&self, response: &UsageView<'_>, _probe: ReuseProbeState) -> ReuseReading {
        extract_reuse(ReuseFieldName::PromptTokensDetailsCachedTokens, response)
    }
    fn overflow_behaviour(&self) -> OverflowBehaviour {
        OverflowBehaviour::Unbounded
    }
    fn allowed_flags(&self) -> &'static [FlagSpec] {
        &[]
    }
    fn place_release(&self) -> PlaceRelease {
        PlaceRelease::AtNodeEnd
    }
    fn advisories(&self) -> &'static [AdvisoryCode] {
        &ADVISORIES
    }
    fn drop_is_node_signal(&self) -> bool {
        false
    }
}
