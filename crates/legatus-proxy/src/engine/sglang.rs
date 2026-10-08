//! The SGLang adapter (facts read from source, ASSUMPTION until captures exist). Its session
//! option gives soft protection with no rule to close it, so the proxy never adds a session field.
use crate::engine::reuse::extract_reuse;
use crate::engine::EngineAdapter;
use legatus_common::engine::*;

pub const SGLANG_DEFAULT_SLOTS: u32 = 1;
/// The request field the proxy must never add to an SGLang request.
pub const SESSION_FIELD: &str = "session_id";
const FORBIDDEN: [&str; 1] = [SESSION_FIELD];
const FLAGS: [FlagSpec; 1] = [FlagSpec { flag: EngineFlag::PrefixCaching }];

pub struct SglangAdapter;

impl EngineAdapter for SglangAdapter {
    fn family(&self) -> EngineFamily {
        EngineFamily::Sglang
    }
    fn cap_source(&self) -> CapSource {
        CapSource::DeclaredSlots { default: SGLANG_DEFAULT_SLOTS }
    }
    fn load_signal(&self) -> LoadSignal {
        LoadSignal::OwnInFlightOnly
    }
    fn reuse_fields(&self, response: &UsageView<'_>, _probe: ReuseProbeState) -> ReuseReading {
        extract_reuse(ReuseFieldName::PromptTokensDetailsCachedTokens, response)
    }
    fn overflow_behaviour(&self) -> OverflowBehaviour {
        OverflowBehaviour::Unknown
    }
    fn allowed_flags(&self) -> &'static [FlagSpec] {
        &FLAGS
    }
    /// Reuse follows a checkpoint grid, so single readings saw-tooth.
    fn reuse_noisy_per_request(&self) -> bool {
        true
    }
    fn forbidden_added_fields(&self) -> &'static [&'static str] {
        &FORBIDDEN
    }
}
