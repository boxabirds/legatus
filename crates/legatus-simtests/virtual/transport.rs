//! TC-06, TC-07 (transport), TC-08 (sink), TC-09 (errors), TC-13 (spawn).
use bytes::Bytes;
use futures_util::StreamExt;
use http::{HeaderMap, Method};
use legatus_common::event::SystemEventKind;
use legatus_proxy::obs::log_sink::{DropReason, LogRecord, LogSink, Offer, SystemRecord};
use legatus_proxy::spawn_named;
use legatus_proxy::time::Instant;
use legatus_proxy::upstream::transport::{UpstreamError, UpstreamRequest, UpstreamTransport};
use legatus_testkit::virt::{FakeTransport, MemorySink, Script};
use std::time::Duration;

const CHUNK_GAP: Duration = Duration::from_millis(250);
const LOG_LINES: usize = 1000;

fn request() -> UpstreamRequest {
    UpstreamRequest {
        method: Method::POST,
        uri: "http://node.invalid/x".parse().unwrap(),
        headers: HeaderMap::new(),
        body: Bytes::from_static(b"{}"),
    }
}

fn system_record() -> LogRecord {
    LogRecord::System(SystemRecord { kind: SystemEventKind::Start })
}

#[tokio::test(start_paused = true)]
async fn tc06_six_chunks_arrive_250_ms_apart() {
    let fake = FakeTransport::new(Script::chunks(6, CHUNK_GAP));
    let started = Instant::now();
    let mut body = fake.send(request()).await.ok().unwrap().body;
    let mut instants = Vec::new();
    let mut bytes = Vec::new();
    while let Some(item) = body.next().await {
        instants.push((Instant::now() - started).as_millis());
        bytes.extend_from_slice(&item.unwrap());
    }
    assert_eq!(instants, vec![250, 500, 750, 1000, 1250, 1500]);
    assert_eq!(bytes, b"chunk-0;chunk-1;chunk-2;chunk-3;chunk-4;chunk-5;");
    assert_eq!(fake.requests().len(), 1);
    assert_eq!(fake.requests()[0].body_len, 2);
}

#[tokio::test(start_paused = true)]
async fn tc07_slow_reader_makes_emission_follow_the_reader() {
    let slow = Duration::from_secs(1);
    let fake = FakeTransport::new(Script::chunks(4, Duration::from_millis(10)));
    let started = Instant::now();
    let mut body = fake.send(request()).await.ok().unwrap().body;
    while let Some(item) = body.next().await {
        item.unwrap();
        tokio::time::sleep(slow).await;
    }
    let produced: Vec<u128> = fake.produced_at().iter().map(|i| (*i - started).as_millis()).collect();
    // Each chunk is produced only after the reader pulled it: 10, 1020, 2030, 3040 ms.
    assert_eq!(produced, vec![10, 1020, 2030, 3040]);
}

#[tokio::test(start_paused = true)]
async fn tc09_connect_error_and_reset_after_n_chunks() {
    let refused = FakeTransport::new(Script::Connect);
    assert_eq!(refused.send(request()).await.err(), Some(UpstreamError::Connect));

    let chunks = vec![
        (CHUNK_GAP, Ok(Bytes::from_static(b"a"))),
        (CHUNK_GAP, Ok(Bytes::from_static(b"b"))),
        (CHUNK_GAP, Ok(Bytes::from_static(b"c"))),
        (CHUNK_GAP, Err(UpstreamError::Reset)),
    ];
    let cut = FakeTransport::new(Script::Response { status: http::StatusCode::OK, chunks });
    let mut body = cut.send(request()).await.ok().unwrap().body;
    let mut got = Vec::new();
    let mut end = None;
    while let Some(item) = body.next().await {
        match item {
            Ok(b) => got.extend_from_slice(&b),
            Err(e) => end = Some(e),
        }
    }
    assert_eq!(got, b"abc");
    assert_eq!(end, Some(UpstreamError::Reset));

    let empty = FakeTransport::new(Script::chunks(0, CHUNK_GAP));
    let mut body = empty.send(request()).await.ok().unwrap().body;
    assert!(body.next().await.is_none(), "a stream of zero chunks ends at once");
}

#[test]
fn tc08_memory_sink_queues_1000_in_order_and_reports_each_drop() {
    let sink = MemorySink::new(LOG_LINES);
    for _ in 0..LOG_LINES {
        assert_eq!(sink.offer(system_record()), Offer::Queued);
    }
    assert_eq!(sink.len(), LOG_LINES);
    assert_eq!(sink.offer(system_record()), Offer::Dropped(DropReason::QueueFull));
    let sink = MemorySink::new(LOG_LINES);
    sink.set_paused(true);
    assert_eq!(sink.offer(system_record()), Offer::Dropped(DropReason::Paused));
    sink.set_paused(false);
    sink.fail_next_build();
    assert_eq!(sink.offer(system_record()), Offer::Dropped(DropReason::BuildError));
    assert_eq!(sink.offer(system_record()), Offer::Queued);
    assert_eq!(sink.take().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn tc13_spawn_named_returns_a_handle_and_aborting_leaves_no_task() {
    let handle = spawn_named("sleeper", async {
        tokio::time::sleep(Duration::from_secs(3600)).await;
    });
    tokio::task::yield_now().await;
    assert!(!handle.is_finished());
    handle.abort();
    assert!(handle.await.unwrap_err().is_cancelled());
}
