//! Story 121 tests of the start entry with the real binary: command line, listen address, bind,
//! and a request that arrives while the registry is still loading.
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

const EXIT_REGISTRY_INVALID: i32 = 2;
const EXIT_BIND_FAILED: i32 = 3;
const WAIT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(50);
const HELD_FOR: Duration = Duration::from_millis(400);
const NODE_ANSWER: &str = "{\"id\":\"1\",\"choices\":[]}";
const REQUEST_BODY: &str = "{\"model\":\"local-coder\",\"messages\":[]}";

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("legatus-start-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(args: &[&str], env_listen: Option<u16>) -> (Option<i32>, String) {
    let mut command = Command::new(crate::legatus_bin::legatus_binary());
    command.args(args).env_remove("LEGATUS_LISTEN").stderr(Stdio::piped()).stdout(Stdio::null());
    if let Some(port) = env_listen {
        command.env("LEGATUS_LISTEN", format!("127.0.0.1:{port}"));
    }
    let mut child = command.spawn().unwrap();
    let code = wait_exit(&mut child);
    let mut stderr = String::new();
    child.stderr.take().unwrap().read_to_string(&mut stderr).unwrap();
    (code, stderr)
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
    let _ = child.kill();
    None
}

#[test]
fn tc25_bad_arguments_print_the_usage_and_exit_2() {
    for args in [vec![], vec!["--registry"], vec!["registry.yaml"], vec!["--registry", "a", "b"], vec!["--other", "a"]] {
        let (code, stderr) = run(&args, None);
        assert_eq!(code, Some(EXIT_REGISTRY_INVALID), "{args:?}");
        assert!(stderr.contains("usage: legatus --registry <path>"), "{args:?}: {stderr}");
    }
}

#[test]
fn tc25_a_missing_file_prints_the_error_exits_2_and_never_listens() {
    let file = scratch("missing").join("absent.yaml");
    let (code, stderr) = run(&["--registry", file.to_str().unwrap()], None);
    assert_eq!(code, Some(EXIT_REGISTRY_INVALID));
    assert!(stderr.contains("registry error file_unreadable"), "{stderr}");
    assert!(!stderr.contains("listening"), "{stderr}");
}

#[test]
fn tc25_a_file_without_a_usable_listen_address_prints_the_errors_exits_2_and_never_listens() {
    let dir = scratch("nolisten");
    let broken = dir.join("broken.yaml");
    std::fs::write(&broken, "version: 1\nnodes: {}\naliases: {}\nzzz: 1\n").unwrap();
    let (code, stderr) = run(&["--registry", broken.to_str().unwrap()], None);
    assert_eq!(code, Some(EXIT_REGISTRY_INVALID));
    assert!(stderr.contains("legatus: registry file") && stderr.contains("registry error unknown_field at zzz"), "{stderr}");
    assert!(!stderr.contains("listening"));
    let valid = dir.join("valid.yaml");
    std::fs::write(&valid, "version: 1\nnodes: {}\naliases: {}\n").unwrap();
    let (code, stderr) = run(&["--registry", valid.to_str().unwrap()], None);
    assert_eq!(code, Some(EXIT_REGISTRY_INVALID));
    assert!(stderr.contains("no usable settings.listen address"), "{stderr}");
}

#[test]
fn tc25_a_port_in_use_prints_cannot_bind_and_exits_3() {
    let taken = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = taken.local_addr().unwrap().port();
    let file = scratch("busy").join("registry.yaml");
    std::fs::write(&file, format!("version: 1\nsettings:\n  listen: 127.0.0.1:{port}\nnodes: {{}}\naliases: {{}}\n")).unwrap();
    let (code, stderr) = run(&["--registry", file.to_str().unwrap()], None);
    assert_eq!(code, Some(EXIT_BIND_FAILED));
    assert!(stderr.contains(&format!("cannot bind 127.0.0.1:{port}:")), "{stderr}");
    drop(taken);
}

/// A named pipe as the registry path: the binary blocks reading it until the test writes, so the
/// time of the load is under the test's control and the order is exact.
struct HeldLoad {
    child: Child,
    fifo: PathBuf,
    port: u16,
}

fn start_with_held_load(name: &str) -> HeldLoad {
    let fifo = scratch(name).join("registry.fifo");
    assert!(Command::new("mkfifo").arg(&fifo).status().unwrap().success());
    let port = free_port();
    let child = Command::new(crate::legatus_bin::legatus_binary()).arg("--registry").arg(&fifo).env("LEGATUS_LISTEN", format!("127.0.0.1:{port}")).stderr(Stdio::piped()).stdout(Stdio::null()).spawn().unwrap();
    let mut waited = Duration::ZERO;
    while TcpStream::connect(("127.0.0.1", port)).is_err() && waited < WAIT {
        std::thread::sleep(POLL);
        waited += POLL;
    }
    HeldLoad { child, fifo, port }
}

