//! The Ollama adapter (contract C62). No slot discovery and no load field exist on this engine:
//! the cap is what the owner declares (lowered by a measurement), and the load is the proxy's own
//! count of requests in flight. The engine cuts a prompt that is too long and answers 200; the
//! guard in `guard.rs` is what stops that.
use crate::engine::reuse::extract_reuse;
use crate::engine::EngineAdapter;
use legatus_common::engine::*;
use legatus_common::protocol::Protocol;

/// The cap of a node that declares nothing (`OLLAMA_NUM_PARALLEL` unset).
pub const OLLAMA_DEFAULT_SLOTS: u32 = 1;
const FLAGS: [FlagSpec; 2] = [FlagSpec { flag: EngineFlag::NumParallel }, FlagSpec { flag: EngineFlag::KeepAliveS }];

pub struct OllamaAdapter;

impl EngineAdapter for OllamaAdapter {
    fn family(&self) -> EngineFamily {
        EngineFamily::Ollama
    }
    fn cap_source(&self) -> CapSource {
        CapSource::DeclaredSlots { default: OLLAMA_DEFAULT_SLOTS }
    }
    fn load_signal(&self) -> LoadSignal {
        LoadSignal::OwnInFlightOnly
    }
    /// The chat route of Ollama 0.40.0 reports `usage.prompt_tokens_details.cached_tokens`
    /// (measured, OPEN-022). The Messages shape of Ollama was not measured, so it reads as unknown.
    fn reuse_fields(&self, response: &UsageView<'_>, _probe: ReuseProbeState) -> ReuseReading {
        match response.protocol {
            Protocol::OpenAiChat => extract_reuse(ReuseFieldName::PromptTokensDetailsCachedTokens, response),
            _ => ReuseReading::Unknown(UnknownReason::FieldAbsent),
        }
    }
    fn overflow_behaviour(&self) -> OverflowBehaviour {
        OverflowBehaviour::SilentTruncate
    }
    fn allowed_flags(&self) -> &'static [FlagSpec] {
        &FLAGS
    }
}

/// The declared cap was above what the probe measured, so the cap is the measurement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlotsAboveMeasured {
    pub declared: u32,
    pub measured: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OllamaCap {
    pub cap: u32,
    pub warning: Option<SlotsAboveMeasured>,
}

/// No declaration gives 1 (a declared 0 counts as 1). A measurement below the declaration lowers
/// the cap and warns; an equal or higher one changes nothing. The cap is never raised.
pub fn ollama_cap(declared: Option<u32>, measured_useful: Option<u32>) -> OllamaCap {
    let declared = declared.unwrap_or(OLLAMA_DEFAULT_SLOTS).max(1);
    match measured_useful {
        Some(measured) if measured < declared => OllamaCap { cap: measured.max(1), warning: Some(SlotsAboveMeasured { declared, measured }) },
        _ => OllamaCap { cap: declared, warning: None },
    }
}

/// Where the load shown for a node comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadSource {
    /// The proxy's own count of requests in flight. No engine page is read for load.
    ProxyCount,
}

/// What the admin read shows of the cap and load of an Ollama node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapView {
    pub cap: OllamaCap,
    pub load: u32,
    pub load_source: LoadSource,
}

pub fn cap_view(declared: Option<u32>, measured_useful: Option<u32>, in_flight: u32) -> CapView {
    CapView { cap: ollama_cap(declared, measured_useful), load: in_flight, load_source: LoadSource::ProxyCount }
}
