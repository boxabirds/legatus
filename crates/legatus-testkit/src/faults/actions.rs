//! Fault actions applied to requests, in memory and in the stub process (both use this wrapper).
use super::rules::RuleEngine;
use crate::stubs::scenario::{FaultAction, FaultRule};
use async_trait::async_trait;
use bytes::Bytes;
use futures_util::{stream, StreamExt};
use http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use legatus_proxy::upstream::transport::{BodyStream, UpstreamError, UpstreamRequest, UpstreamResponse, UpstreamTransport};
use std::sync::Arc;
use std::time::Duration;

/// Wraps an engine and applies the fault rules of its stub.
pub struct FaultedTransport {
    inner: Arc<dyn UpstreamTransport>,
    engine: RuleEngine,
}

impl FaultedTransport {
    pub fn new(inner: Arc<dyn UpstreamTransport>, rules: Vec<FaultRule>) -> FaultedTransport {
        FaultedTransport { inner, engine: RuleEngine::new(rules) }
    }
}

fn single(status: StatusCode, headers: HeaderMap, body: Bytes) -> UpstreamResponse {
    let body: BodyStream = if body.is_empty() { Box::pin(stream::empty()) } else { Box::pin(stream::once(async move { Ok(body) })) };
    UpstreamResponse { status, headers, body }
}

#[async_trait]
impl UpstreamTransport for FaultedTransport {
    async fn send(&self, req: UpstreamRequest) -> Result<UpstreamResponse, UpstreamError> {
        let path = req.uri.path().to_string();
        let Some(action) = self.engine.decide(&path) else {
            return self.inner.send(req).await;
        };
        match action {
            FaultAction::Refuse => Err(UpstreamError::Connect),
            FaultAction::Reset { .. } => Err(UpstreamError::Reset),
            FaultAction::Hang => std::future::pending().await,
            FaultAction::SlowFirstByte { ms } => {
                tokio::time::sleep(Duration::from_millis(ms)).await;
                self.inner.send(req).await
            }
            FaultAction::Status { code, body, retry_after_s } => {
                let mut headers = HeaderMap::new();
                if let Some(s) = retry_after_s {
                    headers.insert(HeaderName::from_static("retry-after"), HeaderValue::from(s));
                }
                Ok(single(StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR), headers, Bytes::from(body)))
            }
            FaultAction::EmptyOk => Ok(single(StatusCode::OK, HeaderMap::new(), Bytes::new())),
            FaultAction::DropAfterChunks { n, clean_end } => {
                let reply = self.inner.send(req).await?;
                let taken = reply.body.take(n as usize);
                let body: BodyStream = if clean_end {
                    Box::pin(taken)
                } else {
                    Box::pin(taken.chain(stream::once(async { Err(UpstreamError::Reset) })))
                };
                Ok(UpstreamResponse { status: reply.status, headers: reply.headers, body })
            }
            FaultAction::SinkFault(_) | FaultAction::ClockJump { .. } => self.inner.send(req).await,
        }
    }
}
