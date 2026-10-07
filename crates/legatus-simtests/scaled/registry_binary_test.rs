//! Story 125 integration tests: the real `legatus` binary on a bad and a good registry file.
use std::io::Read;
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::Duration;

const SPEC_EXAMPLE: &str = include_str!("../fixtures/registry/spec_example.yaml");
const BIN: &str = "legatus";
const EXIT_REGISTRY_INVALID: i32 = 2;
/// How long the test waits for the binary to exit or to open its port.
const WAIT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(50);

fn binary() -> PathBuf {
    static BUILT: OnceLock<PathBuf> = OnceLock::new();
    BUILT
        .get_or_init(|| {
            // Always run cargo: a stale binary would test old code. The test-hooks feature adds the
            // warning check that the warning test switches on with an environment variable.
            let status = Command::new("cargo").args(["build", "-p", "legatus-proxy", "--bin", BIN, "--features", "test-hooks"]).status().expect("run cargo build");
            assert!(status.success());
            let exe = std::env::current_exe().unwrap();
            exe.parent().and_then(|p| p.parent()).unwrap().join(BIN)
        })
        .clone()
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn write_registry(name: &str, text: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("legatus-bin-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("registry.yaml");
    std::fs::write(&file, text).unwrap();
    file
}

fn start(registry: Option<&PathBuf>, port: u16) -> Child {
    let mut command = Command::new(binary());
    if let Some(path) = registry {
        command.arg("--registry").arg(path);
    }
    command.env("LEGATUS_LISTEN", format!("127.0.0.1:{port}")).stderr(Stdio::piped()).stdout(Stdio::null()).spawn().expect("spawn legatus")
}

fn wait_exit(child: &mut Child) -> Option<i32> {
    let mut waited = Duration::ZERO;
    while waited < WAIT {
        if let Some(status) = child.try_wait().unwrap() {
            return status.code();
        }
        std::thread::sleep(POLL);
        waited += POLL;
    }
    None
}

fn port_open(port: u16) -> bool {
    TcpStream::connect(("127.0.0.1", port)).is_ok()
}

fn read_stderr(child: &mut Child) -> String {
    let mut text = String::new();
    child.stderr.take().unwrap().read_to_string(&mut text).unwrap();
    text
}

#[test]
fn tc13_an_invalid_registry_exits_with_status_2_prints_the_lines_and_leaves_no_port_open() {
    let port = free_port();
    let file = write_registry("bad", "version: 2\nnodes: {}\naliases: {}\nzzz: 1\n");
    let mut child = start(Some(&file), port);
    assert_eq!(wait_exit(&mut child), Some(EXIT_REGISTRY_INVALID));
    let stderr = read_stderr(&mut child);
    assert!(stderr.contains("registry error bad_version at version: Schema version 1 is the only version."), "{stderr}");
    assert!(stderr.contains("registry error unknown_field at zzz: Field is not part of the schema."), "{stderr}");
    assert!(stderr.contains("registry rejected: 2 errors"), "{stderr}");
    assert!(!port_open(port), "the port is closed after the exit");
}

#[test]
fn tc13_a_missing_registry_argument_and_a_missing_file_both_exit_with_status_2() {
    let port = free_port();
    let mut no_arg = start(None, port);
    assert_eq!(wait_exit(&mut no_arg), Some(EXIT_REGISTRY_INVALID));
    let missing = std::env::temp_dir().join("legatus-bin-absent").join("none.yaml");
    let mut no_file = start(Some(&missing), port);
    assert_eq!(wait_exit(&mut no_file), Some(EXIT_REGISTRY_INVALID));
    assert!(read_stderr(&mut no_file).contains("registry error file_unreadable"));
    assert!(!port_open(port));
}

#[test]
fn w1_fix_the_file_and_the_binary_starts_and_says_so() {
    let port = free_port();
    let file = write_registry("fixed", "version: 1\nnodes: {}\naliases: {}\nzzz: 1\n");
    let mut bad = start(Some(&file), port);
    assert_eq!(wait_exit(&mut bad), Some(EXIT_REGISTRY_INVALID));

    std::fs::write(&file, SPEC_EXAMPLE).unwrap();
    let mut good = start(Some(&file), port);
    let mut waited = Duration::ZERO;
    while !port_open(port) && waited < WAIT {
        std::thread::sleep(POLL);
        waited += POLL;
    }
    let open = port_open(port);
    let _ = good.kill();
    let _ = good.wait();
    assert!(open, "the binary serves after the file is fixed");
    let stderr = read_stderr(&mut good);
    assert!(stderr.contains("registry loaded:") && stderr.contains("(0 warnings)"), "{stderr}");
}

const WARNING_PATH_ENV: &str = "LEGATUS_TEST_WARNING_PATH";
const WARNING_PATH: &str = "nodes.strix-qwen.engine_flags.np";

#[test]
fn tc14_a_valid_file_with_a_warning_starts_and_the_warning_is_printed_and_counted() {
    let port = free_port();
    let file = write_registry("warn", SPEC_EXAMPLE);
    let mut command = Command::new(binary());
    command.arg("--registry").arg(&file).env("LEGATUS_LISTEN", format!("127.0.0.1:{port}")).env(WARNING_PATH_ENV, WARNING_PATH);
    let mut child = command.stderr(Stdio::piped()).stdout(Stdio::null()).spawn().unwrap();
    let mut waited = Duration::ZERO;
    while !port_open(port) && waited < WAIT {
        std::thread::sleep(POLL);
        waited += POLL;
    }
    let open = port_open(port);
    let _ = child.kill();
    let _ = child.wait();
    assert!(open, "a warning does not stop the start");
    let stderr = read_stderr(&mut child);
    assert!(stderr.contains(&format!("warning flag_mismatch at {WARNING_PATH}: Declared value differs from the tested one.")), "{stderr}");
    assert!(stderr.contains("(1 warnings)"), "{stderr}");
}
