//! Story 145 unit tests: byte identity of the relay, headers, the guard interface, the taps and
//! the end observer, on recorded streams (a real Ollama recording and hand-made shapes).
use bytes::Bytes;
use futures_util::stream;
use http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use http_body_util::BodyExt;
use legatus_common::protocol::Protocol;
use legatus_proxy::protocol::headers::strip_reply_headers;
use legatus_proxy::protocol::observe::{end_flags, EndObserver};
use legatus_proxy::protocol::stream::{EndFlags, CHAT_END_MARKER, MAX_UNPULLED_CHUNKS};
use legatus_proxy::stream::guard::{GuardCarry, NoGuard, ResponseGuard, ScanResult};
use legatus_proxy::stream::tap::{copy_response, copy_response_probed, ResponseHead, ResponseTap, StreamEnd};
use legatus_proxy::upstream::transport::{BodyStream, UpstreamError};
use std::sync::{Arc, Mutex};

const OLLAMA_STREAM: &[u8] = include_bytes!("../fixtures/streams/ollama_chat_stream.sse");
const OLLAMA_REPLY: &[u8] = include_bytes!("../fixtures/streams/ollama_chat_reply.json");
const COMMENTS_CRLF: &[u8] = include_bytes!("../fixtures/streams/chat_comments_crlf.sse");
const MESSAGES_PINGS: &[u8] = include_bytes!("../fixtures/streams/messages_with_pings.sse");
const NO_END_MARKER: &[u8] = include_bytes!("../fixtures/streams/chat_no_end_marker.sse");
const EMPTY_200: &[u8] = include_bytes!("../fixtures/streams/chat_empty_200.sse");
const TOOL_CALL: &[u8] = include_bytes!("../fixtures/streams/chat_tool_call.sse");
const SPLIT_SEEDS: [u64; 6] = [1, 2, 3, 17, 99, 12345];
const KEY: &str = "sk-ant-api03-Zk3QfN8vLm2PxTcR9wYbHd7sUe1AoJqGiV5nXtKzB4";

/// A small deterministic generator, so that a split can be reproduced from its seed.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
}

fn split(data: &[u8], seed: u64) -> Vec<Bytes> {
    let mut rng = Lcg(seed);
    let mut chunks = Vec::new();
    let mut at = 0;
    while at < data.len() {
        let len = 1 + usize::try_from(rng.next() % 37).unwrap();
        let end = (at + len).min(data.len());
        chunks.push(Bytes::copy_from_slice(&data[at..end]));
        at = end;
    }
    chunks
}

fn body_of(chunks: Vec<Bytes>) -> BodyStream {
    Box::pin(stream::iter(chunks.into_iter().map(Ok)))
}

fn head(protocol: Protocol, content_type: &str) -> ResponseHead {
    let mut headers = HeaderMap::new();
    headers.insert("content-type", HeaderValue::from_str(content_type).unwrap());
    ResponseHead { status: StatusCode::OK, headers, protocol }
}

async fn relay(protocol: Protocol, chunks: Vec<Bytes>, taps: Vec<Box<dyn ResponseTap>>) -> Vec<u8> {
    let (_, _, body) = copy_response(head(protocol, "text/event-stream"), body_of(chunks), Arc::new(NoGuard), taps);
    body.collect().await.unwrap().to_bytes().to_vec()
}

#[tokio::test]
async fn tc01_output_equals_input_for_the_real_recording_at_random_chunk_splits() {
    assert!(OLLAMA_STREAM.ends_with(b"data: [DONE]\n\n"), "a real recording");
    for seed in SPLIT_SEEDS {
        assert_eq!(relay(Protocol::OpenAiChat, split(OLLAMA_STREAM, seed), vec![]).await, OLLAMA_STREAM, "seed {seed}");
    }
    assert_eq!(relay(Protocol::OpenAiChat, vec![Bytes::from_static(OLLAMA_STREAM)], vec![]).await, OLLAMA_STREAM, "one chunk");
    assert_eq!(relay(Protocol::OpenAiChat, vec![], vec![]).await, Vec::<u8>::new(), "zero chunks");
    let single_bytes: Vec<Bytes> = OLLAMA_STREAM.iter().map(|b| Bytes::copy_from_slice(&[*b])).collect();
    assert_eq!(relay(Protocol::OpenAiChat, single_bytes, vec![]).await, OLLAMA_STREAM, "chunks of one byte");
}

