//! Spike of story 160 (task 1, PRX-PROTO-064): how real Codex 0.160.1 reads an HTTP 400 before a
//! stream. The stub serves the body that the real renderer makes for a context refusal. Needs
//! `LEGATUS_CODEX_BIN` (npm package `@openai/codex@0.160.1`).
use super::real_support::*;
use legatus_common::protocol::Protocol;
use legatus_proxy::protocol::errors::{refuse, RefusalDetail, RefusalKind};
use std::sync::Arc;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tc18_codex_reads_an_http_400_context_refusal_before_a_stream_as_final_and_shows_the_message() {
    let codex = binary(CODEX_BIN_ENV, "Codex 0.160.1");
    let detail = Arc::new(RefusalDetail { max_tokens: Some(8192), used_tokens: Some(9000), retry_after_s: None });
    let make = {
        let detail = detail.clone();
        move || refuse(RefusalKind::ContextLengthExceeded, Protocol::OpenAiResponses, &detail)
    };
    let stub = counting_stub(make).await;
    let run = tokio::task::spawn_blocking({
        let url = format!("http://{}/v1", stub.addr);
        move || run_codex(&codex, &url, "context")
    })
    .await
    .unwrap();
    // Recorded behaviour of Codex 0.160.1 (2026-10-08): one request, no retry, a non-zero exit, and
    // the message of the body is shown.
    assert_eq!(stub.requests(), 1, "Codex does not retry an HTTP 400 before a stream");
    assert_ne!(run.code, Some(0));
    let shown = format!("{}{}", run.stdout, run.stderr);
    assert!(shown.contains("maximum context length is 8192 tokens"), "{shown}");
    assert!(run.elapsed.as_secs() < 30);
}
