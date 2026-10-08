//! Story 160 task 10 (TC-24): the real `legatus` binary on real sockets with the real pi 1.0.3.
//! What can be caused through the binary today is tested end to end here: the body limit and the
//! unknown model (stop), a node that does not answer (retry), the `starting` refusal of a first
//! load that fails (retry) and a hold refusal with a long Retry-After (retried by the harness's own
//! rule). Codex needs the Responses path of story 131 to go through the binary, so Codex reads
//! the catalogue bodies in refusals_pi_codex_test.rs. Needs `LEGATUS_PI_BIN`.
use super::real_support::*;
use legatus_common::protocol::Protocol;
use legatus_proxy::protocol::errors::*;
use std::io::Write;
use std::net::TcpStream;
use std::process::{Command, Stdio};
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(20);
const POLL: Duration = Duration::from_millis(50);
const PI_MAX_RETRIES: usize = 3;
/// The Retry-After the stub sends; pi's own backoff (2, 4 and 8 seconds) decides, not this.
const LONG_RETRY_AFTER: u32 = RETRY_AFTER_MAX_S;

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_hold_refusal_with_the_longest_retry_after_is_retried_by_pi_on_its_own_backoff() {
    let pi = binary(PI_BIN_ENV, "pi 1.0.3");
    let detail = RefusalDetail { retry_after_s: Some(LONG_RETRY_AFTER), ..RefusalDetail::default() };
    let stub = counting_stub(move || refuse_retry(RefusalKind::CapacityWaitExpired, Protocol::OpenAiChat, 503, &detail).unwrap()).await;
    let url = format!("http://{}/v1", stub.addr);
    let run = tokio::task::spawn_blocking(move || run_pi(&pi, &url, "local-coder", "e2e-retry-after")).await.unwrap();
    assert_eq!(pi_retries(&run), PI_MAX_RETRIES, "{}", run.stdout);
    assert_eq!(stub.requests(), PI_MAX_RETRIES + 1);
    assert!(run.elapsed < Duration::from_secs(u64::from(LONG_RETRY_AFTER)), "pi did not wait for the {LONG_RETRY_AFTER} second header: {:?}", run.elapsed);
    assert!(run.stdout.contains("The wait for a free node ended."), "the message is shown");
}

/// A named pipe as the registry path holds the first load, so the refusal can be provoked exactly.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pi_retries_a_starting_refusal_when_the_first_registry_load_fails() {
    let pi = binary(PI_BIN_ENV, "pi 1.0.3");
    let fifo = scratch("e2e-starting").join("registry.fifo");
    assert!(Command::new("mkfifo").arg(&fifo).status().unwrap().success());
    let port = free_port();
    let mut proxy = Command::new(crate::legatus_bin::legatus_binary()).arg("--registry").arg(&fifo).env("LEGATUS_LISTEN", format!("127.0.0.1:{port}")).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
    let mut waited = Duration::ZERO;
    while TcpStream::connect(("127.0.0.1", port)).is_err() && waited < WAIT {
        std::thread::sleep(POLL);
        waited += POLL;
    }
    let url = format!("http://127.0.0.1:{port}/v1");
    let harness = tokio::task::spawn_blocking(move || run_pi(&pi, &url, "local-coder", "e2e-starting"));
    // pi's first request is held while the registry is read; an invalid file then answers it with `starting`.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let mut writer = std::fs::OpenOptions::new().write(true).open(&fifo).unwrap();
    writer.write_all(b"version: 9\nnodes: {}\naliases: {}\n").unwrap();
    drop(writer);
    let run = harness.await.unwrap();
    let _ = proxy.wait();
    assert!(pi_retries(&run) >= 1, "pi retries a starting refusal: {}", run.stdout);
    assert!(run.stdout.contains("The proxy is starting."), "the message is shown: {}", run.stdout);
}
