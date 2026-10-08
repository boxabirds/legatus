//! Story 160 task 9: real harness binaries read the catalogue. Codex 0.160.1 reads the bodies the
//! real renderer makes (served by a counting stub on the Responses path); pi 1.0.3 goes through
//! the real `legatus` binary for the triggers it can cause. Needs `LEGATUS_CODEX_BIN` and
//! `LEGATUS_PI_BIN`. The rules in `legatus_testkit::harness_rules` are checked against these runs.
use super::real_support::*;
use legatus_common::protocol::Protocol;
use legatus_proxy::protocol::errors::*;
use legatus_testkit::harness_rules::{pi_reaction, Reaction};
use std::net::TcpStream;
use std::process::{Command, Stdio};
use std::time::Duration;

/// Codex sends one request and up to `request_max_retries` more (the test config sets 4).
const CODEX_MAX_REQUESTS: usize = 5;
const WAIT: Duration = Duration::from_secs(20);
const POLL: Duration = Duration::from_millis(50);
/// The Retry-After a stub sends. Codex 0.160.1 waits exactly that long between retries (recorded on
/// 2026-10-08 with 3 seconds on 503, 504 and 529), so the test uses a short one. The proxy never
/// relies on the header; a harness that honours it just waits longer.
const RETRY_AFTER: u32 = 1;
/// pi's default number of automatic retries.
const PI_MAX_RETRIES: usize = 3;

