//! Event enums declared once for every story that logs (story 144, contract C02).
//! Wire values are snake_case.
use serde::{Deserialize, Serialize};

/// The 11 request outcomes of PRX-REST-047.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Ok,
    NodeErrorStatus,
    NodeConnectFailed,
    NodeStreamCut,
    ClientClosed,
    HoldLimit,
    NoNode,
    ContextGuard,
    BadRequest,
    Unauthorized,
    ProxyError,
}

/// How a node was chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlacementLabel {
    Affinity,
    NewWarmSlot,
    NewEvictIdle,
    LeastLoaded,
    HeldThenPlaced,
    Moved,
    SiblingSpill,
    None,
}

/// The 17 system record kinds of story 122.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemEventKind {
    Start,
    Ready,
    RegistryLoaded,
    RegistryRejected,
    NodeState,
    LogPaused,
    LogResumed,
    Shutdown,
    HoldStart,
    HoldEnd,
    HoldRefused,
    Moved,
    AffinityModeChanged,
    TruncationDetected,
    Warning,
    Finding,
    CalibrationDone,
}

/// Signals that start a shutdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShutdownSignal {
    Term,
    Int,
}
