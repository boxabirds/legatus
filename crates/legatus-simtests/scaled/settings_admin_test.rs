//! Story 165 integration tests with the real binary and a real file. The admin transport
//! (`/v1/status`, story 159) and the event log file (story 122) do not exist yet, so the
//! setting view is read in-process from the file and the warning is read from the error output.
use legatus_proxy::config::read::read_registry;
use legatus_proxy::config::settings::{setting_views, Source};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(50);

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn registry_file(name: &str, hold_limit_s: u32, port: u16) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("legatus-settings-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("registry.yaml");
    let text = format!(
        "version: 1\nsettings:\n  listen: 127.0.0.1:{port}\n  hold_limit_s: {hold_limit_s}\n  protected_window_s: 120\nnodes:\n  a:\n    engine: {{ name: ollama }}\n    model: m\n    endpoints: [ {{ protocol: openai-chat, base_url: \"http://127.0.0.1:1\" }} ]\naliases:\n  x: {{ nodes: [a] }}\n"
    );
    std::fs::write(&file, text).unwrap();
    file
}

/// Start the binary (the environment listen override is removed so that the registry decides),
/// wait for the port, stop it and return what it wrote on the error output.
fn run_binary(file: &PathBuf, port: u16) -> (bool, String) {
    let bin = crate::legatus_bin::legatus_binary();
    let mut child = Command::new(bin).arg("--registry").arg(file).env_remove("LEGATUS_LISTEN").stderr(Stdio::piped()).stdout(Stdio::null()).spawn().unwrap();
    let mut waited = Duration::ZERO;
    while TcpStream::connect(("127.0.0.1", port)).is_err() && waited < WAIT {
        std::thread::sleep(POLL);
        waited += POLL;
    }
    let serving = TcpStream::connect(("127.0.0.1", port)).is_ok();
    let stderr = crate::legatus_bin::read_until_listening(&mut child);
    let _ = child.kill();
    let _ = child.wait();
    (serving, stderr)
}

#[test]
fn tc22_a_partial_file_lists_every_setting_with_value_default_unit_status_range_source_and_restart() {
    let loaded = read_registry(&registry_file("view", 280, 0)).expect("loads");
    let views = setting_views(&loaded.settings);
    assert_eq!(views.len(), 38);
    let hold = views.iter().find(|v| v.name == "hold_limit_s").unwrap();
    assert_eq!((hold.value.as_str(), hold.default.as_str(), hold.source, hold.range.as_str(), hold.restart), ("280", "250", Source::File, "above protected_window_s to 3600", false));
    let window = views.iter().find(|v| v.name == "protected_window_s").unwrap();
    assert_eq!((window.value.as_str(), window.source), ("120", Source::File));
    let ttl = views.iter().find(|v| v.name == "table_ttl_s").unwrap();
    assert_eq!((ttl.value.as_str(), ttl.source), ("600", Source::Default));
    assert!(loaded.warnings.is_empty());
}

#[test]
fn tc22_the_same_file_gives_the_same_view_after_a_restart() {
    let file = registry_file("restart", 280, 0);
    let first = setting_views(&read_registry(&file).unwrap().settings);
    let second = setting_views(&read_registry(&file).unwrap().settings);
    assert_eq!(first, second);
}

#[test]
fn tc22_a_hold_limit_of_300_gives_the_budget_warning_and_the_binary_listens_where_the_registry_says() {
    let port = free_port();
    let file = registry_file("warn", 300, port);
    let (serving, stderr) = run_binary(&file, port);
    assert!(serving, "the binary listens on settings.listen: {stderr}");
    assert!(stderr.contains("warning hold_limit_above_budget at settings.hold_limit_s:"), "{stderr}");
    assert!(stderr.contains("(1 warnings)"), "{stderr}");
}

#[test]
fn w1_lowering_the_hold_limit_to_280_removes_the_warning() {
    let port = free_port();
    let file = registry_file("lower", 280, port);
    let (serving, stderr) = run_binary(&file, port);
    assert!(serving, "{stderr}");
    assert!(!stderr.contains("hold_limit_above_budget"), "{stderr}");
    assert!(stderr.contains("(0 warnings)"), "{stderr}");
}

#[test]
fn tc22_the_setting_views_are_handed_to_the_admin_state_with_the_values_of_the_file() {
    use legatus_proxy::lifecycle::start::{publish_setting_views, AdminSettingViews};
    let loaded = read_registry(&registry_file("state", 280, 0)).unwrap();
    let state = AdminSettingViews::default();
    assert!(state.get().is_empty());
    publish_setting_views(&loaded, &state);
    let held = state.get();
    assert_eq!(held.len(), 38);
    assert_eq!(held.iter().find(|v| v.name == "hold_limit_s").unwrap().value, "280");
    publish_setting_views(&read_registry(&registry_file("state2", 200, 0)).unwrap(), &state);
    assert_eq!(state.get().iter().find(|v| v.name == "hold_limit_s").unwrap().value, "200", "the state is replaced whole");
}
