//! Replay a redacted capture through the router and compare each reply with the one the stub
//! gives when the same request is sent to it directly. A mismatch names the request index and
//! the first differing offset, never the bytes.
use crate::capture::{Capture, CapturedRequest};
use crate::harness::script::HarnessScript;
use axum::body::Body;
use axum::Router;
use bytes::Bytes;
use http::{HeaderName, HeaderValue, Method, Request};
use http_body_util::BodyExt;
use legatus_proxy::upstream::transport::{UpstreamRequest, UpstreamTransport};
use std::time::Duration;
use tower::ServiceExt;

/// The address the direct request is sent to (a stub transport ignores it).
const DIRECT_BASE: &str = "http://stub.invalid";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayError {
    Mismatch { index: usize, offset: Option<usize> },
    BadCapture(usize),
    GaveUp(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestReport {
    pub index: usize,
    pub router_status: u16,
    pub direct_status: u16,
    /// The first byte at which the two replies differ, when they do.
    pub first_diff: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayReport {
    pub requests: Vec<RequestReport>,
}

impl ReplayReport {
    /// Ok when every reply equals the direct one in status and bytes.
    pub fn check(&self) -> Result<(), ReplayError> {
        match self.requests.iter().find(|r| r.router_status != r.direct_status || r.first_diff.is_some()) {
            Some(r) => Err(ReplayError::Mismatch { index: r.index, offset: r.first_diff }),
            None => Ok(()),
        }
    }
}

fn body_bytes(request: &CapturedRequest) -> Bytes {
    Bytes::from(serde_json::to_vec(&request.body).unwrap_or_default())
}

fn first_diff(a: &[u8], b: &[u8]) -> Option<usize> {
    let common = a.iter().zip(b.iter()).position(|(x, y)| x != y);
    match common {
        Some(at) => Some(at),
        None if a.len() != b.len() => Some(a.len().min(b.len())),
        None => None,
    }
}

fn http_request(request: &CapturedRequest) -> Result<Request<Body>, ()> {
    let mut builder = Request::builder().method(Method::from_bytes(request.method.as_bytes()).map_err(|_| ())?).uri(&request.path);
    for (name, value) in &request.headers {
        // The body is re-encoded, so its length header is recomputed by the server layer.
        if name.eq_ignore_ascii_case("content-length") || name.eq_ignore_ascii_case("transfer-encoding") {
            continue;
        }
        builder = builder.header(HeaderName::from_bytes(name.as_bytes()).map_err(|_| ())?, HeaderValue::from_str(value).map_err(|_| ())?);
    }
    builder.body(Body::from(body_bytes(request))).map_err(|_| ())
}

/// Send each recorded request through `router` and straight to `direct`; compare status and bytes.
/// The script's give-up time bounds each request (on a paused clock it costs no real time).
pub async fn replay(capture: &Capture, script: &HarnessScript, router: Router, direct: &dyn UpstreamTransport) -> Result<ReplayReport, ReplayError> {
    let limit = script.give_up.unwrap_or(Duration::from_secs(600));
    let mut requests = Vec::new();
    for (index, request) in capture.requests.iter().enumerate() {
        let via_router = http_request(request).map_err(|_| ReplayError::BadCapture(index))?;
        let response = tokio::time::timeout(limit, router.clone().oneshot(via_router)).await.map_err(|_| ReplayError::GaveUp(index))?.map_err(|_| ReplayError::BadCapture(index))?;
        let router_status = response.status().as_u16();
        let router_bytes = response.into_body().collect().await.map_err(|_| ReplayError::BadCapture(index))?.to_bytes();

        let mut upstream_headers = http::HeaderMap::new();
        for (name, value) in &request.headers {
            if let (Ok(n), Ok(v)) = (HeaderName::from_bytes(name.as_bytes()), HeaderValue::from_str(value)) {
                upstream_headers.append(n, v);
            }
        }
        let uri = format!("{DIRECT_BASE}{}", request.path).parse().map_err(|_| ReplayError::BadCapture(index))?;
        let sent = UpstreamRequest { method: Method::POST, uri, headers: upstream_headers, body: body_bytes(request) };
        let reply = direct.send(sent).await.map_err(|_| ReplayError::GaveUp(index))?;
        let direct_status = reply.status.as_u16();
        let direct_bytes = {
            use futures_util::StreamExt;
            let mut all = Vec::new();
            let mut body = reply.body;
            while let Some(chunk) = body.next().await {
                all.extend_from_slice(&chunk.map_err(|_| ReplayError::GaveUp(index))?);
            }
            all
        };
        requests.push(RequestReport { index, router_status, direct_status, first_diff: first_diff(&router_bytes, &direct_bytes) });
    }
    Ok(ReplayReport { requests })
}
