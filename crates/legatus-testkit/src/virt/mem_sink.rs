//! In-memory log sink: offer runs in the calling task, never blocks, never spawns.
use legatus_proxy::obs::log_sink::{DropReason, LogRecord, LogSink, Offer};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

pub struct MemorySink {
    records: Mutex<Vec<LogRecord>>,
    capacity: usize,
    paused: AtomicBool,
    fail_build: AtomicBool,
}

impl MemorySink {
    pub fn new(capacity: usize) -> MemorySink {
        MemorySink {
            records: Mutex::new(Vec::new()),
            capacity,
            paused: AtomicBool::new(false),
            fail_build: AtomicBool::new(false),
        }
    }
    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::SeqCst);
    }
    /// The next offer is refused with `BuildError` (a record that cannot be built).
    pub fn fail_next_build(&self) {
        self.fail_build.store(true, Ordering::SeqCst);
    }
    pub fn len(&self) -> usize {
        self.records.lock().map(|r| r.len()).unwrap_or(0)
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn take(&self) -> Vec<LogRecord> {
        self.records.lock().map(|mut r| std::mem::take(&mut *r)).unwrap_or_default()
    }
}

impl LogSink for MemorySink {
    fn offer(&self, record: LogRecord) -> Offer {
        if self.paused.load(Ordering::SeqCst) {
            return Offer::Dropped(DropReason::Paused);
        }
        if self.fail_build.swap(false, Ordering::SeqCst) {
            return Offer::Dropped(DropReason::BuildError);
        }
        let Ok(mut records) = self.records.lock() else {
            return Offer::Dropped(DropReason::BuildError);
        };
        if records.len() >= self.capacity {
            return Offer::Dropped(DropReason::QueueFull);
        }
        records.push(record);
        Offer::Queued
    }
    fn flush(&self) {}
}
