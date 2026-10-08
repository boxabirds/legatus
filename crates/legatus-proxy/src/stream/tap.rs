//! The pull-through reply relay (story 145). The harness connection pulls a chunk, the stream pulls
//! the next one from the node, and nothing is queued between them: no channel, no spawned task, no
//! collect. Taps see each chunk by reference, so they cannot change a byte.
use crate::stream::guard::{GuardCarry, ResponseGuard, ScanResult};
use crate::upstream::transport::{BodyStream, UpstreamError};
use axum::body::Body;
use futures_util::Stream;
use legatus_common::protocol::Protocol;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};

/// The head of a reply as the taps see it.
pub struct ResponseHead {
    pub status: http::StatusCode,
    pub headers: http::HeaderMap,
    /// The protocol of the request this reply answers.
    pub protocol: Protocol,
}

pub type ResponseBody = Body;

/// How a reply ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamEnd {
    Complete,
    UpstreamCut,
    ClientClosed,
    GuardAbort,
}

/// A read-only observer of one reply. Chunks come by reference.
pub trait ResponseTap: Send {
    fn on_head(&mut self, _head: &ResponseHead) {}
    fn on_chunk(&mut self, chunk: &[u8]);
    fn on_end(&mut self, end: StreamEnd);
}

/// Counts for tests: how many chunks the relay took from the node and gave to the harness.
#[derive(Clone, Debug, Default)]
pub struct RelayProbe {
    from_node: Arc<AtomicUsize>,
    to_client: Arc<AtomicUsize>,
}

impl RelayProbe {
    pub fn from_node(&self) -> usize {
        self.from_node.load(Ordering::SeqCst)
    }
    pub fn to_client(&self) -> usize {
        self.to_client.load(Ordering::SeqCst)
    }
    /// Chunks read from the node and not yet handed to the harness. Never above
    /// `MAX_UNPULLED_CHUNKS`.
    pub fn unpulled_chunks(&self) -> usize {
        self.from_node().saturating_sub(self.to_client())
    }
}

struct Relay {
    body: BodyStream,
    guard: Arc<dyn ResponseGuard>,
    carry: GuardCarry,
    taps: Vec<Box<dyn ResponseTap>>,
    probe: RelayProbe,
    ended: bool,
}

impl Relay {
    fn end(&mut self, end: StreamEnd) {
        if !self.ended {
            self.ended = true;
            for tap in self.taps.iter_mut() {
                tap.on_end(end);
            }
        }
    }
}

impl Stream for Relay {
    type Item = Result<bytes::Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = &mut *self;
        if this.ended {
            return Poll::Ready(None);
        }
        match this.body.as_mut().poll_next(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(None) => {
                this.end(StreamEnd::Complete);
                Poll::Ready(None)
            }
            Poll::Ready(Some(Err(error))) => {
                this.end(StreamEnd::UpstreamCut);
                Poll::Ready(Some(Err(cut_error(error))))
            }
            Poll::Ready(Some(Ok(chunk))) => {
                this.probe.from_node.fetch_add(1, Ordering::SeqCst);
                if this.guard.is_active() && this.guard.scan_chunk(&mut this.carry, &chunk) == ScanResult::Found {
                    this.end(StreamEnd::GuardAbort);
                    return Poll::Ready(Some(Err(std::io::Error::other("the reply held a hosted key"))));
                }
                this.probe.to_client.fetch_add(1, Ordering::SeqCst);
                for tap in this.taps.iter_mut() {
                    tap.on_chunk(&chunk);
                }
                Poll::Ready(Some(Ok(chunk)))
            }
        }
    }
}

impl Drop for Relay {
    /// The harness closed the connection before the node body ended.
    fn drop(&mut self) {
        self.end(StreamEnd::ClientClosed);
    }
}

fn cut_error(error: UpstreamError) -> std::io::Error {
    std::io::Error::other(error.to_string())
}

/// Relay a node reply: the same bytes in the same order. The head goes out when it arrives. The
/// guard stage runs first (free while it is inactive), then every tap, in list order.
pub fn copy_response(head: ResponseHead, body: BodyStream, guard: Arc<dyn ResponseGuard>, taps: Vec<Box<dyn ResponseTap>>) -> (http::StatusCode, http::HeaderMap, ResponseBody) {
    let (status, headers, body, _probe) = copy_response_probed(head, body, guard, taps);
    (status, headers, body)
}

/// The same as `copy_response`, also returning the probe that tests use to count chunks.
pub fn copy_response_probed(
    head: ResponseHead,
    body: BodyStream,
    guard: Arc<dyn ResponseGuard>,
    mut taps: Vec<Box<dyn ResponseTap>>,
) -> (http::StatusCode, http::HeaderMap, ResponseBody, RelayProbe) {
    let mut headers = crate::protocol::headers::strip_reply_headers(&head.headers);
    if guard.is_active() {
        guard.scrub_headers(&mut headers);
    }
    let status = head.status;
    for tap in taps.iter_mut() {
        tap.on_head(&ResponseHead { status, headers: headers.clone(), protocol: head.protocol });
    }
    let probe = RelayProbe::default();
    let relay = Relay { body, guard, carry: GuardCarry::default(), taps, probe: probe.clone(), ended: false };
    (status, headers, Body::from_stream(relay), probe)
}