#[tokio::test]
async fn tc02_comments_crlf_and_multi_line_data_are_identical_byte_for_byte() {
    for seed in SPLIT_SEEDS {
        assert_eq!(relay(Protocol::OpenAiChat, split(COMMENTS_CRLF, seed), vec![]).await, COMMENTS_CRLF, "seed {seed}");
    }
    assert!(COMMENTS_CRLF.windows(2).any(|w| w == b"\r\n") && COMMENTS_CRLF.starts_with(b": keep-alive"));
}

#[tokio::test]
async fn tc03_messages_ping_events_are_identical_byte_for_byte() {
    for seed in SPLIT_SEEDS {
        assert_eq!(relay(Protocol::AnthropicMessages, split(MESSAGES_PINGS, seed), vec![]).await, MESSAGES_PINGS, "seed {seed}");
    }
}

#[tokio::test]
async fn tc09_a_gzip_body_and_its_content_encoding_header_pass_as_sent() {
    let gzip: Vec<u8> = vec![0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0xff, 0x00, 0x7f, 0x80, 0xfe, 0x01, 0x02];
    let mut headers = HeaderMap::new();
    headers.insert("content-encoding", HeaderValue::from_static("gzip"));
    headers.insert("content-type", HeaderValue::from_static("application/json"));
    let (status, out_headers, body) = copy_response(ResponseHead { status: StatusCode::OK, headers, protocol: Protocol::OpenAiChat }, body_of(vec![Bytes::from(gzip.clone())]), Arc::new(NoGuard), vec![]);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(out_headers["content-encoding"], "gzip");
    assert_eq!(body.collect().await.unwrap().to_bytes().to_vec(), gzip);
}

fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (n, v) in pairs {
        map.append(HeaderName::from_bytes(n.as_bytes()).unwrap(), HeaderValue::from_str(v).unwrap());
    }
    map
}

#[test]
fn tc14_reply_headers_keep_every_name_and_value_lose_only_the_hop_headers_and_gain_nothing() {
    let node = headers(&[
        ("content-type", "text/event-stream"),
        ("cache-control", "no-cache"),
        ("x-request-id", "abc"),
        ("set-cookie", "a=1"),
        ("set-cookie", "b=2"),
        ("connection", "keep-alive, x-private"),
        ("x-private", "hop"),
        ("keep-alive", "timeout=5"),
        ("transfer-encoding", "chunked"),
        ("content-length", "12"),
    ]);
    let out = strip_reply_headers(&node);
    let names: Vec<&str> = out.keys().map(|k| k.as_str()).collect();
    assert_eq!(names.len(), 5, "{names:?}");
    for kept in ["content-type", "cache-control", "x-request-id", "set-cookie", "content-length"] {
        assert!(out.contains_key(kept), "{kept}");
    }
    assert_eq!(out.get_all("set-cookie").iter().map(|v| v.to_str().unwrap()).collect::<Vec<_>>(), vec!["a=1", "b=2"]);
    assert_eq!(out["content-length"], "12", "a length the node sent passes with the body it describes");
}

#[tokio::test]
async fn tc14_the_output_is_never_longer_than_the_input_and_no_marker_is_added() {
    for fixture in [NO_END_MARKER, EMPTY_200, OLLAMA_STREAM, MESSAGES_PINGS, TOOL_CALL] {
        let out = relay(Protocol::OpenAiChat, split(fixture, 7), vec![]).await;
        assert_eq!(out.len(), fixture.len());
    }
    let out = relay(Protocol::OpenAiChat, split(NO_END_MARKER, 7), vec![]).await;
    assert!(!out.windows(CHAT_END_MARKER.len()).any(|w| w == CHAT_END_MARKER), "no end marker is added");
}

#[tokio::test]
async fn tc12_the_reply_model_name_is_not_rewritten() {
    let out = relay(Protocol::OpenAiChat, split(OLLAMA_STREAM, 3), vec![]).await;
    assert!(String::from_utf8_lossy(&out).contains("\"model\":\"qwen3:1.7b\""));
    let (_, _, body) = copy_response(head(Protocol::OpenAiChat, "application/json"), body_of(vec![Bytes::from_static(OLLAMA_REPLY)]), Arc::new(NoGuard), vec![]);
    assert_eq!(body.collect().await.unwrap().to_bytes().to_vec(), OLLAMA_REPLY);
}

