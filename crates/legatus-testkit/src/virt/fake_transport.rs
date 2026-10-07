//! Fake node transport with per-chunk delays on the runtime clock (base of contract C03).
use async_trait::async_trait;
use bytes::Bytes;
use futures_util::stream;
use http::{HeaderMap, Method, StatusCode, Uri};
use legatus_proxy::upstream::transport::{
    BodyStream, UpstreamError, UpstreamRequest, UpstreamResponse, UpstreamTransport,
};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// What the fake does with a request.
#[derive(Clone)]
pub enum Script {
    /// Refuse before the first byte.
    Connect,
    /// Answer with a status and a stream; each item is delayed on pull.
    Response { status: StatusCode, chunks: Vec<(Duration, Result<Bytes, UpstreamError>)> },
}

impl Script {
    /// A 200 answer with `count` chunks, `gap` apart.
    pub fn chunks(count: usize, gap: Duration) -> Script {
        let chunks = (0..count).map(|i| (gap, Ok(Bytes::from(format!("chunk-{i};"))))).collect();
        Script::Response { status: StatusCode::OK, chunks }
    }
}

#[derive(Debug, Clone)]
pub struct RecordedRequest {
    pub method: Method,
    pub uri: Uri,
    pub headers: HeaderMap,
    pub body_len: usize,
    /// The body as the node received it.
    pub body: Bytes,
}

pub struct FakeTransport {
    script: Script,
    requests: Mutex<Vec<RecordedRequest>>,
    /// Instants (ms since the first pull) at which each chunk was produced, per request.
    produced: Arc<Mutex<Vec<tokio::time::Instant>>>,
}

impl FakeTransport {
    pub fn new(script: Script) -> FakeTransport {
        FakeTransport { script, requests: Mutex::new(Vec::new()), produced: Arc::new(Mutex::new(Vec::new())) }
    }
    pub fn requests(&self) -> Vec<RecordedRequest> {
        self.requests.lock().map(|r| r.clone()).unwrap_or_default()
    }
    /// Instants at which the fake produced each chunk (when it was pulled and its delay ended).
    pub fn produced_at(&self) -> Vec<tokio::time::Instant> {
        self.produced.lock().map(|p| p.clone()).unwrap_or_default()
    }
}

#[async_trait]
impl UpstreamTransport for FakeTransport {
    async fn send(&self, req: UpstreamRequest) -> Result<UpstreamResponse, UpstreamError> {
        if let Ok(mut r) = self.requests.lock() {
            r.push(RecordedRequest {
                method: req.method,
                uri: req.uri,
                headers: req.headers,
                body_len: req.body.len(),
                body: req.body,
            });
        }
        match &self.script {
            Script::Connect => Err(UpstreamError::Connect),
            Script::Response { status, chunks } => {
                let queue: VecDeque<_> = chunks.clone().into();
                let produced = self.produced.clone();
                let body: BodyStream = Box::pin(stream::unfold(queue, move |mut queue| {
                    let produced = produced.clone();
                    async move {
                        let (delay, item) = queue.pop_front()?;
                        tokio::time::sleep(delay).await;
                        if let Ok(mut p) = produced.lock() {
                            p.push(tokio::time::Instant::now());
                        }
                        Some((item, queue))
                    }
                }));
                Ok(UpstreamResponse { status: *status, headers: HeaderMap::new(), body })
            }
        }
    }
}
