//! Reload on the hang-up signal (contract C13, story 179): validate the whole new file, keep the
//! old registry when it is wrong, swap in one step, let a running request keep its record, and
//! tell the observers what changed.
use crate::config::diff::{diff_registries, RegistryDiff};
use crate::config::read::{read_registry_from, RegistrySource};
use crate::config::registry::*;
use crate::config::typed::{Registry, RegistryHandle};
use crate::obs::log_sink::{LogRecord, LogSink, SystemRecord};
use crate::sim::{sim_point, SimPointName, SimPoints};
use crate::time::WallClock;
use async_trait::async_trait;
use legatus_common::event::SystemEventKind;
use std::io::Write;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex, RwLock};

/// Settings that bind at start. A reload keeps the running value and warns (PRX-REG-025).
pub const RESTART_REQUIRED_KEYS: &[&str] = &["listen", "admin.listen", "admin.token_file", "log_dir", "log_queue_capacity", "state_dir"];

pub const TEXT_RESTART_REQUIRED: &str = "The change needs a restart; the running value stays in use.";

/// Something that asks for a reload. `false` means no trigger will ever come again.
#[async_trait]
pub trait ReloadTrigger: Send {
    async fn next(&mut self) -> bool;
}

/// The real trigger: the hang-up signal. Nothing else (no file watch, no timer) starts a reload.
#[cfg(unix)]
pub struct SighupTrigger(tokio::signal::unix::Signal);

#[cfg(unix)]
impl SighupTrigger {
    pub fn new() -> std::io::Result<SighupTrigger> {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup()).map(SighupTrigger)
    }
}

#[cfg(unix)]
#[async_trait]
impl ReloadTrigger for SighupTrigger {
    async fn next(&mut self) -> bool {
        self.0.recv().await.is_some()
    }
}

/// A trigger for tests. A burst of `send` calls while a reload runs keeps one follow-up.
pub struct ManualTrigger(tokio::sync::mpsc::Receiver<()>);

/// The sending half of a `ManualTrigger`.
#[derive(Clone)]
pub struct TriggerSender(tokio::sync::mpsc::Sender<()>);

impl TriggerSender {
    /// Ask for a reload; a request that is already waiting absorbs this one.
    pub fn send(&self) {
        let _ = self.0.try_send(());
    }
}

pub fn manual_trigger() -> (ManualTrigger, TriggerSender) {
    let (sender, receiver) = tokio::sync::mpsc::channel(1);
    (ManualTrigger(receiver), TriggerSender(sender))
}

#[async_trait]
impl ReloadTrigger for ManualTrigger {
    async fn next(&mut self) -> bool {
        self.0.recv().await.is_some()
    }
}

/// Called after a swap, in the order of registration. Gets shared references only.
pub trait ReloadObserver: Send + Sync {
    fn on_reload(&self, old: &Registry, new: &Registry, diff: &RegistryDiff);
}

/// The observers, notified in the fixed order they were registered in (the table of the design:
/// secrets, client tokens, cap counter, seat table, availability, modes, affinity table, hold
/// queues, response ids, probe controller).
#[derive(Default)]
pub struct ReloadHub {
    observers: RwLock<Vec<Arc<dyn ReloadObserver>>>,
}

impl ReloadHub {
    pub fn new() -> ReloadHub {
        ReloadHub::default()
    }

    pub fn register(&self, observer: Arc<dyn ReloadObserver>) {
        match self.observers.write() {
            Ok(mut list) => list.push(observer),
            Err(poisoned) => poisoned.into_inner().push(observer),
        }
    }

    /// Call every observer. One that panics is caught: the swap stands and the others still run.
    pub fn notify(&self, old: &Registry, new: &Registry, diff: &RegistryDiff) {
        let list: Vec<Arc<dyn ReloadObserver>> = match self.observers.read() {
            Ok(list) => list.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        };
        for observer in list {
            let _ = catch_unwind(AssertUnwindSafe(|| observer.on_reload(old, new, diff)));
        }
    }
}

/// The last reload result, for the admin read.
#[derive(Default)]
pub struct ReloadState(Mutex<Option<ReloadResult>>);

impl ReloadState {
    pub fn get(&self) -> ReloadResult {
        self.0.lock().ok().and_then(|g| g.clone()).unwrap_or(ReloadResult::NoneYet)
    }
    fn set(&self, result: ReloadResult) {
        if let Ok(mut held) = self.0.lock() {
            *held = Some(result);
        }
    }
}

