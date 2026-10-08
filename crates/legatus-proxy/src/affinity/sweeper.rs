//! The task that removes idle entries when they are due. One `sleep_until` to the next due
//! instant, no interval: it is woken when an entry becomes idle and parked when none is. Expiry
//! on lookup is exact, so the result does not depend on whether the sweeper ran first.
use crate::affinity::table::AffinityTable;
use std::sync::Arc;
use tokio::sync::watch;
use tokio::time::Instant;

pub type StopToken = watch::Receiver<bool>;

pub async fn run_sweeper(table: Arc<AffinityTable>, mut stop: StopToken) {
    loop {
        if *stop.borrow() {
            return;
        }
        // Ask for the wake before reading the due time, so a change in between is not missed.
        let notified = table.woken().notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        match table.next_due() {
            Some(due) => tokio::select! {
                _ = tokio::time::sleep_until(due) => {
                    table.expire(Instant::now());
                }
                _ = &mut notified => {}
                _ = stop.changed() => {}
            },
            None => tokio::select! {
                _ = &mut notified => {}
                _ = stop.changed() => {}
            },
        }
    }
}
