//! Stepwise driver. Time only moves because the runtime auto-advances while every task sleeps.
//! There is deliberately no `advance` or `jump` method (PRX-TEST-011).
use std::time::Duration;

pub struct Driver;

impl Driver {
    /// Sleep for `total` in pieces of at most `step`, so timers run in exact order.
    pub async fn run_for(total: Duration, step: Duration) {
        let step = step.max(Duration::from_millis(1));
        let mut left = total;
        while !left.is_zero() {
            let piece = left.min(step);
            tokio::time::sleep(piece).await;
            left -= piece;
        }
    }

    pub async fn sleep_ms(ms: u64) {
        tokio::time::sleep(Duration::from_millis(ms)).await;
    }
}
