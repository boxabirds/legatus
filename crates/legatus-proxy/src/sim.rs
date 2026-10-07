//! Named pause points for forced interleavings in tests (story 144).
//! An unarmed point costs one atomic load.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::{oneshot, Notify};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimPointName {
    TableLookup,
    AdmitDecide,
    SlotFree,
    HoldExpire,
    StreamEnd,
    LogAppend,
    SweepAfterDue,
    ReloadApply,
}

const NAMES: [(&str, SimPointName); 8] = [
    ("table_lookup", SimPointName::TableLookup),
    ("admit_decide", SimPointName::AdmitDecide),
    ("slot_free", SimPointName::SlotFree),
    ("hold_expire", SimPointName::HoldExpire),
    ("stream_end", SimPointName::StreamEnd),
    ("log_append", SimPointName::LogAppend),
    ("sweep_after_due", SimPointName::SweepAfterDue),
    ("reload_apply", SimPointName::ReloadApply),
];

impl SimPointName {
    fn index(self) -> usize {
        NAMES.iter().position(|(_, n)| *n == self).unwrap_or(0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimError {
    UnknownSimPoint(String),
}

impl std::fmt::Display for SimError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let SimError::UnknownSimPoint(name) = self;
        let valid: Vec<&str> = NAMES.iter().map(|(n, _)| *n).collect();
        write!(f, "unknown sim point '{name}'; valid names: {}", valid.join(", "))
    }
}

impl std::error::Error for SimError {}

#[derive(Default)]
struct PointState {
    pass_left: usize,
    reached: usize,
    held: Vec<Option<oneshot::Sender<()>>>,
}

#[derive(Default)]
struct Point {
    armed: AtomicBool,
    state: Mutex<PointState>,
    changed: Notify,
}

struct Inner {
    points: [Point; 8],
}

#[derive(Clone)]
pub struct SimPoints {
    inner: Arc<Inner>,
}

impl Default for SimPoints {
    fn default() -> Self {
        Self::new()
    }
}

impl SimPoints {
    pub fn new() -> SimPoints {
        SimPoints { inner: Arc::new(Inner { points: Default::default() }) }
    }

    /// Arm a point by name. A freshly armed point holds every arrival (arm_after(0)).
    pub fn arm(&self, name: &str) -> Result<PointController, SimError> {
        let found = NAMES.iter().find(|(n, _)| *n == name).map(|(_, p)| *p);
        let Some(point) = found else {
            return Err(SimError::UnknownSimPoint(name.to_string()));
        };
        let controller = PointController { inner: self.inner.clone(), index: point.index() };
        controller.arm_after(0);
        Ok(controller)
    }

    fn point(&self, name: SimPointName) -> &Point {
        &self.inner.points[name.index()]
    }
}

/// Called from proxy code. Returns at once when the point is unarmed.
pub async fn sim_point(sim: &SimPoints, name: SimPointName) {
    let point = sim.point(name);
    if !point.armed.load(Ordering::Acquire) {
        return;
    }
    let rx = {
        let Ok(mut state) = point.state.lock() else { return };
        state.reached += 1;
        if state.pass_left > 0 {
            state.pass_left -= 1;
            return;
        }
        let (tx, rx) = oneshot::channel();
        state.held.push(Some(tx));
        rx
    };
    point.changed.notify_waiters();
    let _ = rx.await;
}

pub struct PointController {
    inner: Arc<Inner>,
    index: usize,
}

impl PointController {
    fn point(&self) -> &Point {
        &self.inner.points[self.index]
    }

    /// Let the first `n` arrivals pass and hold every later one.
    pub fn arm_after(&self, n: usize) {
        let point = self.point();
        if let Ok(mut state) = point.state.lock() {
            state.pass_left = n;
        }
        point.armed.store(true, Ordering::Release);
    }

    pub fn disarm(&self) {
        self.point().armed.store(false, Ordering::Release);
        self.release_all();
    }

    /// Wait until `count` tasks are held at this point.
    pub async fn wait_held(&self, count: usize) {
        let point = self.point();
        loop {
            let notified = point.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.held_now() >= count {
                return;
            }
            notified.await;
        }
    }

    fn held_now(&self) -> usize {
        self.point().state.lock().map(|s| s.held.iter().filter(|h| h.is_some()).count()).unwrap_or(0)
    }

    /// Release the held task with this arrival index. A non-held index changes nothing.
    pub fn release(&self, index: usize) {
        if let Ok(mut state) = self.point().state.lock() {
            if let Some(slot) = state.held.get_mut(index) {
                if let Some(tx) = slot.take() {
                    let _ = tx.send(());
                }
            }
        }
    }

    pub fn release_all(&self) {
        if let Ok(mut state) = self.point().state.lock() {
            for slot in state.held.iter_mut() {
                if let Some(tx) = slot.take() {
                    let _ = tx.send(());
                }
            }
        }
    }

    /// Number of arrivals seen while armed.
    pub fn reached(&self) -> usize {
        self.point().state.lock().map(|s| s.reached).unwrap_or(0)
    }
}
