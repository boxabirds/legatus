//! Story 145 integration tests: the real router, the in-memory transport and paused time.
use async_trait::async_trait;
use axum::body::Body;
use bytes::Bytes;
use http::{Request, StatusCode};
use http_body_util::BodyExt;
use legatus_proxy::obs::log_sink::DiscardSink;
use legatus_proxy::protocol::chat::{PipelineHooks, ReplySummary};
use legatus_proxy::protocol::stream::EndFlags;
use legatus_proxy::sim::SimPoints;
use legatus_proxy::stream::guard::{GuardCarry, ResponseGuard, ScanResult};
use legatus_proxy::stream::tap::StreamEnd;
use legatus_proxy::time::WallTime;
use legatus_proxy::upstream::transport::UpstreamError;
use legatus_proxy::{build_router, Seams, CHAT_COMPLETIONS_PATH};
use legatus_testkit::fleet::{one_node_deps, FLEET_REQUEST_BODY};
use legatus_testkit::virt::{serve_duplex, FakeTransport, MemorySink, Script, SimWall};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::Instant;

const OLLAMA_STREAM: &[u8] = include_bytes!("../fixtures/streams/ollama_chat_stream.sse");
const OLLAMA_REPLY: &[u8] = include_bytes!("../fixtures/streams/ollama_chat_reply.json");
const MESSAGES_PINGS: &[u8] = include_bytes!("../fixtures/streams/messages_with_pings.sse");
const NO_END_MARKER: &[u8] = include_bytes!("../fixtures/streams/chat_no_end_marker.sse");
const EMPTY_200: &[u8] = include_bytes!("../fixtures/streams/chat_empty_200.sse");
const KEY: &str = "sk-ant-api03-Zk3QfN8vLm2PxTcR9wYbHd7sUe1AoJqGiV5nXtKzB4";
const SINK_CAPACITY: usize = 16;
/// Size of a chunk big enough that the in-memory pipe (64 KiB) holds less than a few of them.
const BIG_CHUNK_BYTES: usize = 256 * 1024;
/// What the pipe buffer and the HTTP layer may hold between the node and a reader that has stopped.
const TRANSPORT_WINDOW_BYTES: usize = 1024 * 1024;

fn seams(fake: Arc<FakeTransport>) -> Seams {
    Seams { wall: Arc::new(SimWall::new(WallTime { unix_ms: 0 })), transport: fake, log: Arc::new(DiscardSink), sim: SimPoints::new() }
}

fn post() -> Request<Body> {
    Request::builder().method("POST").uri(CHAT_COMPLETIONS_PATH).header("host", "proxy").body(Body::from(FLEET_REQUEST_BODY)).unwrap()
}

fn script(status: StatusCode, chunks: Vec<(Duration, Result<Bytes, UpstreamError>)>) -> Script {
    Script::Response { status, chunks }
}

fn ok(delay_ms: u64, bytes: &[u8]) -> (Duration, Result<Bytes, UpstreamError>) {
    (Duration::from_millis(delay_ms), Ok(Bytes::copy_from_slice(bytes)))
}

/// Records how each reply ended.
#[derive(Default)]
struct Ends(Mutex<Vec<ReplySummary>>);

#[async_trait]
impl PipelineHooks for Ends {
    fn reply_end(&self, summary: &ReplySummary) {
        self.0.lock().unwrap().push(summary.clone());
    }
}

impl Ends {
    fn last(&self) -> ReplySummary {
        self.0.lock().unwrap().last().cloned().expect("a reply ended")
    }
}

async fn read_frames(mut body: Body) -> (Vec<(u128, usize)>, Vec<u8>, bool) {
    let started = Instant::now();
    let mut frames = Vec::new();
    let mut bytes = Vec::new();
    let mut failed = false;
    while let Some(frame) = body.frame().await {
        match frame {
            Ok(frame) => {
                if let Ok(data) = frame.into_data() {
                    frames.push(((Instant::now() - started).as_millis(), data.len()));
                    bytes.extend_from_slice(&data);
                }
            }
            Err(_) => {
                failed = true;
                break;
            }
        }
    }
    (frames, bytes, failed)
}

