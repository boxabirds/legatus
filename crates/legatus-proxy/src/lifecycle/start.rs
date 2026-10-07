//! Registry loading at start. Story 121 adds the rest of the start sequence and owns this file.
use crate::config::read::{print_errors, read_registry};
use crate::config::registry::{LoadedRegistry, RegistryWarning};
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
