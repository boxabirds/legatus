//! A log sink with scripted faults. `LogSink::offer` is synchronous, so a stall is modelled as the
//! sink refusing offers (`Paused`) until the stall ends on the runtime clock.
use crate::stubs::scenario::SinkFault;
use legatus_proxy::obs::log_sink::{DropReason, LogRecord, LogSink, Offer};
use std::sync::Mutex;
use std::time::Duration;
use tokio::time::Instant;

#[derive(Default)]
struct State {
    fault: Option<SinkFault>,
    stall_until: Option<Instant>,
    dropped: usize,
    accepted: usize,
}

#[derive(Default)]
pub struct FaultySink {
    state: Mutex<State>,
}

impl FaultySink {
    pub fn new() -> FaultySink {
        FaultySink::default()
    }
    /// Start a fault; `None` clears it.
    pub fn set_fault(&self, fault: Option<SinkFault>) {
        if let Ok(mut s) = self.state.lock() {
            s.stall_until = match fault {
                Some(SinkFault::Stall { ms }) => Some(Instant::now() + Duration::from_millis(ms)),
                _ => None,
            };
            s.fault = fault;
        }
    }
    pub fn dropped(&self) -> usize {
        self.state.lock().map(|s| s.dropped).unwrap_or(0)
    }
    pub fn accepted(&self) -> usize {
        self.state.lock().map(|s| s.accepted).unwrap_or(0)
    }
    pub fn disk_low(&self) -> bool {
        self.state.lock().map(|s| s.fault == Some(SinkFault::DiskLow)).unwrap_or(false)
    }
}

impl LogSink for FaultySink {
    fn offer(&self, _record: LogRecord) -> Offer {
        let Ok(mut s) = self.state.lock() else { return Offer::Dropped(DropReason::BuildError) };
        match s.fault {
            Some(SinkFault::Fail) => {
                s.dropped += 1;
                Offer::Dropped(DropReason::BuildError)
            }
            Some(SinkFault::Stall { .. }) if s.stall_until.is_some_and(|t| Instant::now() < t) => {
                s.dropped += 1;
                Offer::Dropped(DropReason::Paused)
            }
            _ => {
                s.accepted += 1;
                Offer::Queued
            }
        }
    }
    fn flush(&self) {}
}