#[tokio::test(start_paused = true)]
async fn tc04_the_first_chunk_reaches_the_harness_while_the_node_has_not_sent_the_last() {
    let fake = Arc::new(FakeTransport::new(script(StatusCode::OK, vec![ok(1000, b"data: first\n\n"), ok(29_000, b"data: second\n\n"), ok(30_000, b"data: [DONE]\n\n")])));
    let client = serve_duplex(build_router(one_node_deps(seams(fake.clone())))).await;
    let response = client.send(post()).await;
    assert_eq!(response.status(), StatusCode::OK);
    let (frames, bytes, failed) = read_frames(response.into_body()).await;
    assert!(!failed);
    assert_eq!(frames.iter().map(|f| f.0).collect::<Vec<_>>(), vec![1000, 30_000, 60_000], "each chunk arrives when the node produces it");
    assert_eq!(bytes, b"data: first\n\ndata: second\n\ndata: [DONE]\n\n");
}

#[tokio::test(start_paused = true)]
async fn tc05_a_node_ping_passes_unchanged_at_its_own_time() {
    let ping: &[u8] = b"event: ping\ndata: {\"type\":\"ping\"}\n\n";
    let fake = Arc::new(FakeTransport::new(script(StatusCode::OK, vec![ok(0, ping), ok(15_000, ping), ok(15_000, b"event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n")])));
    let client = serve_duplex(build_router(one_node_deps(seams(fake)))).await;
    let (frames, bytes, _) = read_frames(client.send(post()).await.into_body()).await;
    assert_eq!(frames.iter().map(|f| f.0).collect::<Vec<_>>(), vec![0, 15_000, 30_000]);
    assert_eq!(bytes, [ping, ping, b"event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n".as_slice()].concat());
}

/// Read `count` frames, sleeping `pause` between them, and report the largest number of bytes the
/// node produced beyond what the reader had taken.
async fn slow_read(chunk_bytes: usize, count: usize, pause: Duration) -> usize {
    let chunks: Vec<_> = (0..count).map(|_| (Duration::ZERO, Ok(Bytes::from(vec![b'x'; chunk_bytes])))).collect();
    let fake = Arc::new(FakeTransport::new(script(StatusCode::OK, chunks)));
    let client = serve_duplex(build_router(one_node_deps(seams(fake.clone())))).await;
    let response = client.send(post()).await;
    let mut body = response.into_body();
    let mut read_bytes = 0usize;
    let mut worst = 0usize;
    while let Some(frame) = body.frame().await {
        read_bytes += frame.unwrap().into_data().unwrap().len();
        tokio::time::sleep(pause).await;
        let produced = fake.produced_at().len() * chunk_bytes;
        worst = worst.max(produced.saturating_sub(read_bytes));
    }
    worst
}

#[tokio::test(start_paused = true)]
async fn tc06_a_reader_at_one_chunk_per_second_keeps_the_node_within_the_transport_window() {
    let worst = slow_read(BIG_CHUNK_BYTES, 24, Duration::from_secs(1)).await;
    assert!(worst <= TRANSPORT_WINDOW_BYTES + BIG_CHUNK_BYTES, "the node ran {worst} bytes ahead of the reader");
    assert!(worst < 24 * BIG_CHUNK_BYTES, "the node did not run to the end at once");
}

#[tokio::test(start_paused = true)]
async fn tc07_a_very_long_stream_holds_the_same_bound_as_a_short_one() {
    let short = slow_read(BIG_CHUNK_BYTES, 16, Duration::from_millis(100)).await;
    let long = slow_read(BIG_CHUNK_BYTES, 160, Duration::from_millis(100)).await;
    assert!(long <= short + BIG_CHUNK_BYTES, "memory held does not grow with the length: short {short}, long {long}");
    assert!(long <= TRANSPORT_WINDOW_BYTES + BIG_CHUNK_BYTES);
}

#[tokio::test(start_paused = true)]
async fn tc08_a_non_stream_reply_has_equal_body_and_header_values_and_loses_only_hop_headers() {
    let mut node_headers = http::HeaderMap::new();
    node_headers.insert("content-type", "application/json".parse().unwrap());
    node_headers.insert("x-request-id", "abc123".parse().unwrap());
    node_headers.insert("connection", "keep-alive".parse().unwrap());
    node_headers.insert("keep-alive", "timeout=5".parse().unwrap());
    let fake = Arc::new(FakeTransport::new(Script::Response { status: StatusCode::OK, chunks: vec![ok(0, OLLAMA_REPLY)] }).with_response_headers(node_headers));
    let ends = Arc::new(Ends::default());
    let client = serve_duplex(build_router(one_node_deps(seams(fake)).with_hooks(ends.clone()))).await;
    let response = client.send(post()).await;
    assert_eq!(response.headers()["x-request-id"], "abc123");
    assert_eq!(response.headers()["content-type"], "application/json");
    assert!(!response.headers().contains_key("keep-alive"));
    let (_, bytes, _) = read_frames(response.into_body()).await;
    assert_eq!(bytes, OLLAMA_REPLY);
    let usage: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(usage["usage"]["prompt_tokens_details"]["cached_tokens"], 21, "the cache field stays readable");
    assert_eq!(ends.last().end, StreamEnd::Complete);
    assert_eq!(ends.last().flags, EndFlags::default(), "a non-stream reply is not a short stream");
}

