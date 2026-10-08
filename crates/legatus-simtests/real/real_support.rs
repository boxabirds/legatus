//! Helpers shared by the real-tier tests of the refusal story: a counting stub server that answers
//! with a rendered refusal, and runners for the real Codex and pi binaries.
use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::Router;
use bytes::Bytes;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const CODEX_BIN_ENV: &str = "LEGATUS_CODEX_BIN";
pub const PI_BIN_ENV: &str = "LEGATUS_PI_BIN";
pub const RUN_LIMIT: Duration = Duration::from_secs(120);
const POLL: Duration = Duration::from_millis(50);

pub fn binary(env: &str, what: &str) -> PathBuf {
    match std::env::var(env) {
        Ok(path) => PathBuf::from(path),
        Err(_) => panic!("set {env} to the {what} executable"),
    }
}

/// A server that counts requests and answers each with a response built by `make`.
pub struct CountingStub {
    pub addr: std::net::SocketAddr,
    pub count: Arc<AtomicUsize>,
}

impl CountingStub {
    pub fn requests(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }
}

type Make = Arc<dyn Fn() -> Response + Send + Sync>;

#[derive(Clone)]
struct StubState {
    count: Arc<AtomicUsize>,
    make: Make,
}

async fn answer(State(state): State<StubState>, _headers: HeaderMap, _body: Bytes) -> Response {
    state.count.fetch_add(1, Ordering::SeqCst);
    (state.make)()
}

pub async fn counting_stub(make: impl Fn() -> Response + Send + Sync + 'static) -> CountingStub {
    let count = Arc::new(AtomicUsize::new(0));
    let state = StubState { count: count.clone(), make: Arc::new(make) };
    let app = Router::new().fallback(axum::routing::any(answer)).with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    CountingStub { addr, count }
}

pub fn text_response(status: u16, body: &str) -> Response {
    Response::builder().status(StatusCode::from_u16(status).unwrap()).header("content-type", "application/json").body(Body::from(body.to_string())).unwrap()
}

pub struct Run {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub elapsed: Duration,
}

pub fn run_with_limit(mut command: Command) -> Run {
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let started = Instant::now();
    let mut child = command.spawn().expect("spawn the harness");
    let status = loop {
        if let Some(s) = child.try_wait().unwrap() {
            break s;
        }
        if started.elapsed() > RUN_LIMIT {
            let _ = child.kill();
            panic!("the harness did not finish in {RUN_LIMIT:?}");
        }
        std::thread::sleep(POLL);
    };
    let out = child.wait_with_output().unwrap();
    Run { code: status.code(), stdout: String::from_utf8_lossy(&out.stdout).to_string(), stderr: String::from_utf8_lossy(&out.stderr).to_string(), elapsed: started.elapsed() }
}

/// A scratch folder for one run.
pub fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("legatus-real-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Run real Codex (`codex exec`) against a base address that speaks the Responses API. Codex
/// retries a failed request up to `request_max_retries` times.
pub fn run_codex(codex: &Path, base_url: &str, name: &str) -> Run {
    let home = scratch(name).join("codex_home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(
        home.join("config.toml"),
        format!("model = \"gpt-test\"\nmodel_provider = \"stub\"\n[model_providers.stub]\nname = \"stub\"\nbase_url = \"{base_url}\"\nwire_api = \"responses\"\nenv_key = \"STUB_KEY\"\nrequest_max_retries = 4\nstream_max_retries = 0\n"),
    )
    .unwrap();
    let mut command = Command::new(codex);
    command.args(["exec", "--skip-git-repo-check", "say hello"]).env("CODEX_HOME", &home).env("STUB_KEY", "x").current_dir(scratch(name));
    run_with_limit(command)
}

/// Run real pi 1.0.3 in json mode against a base address that speaks chat completions.
pub fn run_pi(pi: &Path, base_url: &str, model: &str, name: &str) -> Run {
    let dir = scratch(name);
    let agent = dir.join("agent");
    std::fs::create_dir_all(&agent).unwrap();
    std::fs::write(
        agent.join("models.json"),
        format!("{{\"providers\":{{\"legatus\":{{\"baseUrl\":\"{base_url}\",\"api\":\"openai-completions\",\"apiKey\":\"local\",\"models\":[{{\"id\":\"{model}\",\"name\":\"alias\",\"input\":[\"text\"],\"contextWindow\":8192,\"maxTokens\":512,\"reasoning\":false}}]}}}}}}"),
    )
    .unwrap();
    let mut command = Command::new(pi);
    command
        .args(["-p", "--mode", "json", "--provider", "legatus", "--model", model, "--no-session", "--no-tools", "--no-extensions", "--no-skills", "--no-context-files", "--offline", "say hello"])
        .env("PI_CODING_AGENT_DIR", &agent)
        .current_dir(&dir);
    run_with_limit(command)
}

/// How many times pi announced a retry (json mode events of type auto_retry_start).
pub fn pi_retries(run: &Run) -> usize {
    run.stdout.lines().filter(|l| l.contains("\"auto_retry_start\"")).count()
}
