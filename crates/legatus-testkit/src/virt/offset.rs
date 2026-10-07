//! Whole-millisecond offsets only: the paused runtime clock rounds sub-millisecond sleeps up.
use std::time::Duration;

const NANOS_PER_MS: u128 = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MsOffset(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OffsetError {
    SubMillisecond,
}

impl MsOffset {
    pub fn new(d: Duration) -> Result<MsOffset, OffsetError> {
        let nanos = d.as_nanos();
        if !nanos.is_multiple_of(NANOS_PER_MS) {
            return Err(OffsetError::SubMillisecond);
        }
        u64::try_from(nanos / NANOS_PER_MS).map(MsOffset).map_err(|_| OffsetError::SubMillisecond)
    }

    pub fn ms(self) -> u64 {
        self.0
    }

    pub fn duration(self) -> Duration {
        Duration::from_millis(self.0)
    }
}