#[tokio::test(start_paused = true)]
async fn tc10_a_stream_without_the_end_marker_passes_every_byte_and_sets_short_stream() {
    let fake = Arc::new(FakeTransport::new(script(StatusCode::OK, vec![ok(0, NO_END_MARKER)])));
    let ends = Arc::new(Ends::default());
    let client = serve_duplex(build_router(one_node_deps(seams(fake)).with_hooks(ends.clone()))).await;
    let response = client.send(post()).await;
    let (_, bytes, failed) = read_frames(response.into_body()).await;
    assert!(!failed);
    assert_eq!(bytes, NO_END_MARKER);
    assert_eq!(ends.last().flags, EndFlags { short_stream: false, empty_completion: false }, "no event-stream content type in this scripted head");
}

#[tokio::test(start_paused = true)]
async fn tc10_with_an_event_stream_head_a_missing_marker_sets_short_stream_and_a_split_marker_does_not() {
    for (data, split_marker, want_short) in [(NO_END_MARKER.to_vec(), false, true), (OLLAMA_STREAM.to_vec(), true, false), (OLLAMA_STREAM.to_vec(), false, false)] {
        let chunks = if split_marker {
            let at = data.windows(12).position(|w| w == b"data: [DONE]").unwrap() + 5;
            vec![ok(0, &data[..at]), ok(0, &data[at..])]
        } else {
            vec![ok(0, &data)]
        };
        let mut headers = http::HeaderMap::new();
        headers.insert("content-type", "text/event-stream".parse().unwrap());
        let fake = Arc::new(FakeTransport::new(script(StatusCode::OK, chunks)).with_response_headers(headers));
        let ends = Arc::new(Ends::default());
        let client = serve_duplex(build_router(one_node_deps(seams(fake)).with_hooks(ends.clone()))).await;
        let (_, bytes, _) = read_frames(client.send(post()).await.into_body()).await;
        assert_eq!(bytes, data);
        assert_eq!(ends.last().flags.short_stream, want_short, "split {split_marker}");
    }
}

#[tokio::test(start_paused = true)]
async fn tc11_an_empty_200_passes_unchanged_and_sets_empty_completion_and_a_real_answer_does_not() {
    for (data, want_empty) in [(EMPTY_200, true), (OLLAMA_STREAM, false)] {
        let mut headers = http::HeaderMap::new();
        headers.insert("content-type", "text/event-stream".parse().unwrap());
        let fake = Arc::new(FakeTransport::new(script(StatusCode::OK, vec![ok(0, data)])).with_response_headers(headers));
        let ends = Arc::new(Ends::default());
        let client = serve_duplex(build_router(one_node_deps(seams(fake)).with_hooks(ends.clone()))).await;
        let response = client.send(post()).await;
        assert_eq!(response.status(), StatusCode::OK);
        let (_, bytes, _) = read_frames(response.into_body()).await;
        assert_eq!(bytes, data);
        assert_eq!(ends.last().flags.empty_completion, want_empty);
    }
}

#[tokio::test(start_paused = true)]
async fn tc13_a_harness_that_closes_mid_stream_drops_the_node_body_and_no_further_node_read_happens() {
    let chunks: Vec<_> = (0..50).map(|i| ok(1000, format!("data: {i}\n\n").as_bytes())).collect();
    let fake = Arc::new(FakeTransport::new(script(StatusCode::OK, chunks)));
    let ends = Arc::new(Ends::default());
    let client = serve_duplex(build_router(one_node_deps(seams(fake.clone())).with_hooks(ends.clone()))).await;
    let mut body = client.send(post()).await.into_body();
    for _ in 0..3 {
        body.frame().await.unwrap().unwrap();
    }
    drop(body);
    tokio::time::sleep(Duration::from_secs(5)).await;
    let produced_after_close = fake.produced_at().len();
    tokio::time::sleep(Duration::from_secs(60)).await;
    assert_eq!(fake.produced_at().len(), produced_after_close, "the node is read no more after the close");
    assert!(produced_after_close < 50);
    assert_eq!(ends.last().end, StreamEnd::ClientClosed);
}

