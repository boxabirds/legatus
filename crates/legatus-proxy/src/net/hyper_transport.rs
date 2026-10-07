//! Production transport: plain HTTP/1 to a node over hyper (no other module sends to a node).
use crate::upstream::transport::{BodyStream, UpstreamError, UpstreamRequest, UpstreamResponse, UpstreamTransport};
use async_trait::async_trait;
use bytes::Bytes;
use futures_util::StreamExt;
use http_body_util::{BodyStream as HttpBodyStream, Full};
use hyper_util::client::legacy::{connect::HttpConnector, Client};
use hyper_util::rt::TokioExecutor;

pub struct HyperTransport {
    client: Client<HttpConnector, Full<Bytes>>,
}

impl HyperTransport {
    pub fn new() -> HyperTransport {
        HyperTransport { client: Client::builder(TokioExecutor::new()).build_http() }
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
        let response = self.client.request(request).await.map_err(|_| UpstreamError::Connect)?;
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
