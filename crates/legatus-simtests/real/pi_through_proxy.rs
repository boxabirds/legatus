//! Slice 1 gate of story 121 (AC1): a real pi 1.0.3 `-p` session through the real proxy binary to
//! a node. The node here is an in-process server that answers like an OpenAI-compatible engine and
//! records what it received; a real engine is the second half of the acceptance (task 121.11).
//!
//! Needs `LEGATUS_PI_BIN`: the path of the pi 1.0.3 executable (npm package
//! `@earendil-works/pi-coding-agent@1.0.3`). Run with `scripts/test.sh real`.
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::post;
use axum::Router;
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const PI_BIN_ENV: &str = "LEGATUS_PI_BIN";
const ALIAS: &str = "local-coder";
const NODE_MODEL: &str = "qwen-node";
const ANSWER: &str = "Hello from the node";
const WAIT: Duration = Duration::from_secs(20);
const POLL: Duration = Duration::from_millis(50);
const PI_RUN_LIMIT: Duration = Duration::from_secs(90);
const PI_FIXTURE: &str = include_str!("../fixtures/bodies/pi_103_chat.json");
const PI_FIXTURE_HEADERS: &str = include_str!("../fixtures/bodies/pi_103_chat_headers.json");

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<(HeaderMap, Bytes)>>>);

