//! Production transport: plain HTTP/1 to a node over hyper (no other module sends to a node).
use crate::upstream::transport::{BodyStream, UpstreamError, UpstreamRequest, UpstreamResponse, UpstreamTransport};
use async_trait::async_trait;
use bytes::Bytes;
use futures_util::StreamExt;
use http_body_util::{BodyStream as HttpBodyStream, Full};
use hyper_util::client::legacy::{connect::HttpConnector, Client};
use hyper_util::rt::{TokioExecutor, TokioTimer};
use std::sync::OnceLock;
use std::time::Duration;

/// Idle time after which a pooled connection is not reused when nothing else is set: the
/// catalogue default of `upstream_idle_reuse_max_s` (story 165).
pub const DEFAULT_IDLE_REUSE_MAX: Duration = Duration::from_secs(4);

type NodeClient = Client<HttpConnector, Full<Bytes>>;

pub struct HyperTransport {
    /// The pooled client and the idle time it was built with. Built on first use or by `configure`.
    client: OnceLock<(NodeClient, Duration)>,
}

fn build_client(idle_timeout: Duration) -> (NodeClient, Duration) {
    let client = Client::builder(TokioExecutor::new()).pool_idle_timeout(idle_timeout).pool_timer(TokioTimer::new()).build_http();
    (client, idle_timeout)
}

impl HyperTransport {
    /// A transport with the default idle time.
    pub fn new() -> HyperTransport {
        HyperTransport::with_idle_timeout(DEFAULT_IDLE_REUSE_MAX)
    }

    /// A connection idle longer than `idle_timeout` is dropped from the pool, so it is never
    /// reused (`upstream_idle_reuse_max_s`; story 151 tests the node side).
    pub fn with_idle_timeout(idle_timeout: Duration) -> HyperTransport {
        let transport = HyperTransport::unconfigured();
        transport.configure(idle_timeout);
        transport
    }

    /// A transport whose idle time is set later by `configure`; the first send uses the default
    /// if nothing was set.
    pub fn unconfigured() -> HyperTransport {
        HyperTransport { client: OnceLock::new() }
    }

    /// Set the idle time once. Returns false when the pool already exists.
    pub fn configure(&self, idle_timeout: Duration) -> bool {
        self.client.set(build_client(idle_timeout)).is_ok()
    }

    pub fn idle_timeout(&self) -> Duration {
        self.client.get_or_init(|| build_client(DEFAULT_IDLE_REUSE_MAX)).1
    }
}

impl Default for HyperTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl UpstreamTransport for HyperTransport {
    async fn send(&self, req: UpstreamRequest) -> Result<UpstreamResponse, UpstreamError> {
        let mut builder = http::Request::builder().method(req.method).uri(req.uri);
        if let Some(h) = builder.headers_mut() {
            *h = req.headers;
        }
        let request = builder.body(Full::new(req.body)).map_err(|_| UpstreamError::Connect)?;
        let client = &self.client.get_or_init(|| build_client(DEFAULT_IDLE_REUSE_MAX)).0;
        let response = client.request(request).await.map_err(|_| UpstreamError::Connect)?;
        let (parts, body) = response.into_parts();
        let stream = HttpBodyStream::new(body).filter_map(|frame| async move {
            match frame {
                Ok(frame) => frame.into_data().ok().map(Ok),
                Err(_) => Some(Err(UpstreamError::Reset)),
            }
        });
        let body: BodyStream = Box::pin(stream);
        Ok(UpstreamResponse { status: parts.status, headers: parts.headers, body })
    }
}