/// A tap that records what it saw.
#[derive(Clone, Default)]
struct Recorder {
    name: &'static str,
    log: Arc<Mutex<Vec<String>>>,
    bytes: Arc<Mutex<Vec<u8>>>,
}

impl ResponseTap for Recorder {
    fn on_head(&mut self, head: &ResponseHead) {
        self.log.lock().unwrap().push(format!("{}:head:{}", self.name, head.status.as_u16()));
    }
    fn on_chunk(&mut self, chunk: &[u8]) {
        self.bytes.lock().unwrap().extend_from_slice(chunk);
        self.log.lock().unwrap().push(format!("{}:chunk", self.name));
    }
    fn on_end(&mut self, end: StreamEnd) {
        self.log.lock().unwrap().push(format!("{}:end:{end:?}", self.name));
    }
}

fn recorders(names: &[&'static str]) -> (Arc<Mutex<Vec<String>>>, Vec<Recorder>) {
    let log = Arc::new(Mutex::new(Vec::new()));
    (log.clone(), names.iter().map(|name| Recorder { name, log: log.clone(), bytes: Arc::default() }).collect())
}

fn boxed(taps: &[Recorder]) -> Vec<Box<dyn ResponseTap>> {
    taps.iter().cloned().map(|t| Box::new(t) as Box<dyn ResponseTap>).collect()
}

#[tokio::test]
async fn tc18_every_tap_sees_the_head_each_chunk_in_list_order_and_the_end_complete() {
    let (log, taps) = recorders(&["observer", "cut", "id", "usage"]);
    let chunks = vec![Bytes::from_static(b"one;"), Bytes::from_static(b"two;")];
    let out = relay(Protocol::OpenAiChat, chunks, boxed(&taps)).await;
    assert_eq!(out, b"one;two;");
    let log = log.lock().unwrap().clone();
    let expect_chunk = |name: &str| format!("{name}:chunk");
    assert_eq!(&log[..4], &["observer:head:200", "cut:head:200", "id:head:200", "usage:head:200"]);
    let first_chunk: Vec<&String> = log[4..8].iter().collect();
    assert_eq!(first_chunk, vec![&expect_chunk("observer"), &expect_chunk("cut"), &expect_chunk("id"), &expect_chunk("usage")]);
    assert_eq!(&log[log.len() - 4..], &["observer:end:Complete", "cut:end:Complete", "id:end:Complete", "usage:end:Complete"]);
    assert_eq!(*taps[0].bytes.lock().unwrap(), b"one;two;", "a tap gets the same bytes by reference");
}

#[tokio::test]
async fn tc17_a_node_body_error_reaches_every_tap_as_upstream_cut_and_no_byte_is_appended() {
    let (log, taps) = recorders(&["a", "b"]);
    let items: Vec<Result<Bytes, UpstreamError>> = vec![Ok(Bytes::from_static(b"partial;")), Err(UpstreamError::Reset), Ok(Bytes::from_static(b"never"))];
    let body: BodyStream = Box::pin(stream::iter(items));
    let (_, _, out) = copy_response(head(Protocol::OpenAiChat, "text/event-stream"), body, Arc::new(NoGuard), boxed(&taps));
    let mut out = out;
    let mut seen = Vec::new();
    let mut failed = false;
    while let Some(frame) = out.frame().await {
        match frame {
            Ok(frame) => seen.extend_from_slice(&frame.into_data().unwrap()),
            Err(_) => {
                failed = true;
                break;
            }
        }
    }
    assert!(failed, "the error reaches the server layer");
    assert_eq!(seen, b"partial;", "nothing is appended and the later chunk is not read");
    let log = log.lock().unwrap().clone();
    assert_eq!(&log[log.len() - 2..], &["a:end:UpstreamCut", "b:end:UpstreamCut"]);
}

#[tokio::test]
async fn tc13_dropping_the_body_before_the_end_gives_every_tap_client_closed() {
    let (log, taps) = recorders(&["a", "b"]);
    let body: BodyStream = Box::pin(stream::iter(vec![Ok(Bytes::from_static(b"1")), Ok(Bytes::from_static(b"2")), Ok(Bytes::from_static(b"3"))]));
    let (_, _, mut out) = copy_response(head(Protocol::OpenAiChat, "text/event-stream"), body, Arc::new(NoGuard), boxed(&taps));
    let first = out.frame().await.unwrap().unwrap().into_data().unwrap();
    assert_eq!(first, Bytes::from_static(b"1"));
    drop(out);
    let log = log.lock().unwrap().clone();
    assert_eq!(&log[log.len() - 2..], &["a:end:ClientClosed", "b:end:ClientClosed"]);
    assert_eq!(log.iter().filter(|l| l.ends_with(":chunk")).count(), 2, "the second chunk was never read");
}

/// A guard that counts what is asked of it.
struct Counting {
    active: bool,
    scrubs: Mutex<usize>,
    scans: Mutex<usize>,
}

impl ResponseGuard for Counting {
    fn is_active(&self) -> bool {
        self.active
    }
    fn scrub_headers(&self, headers: &mut HeaderMap) {
        *self.scrubs.lock().unwrap() += 1;
        let holding: Vec<HeaderName> = headers.iter().filter(|(_, v)| v.to_str().is_ok_and(|v| v.contains(KEY))).map(|(n, _)| n.clone()).collect();
        for name in holding {
            headers.remove(name);
        }
    }
    fn scan_chunk(&self, carry: &mut GuardCarry, chunk: &[u8]) -> ScanResult {
        *self.scans.lock().unwrap() += 1;
        let mut window = carry.tail.clone();
        window.extend_from_slice(chunk);
        let found = window.windows(KEY.len()).any(|w| w == KEY.as_bytes());
        let keep = KEY.len() - 1;
        carry.tail = window[window.len().saturating_sub(keep)..].to_vec();
        if found {
            ScanResult::Found
        } else {
            ScanResult::Clean
        }
    }
}

fn counting(active: bool) -> Arc<Counting> {
    Arc::new(Counting { active, scrubs: Mutex::new(0), scans: Mutex::new(0) })
}

#[tokio::test]
async fn tc20_with_an_inactive_guard_neither_scrub_nor_scan_is_called_and_a_key_shaped_text_passes() {
    let guard = counting(false);
    let chunks = vec![Bytes::from(format!("data: {{\"content\":\"{KEY}\"}}\n\n"))];
    let mut h = headers(&[("content-type", "text/event-stream"), ("x-echo", KEY)]);
    h.insert("connection", HeaderValue::from_static("close"));
    let (_, out_headers, body) = copy_response(ResponseHead { status: StatusCode::OK, headers: h, protocol: Protocol::OpenAiChat }, body_of(chunks.clone()), guard.clone(), vec![]);
    assert_eq!(out_headers["x-echo"], KEY);
    assert_eq!(body.collect().await.unwrap().to_bytes(), chunks[0]);
    assert_eq!((*guard.scrubs.lock().unwrap(), *guard.scans.lock().unwrap()), (0, 0));
    assert!(!NoGuard.is_active());
}

#[test]
fn tc21_an_active_guard_removes_the_header_that_holds_the_key_and_nothing_else() {
    let guard = counting(true);
    let h = headers(&[("content-type", "application/json"), ("x-leak", KEY), ("x-fine", "ok"), ("connection", "close"), ("transfer-encoding", "chunked")]);
    let (_, out, _body) = copy_response(ResponseHead { status: StatusCode::OK, headers: h, protocol: Protocol::OpenAiChat }, body_of(vec![]), guard.clone(), vec![]);
    assert!(!out.contains_key("x-leak"));
    assert_eq!(out["x-fine"], "ok");
    assert_eq!(out["content-type"], "application/json");
    assert!(!out.contains_key("connection") && !out.contains_key("transfer-encoding"), "hop headers are still removed");
    assert_eq!(*guard.scrubs.lock().unwrap(), 1);
}

#[tokio::test]
async fn tc19_an_active_guard_that_finds_a_key_stops_the_stream_without_sending_the_chunk_and_taps_get_guard_abort() {
    let guard = counting(true);
    let (log, taps) = recorders(&["a", "b"]);
    let body: BodyStream = Box::pin(stream::iter(vec![
        Ok(Bytes::from_static(b"data: clean\n\n")),
        Ok(Bytes::from(format!("data: {KEY}\n\n"))),
        Ok(Bytes::from_static(b"data: after\n\n")),
    ]));
    let (_, _, mut out) = copy_response(head(Protocol::OpenAiChat, "text/event-stream"), body, guard, boxed(&taps));
    let mut seen = Vec::new();
    let mut failed = false;
    while let Some(frame) = out.frame().await {
        match frame {
            Ok(frame) => seen.extend_from_slice(&frame.into_data().unwrap()),
            Err(_) => {
                failed = true;
                break;
            }
        }
    }
    assert!(failed);
    assert_eq!(seen, b"data: clean\n\n", "the chunk with the key was not sent and nothing was appended");
    let log = log.lock().unwrap().clone();
    assert_eq!(&log[log.len() - 2..], &["a:end:GuardAbort", "b:end:GuardAbort"]);
}

#[tokio::test]
async fn tc19_a_key_split_over_two_chunks_is_found_through_the_carry_the_relay_owns() {
    let guard = counting(true);
    let (head_part, tail_part) = KEY.split_at(20);
    let body: BodyStream = Box::pin(stream::iter(vec![Ok(Bytes::from(format!("x{head_part}"))), Ok(Bytes::from(format!("{tail_part}y")))]));
    let (_, _, mut out) = copy_response(head(Protocol::OpenAiChat, "text/event-stream"), body, guard, vec![]);
    let first = out.frame().await.unwrap().unwrap().into_data().unwrap();
    assert_eq!(first, Bytes::from(format!("x{head_part}")));
    assert!(out.frame().await.unwrap().is_err(), "the second chunk completes the key");
}

fn run_observer(protocol: Protocol, content_type: &str, status: StatusCode, chunks: Vec<Bytes>, end: StreamEnd) -> EndFlags {
    let flags: Arc<Mutex<Option<(StreamEnd, EndFlags)>>> = Arc::default();
    let sink = flags.clone();
    let mut observer = EndObserver::new(protocol).with_report(move |e, f| *sink.lock().unwrap() = Some((e, f)));
    let mut h = HeaderMap::new();
    h.insert("content-type", HeaderValue::from_str(content_type).unwrap());
    observer.on_head(&ResponseHead { status, headers: h, protocol });
    for chunk in &chunks {
        observer.on_chunk(chunk);
    }
    let direct_before_end = end_flags(&observer);
    assert_eq!(direct_before_end, EndFlags::default(), "no flag before the end");
    observer.on_end(end);
    let reported = flags.lock().unwrap().expect("reported once at the end");
    assert_eq!(reported.0, end);
    assert_eq!(reported.1, end_flags(&observer));
    reported.1
}

#[test]
fn tc10_a_stream_with_the_end_marker_is_complete_and_one_without_it_is_a_short_stream() {
    let ok = run_observer(Protocol::OpenAiChat, "text/event-stream", StatusCode::OK, split(OLLAMA_STREAM, 5), StreamEnd::Complete);
    assert_eq!(ok, EndFlags::default());
    let short = run_observer(Protocol::OpenAiChat, "text/event-stream", StatusCode::OK, split(NO_END_MARKER, 5), StreamEnd::Complete);
    assert_eq!(short, EndFlags { short_stream: true, empty_completion: false });
    let messages = run_observer(Protocol::AnthropicMessages, "text/event-stream", StatusCode::OK, split(MESSAGES_PINGS, 5), StreamEnd::Complete);
    assert_eq!(messages, EndFlags::default());
}

#[test]
fn tc10_a_marker_split_across_chunks_is_found_at_every_split_point() {
    let marker_at = OLLAMA_STREAM.windows(CHAT_END_MARKER.len()).position(|w| w == CHAT_END_MARKER).unwrap();
    for cut in marker_at + 1..marker_at + CHAT_END_MARKER.len() {
        let chunks = vec![Bytes::copy_from_slice(&OLLAMA_STREAM[..cut]), Bytes::copy_from_slice(&OLLAMA_STREAM[cut..])];
        let flags = run_observer(Protocol::OpenAiChat, "text/event-stream", StatusCode::OK, chunks, StreamEnd::Complete);
        assert!(!flags.short_stream, "split at {cut}");
    }
}

#[test]
fn tc10_a_cut_or_a_client_close_sets_neither_flag() {
    for end in [StreamEnd::UpstreamCut, StreamEnd::ClientClosed, StreamEnd::GuardAbort] {
        assert_eq!(run_observer(Protocol::OpenAiChat, "text/event-stream", StatusCode::OK, split(NO_END_MARKER, 5), end), EndFlags::default(), "{end:?}");
        assert_eq!(run_observer(Protocol::OpenAiChat, "text/event-stream", StatusCode::OK, split(EMPTY_200, 5), end), EndFlags::default(), "{end:?}");
    }
}

#[test]
fn tc10_a_non_stream_reply_is_never_a_short_stream() {
    let flags = run_observer(Protocol::OpenAiChat, "application/json", StatusCode::OK, vec![Bytes::from_static(OLLAMA_REPLY)], StreamEnd::Complete);
    assert_eq!(flags, EndFlags::default());
}

#[test]
fn tc11_an_empty_200_sets_empty_completion_and_a_200_with_content_or_a_tool_call_does_not() {
    let empty = run_observer(Protocol::OpenAiChat, "text/event-stream", StatusCode::OK, split(EMPTY_200, 5), StreamEnd::Complete);
    assert_eq!(empty, EndFlags { short_stream: false, empty_completion: true });
    assert_eq!(run_observer(Protocol::OpenAiChat, "text/event-stream", StatusCode::OK, split(OLLAMA_STREAM, 5), StreamEnd::Complete).empty_completion, false);
    assert_eq!(run_observer(Protocol::OpenAiChat, "text/event-stream", StatusCode::OK, split(TOOL_CALL, 5), StreamEnd::Complete).empty_completion, false);
    assert_eq!(run_observer(Protocol::OpenAiChat, "text/event-stream", StatusCode::BAD_GATEWAY, split(EMPTY_200, 5), StreamEnd::Complete).empty_completion, false, "only a 200 counts");
    assert_eq!(run_observer(Protocol::OpenAiChat, "application/json", StatusCode::OK, vec![Bytes::from_static(OLLAMA_REPLY)], StreamEnd::Complete).empty_completion, false, "a non-stream answer with text");
}

#[test]
fn tc11_the_empty_completion_scan_works_when_keys_are_split_and_when_white_space_is_added() {
    for seed in SPLIT_SEEDS {
        let flags = run_observer(Protocol::OpenAiChat, "text/event-stream", StatusCode::OK, split(EMPTY_200, seed), StreamEnd::Complete);
        assert!(flags.empty_completion, "seed {seed}");
        let content = run_observer(Protocol::OpenAiChat, "text/event-stream", StatusCode::OK, split(OLLAMA_STREAM, seed), StreamEnd::Complete);
        assert!(!content.empty_completion, "seed {seed}");
    }
    let spaced = b"data: {\"choices\":[{\"delta\": {\"content\" : \"hi\"}, \"finish_reason\" : \"stop\"}]}\n\ndata: [DONE]\n\n";
    assert!(!run_observer(Protocol::OpenAiChat, "text/event-stream", StatusCode::OK, split(spaced, 3), StreamEnd::Complete).empty_completion);
}

#[test]
fn tc11_a_messages_reply_with_text_is_not_empty_and_one_with_no_text_is() {
    let empty = b"event: content_block_start\ndata: {\"type\":\"content_block_start\",\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\nevent: message_delta\ndata: {\"delta\":{\"stop_reason\":\"end_turn\"}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";
    assert!(run_observer(Protocol::AnthropicMessages, "text/event-stream", StatusCode::OK, split(empty, 4), StreamEnd::Complete).empty_completion);
    assert!(!run_observer(Protocol::AnthropicMessages, "text/event-stream", StatusCode::OK, split(MESSAGES_PINGS, 4), StreamEnd::Complete).empty_completion);
}

#[tokio::test]
async fn tc12_the_observer_keeps_no_text_and_the_relay_probe_counts_chunks() {
    let (_, _, body, probe) = copy_response_probed(head(Protocol::OpenAiChat, "text/event-stream"), body_of(split(OLLAMA_STREAM, 4)), Arc::new(NoGuard), vec![Box::new(EndObserver::new(Protocol::OpenAiChat))]);
    let mut body = body;
    let mut frames = 0;
    while let Some(frame) = body.frame().await {
        frame.unwrap();
        frames += 1;
        assert!(probe.unpulled_chunks() <= MAX_UNPULLED_CHUNKS);
    }
    assert_eq!((probe.from_node(), probe.to_client()), (frames, frames));
    assert_eq!(MAX_UNPULLED_CHUNKS, 1);
}