async fn codex_against(kind: RefusalKind, status: Option<u16>, name: &str) -> (usize, Run) {
    let codex = binary(CODEX_BIN_ENV, "Codex 0.160.1");
    let detail = RefusalDetail { max_tokens: Some(8192), used_tokens: Some(9000), retry_after_s: status.map(|_| RETRY_AFTER) };
    let make = move || match status {
        Some(status) => refuse_retry(kind, Protocol::OpenAiResponses, status, &detail).unwrap(),
        None => refuse(kind, Protocol::OpenAiResponses, &detail),
    };
    let stub = counting_stub(make).await;
    let url = format!("http://{}/v1", stub.addr);
    let name = name.to_string();
    let run = tokio::task::spawn_blocking(move || run_codex(&codex, &url, &name)).await.unwrap();
    (stub.requests(), run)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn codex_stops_at_once_on_every_must_stop_refusal_and_shows_the_message() {
    for kind in RefusalKind::ALL.into_iter().filter(|k| k.must_stop()) {
        let (requests, run) = codex_against(kind, None, kind.code()).await;
        assert_eq!(requests, 1, "{kind:?}: Codex must not retry a refusal that must stop");
        assert_ne!(run.code, Some(0), "{kind:?}");
        let shown = format!("{}{}", run.stdout, run.stderr);
        assert!(shown.contains(&kind.template().replace("{max_tokens}", "8192").replace("{used_tokens}", "9000")) || kind == RefusalKind::ContextLengthExceeded, "{kind:?}: {shown}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn codex_retries_the_retry_kinds_and_honours_retry_after_when_it_is_given() {
    for kind in [RefusalKind::NodeConnectFailed, RefusalKind::NoNodeAvailable, RefusalKind::Starting] {
        let (requests, run) = codex_against(kind, None, kind.code()).await;
        assert_eq!(requests, CODEX_MAX_REQUESTS, "{kind:?}: Codex retries and then gives up");
        assert_ne!(run.code, Some(0));
    }
    for status in [503u16, 504, 529] {
        for kind in [RefusalKind::CapacityWaitExpired, RefusalKind::QueueFull] {
            let (requests, run) = codex_against(kind, Some(status), &format!("{}-{status}", kind.code())).await;
            assert_eq!(requests, CODEX_MAX_REQUESTS, "{kind:?} {status}: Codex retried {requests} time(s)");
            let gaps = u64::try_from(CODEX_MAX_REQUESTS - 1).unwrap() * u64::from(RETRY_AFTER);
            assert!(run.elapsed >= Duration::from_secs(gaps), "{kind:?} {status}: Codex waited the Retry-After between retries: {:?}", run.elapsed);
        }
    }
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

struct Proxy {
    child: std::process::Child,
    port: u16,
}

impl Drop for Proxy {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn start_proxy(name: &str, node_url: &str, extra_settings: &str) -> Proxy {
    let port = free_port();
    let file = scratch(name).join("registry.yaml");
    std::fs::write(
        &file,
        format!("version: 1\nsettings:\n  listen: 127.0.0.1:{port}\n{extra_settings}nodes:\n  n1:\n    engine: {{ name: llama-server }}\n    model: node-model\n    endpoints: [ {{ protocol: openai-chat, base_url: \"{node_url}\" }} ]\naliases:\n  local-coder: {{ nodes: [n1] }}\n"),
    )
    .unwrap();
    let child = Command::new(crate::legatus_bin::legatus_binary()).arg("--registry").arg(&file).env_remove("LEGATUS_LISTEN").stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
    let mut waited = Duration::ZERO;
    while TcpStream::connect(("127.0.0.1", port)).is_err() && waited < WAIT {
        std::thread::sleep(POLL);
        waited += POLL;
    }
    Proxy { child, port }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pi_stops_at_once_on_an_unknown_model_and_the_node_sees_nothing() {
    let pi = binary(PI_BIN_ENV, "pi 1.0.3");
    let node = counting_stub(|| text_response(200, "{}")).await;
    let proxy = start_proxy("pi-unknown", &format!("http://{}", node.addr), "");
    let url = format!("http://127.0.0.1:{}/v1", proxy.port);
    // pi sends the model it is told to: an alias the registry does not have.
    let run = tokio::task::spawn_blocking(move || run_pi(&pi, &url, "no-such-alias", "pi-unknown")).await.unwrap();
    assert_eq!(pi_retries(&run), 0, "pi must not retry an unknown model: {}", run.stdout);
    assert!(run.stdout.contains("No model with that name is served here."), "the message is shown: {}", run.stdout);
    assert_eq!(node.requests(), 0, "no node was called");
    assert!(run.elapsed < Duration::from_secs(20));
    assert_eq!(pi_reaction(404, RefusalKind::ModelNotFound.template()), Reaction::Stop, "the stub rule agrees with the real run");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pi_stops_at_once_on_a_body_over_the_limit() {
    let pi = binary(PI_BIN_ENV, "pi 1.0.3");
    let node = counting_stub(|| text_response(200, "{}")).await;
    // pi's request is about 6000 bytes; the limit is smaller.
    let proxy = start_proxy("pi-413", &format!("http://{}", node.addr), "  body_limit_bytes: 1000\n");
    let url = format!("http://127.0.0.1:{}/v1", proxy.port);
    let run = tokio::task::spawn_blocking(move || run_pi(&pi, &url, "local-coder", "pi-413")).await.unwrap();
    assert_eq!(pi_retries(&run), 0, "{}", run.stdout);
    assert!(run.stdout.contains("The request body is too large."), "{}", run.stdout);
    assert_eq!(node.requests(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pi_retries_when_the_node_is_not_reachable_and_then_gives_up() {
    let pi = binary(PI_BIN_ENV, "pi 1.0.3");
    let closed = free_port();
    let proxy = start_proxy("pi-502", &format!("http://127.0.0.1:{closed}"), "");
    let url = format!("http://127.0.0.1:{}/v1", proxy.port);
    let run = tokio::task::spawn_blocking(move || run_pi(&pi, &url, "local-coder", "pi-502")).await.unwrap();
    assert_eq!(pi_retries(&run), PI_MAX_RETRIES, "pi retries a 502 with its backoff: {}", run.stdout);
    assert!(run.stdout.contains("The node did not answer."), "the last message is shown");
    assert!(run.elapsed >= Duration::from_secs(10), "the gaps are 2, 4 and 8 seconds: {:?}", run.elapsed);
    assert_eq!(pi_reaction(502, RefusalKind::NodeConnectFailed.template()), Reaction::Retry);
}
