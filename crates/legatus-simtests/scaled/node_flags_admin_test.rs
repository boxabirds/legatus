//! Story 136 integration tests with the real binary and a real file holding FIX-337 and FIX-015.
//! The admin transport (story 159) and the Responses route (story 131) do not exist yet, so the
//! node view is read from the file through the library, and the binary is checked for its
//! warning output and for serving.
use legatus_proxy::config::node::TriState;
use legatus_proxy::config::read::read_registry;
use legatus_proxy::config::registry::WarningCode;
use legatus_proxy::lifecycle::start::{publish_node_views, AdminNodeViews};
use std::io::Read;
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

const NODE_FLAGS: &str = include_str!("../fixtures/registry/node_flags.yaml");
const WAIT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(50);

fn registry_file() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("legatus-node-flags-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("registry.yaml");
    std::fs::write(&file, NODE_FLAGS).unwrap();
    file
}

#[test]
fn tc20_the_node_view_lists_each_node_with_its_flags_cache_flags_and_warnings() {
    let loaded = read_registry(&registry_file()).expect("the FIX-337 registry loads");
    let state = AdminNodeViews::default();
    publish_node_views(&loaded, &state);
    let views = state.get();
    let by_name = |n: &str| views.iter().find(|v| v.name.0 == n).unwrap_or_else(|| panic!("no view for {n}"));
    let codex = by_name("codex-capable");
    assert_eq!((codex.responses, codex.prefix_caching, codex.speculative_decoding), (true, TriState::True, TriState::Unknown));
    assert!(codex.warnings.is_empty());
    let chat = by_name("chat-only");
    assert_eq!((chat.responses, chat.prefix_caching.as_str(), chat.speculative_decoding.as_str()), (false, "unknown", "unknown"));
    let mlx = by_name("mlx-claims-responses");
    assert_eq!((mlx.responses, mlx.speculative_decoding), (true, TriState::False));
    assert_eq!(mlx.warnings, vec![WarningCode::ResponsesEngineMismatch]);
    assert_eq!(views.len(), 3);
}

#[test]
fn tc21_the_binary_loads_the_mlx_responses_claim_with_one_warning_and_keeps_serving() {
    let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);
    let exe = std::env::current_exe().unwrap();
    let bin = exe.parent().and_then(|p| p.parent()).unwrap().join("legatus");
    // Same feature set as the registry binary test, so parallel builds do not swap the binary.
    let built = Command::new("cargo").args(["build", "-p", "legatus-proxy", "--bin", "legatus", "--features", "test-hooks"]).status().unwrap();
    assert!(built.success());
    let mut child = Command::new(bin)
        .arg("--registry")
        .arg(registry_file())
        .env("LEGATUS_LISTEN", format!("127.0.0.1:{port}"))
        .stderr(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let mut waited = Duration::ZERO;
    while TcpStream::connect(("127.0.0.1", port)).is_err() && waited < WAIT {
        std::thread::sleep(POLL);
        waited += POLL;
    }
    let serving = TcpStream::connect(("127.0.0.1", port)).is_ok();
    let _ = child.kill();
    let _ = child.wait();
    let mut stderr = String::new();
    child.stderr.take().unwrap().read_to_string(&mut stderr).unwrap();
    assert!(serving, "{stderr}");
    assert!(stderr.contains("warning responses_engine_mismatch at nodes.mlx-claims-responses.responses: The engine answers 404 on the Responses path."), "{stderr}");
    assert!(stderr.contains("(1 warnings)"), "{stderr}");
}
