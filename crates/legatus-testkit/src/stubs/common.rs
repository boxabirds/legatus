//! Helpers shared by the stub engines.
use bytes::Bytes;
use futures_util::stream;
use http::{HeaderMap, StatusCode};
use legatus_proxy::upstream::transport::{BodyStream, UpstreamError, UpstreamRequest, UpstreamResponse};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::Instant;

/// One line of a stub log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    pub at_ms: u64,
    pub path: String,
    pub event: String,
}

#[derive(Clone)]
pub struct StubLog {
    started: Instant,
    entries: Arc<Mutex<Vec<LogEntry>>>,
}

impl Default for StubLog {
    fn default() -> Self {
        StubLog { started: Instant::now(), entries: Arc::default() }
    }
}

impl StubLog {
    pub fn push(&self, path: &str, event: impl Into<String>) {
        let at_ms = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        if let Ok(mut e) = self.entries.lock() {
            e.push(LogEntry { at_ms, path: path.to_string(), event: event.into() });
        }
    }
    pub fn entries(&self) -> Vec<LogEntry> {
        self.entries.lock().map(|e| e.clone()).unwrap_or_default()
    }
    pub fn has(&self, event: &str) -> bool {
        self.entries().iter().any(|e| e.event == event)
    }
}

pub fn json_body(req: &UpstreamRequest) -> Option<serde_json::Value> {
    serde_json::from_slice(&req.body).ok()
}

pub fn u32_field(v: &serde_json::Value, name: &str) -> u32 {
    v.get(name).and_then(|x| x.as_u64()).and_then(|x| u32::try_from(x).ok()).unwrap_or(0)
}

/// The default number of output tokens when a request does not say.
pub const DEFAULT_OUTPUT_TOKENS: u32 = 8;

pub fn output_tokens(v: &serde_json::Value) -> u32 {
    v.get("output_tokens").and_then(|x| x.as_u64()).and_then(|x| u32::try_from(x).ok()).unwrap_or(DEFAULT_OUTPUT_TOKENS)
}

pub fn reply(status: u16, body: &str) -> UpstreamResponse {
    let bytes = Bytes::from(body.to_string());
    let stream: BodyStream = Box::pin(stream::once(async move { Ok(bytes) }));
    UpstreamResponse { status: StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR), headers: HeaderMap::new(), body: stream }
}

pub const END_MARKER: &str = "data: [DONE]\n\n";

/// Output stream: after `first_delay_ms` one chunk per token, `interval()` ms apart, then the end
/// marker. `guard` is dropped when the stream finishes or is dropped.
pub fn token_stream(
    first_delay_ms: u64,
    tokens: u32,
    interval: Arc<dyn Fn() -> u64 + Send + Sync>,
    guard: Box<dyn Send>,
) -> BodyStream {
    Box::pin(stream::unfold((0u32, Some(guard)), move |(i, guard)| {
        let interval = interval.clone();
        async move {
            if i > tokens {
                return None;
            }
            let wait = if i == 0 { first_delay_ms } else { interval() };
            if wait > 0 {
                tokio::time::sleep(Duration::from_millis(wait)).await;
            }
            let item: Result<Bytes, UpstreamError> = if i < tokens {
                Ok(Bytes::from(format!("data: {{\"t\":{i}}}\n\n")))
            } else {
                Ok(Bytes::from_static(END_MARKER.as_bytes()))
            };
            Some((item, (i + 1, guard)))
        }
    }))
}

/// Run `f` when dropped (stream end or drop).
pub struct OnDrop<F: FnMut() + Send>(pub F);
impl<F: FnMut() + Send> Drop for OnDrop<F> {
    fn drop(&mut self) {
        (self.0)();
    }
}
