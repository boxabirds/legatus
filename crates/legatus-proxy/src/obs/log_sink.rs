//! Log sink seam and the skeleton record types (story 144). Story 122 adds the fields.
use legatus_common::event::SystemEventKind;

pub trait LogSink: Send + Sync {
    fn offer(&self, record: LogRecord) -> Offer;
    fn flush(&self);
}

pub enum LogRecord {
    Request(Box<RequestRecord>),
    System(SystemRecord),
}

/// Fields are added by story 122.
#[derive(Debug, Clone, Default)]
pub struct RequestRecord {}

/// Envelope and fields are added by story 122.
#[derive(Debug, Clone)]
pub struct SystemRecord {
    pub kind: SystemEventKind,
    /// Set for `TruncationDetected` (story 174): counts and the node name, never text.
    pub truncation: Option<TruncationFields>,
}

impl SystemRecord {
    pub fn new(kind: SystemEventKind) -> SystemRecord {
        SystemRecord { kind, truncation: None }
    }
}

/// The fields of the event `truncation_detected`.
#[derive(Debug, Clone, PartialEq)]
pub struct TruncationFields {
    pub node: legatus_common::ids::NodeId,
    pub prompt_tokens_sent: u32,
    pub prompt_tokens_seen: u32,
    pub ratio: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Offer {
    Queued,
    Dropped(DropReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropReason {
    QueueFull,
    Paused,
    BuildError,
    Marker,
}

/// Accepts and forgets every record; used until story 122 writes real event files.
pub struct DiscardSink;

impl LogSink for DiscardSink {
    fn offer(&self, _record: LogRecord) -> Offer {
        Offer::Queued
    }
    fn flush(&self) {}
}
