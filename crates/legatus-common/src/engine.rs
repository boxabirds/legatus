//! Plain engine types shared by the proxy, the node agent and the test kit (contract C60).
//! The adapter trait that uses them lives in the proxy crate.
use crate::protocol::Protocol;

/// A node whose engine has no adapter serves with exactly one request at a time.
pub const UNKNOWN_ENGINE_CAP: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EngineFamily {
    LlamaServer,
    Ollama,
    MlxLm,
    Vllm,
    Sglang,
    Gufo,
    Unknown,
}

/// Where the cap of concurrent requests comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapSource {
    PropsTotalSlots,
    DeclaredSlots { default: u32 },
    FixedOne,
}

/// Where the load of a node can be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadSignal {
    MetricsEndpoint,
    SlotsEndpoint,
    OwnInFlightOnly,
}

/// What an engine does when a prompt is longer than its context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverflowBehaviour {
    Error400,
    SilentTruncate,
    Unbounded,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineFlag {
    Jinja,
    Np,
    CtxCheckpoints,
    CheckpointMinStep,
    CacheRamMib,
    NumParallel,
    KeepAliveS,
    KvUnified,
    PrefixCaching,
    SpeculativeDecoding,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlagSpec {
    pub flag: EngineFlag,
}

/// The reply fields that can carry the number of reused tokens. There is no variant for the
/// stored-length field (`tokens_cached`): it is the size of the slot, not reuse (PRX-AFF-022).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReuseFieldName {
    TimingsCacheN,
    PromptTokensDetailsCachedTokens,
    InputTokensDetailsCachedTokens,
}

impl ReuseFieldName {
    /// The dotted path of the field in the reply JSON.
    pub fn wire_name(&self) -> &'static str {
        match self {
            ReuseFieldName::TimingsCacheN => "timings.cache_n",
            ReuseFieldName::PromptTokensDetailsCachedTokens => "usage.prompt_tokens_details.cached_tokens",
            ReuseFieldName::InputTokensDetailsCachedTokens => "usage.input_tokens_details.cached_tokens",
        }
    }
}

/// What the reuse probe of a node found (the probe itself is story 147).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReuseProbeState {
    Unprobed,
    Probed { field: Option<ReuseFieldName> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnknownReason {
    FieldAbsent,
    FieldMalformed,
    NotYetProbed,
    NoneFound,
}

/// A reuse reading. `Unknown` is no data: it never marks a node poor (EDGE-CAC-005, EDGE-CAC-014).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReuseReading {
    Reused { cached_tokens: u64, field: ReuseFieldName },
    Unknown(UnknownReason),
}

/// The borrowed JSON tail of a reply. Its Debug output shows the length only, never the bytes.
#[derive(Clone, Copy)]
pub struct UsageView<'a> {
    pub json_tail: &'a [u8],
    pub protocol: Protocol,
    pub stream: bool,
}

impl std::fmt::Debug for UsageView<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "UsageView({} bytes, {:?}, stream {})", self.json_tail.len(), self.protocol, self.stream)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineVersionStatus {
    Tested,
    Untested { found: String },
    Unknown,
}

/// Why a node is measured again. Story 127 produces `EngineVersionChanged`; story 147 the others.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CalibrationReason {
    Join,
    EngineVersionChanged { from: String, to: String },
    SettingsChanged,
    NodeReturned,
}

/// Advisories an engine adapter can attach to a node (the table is story 183).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdvisoryCode {
    MlxMemoryGrowth,
    VllmHybridNoReuseSignal,
}

/// The shape of the answers of a node, read from the reply of its health probe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnswerShape {
    LlamaServerLike,
    OllamaLike,
    Other,
}

/// A note shown with a node: a code and a fixed text, never node data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Advisory {
    pub code: AdvisoryCode,
    pub text: &'static str,
}

/// How sure a recorded fact is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GapLabel {
    /// Written in the engine's own documents.
    Documented,
    /// Read from the engine's source.
    Inferred,
    /// Not checked.
    Unverified,
}

/// A known gap in the Responses API support of an engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResponsesGap {
    pub text: &'static str,
    pub label: GapLabel,
    pub source: &'static str,
}
