//! TC-05 to TC-08 of story 162: the serial Ollama stub.
use super::stub_helpers::*;
use legatus_proxy::upstream::transport::UpstreamTransport;
use legatus_testkit::stubs::ollama::SerialOllamaStub;
use legatus_testkit::stubs::scenario::{StubKind, StubSpec};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;

fn spec() -> StubSpec {
    let mut s = StubSpec::new("ollama", StubKind::SerialOllama);
    s.speed.load_time_ms = 0;
    s
}

#[tokio::test(start_paused = true)]
async fn tc05_queued_request_gets_headers_only_when_the_running_one_ends() {
    let stub = Arc::new(SerialOllamaStub::new(spec()));
    let started = Instant::now();
    let a = stub.send(chat(260, 3)).await.ok().unwrap();
    let b_stub = stub.clone();
    let b = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(10)).await;
        let reply = b_stub.send(chat(260, 3)).await.ok().unwrap();
        (started.elapsed().as_millis(), reply)
    });
    let (_, _, a_instants) = drain(a, started).await;
    let a_end = *a_instants.last().unwrap();
    let (b_headers_at, _) = b.await.unwrap();
    assert_eq!(a_instants, vec![1000, 1020, 1040, 1060]);
    assert_eq!(b_headers_at, a_end, "headers of B come at service start, which is the end of A");
}

#[tokio::test(start_paused = true)]
async fn tc05_three_queued_are_served_in_arrival_order() {
    let stub = Arc::new(SerialOllamaStub::new(spec()));
    let started = Instant::now();
    let mut handles = Vec::new();
    for i in 0..3u32 {
        let stub = stub.clone();
        handles.push(tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(u64::from(i))).await;
            let reply = stub.send(chat(260, 1)).await.ok().unwrap();
            let at = started.elapsed().as_millis();
            let _ = drain(reply, started).await;
            (i, at)
        }));
    }
    let mut served = Vec::new();
    for h in handles {
        served.push(h.await.unwrap());
    }
    served.sort_by_key(|(_, at)| *at);
    assert_eq!(served.iter().map(|(i, _)| *i).collect::<Vec<_>>(), vec![0, 1, 2]);
}

#[tokio::test(start_paused = true)]
async fn tc06_eight_queued_no_error_and_max_queue_two_refuses_the_third() {
    let stub = Arc::new(SerialOllamaStub::new(spec()));
    let mut handles = Vec::new();
    for _ in 0..9 {
        let stub = stub.clone();
        handles.push(tokio::spawn(async move {
            let reply = stub.send(chat(26, 1)).await.ok().unwrap();
            let (status, _, _) = drain(reply, Instant::now()).await;
            status
        }));
    }
    for h in handles {
        assert_eq!(h.await.unwrap(), 200, "no error with 8 queued");
    }
    let mut limited = spec();
    limited.max_queue = 2;
    let stub = Arc::new(SerialOllamaStub::new(limited));
    let first = stub.send(chat(26, 1)).await.ok().unwrap(); // running, stream not read
    let second_stub = stub.clone();
    let second = tokio::spawn(async move { second_stub.send(chat(26, 1)).await.ok().unwrap().status.as_u16() });
    tokio::task::yield_now().await;
    let third = stub.send(chat(26, 1)).await.ok().unwrap();
    assert_eq!(third.status.as_u16(), 503);
    drop(first);
    assert_eq!(second.await.unwrap(), 200);
}

#[tokio::test(start_paused = true)]
async fn tc07_prompt_above_the_ratio_is_truncated_at_the_boundary_it_is_not() {
    let mut s = spec();
    s.ctx_total = 1000;
    let stub = SerialOllamaStub::new(s);
    let log = stub.log();
    assert!(!log.has("truncated"), "state before");
    let (status, _) = text_of(&stub, chat(500, 1)).await;
    assert_eq!(status, 200);
    assert!(!log.has("truncated"), "at the ratio times ctx there is no flag");
    let (status, _) = text_of(&stub, chat(501, 1)).await;
    assert_eq!(status, 200, "truncation is not an error");
    assert!(log.has("truncated"), "one token above sets the flag");
    assert_eq!(log.entries().iter().filter(|e| e.event == "truncated").count(), 1);
}

#[tokio::test(start_paused = true)]
async fn tc08_process_page_has_no_queue_field_and_the_model_unloads_after_keep_alive() {
    let mut s = spec();
    s.keep_alive_s = 10;
    s.speed.load_time_ms = 2000;
    let stub = SerialOllamaStub::new(s);
    let (_, empty) = text_of(&stub, get("/api/ps")).await;
    assert!(empty.contains("\"models\":[]"), "{empty}");
    let started = Instant::now();
    let reply = stub.send(chat(26, 1)).await.ok().unwrap();
    assert_eq!(started.elapsed().as_millis(), 2000, "first request pays the load time");
    let _ = drain(reply, started).await;
    let (_, page) = text_of(&stub, get("/api/ps")).await;
    assert!(page.contains("stub-model") && page.contains("size_vram") && page.contains("expires_in_ms"), "{page}");
    for forbidden in ["queue", "slot", "waiting", "running"] {
        assert!(!page.contains(forbidden), "{forbidden} must not appear: {page}");
    }
    tokio::time::sleep(Duration::from_secs(11)).await;
    let (_, after) = text_of(&stub, get("/api/ps")).await;
    assert!(after.contains("\"models\":[]"), "unloaded after keep alive: {after}");
    let again = Instant::now();
    let reply = stub.send(chat(26, 1)).await.ok().unwrap();
    assert_eq!(again.elapsed().as_millis(), 2000, "the next request pays the load time again");
    let _ = drain(reply, again).await;
    let (status, body) = text_of(&stub, post("/v1/chat/completions", "{\"model\":\"other\",\"messages\":[]}")).await;
    assert_eq!((status, body.contains("not found")), (404, true));
}