fn send_request(port: u16) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream.set_read_timeout(Some(WAIT)).unwrap();
        let request = format!("POST /v1/chat/completions HTTP/1.1\r\nHost: proxy\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{REQUEST_BODY}", REQUEST_BODY.len());
        stream.write_all(request.as_bytes()).unwrap();
        let mut reply = String::new();
        let _ = stream.read_to_string(&mut reply);
        reply
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tc26_a_request_received_while_the_registry_loads_waits_and_is_served_when_the_load_is_valid() {
    use axum::routing::post;
    let node = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let node_port = node.local_addr().unwrap().port();
    let app = axum::Router::new().route("/v1/chat/completions", post(|| async { NODE_ANSWER }));
    tokio::spawn(async move { axum::serve(node, app).await.unwrap() });

    let mut held = start_with_held_load("valid");
    let reply = send_request(held.port);
    std::thread::sleep(HELD_FOR);
    assert!(!reply.is_finished(), "the request waits while the registry is read");
    let registry = format!("version: 1\nnodes:\n  n1:\n    engine: {{ name: llama-server }}\n    model: qwen-node\n    endpoints: [ {{ protocol: openai-chat, base_url: \"http://127.0.0.1:{node_port}\" }} ]\naliases:\n  local-coder: {{ nodes: [n1] }}\n");
    std::fs::write(&held.fifo, registry).unwrap();
    let reply = reply.join().unwrap();
    assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
    assert!(reply.contains(NODE_ANSWER), "{reply}");
    let _ = held.child.kill();
    let _ = held.child.wait();
}

#[test]
fn tc26_a_first_load_that_fails_answers_the_held_request_with_503_starting_prints_the_errors_and_exits_2() {
    let mut held = start_with_held_load("invalid");
    let reply = send_request(held.port);
    std::thread::sleep(HELD_FOR);
    assert!(!reply.is_finished());
    std::fs::write(&held.fifo, "version: 3\nnodes: {}\naliases: {}\n").unwrap();
    let reply = reply.join().unwrap();
    assert!(reply.starts_with("HTTP/1.1 503"), "{reply}");
    assert!(reply.contains("\"code\":\"starting\""), "{reply}");
    assert_eq!(wait_exit(&mut held.child), Some(EXIT_REGISTRY_INVALID));
    let mut stderr = String::new();
    held.child.stderr.take().unwrap().read_to_string(&mut stderr).unwrap();
    assert!(stderr.contains("legatus: registry file") && stderr.contains("registry error bad_version at version"), "{stderr}");
}

#[test]
fn a_valid_file_with_a_listen_address_serves_without_the_environment_override() {
    let port = free_port();
    let file = scratch("serve").join("registry.yaml");
    std::fs::write(&file, format!("version: 1\nsettings:\n  listen: 127.0.0.1:{port}\nnodes: {{}}\naliases: {{}}\n")).unwrap();
    let mut child = Command::new(crate::legatus_bin::legatus_binary()).arg("--registry").arg(&file).env_remove("LEGATUS_LISTEN").stderr(Stdio::piped()).stdout(Stdio::null()).spawn().unwrap();
    let mut waited = Duration::ZERO;
    while TcpStream::connect(("127.0.0.1", port)).is_err() && waited < WAIT {
        std::thread::sleep(POLL);
        waited += POLL;
    }
    let up = TcpStream::connect(("127.0.0.1", port)).is_ok();
    let _ = child.kill();
    let _ = child.wait();
    assert!(up);
}

#[test]
fn listen_peek_reads_the_address_without_running_the_checks() {
    use legatus_proxy::lifecycle::start::listen_peek;
    let dir = scratch("peek");
    let write = |name: &str, text: &str| {
        let file = dir.join(name);
        std::fs::write(&file, text).unwrap();
        file
    };
    // A file with other mistakes still gives its address.
    let wrong = write("wrong.yaml", "version: 9\nsettings:\n  listen: 127.0.0.1:4242\n  hold_limt_s: 1\n");
    assert_eq!(listen_peek(&wrong), Some("127.0.0.1:4242".parse().unwrap()));
    assert_eq!(listen_peek(&write("none.yaml", "version: 1\nnodes: {}\n")), None);
    assert_eq!(listen_peek(&write("badaddr.yaml", "settings:\n  listen: nowhere\n")), None);
    assert_eq!(listen_peek(&write("notyaml.yaml", "settings: [unterminated")), None);
    assert_eq!(listen_peek(&dir.join("absent.yaml")), None);
}
