//! TC-18 of story 179: the real binary, a real file and a real hang-up signal sent to the child.
use axum::routing::post;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(50);

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

async fn node(answer: &'static str) -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let app = axum::Router::new().route("/v1/chat/completions", post(move || async move { answer }));
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    port
}

fn registry(proxy: u16, node_port: u16) -> String {
    format!("version: 1\nsettings:\n  listen: 127.0.0.1:{proxy}\nnodes:\n  n1:\n    engine: {{ name: llama-server }}\n    model: node-model\n    endpoints: [ {{ protocol: openai-chat, base_url: \"http://127.0.0.1:{node_port}\" }} ]\naliases:\n  local-coder: {{ nodes: [n1] }}\n")
}

fn ask(port: u16) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream.set_read_timeout(Some(WAIT)).unwrap();
    let body = "{\"model\":\"local-coder\",\"messages\":[]}";
    write!(stream, "POST /v1/chat/completions HTTP/1.1\r\nHost: proxy\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
    let mut reply = String::new();
    let _ = stream.read_to_string(&mut reply);
    reply
}

fn hang_up(pid: u32) {
    assert!(Command::new("kill").args(["-HUP", &pid.to_string()]).status().unwrap().success());
}

/// Read lines of the child's error output until one contains `needle`.
fn wait_for_line(lines: &std::sync::mpsc::Receiver<String>, needle: &str) -> String {
    let deadline = std::time::Instant::now() + WAIT;
    let mut seen = String::new();
    while std::time::Instant::now() < deadline {
        if let Ok(line) = lines.recv_timeout(POLL) {
            seen.push_str(&line);
            seen.push('\n');
            if line.contains(needle) {
                return seen;
            }
        }
    }
    panic!("no line with {needle:?}; saw: {seen}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tc18_a_valid_then_an_invalid_file_with_the_real_signal_keeps_serving_and_the_process_stays_up() {
    let node_one = node("answer-from-node-one").await;
    let node_two = node("answer-from-node-two").await;
    let proxy_port = free_port();
    let file: PathBuf = std::env::temp_dir().join(format!("legatus-reload-{}", std::process::id())).join("registry.yaml");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, registry(proxy_port, node_one)).unwrap();
    let mut child = Command::new(crate::legatus_bin::legatus_binary()).arg("--registry").arg(&file).env_remove("LEGATUS_LISTEN").stderr(Stdio::piped()).stdout(Stdio::null()).spawn().unwrap();
    let stderr = child.stderr.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            let _ = tx.send(line);
        }
    });
    wait_for_line(&rx, "legatus listening on");
    assert!(ask(proxy_port).contains("answer-from-node-one"));

    // A valid change: the alias now reaches node two.
    std::fs::write(&file, registry(proxy_port, node_two)).unwrap();
    hang_up(child.id());
    wait_for_line(&rx, "registry reloaded: generation 2");
    assert!(ask(proxy_port).contains("answer-from-node-two"), "the new file took effect");

    // An invalid file: nothing of it is applied and the process stays up.
    std::fs::write(&file, "version: 7\nnodes: {}\naliases: {}\n").unwrap();
    hang_up(child.id());
    let lines = wait_for_line(&rx, "registry reload rejected");
    assert!(lines.contains("bad_version") || wait_for_line(&rx, "bad_version").contains("bad_version"));
    assert!(ask(proxy_port).contains("answer-from-node-two"), "requests are served between reloads");
    assert!(child.try_wait().unwrap().is_none(), "the process did not exit");

    // Fixing the file loads it again.
    std::fs::write(&file, registry(proxy_port, node_one)).unwrap();
    hang_up(child.id());
    wait_for_line(&rx, "registry reloaded: generation 3");
    assert!(ask(proxy_port).contains("answer-from-node-one"));
    let _ = child.kill();
    let _ = child.wait();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_edit_without_a_signal_changes_nothing() {
    let node_one = node("answer-from-node-one").await;
    let node_two = node("answer-from-node-two").await;
    let proxy_port = free_port();
    let file = std::env::temp_dir().join(format!("legatus-noreload-{}", std::process::id())).join("registry.yaml");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, registry(proxy_port, node_one)).unwrap();
    let mut child = Command::new(crate::legatus_bin::legatus_binary()).arg("--registry").arg(&file).env_remove("LEGATUS_LISTEN").stderr(Stdio::piped()).stdout(Stdio::null()).spawn().unwrap();
    crate::legatus_bin::read_until_listening(&mut child);
    std::fs::write(&file, registry(proxy_port, node_two)).unwrap();
    std::thread::sleep(Duration::from_secs(2));
    assert!(ask(proxy_port).contains("answer-from-node-one"), "no file watch: the edit is not read");
    let _ = child.kill();
    let _ = child.wait();
}

/// A node that streams its answer slowly: four parts, half a second apart.
async fn slow_node() -> u16 {
    use axum::body::Body;
    use futures_util::stream;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let app = axum::Router::new().route(
        "/v1/chat/completions",
        post(|| async {
            let parts = stream::unfold(0u8, |i| async move {
                if i >= 4 {
                    return None;
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
                Some((Ok::<_, std::io::Error>(bytes::Bytes::from(format!("slow-part-{i};"))), i + 1))
            });
            Body::from_stream(parts)
        }),
    );
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    port
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tc08_a_stream_from_a_node_the_real_reload_removed_finishes_and_a_new_request_uses_the_new_node() {
    let slow = slow_node().await;
    let fast = node("answer-from-node-two").await;
    let proxy_port = free_port();
    let file = std::env::temp_dir().join(format!("legatus-span-{}", std::process::id())).join("registry.yaml");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, registry(proxy_port, slow)).unwrap();
    let mut child = Command::new(crate::legatus_bin::legatus_binary()).arg("--registry").arg(&file).env_remove("LEGATUS_LISTEN").stderr(Stdio::piped()).stdout(Stdio::null()).spawn().unwrap();
    let stderr = child.stderr.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            let _ = tx.send(line);
        }
    });
    wait_for_line(&rx, "legatus listening on");
    let streaming = std::thread::spawn(move || ask(proxy_port));
    std::thread::sleep(Duration::from_millis(700));
    std::fs::write(&file, registry(proxy_port, fast)).unwrap();
    hang_up(child.id());
    wait_for_line(&rx, "registry reloaded: generation 2");
    assert!(ask(proxy_port).contains("answer-from-node-two"), "a request that starts after the swap does not use the removed node");
    let reply = streaming.join().unwrap();
    for i in 0..4 {
        assert!(reply.contains(&format!("slow-part-{i};")), "the stream that began before the swap finished (part {i}): {reply}");
    }
    assert!(reply.trim_end().ends_with("0"), "the chunked body ended cleanly: {reply}");
    let _ = child.kill();
    let _ = child.wait();
}