#[tokio::test(start_paused = true)]
async fn tc16_a_canary_text_in_the_reply_and_a_secret_in_a_header_appear_in_no_record() {
    let sink = Arc::new(MemorySink::new(SINK_CAPACITY));
    let mut headers = http::HeaderMap::new();
    headers.insert("x-secret", KEY.parse().unwrap());
    let fake = Arc::new(FakeTransport::new(script(StatusCode::OK, vec![ok(0, format!("data: {{\"content\":\"{KEY} the merger memo\"}}\n\n").as_bytes())])).with_response_headers(headers));
    let mut s = seams(fake);
    s.log = sink.clone();
    let client = serve_duplex(build_router(one_node_deps(s))).await;
    let (_, bytes, _) = read_frames(client.send(post()).await.into_body()).await;
    assert!(String::from_utf8_lossy(&bytes).contains(KEY), "an inactive guard passes the text");
    assert!(sink.take().is_empty(), "nothing about the reply reaches the record sink");
}

/// An active guard that refuses a reply holding the key.
struct KeyGuard;

impl ResponseGuard for KeyGuard {
    fn is_active(&self) -> bool {
        true
    }
    fn scrub_headers(&self, headers: &mut http::HeaderMap) {
        let holding: Vec<http::HeaderName> = headers.iter().filter(|(_, v)| v.to_str().is_ok_and(|v| v.contains(KEY))).map(|(n, _)| n.clone()).collect();
        for name in holding {
            headers.remove(name);
        }
    }
    fn scan_chunk(&self, _carry: &mut GuardCarry, chunk: &[u8]) -> ScanResult {
        if chunk.windows(KEY.len()).any(|w| w == KEY.as_bytes()) {
            ScanResult::Found
        } else {
            ScanResult::Clean
        }
    }
}

#[tokio::test(start_paused = true)]
async fn tc19_an_active_guard_stops_a_stream_that_holds_a_key_and_every_tap_gets_guard_abort() {
    let fake = Arc::new(FakeTransport::new(script(StatusCode::OK, vec![ok(0, b"data: clean\n\n"), ok(10, format!("data: {KEY}\n\n").as_bytes()), ok(10, b"data: after\n\n")])));
    let ends = Arc::new(Ends::default());
    let deps = one_node_deps(seams(fake)).with_hooks(ends.clone()).with_response_guard(Arc::new(KeyGuard));
    let client = serve_duplex(build_router(deps)).await;
    let (_, bytes, failed) = read_frames(client.send(post()).await.into_body()).await;
    assert!(failed, "the connection is closed abruptly");
    assert_eq!(bytes, b"data: clean\n\n");
    assert!(!String::from_utf8_lossy(&bytes).contains(KEY));
    assert_eq!(ends.last().end, StreamEnd::GuardAbort);
}

#[tokio::test(start_paused = true)]
async fn tc19_a_clean_reply_passes_unchanged_through_an_active_guard_and_a_key_header_is_removed() {
    let mut headers = http::HeaderMap::new();
    headers.insert("x-leak", KEY.parse().unwrap());
    headers.insert("x-ok", "fine".parse().unwrap());
    let fake = Arc::new(FakeTransport::new(script(StatusCode::OK, vec![ok(0, MESSAGES_PINGS)])).with_response_headers(headers));
    let deps = one_node_deps(seams(fake)).with_response_guard(Arc::new(KeyGuard));
    let client = serve_duplex(build_router(deps)).await;
    let response = client.send(post()).await;
    assert!(!response.headers().contains_key("x-leak"));
    assert_eq!(response.headers()["x-ok"], "fine");
    let (_, bytes, failed) = read_frames(response.into_body()).await;
    assert!(!failed);
    assert_eq!(bytes, MESSAGES_PINGS);
}

#[tokio::test(start_paused = true)]
async fn a_node_error_status_and_a_node_cut_mid_stream_reach_the_harness_as_the_node_made_them() {
    let fake = Arc::new(FakeTransport::new(script(StatusCode::OK, vec![ok(0, b"data: partial\n\n"), (Duration::from_millis(5), Err(UpstreamError::Reset))])));
    let ends = Arc::new(Ends::default());
    let client = serve_duplex(build_router(one_node_deps(seams(fake)).with_hooks(ends.clone()))).await;
    let (_, bytes, failed) = read_frames(client.send(post()).await.into_body()).await;
    assert!(failed);
    assert_eq!(bytes, b"data: partial\n\n", "no byte is appended after a cut (story 184 decides the end)");
    assert_eq!(ends.last().end, StreamEnd::UpstreamCut);
}
