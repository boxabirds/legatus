//! The one module that reads the operating-system wall clock (listed in lint-optouts.txt).
#![allow(clippy::disallowed_types, reason = "production wall clock (story 144)")]
use crate::time::{WallClock, WallTime};

pub struct SystemWallClock;

impl WallClock for SystemWallClock {
    fn now(&self) -> WallTime {
        let since_epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        WallTime { unix_ms: i64::try_from(since_epoch.as_millis()).unwrap_or(i64::MAX) }
    }
}
