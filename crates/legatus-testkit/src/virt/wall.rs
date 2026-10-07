//! Fake wall clock: base time plus paused elapsed time plus the jumps made by the test.
use legatus_proxy::time::{Instant, WallClock, WallTime};
use std::sync::atomic::{AtomicI64, Ordering};

pub struct SimWall {
    base: WallTime,
    started: Instant,
    jumps_ms: AtomicI64,
}

impl SimWall {
    pub fn new(base: WallTime) -> SimWall {
        SimWall { base, started: Instant::now(), jumps_ms: AtomicI64::new(0) }
    }

    /// Move the wall clock by `delta_ms` (negative allowed); the monotonic clock is unaffected.
    pub fn jump(&self, delta_ms: i64) {
        self.jumps_ms.fetch_add(delta_ms, Ordering::SeqCst);
    }
}

impl WallClock for SimWall {
    fn now(&self) -> WallTime {
        let elapsed = i64::try_from(self.started.elapsed().as_millis()).unwrap_or(i64::MAX);
        WallTime { unix_ms: self.base.unix_ms + elapsed + self.jumps_ms.load(Ordering::SeqCst) }
    }
}
