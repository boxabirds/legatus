//! Start of the process: command line, listen address, bind, registry load, serve (story 121).
//! Story 119 adds the full start state machine; stories 168 and 179 add bind mapping and reload.
use crate::config::node::NodeConfigView;
use crate::config::read::{print_errors, read_registry};
use crate::config::registry::{LoadedRegistry, RegistryWarning};
use crate::config::settings::{setting_views, SettingView};
use crate::config::typed::{Registry, RegistryHandle};
use crate::config::validate::{report_warnings, WarningSink};
use crate::deps::{build_router, RouterDeps};
use crate::lifecycle::exit::{EXIT_BIND_FAILED, EXIT_OK, EXIT_REGISTRY_INVALID};
use crate::net::hyper_transport::HyperTransport;
use crate::net::listen;
use crate::net::wall_system::SystemWallClock;
use crate::obs::log_sink::{DiscardSink, LogRecord, LogSink, SystemRecord};
use crate::sim::SimPoints;
use crate::Seams;
use legatus_common::event::SystemEventKind;
use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::watch;

const USAGE: &str = "usage: legatus --registry <path>";
const REGISTRY_FLAG: &str = "--registry";
/// Environment override of the listen address (it wins over `settings.listen`).
pub const LISTEN_ENV: &str = "LEGATUS_LISTEN";
/// How long a failed first load waits for the requests it held to be answered.
const FAILED_LOAD_DRAIN: Duration = Duration::from_secs(2);

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
    state.replace(crate::config::node::node_config_views(&loaded.nodes, &loaded.warnings));
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
            let _ = writeln!(err, "legatus: registry file {} has {} error(s)", path.display(), errors.len());
            print_errors(&errors, err);
            Err(EXIT_REGISTRY_INVALID)
        }
    }
}

/// The registry path from `--registry <path>`; `None` for any other command line.
fn parse_args(args: &[String]) -> Option<PathBuf> {
    match args {
        [_, flag, path] if flag == REGISTRY_FLAG => Some(PathBuf::from(path)),
        _ => None,
    }
}

/// The listen address from `settings.listen`, read without running the checks. `None` when the
/// file cannot be read, parsed, or holds no usable address.
pub fn listen_peek(path: &Path) -> Option<SocketAddr> {
    let bytes = crate::net::file_system::read_file_bytes(path).ok()?;
    let text = String::from_utf8(bytes).ok()?;
    let value: yaml_serde::Value = yaml_serde::from_str(&text).ok()?;
    value.as_mapping()?.get("settings")?.as_mapping()?.get("listen")?.as_str()?.parse().ok()
}

/// Where requests wait while the registry loads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateState {
    Loading,
    Ready,
    Failed,
}

/// Holds requests until the first load ends. A failed load answers them with 503 `starting`.
pub struct GateHooks {
    state: watch::Receiver<GateState>,
}

/// The start gate: the sender settles it, the hooks hold requests until it is settled.
pub fn start_gate() -> (watch::Sender<GateState>, GateHooks) {
    let (sender, state) = watch::channel(GateState::Loading);
    (sender, GateHooks { state })
}

#[async_trait::async_trait]
impl crate::protocol::chat::PipelineHooks for GateHooks {
    async fn accept_and_auth(&self, _protocol: legatus_common::protocol::Protocol, _headers: &http::HeaderMap) -> crate::protocol::chat::HookOutcome {
        use crate::protocol::chat::HookOutcome;
        let mut state = self.state.clone();
        let settled = state.wait_for(|s| *s != GateState::Loading).await.map(|s| *s);
        match settled {
            Ok(GateState::Ready) => HookOutcome::Continue,
            _ => HookOutcome::Refuse(crate::protocol::errors::RefusalKind::Starting),
        }
    }
}

/// The process entry. Returns the exit code; the binary only calls this.
pub fn main_entry(args: Vec<String>) -> ExitCode {
    let code = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(runtime) => runtime.block_on(run(args)),
        Err(e) => {
            eprintln!("legatus: cannot start the runtime: {e}");
            crate::lifecycle::exit::EXIT_REGISTRY_INVALID
        }
    };
    ExitCode::from(code)
}

async fn run(args: Vec<String>) -> u8 {
    let Some(path) = parse_args(&args) else {
        eprintln!("{USAGE}");
        return EXIT_REGISTRY_INVALID;
    };
    let from_env = std::env::var(LISTEN_ENV).ok().and_then(|v| v.parse::<SocketAddr>().ok());
    let Some(addr) = from_env.or_else(|| listen_peek(&path)) else {
        // No usable listen address: the full loader says why, and nothing is bound.
        let log: Arc<dyn LogSink> = Arc::new(DiscardSink);
        let reporter = LoadReporter { out: Mutex::new(Box::new(std::io::stderr())), log, state: Arc::new(AdminWarnings::default()) };
        return match load_registry(&path, &mut std::io::stderr(), &reporter) {
            Err(code) => code,
            Ok(_) => {
                eprintln!("legatus: the registry has no usable settings.listen address");
                EXIT_REGISTRY_INVALID
            }
        };
    };
    let listener = match listen::bind(addr).await {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!("cannot bind {addr}: {e}");
            return EXIT_BIND_FAILED;
        }
    };
    let (gate_tx, gate_hooks) = start_gate();
    let log: Arc<dyn LogSink> = Arc::new(DiscardSink);
    let transport = Arc::new(HyperTransport::unconfigured());
    let seams = Seams { wall: Arc::new(SystemWallClock), transport: transport.clone(), log: log.clone(), sim: SimPoints::new() };
    let registry = Arc::new(RegistryHandle::empty());
    let deps = RouterDeps::for_test(seams).with_registry(registry.clone()).with_hooks(Arc::new(gate_hooks));
    let (stop_tx, mut stop_rx) = watch::channel(false);
    let router = build_router(deps);
    let server = tokio::spawn(listen::serve(listener, router, async move {
        let _ = stop_rx.wait_for(|stop| *stop).await;
    }));
    let reporter = LoadReporter { out: Mutex::new(Box::new(std::io::stderr())), log, state: Arc::new(AdminWarnings::default()) };
    match load_registry(&path, &mut std::io::stderr(), &reporter) {
        Ok(loaded) => {
            transport.configure(Duration::from_secs(u64::from(loaded.settings.upstream_idle_reuse_max_s)));
            registry.store(Arc::new(Registry::from_loaded(&loaded, 1)));
            publish_node_views(&loaded, &AdminNodeViews::default());
            publish_setting_views(&loaded, &AdminSettingViews::default());
            install_reload_hook();
            let _ = gate_tx.send(GateState::Ready);
            eprintln!("registry loaded: {} ({} warnings)", path.display(), loaded.warnings.len());
            eprintln!("legatus listening on {addr}");
            match server.await {
                Ok(Ok(())) => EXIT_OK,
                _ => crate::lifecycle::exit::EXIT_REGISTRY_INVALID,
            }
        }
        Err(code) => {
            let _ = gate_tx.send(GateState::Failed);
            let _ = tokio::time::timeout(FAILED_LOAD_DRAIN, async {
                tokio::task::yield_now().await;
                let _ = stop_tx.send(true);
                let _ = server.await;
            })
            .await;
            code
        }
    }
}

/// The place where story 179 plugs in the reload on SIGHUP. Nothing reloads yet.
fn install_reload_hook() {}
