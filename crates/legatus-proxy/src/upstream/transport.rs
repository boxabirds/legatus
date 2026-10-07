//! The only way the proxy reaches a node (contract C02, story 144).
use async_trait::async_trait;
use bytes::Bytes;
use futures_util::Stream;
use http::{HeaderMap, Method, StatusCode, Uri};
use std::pin::Pin;

pub struct UpstreamRequest {
    pub method: Method,
    /// Absolute URI of the node endpoint.
    pub uri: Uri,
    pub headers: HeaderMap,
    pub body: Bytes,
}

pub type BodyStream = Pin<Box<dyn Stream<Item = Result<Bytes, UpstreamError>> + Send>>;

pub struct UpstreamResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: BodyStream,
}

/// No Timeout variant: a stalled stub is a stub behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpstreamError {
    /// Connect refused, or any failure before the first byte.
    Connect,
    /// Reset before the first byte, or an error item inside the stream.
    Reset,
}

impl std::fmt::Display for UpstreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpstreamError::Connect => f.write_str("connect failed"),
            UpstreamError::Reset => f.write_str("connection reset"),
        }
    }
}

impl std::error::Error for UpstreamError {}

#[async_trait]
pub trait UpstreamTransport: Send + Sync {
    async fn send(&self, req: UpstreamRequest) -> Result<UpstreamResponse, UpstreamError>;
}
