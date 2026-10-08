//! TC-15 of story 145: a real pi 1.0.3 print-mode session shows the streamed text through the
//! proxy before the node has finished. The node sends the first part, waits, then sends the rest.
//! Needs `LEGATUS_PI_BIN` (see pi_through_proxy_test.rs).
use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::post;
use axum::Router;
use bytes::Bytes;
use futures_util::stream;
use std::io::{BufRead, BufReader};
use std::net::TcpStream;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const PI_BIN_ENV: &str = "LEGATUS_PI_BIN";
const ALIAS: &str = "local-coder";
const NODE_MODEL: &str = "slow-node-model";
const FIRST_PART: &str = "First part. ";
const LAST_PART: &str = "Last part.";
const NODE_PAUSE: Duration = Duration::from_secs(6);
/// The text must show at least this long before the node finishes.
const EARLY_BY: Duration = Duration::from_secs(3);
const WAIT: Duration = Duration::from_secs(20);
const POLL: Duration = Duration::from_millis(50);
const PI_RUN_LIMIT: Duration = Duration::from_secs(90);

#[derive(Clone, Default)]
struct Finished(Arc<Mutex<Option<Instant>>>);

fn sse(delta: &str, finish: &str, usage: &str) -> Bytes {
    Bytes::from(format!("data: {{\"id\":\"c1\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"{NODE_MODEL}\",\"choices\":[{{\"index\":0,\"delta\":{delta},\"finish_reason\":{finish}}}]{usage}}}\n\n"))
}

async fn node(State(finished): State<Finished>, _headers: HeaderMap, _body: Bytes) -> Response {
    let finished_cell = finished.0.clone();
    let steps = stream::unfold(0u8, move |step| {
        let finished_cell = finished_cell.clone();
        async move {
            match step {
                0 => Some((Ok::<_, std::io::Error>(sse("{\"role\":\"assistant\",\"content\":\"\"}", "null", "")), 1)),
                1 => Some((Ok(sse(&format!("{{\"content\":\"{FIRST_PART}\"}}"), "null", "")), 2)),
                2 => {
                    tokio::time::sleep(NODE_PAUSE).await;
                    Some((Ok(sse(&format!("{{\"content\":\"{LAST_PART}\"}}"), "null", "")), 3))
                }
                3 => Some((Ok(sse("{}", "\"stop\"", ",\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":4,\"total_tokens\":9}")), 4)),
                4 => {
                    *finished_cell.lock().unwrap() = Some(Instant::now());
                    Some((Ok(Bytes::from_static(b"data: [DONE]\n\n")), 5))
                }
                _ => None,
            }
        }
    });
    Response::builder().status(StatusCode::OK).header("content-type", "text/event-stream").body(Body::from_stream(steps)).unwrap()
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tc15_pi_shows_the_first_part_of_the_answer_long_before_the_node_finishes() {
    let pi = std::env::var(PI_BIN_ENV).unwrap_or_else(|_| panic!("set {PI_BIN_ENV} to the pi 1.0.3 executable"));
    let finished = Finished::default();
    let node_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let node_port = node_listener.local_addr().unwrap().port();
    let app = Router::new().route("/v1/chat/completions", post(node)).with_state(finished.clone());
    tokio::spawn(async move { axum::serve(node_listener, app).await.unwrap() });

    let dir = std::env::temp_dir().join(format!("legatus-pi-stream-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let proxy_port = free_port();
    let registry = dir.join("registry.yaml");
    std::fs::write(
        &registry,
        format!("version: 1\nsettings:\n  listen: 127.0.0.1:{proxy_port}\nnodes:\n  n1:\n    engine: {{ name: llama-server }}\n    model: {NODE_MODEL}\n    endpoints: [ {{ protocol: openai-chat, base_url: \"http://127.0.0.1:{node_port}\" }} ]\naliases:\n  {ALIAS}: {{ nodes: [n1] }}\n"),
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
        format!("{{\"providers\":{{\"legatus\":{{\"baseUrl\":\"http://127.0.0.1:{proxy_port}/v1\",\"api\":\"openai-completions\",\"apiKey\":\"local\",\"models\":[{{\"id\":\"{ALIAS}\",\"name\":\"alias\",\"input\":[\"text\"],\"contextWindow\":8192,\"maxTokens\":1024,\"reasoning\":false}}]}}}}}}"),
    )
    .unwrap();
    // `--mode json` prints each event as pi gets it, so the time of the first text can be read.
    let mut run = Command::new(&pi)
        .args(["-p", "--mode", "json", "--provider", "legatus", "--model", ALIAS, "--no-session", "--no-tools", "--no-extensions", "--no-skills", "--no-context-files", "--offline", "say hello"])
        .env("PI_CODING_AGENT_DIR", &agent)
        .current_dir(&dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let stdout = run.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut first_text_at: Option<Instant> = None;
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if first_text_at.is_none() && line.contains("\"message_update\"") && line.contains(FIRST_PART.trim()) {
                first_text_at = Some(Instant::now());
            }
        }
        first_text_at
    });
    let started = Instant::now();
    let status = loop {
        if let Some(s) = run.try_wait().unwrap() {
            break s;
        }
        assert!(started.elapsed() < PI_RUN_LIMIT, "pi did not finish");
        std::thread::sleep(POLL);
    };
    let first_text_at = reader.join().unwrap();
    let _ = proxy.kill();
    let _ = proxy.wait();
    assert!(status.success());
    let first_text_at = first_text_at.expect("pi printed the first part as an update");
    let node_finished = finished.0.lock().unwrap().expect("the node finished");
    assert!(node_finished.duration_since(first_text_at) >= EARLY_BY, "the first text came only {:?} before the node finished", node_finished.duration_since(first_text_at));
}
