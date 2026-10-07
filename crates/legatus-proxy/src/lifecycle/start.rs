//! Registry loading at start. Story 121 adds the rest of the start sequence and owns this file.
use crate::config::node::{node_config_views, NodeConfigView};
use crate::config::read::{print_errors, read_registry};
use crate::config::registry::{LoadedRegistry, RegistryWarning};
use crate::config::settings::{setting_views, SettingView};
use crate::config::validate::{report_warnings, WarningSink};
use crate::lifecycle::exit::EXIT_REGISTRY_INVALID;
use crate::obs::log_sink::{LogRecord, LogSink, SystemRecord};
use legatus_common::event::SystemEventKind;
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// The load warnings that the admin read serves (story 159).
#[derive(Default)]
pub struct AdminWarnings(Mutex<Vec<RegistryWarning>>);

impl AdminWarnings {
    pub fn replace(&self, all: &[RegistryWarning]) {
        if let Ok(mut held) = self.0.lock() {
            *held = all.to_vec();
        }
    }
    pub fn get(&self) -> Vec<RegistryWarning> {
        self.0.lock().map(|held| held.clone()).unwrap_or_default()
    }
}

/// The node view that the admin read serves (story 159).
#[derive(Default)]
pub struct AdminNodeViews(Mutex<Vec<NodeConfigView>>);

impl AdminNodeViews {
    pub fn replace(&self, views: Vec<NodeConfigView>) {
        if let Ok(mut held) = self.0.lock() {
            *held = views;
        }
    }
    pub fn get(&self) -> Vec<NodeConfigView> {
        self.0.lock().map(|held| held.clone()).unwrap_or_default()
    }
}

/// The setting view that the admin read serves at `/v1/status` (story 159).
#[derive(Default)]
pub struct AdminSettingViews(Mutex<Vec<SettingView>>);

impl AdminSettingViews {
    pub fn replace(&self, views: Vec<SettingView>) {
        if let Ok(mut held) = self.0.lock() {
            *held = views;
        }
    }
    pub fn get(&self) -> Vec<SettingView> {
        self.0.lock().map(|held| held.clone()).unwrap_or_default()
    }
}

/// Build the setting views from a loaded registry and hand them to the admin state.
pub fn publish_setting_views(loaded: &LoadedRegistry, state: &AdminSettingViews) {
    state.replace(setting_views(&loaded.settings));
}

/// Build the node views from a loaded registry and hand them to the admin state.
pub fn publish_node_views(loaded: &LoadedRegistry, state: &AdminNodeViews) {
    state.replace(node_config_views(&loaded.nodes, &loaded.warnings));
}

/// Shows each warning on the terminal, writes one warning event and stores the list.
pub struct LoadReporter {
    pub out: Mutex<Box<dyn Write + Send>>,
    pub log: Arc<dyn LogSink>,
    pub state: Arc<AdminWarnings>,
}

impl WarningSink for LoadReporter {
    fn warn(&self, w: &RegistryWarning) {
        if let Ok(mut out) = self.out.lock() {
            let _ = writeln!(out, "{w}");
        }
        let _ = self.log.offer(LogRecord::System(SystemRecord { kind: SystemEventKind::Warning }));
    }
    fn done(&self, all: &[RegistryWarning]) {
        self.state.replace(all);
    }
}

/// Load the registry or give the exit code. Errors go to `err`, one line each, then the count.
pub fn load_registry(path: &Path, err: &mut dyn Write, sink: &dyn WarningSink) -> Result<LoadedRegistry, u8> {
    match read_registry(path) {
        Ok(loaded) => {
            report_warnings(&loaded.warnings, sink);
            Ok(loaded)
        }
        Err(errors) => {
            print_errors(&errors, err);
            Err(EXIT_REGISTRY_INVALID)
        }
    }
}