/// What a reloader works with. Grouped so the constructor stays small.
pub struct ReloadParts {
    pub source: Arc<dyn RegistrySource>,
    pub handle: Arc<RegistryHandle>,
    pub hub: Arc<ReloadHub>,
    pub wall: Arc<dyn WallClock>,
    pub sink: Arc<dyn LogSink>,
    pub sim: SimPoints,
    pub state: Arc<ReloadState>,
    /// Where the one-line results and warnings are printed.
    pub out: Box<dyn Write + Send>,
}

pub struct Reloader {
    source: Arc<dyn RegistrySource>,
    handle: Arc<RegistryHandle>,
    hub: Arc<ReloadHub>,
    wall: Arc<dyn WallClock>,
    sink: Arc<dyn LogSink>,
    sim: SimPoints,
    state: Arc<ReloadState>,
    out: Mutex<Box<dyn Write + Send>>,
}

impl Reloader {
    pub fn new(parts: ReloadParts) -> Reloader {
        Reloader { source: parts.source, handle: parts.handle, hub: parts.hub, wall: parts.wall, sink: parts.sink, sim: parts.sim, state: parts.state, out: Mutex::new(parts.out) }
    }

    /// Run one reload for each trigger, one at a time, until the trigger closes. A cycle that
    /// panics leaves the old registry in use and waits for the next trigger.
    pub async fn run(self: Arc<Self>, mut trigger: impl ReloadTrigger) {
        while trigger.next().await {
            let this = self.clone();
            let cycle = tokio::spawn(async move { this.reload_once().await });
            if cycle.await.is_err() {
                self.say("registry reload failed internally; the old registry stays in use");
            }
        }
    }

    fn say(&self, line: &str) {
        if let Ok(mut out) = self.out.lock() {
            let _ = writeln!(out, "{line}");
        }
    }

    fn event(&self, kind: SystemEventKind) {
        let _ = self.sink.offer(LogRecord::System(SystemRecord { kind }));
    }

    /// One reload cycle. The handle changes only at the swap, and only for a valid file whose
    /// content differs.
    pub async fn reload_once(&self) -> ReloadResult {
        let old = self.handle.snapshot();
        let loaded = match read_registry_from(self.source.as_ref()) {
            Ok(loaded) => loaded,
            Err(errors) => {
                self.say(&format!("registry reload rejected: {} error(s), the old registry stays in use", errors.len()));
                for error in &errors {
                    self.say(&error.to_string());
                }
                self.event(SystemEventKind::RegistryRejected);
                let result = ReloadResult::Rejected { at: self.wall.now(), errors };
                self.state.set(result.clone());
                return result;
            }
        };
        sim_point(&self.sim, SimPointName::ReloadApply).await;
        let mut next = Registry::from_loaded(&loaded, old.generation + 1);
        let changed_keys = next.settings.pin_restart_keys(&old.settings);
        for key in &changed_keys {
            next.warnings.push(warning_fixed(WarningCode::RestartRequired, &format!("settings.{key}"), TEXT_RESTART_REQUIRED));
        }
        for warning in &next.warnings {
            self.say(&warning.to_string());
        }
        let diff = diff_registries(&old, &next);
        let at = self.wall.now();
        if diff.is_empty() {
            self.say(&format!("registry reloaded: unchanged (generation {})", old.generation));
            self.event(SystemEventKind::RegistryLoaded);
            let result = ReloadResult::Loaded { generation: old.generation, at, nodes: old.nodes.len(), aliases: old.aliases.len(), changed: false };
            self.state.set(result.clone());
            return result;
        }
        let (nodes, aliases, generation) = (next.nodes.len(), next.aliases.len(), next.generation);
        let next = Arc::new(next);
        self.handle.store(next.clone());
        self.hub.notify(&old, &next, &diff);
        self.say(&format!("registry reloaded: generation {generation} ({nodes} nodes, {aliases} aliases)"));
        self.event(SystemEventKind::RegistryLoaded);
        for _ in &next.warnings {
            self.event(SystemEventKind::Warning);
        }
        let result = ReloadResult::Loaded { generation, at, nodes, aliases, changed: true };
        self.state.set(result.clone());
        result
    }
}
