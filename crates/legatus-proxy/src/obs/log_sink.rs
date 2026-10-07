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