async fn node_chat(State(captured): State<Captured>, headers: HeaderMap, body: Bytes) -> Response {
    captured.0.lock().unwrap().push((headers, body));
    let chunk = |delta: &str, finish: &str, usage: &str| format!("data: {{\"id\":\"c1\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"{NODE_MODEL}\",\"choices\":[{{\"index\":0,\"delta\":{delta},\"finish_reason\":{finish}}}]{usage}}}\n\n");
    let text = format!(
        "{}{}{}data: [DONE]\n\n",
        chunk("{\"role\":\"assistant\",\"content\":\"\"}", "null", ""),
        chunk(&format!("{{\"content\":\"{ANSWER}\"}}"), "null", ""),
        chunk("{}", "\"stop\"", ",\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":4,\"total_tokens\":9}"),
    );
    Response::builder().status(StatusCode::OK).header("content-type", "text/event-stream").body(axum::body::Body::from(text)).unwrap()
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn pi_binary() -> PathBuf {
    match std::env::var(PI_BIN_ENV) {
        Ok(path) => PathBuf::from(path),
        Err(_) => panic!("set {PI_BIN_ENV} to the pi 1.0.3 executable (npm install @earendil-works/pi-coding-agent@1.0.3)"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tc22_pi_1_0_3_print_mode_through_the_proxy_reaches_the_node_with_only_the_model_changed() {
    let pi = pi_binary();
    // The node.
    let captured = Captured::default();
    let node_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let node_port = node_listener.local_addr().unwrap().port();
    let app = Router::new().route("/v1/chat/completions", post(node_chat)).with_state(captured.clone());
    tokio::spawn(async move { axum::serve(node_listener, app).await.unwrap() });

    // The proxy: the real binary on a real registry file.
    let dir = std::env::temp_dir().join(format!("legatus-pi-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let proxy_port = free_port();
    let registry = dir.join("registry.yaml");
    std::fs::write(
        &registry,
        format!("version: 1\nsettings:\n  listen: 127.0.0.1:{proxy_port}\nnodes:\n  n1:\n    engine: {{ name: llama-server }}\n    model: {NODE_MODEL}\n    endpoints: [ {{ protocol: openai-chat, base_url: \"http://127.0.0.1:{node_port}\" }} ]\naliases:\n  {ALIAS}: {{ nodes: [n1] }}\n"),
    )
    .unwrap();
    let mut proxy = Command::new(crate::legatus_bin::legatus_binary()).arg("--registry").arg(&registry).env_remove("LEGATUS_LISTEN").stderr(Stdio::piped()).spawn().unwrap();
    let mut waited = Duration::ZERO;
    while TcpStream::connect(("127.0.0.1", proxy_port)).is_err() && waited < WAIT {
        std::thread::sleep(POLL);
        waited += POLL;
    }

    // pi: its own agent directory, one provider whose base address is the proxy.
    let agent = dir.join("agent");
    std::fs::create_dir_all(&agent).unwrap();
    std::fs::write(
        agent.join("models.json"),
        format!("{{\"providers\":{{\"legatus\":{{\"baseUrl\":\"http://127.0.0.1:{proxy_port}/v1\",\"api\":\"openai-completions\",\"apiKey\":\"local\",\"models\":[{{\"id\":\"{ALIAS}\",\"name\":\"alias\",\"input\":[\"text\"],\"contextWindow\":8192,\"maxTokens\":1024,\"reasoning\":false}}]}}}}}}"),
    )
    .unwrap();
    let mut run = Command::new(&pi)
        .args(["-p", "--provider", "legatus", "--model", ALIAS, "--no-session", "--no-extensions", "--no-skills", "--no-context-files", "--offline", "say hello"])
        .env("PI_CODING_AGENT_DIR", &agent)
        .current_dir(&dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let started = std::time::Instant::now();
    let status = loop {
        if let Some(s) = run.try_wait().unwrap() {
            break s;
        }
        assert!(started.elapsed() < PI_RUN_LIMIT, "pi did not finish");
        std::thread::sleep(POLL);
    };
    let _ = proxy.kill();
    let _ = proxy.wait();
    let output = run.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(status.success(), "pi failed: {} {}", stdout, String::from_utf8_lossy(&output.stderr));
    assert!(stdout.contains(ANSWER), "pi printed: {stdout}");

    // What the node received.
    let seen = captured.0.lock().unwrap().clone();
    assert_eq!(seen.len(), 1, "one request reached the node");
    let (headers, body) = &seen[0];
    let received: serde_json::Value = serde_json::from_slice(body).unwrap();
    assert_eq!(received["model"], NODE_MODEL, "the node gets its own model name");
    let fixture: serde_json::Value = serde_json::from_str(PI_FIXTURE).unwrap();
    let keys = |v: &serde_json::Value| v.as_object().unwrap().keys().cloned().collect::<Vec<_>>();
    assert_eq!(keys(&received), keys(&fixture), "field order is what pi sends");
    for field in ["stream", "stream_options", "store", "max_completion_tokens"] {
        assert_eq!(received[field], fixture[field], "{field}");
    }
    assert_eq!(received["tools"].as_array().unwrap().len(), fixture["tools"].as_array().unwrap().len());
    let expected: Vec<(String, String)> = serde_json::from_str::<Vec<(String, String)>>(PI_FIXTURE_HEADERS).unwrap();
    for (name, _) in expected.iter().filter(|(n, _)| ["authorization", "content-type", "accept", "x-stainless-lang"].contains(&n.as_str())) {
        assert!(headers.contains_key(name.as_str()), "header {name} reached the node");
    }
    assert!(!headers.contains_key("connection") || headers["connection"] != "close", "no hop header from the proxy side");
}

const REAL_ENGINES_ENV: &str = "LEGATUS_REAL_ENGINES";
const REAL_MODEL_ENV: &str = "LEGATUS_REAL_OLLAMA_MODEL";
const DEFAULT_REAL_MODEL: &str = "qwen3:1.7b";
const REAL_PI_RUN_LIMIT: Duration = Duration::from_secs(300);

/// The `ollama=host:port` entry of `LEGATUS_REAL_ENGINES`, if there is one.
fn real_ollama() -> Option<String> {
    std::env::var(REAL_ENGINES_ENV).ok()?.split(',').find_map(|entry| entry.trim().strip_prefix("ollama=").map(str::to_string))
}

/// The second half of the acceptance (task 121.11): the same session, but the node is a real
/// Ollama. It needs `LEGATUS_REAL_ENGINES=ollama=127.0.0.1:11434` and `LEGATUS_PI_BIN`; the model
/// comes from `LEGATUS_REAL_OLLAMA_MODEL` (default qwen3:1.7b).
#[test]
fn tc22_pi_1_0_3_print_mode_through_the_proxy_to_a_real_ollama_shows_an_answer() {
    let pi = pi_binary();
    let Some(ollama) = real_ollama() else {
        panic!("set {REAL_ENGINES_ENV} with an ollama=host:port entry to run the real-node half");
    };
    let model = std::env::var(REAL_MODEL_ENV).unwrap_or_else(|_| DEFAULT_REAL_MODEL.to_string());
    let dir = std::env::temp_dir().join(format!("legatus-pi-real-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let proxy_port = free_port();
    let registry = dir.join("registry.yaml");
    std::fs::write(
        &registry,
        format!("version: 1\nsettings:\n  listen: 127.0.0.1:{proxy_port}\nnodes:\n  ollama1:\n    engine: {{ name: ollama }}\n    model: \"{model}\"\n    endpoints: [ {{ protocol: openai-chat, base_url: \"http://{ollama}\" }} ]\n    slots: 1\naliases:\n  {ALIAS}: {{ nodes: [ollama1] }}\n"),
    )
    .unwrap();
    let mut proxy = Command::new(crate::legatus_bin::legatus_binary()).arg("--registry").arg(&registry).env_remove("LEGATUS_LISTEN").stderr(Stdio::null()).spawn().unwrap();
    let mut waited = Duration::ZERO;
    while TcpStream::connect(("127.0.0.1", proxy_port)).is_err() && waited < WAIT {
        std::thread::sleep(POLL);
        waited += POLL;
    }
    let agent = dir.join("agent");
    std::fs::create_dir_all(&agent).unwrap();
    std::fs::write(
        agent.join("models.json"),
        format!("{{\"providers\":{{\"legatus\":{{\"baseUrl\":\"http://127.0.0.1:{proxy_port}/v1\",\"api\":\"openai-completions\",\"apiKey\":\"local\",\"models\":[{{\"id\":\"{ALIAS}\",\"name\":\"alias\",\"input\":[\"text\"],\"contextWindow\":8192,\"maxTokens\":512,\"reasoning\":false}}]}}}}}}"),
    )
    .unwrap();
    let mut run = Command::new(&pi)
        .args(["-p", "--provider", "legatus", "--model", ALIAS, "--no-session", "--no-tools", "--no-extensions", "--no-skills", "--no-context-files", "--offline", "Reply with the single word: ready"])
        .env("PI_CODING_AGENT_DIR", &agent)
        .current_dir(&dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let started = std::time::Instant::now();
    let status = loop {
        if let Some(s) = run.try_wait().unwrap() {
            break s;
        }
        assert!(started.elapsed() < REAL_PI_RUN_LIMIT, "pi did not finish");
        std::thread::sleep(POLL);
    };
    let _ = proxy.kill();
    let _ = proxy.wait();
    let output = run.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(status.success(), "pi failed: {} {}", stdout, String::from_utf8_lossy(&output.stderr));
    assert!(!stdout.trim().is_empty(), "pi showed no answer");
    println!("real node answer: {}", stdout.trim());
}
