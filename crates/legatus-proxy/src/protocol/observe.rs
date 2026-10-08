//! The read-only end observer (story 145): did a stream end without its end marker, and did a 200
//! answer carry nothing at all? It keeps no reply text: a short tail window and a few booleans.
use crate::protocol::stream::{EndFlags, CHAT_END_MARKER, MESSAGES_END_MARKER, RESPONSES_END_MARKER};
use crate::stream::tap::{ResponseHead, ResponseTap, StreamEnd};
use legatus_common::protocol::Protocol;

/// How many bytes of the previous chunks are kept so that a marker or key split across chunks is
/// found. It covers the longest key, the white space after it and the start of its value.
const TAIL_WINDOW: usize = 64;
const EVENT_STREAM: &str = "text/event-stream";
const STATUS_OK: u16 = 200;

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}

/// The bytes after `"key"` and an optional colon with white space around it.
fn value_after(buf: &[u8], key_end: usize) -> Option<&[u8]> {
    let mut at = key_end;
    while buf.get(at).copied().is_some_and(is_ws) {
        at += 1;
    }
    if buf.get(at) != Some(&b':') {
        return None;
    }
    at += 1;
    while buf.get(at).copied().is_some_and(is_ws) {
        at += 1;
    }
    buf.get(at..)
}

fn find_all(buf: &[u8], needle: &[u8]) -> Vec<usize> {
    if needle.is_empty() || buf.len() < needle.len() {
        return Vec::new();
    }
    (0..=buf.len() - needle.len()).filter(|&i| &buf[i..i + needle.len()] == needle).collect()
}

/// Does `"key"` occur with a value that satisfies `accept`?
fn key_has_value(buf: &[u8], key: &str, accept: impl Fn(&[u8]) -> bool) -> bool {
    let needle = format!("\"{key}\"");
    find_all(buf, needle.as_bytes()).into_iter().any(|at| value_after(buf, at + needle.len()).is_some_and(&accept))
}

/// A string value that is not empty: an opening quote and then any byte but a closing quote.
fn non_empty_string(value: &[u8]) -> bool {
    value.first() == Some(&b'"') && value.get(1).is_some_and(|b| *b != b'"')
}

fn starts_with(value: &[u8], text: &str) -> bool {
    value.starts_with(text.as_bytes())
}

type Report = Box<dyn Fn(StreamEnd, EndFlags) + Send>;

pub struct EndObserver {
    protocol: Protocol,
    status: u16,
    is_event_stream: bool,
    tail: Vec<u8>,
    saw_marker: bool,
    saw_content: bool,
    saw_tool_call: bool,
    saw_finish_stop: bool,
    end: Option<StreamEnd>,
    report: Option<Report>,
}

impl EndObserver {
    pub fn new(protocol: Protocol) -> EndObserver {
        EndObserver {
            protocol,
            status: 0,
            is_event_stream: false,
            tail: Vec::new(),
            saw_marker: false,
            saw_content: false,
            saw_tool_call: false,
            saw_finish_stop: false,
            end: None,
            report: None,
        }
    }

    /// Call `report` once, at the end of the reply, with how it ended and the flags.
    pub fn with_report(mut self, report: impl Fn(StreamEnd, EndFlags) + Send + 'static) -> EndObserver {
        self.report = Some(Box::new(report));
        self
    }

    fn marker(&self) -> Option<&'static [u8]> {
        match self.protocol {
            Protocol::OpenAiChat => Some(CHAT_END_MARKER),
            Protocol::AnthropicMessages => Some(MESSAGES_END_MARKER),
            Protocol::OpenAiResponses => Some(RESPONSES_END_MARKER),
            Protocol::Passthrough => None,
        }
    }

    fn scan(&mut self, buf: &[u8]) {
        if let Some(marker) = self.marker() {
            if !find_all(buf, marker).is_empty() {
                self.saw_marker = true;
            }
        }
        match self.protocol {
            Protocol::OpenAiChat => {
                self.saw_content |= key_has_value(buf, "content", non_empty_string);
                self.saw_tool_call |= key_has_value(buf, "tool_calls", |v| v.first() == Some(&b'['));
                self.saw_finish_stop |= key_has_value(buf, "finish_reason", |v| starts_with(v, "\"stop\""));
            }
            Protocol::AnthropicMessages => {
                self.saw_content |= key_has_value(buf, "text", non_empty_string) || key_has_value(buf, "partial_json", non_empty_string);
                self.saw_tool_call |= key_has_value(buf, "type", |v| starts_with(v, "\"tool_use\""));
                self.saw_finish_stop |= key_has_value(buf, "stop_reason", |v| starts_with(v, "\"end_turn\""));
            }
            Protocol::OpenAiResponses | Protocol::Passthrough => {}
        }
    }
}

impl ResponseTap for EndObserver {
    fn on_head(&mut self, head: &ResponseHead) {
        self.status = head.status.as_u16();
        self.is_event_stream = head.headers.get(http::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).is_some_and(|v| v.contains(EVENT_STREAM));
    }

    fn on_chunk(&mut self, chunk: &[u8]) {
        let mut buf = Vec::with_capacity(self.tail.len() + chunk.len());
        buf.extend_from_slice(&self.tail);
        buf.extend_from_slice(chunk);
        self.scan(&buf);
        let keep = buf.len().min(TAIL_WINDOW);
        self.tail = buf[buf.len() - keep..].to_vec();
    }

    fn on_end(&mut self, end: StreamEnd) {
        self.end = Some(end);
        if let Some(report) = &self.report {
            report(end, end_flags(self));
        }
    }
}

/// The flags of a finished reply. A reply that ended any way but `Complete` sets neither flag.
pub fn end_flags(obs: &EndObserver) -> EndFlags {
    let complete = obs.end == Some(StreamEnd::Complete);
    EndFlags {
        short_stream: complete && obs.is_event_stream && obs.marker().is_some() && !obs.saw_marker,
        empty_completion: complete && obs.status == STATUS_OK && !obs.saw_content && !obs.saw_tool_call && obs.saw_finish_stop,
    }
}
