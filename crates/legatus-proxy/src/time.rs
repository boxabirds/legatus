//! Clocks (story 144). Monotonic time is the runtime clock, so paused time works.
pub use tokio::time::Instant;

/// Wall clock reading in milliseconds since the Unix epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WallTime {
    pub unix_ms: i64,
}

/// The only way proxy code reads the wall clock.
pub trait WallClock: Send + Sync + 'static {
    fn now(&self) -> WallTime;
}
