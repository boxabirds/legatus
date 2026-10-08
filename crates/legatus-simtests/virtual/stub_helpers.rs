//! Helpers shared by the stub kit tests (test-local).
use async_trait::async_trait;
use bytes::Bytes;
use futures_util::StreamExt;
use http::{HeaderMap, Method};
use legatus_proxy::upstream::transport::{UpstreamError, UpstreamRequest, UpstreamResponse, UpstreamTransport};
use std::sync::{Arc, Mutex};
use tokio::time::Instant;

pub fn post(path: &str, body: &str) -> UpstreamRequest {
    UpstreamRequest { method: Method::POST, uri: format!("http://stub{path}").parse().unwrap(), headers: HeaderMap::new(), body: Bytes::from(body.to_string()) }
}

pub fn get(path: &str) -> UpstreamRequest {
    UpstreamRequest { method: Method::GET, uri: format!("http://stub{path}").parse().unwrap(), headers: HeaderMap::new(), body: Bytes::new() }
}

pub fn chat(prompt_tokens: u32, output_tokens: u32) -> UpstreamRequest {
    post("/v1/chat/completions", &format!("{{\"messages\":[],\"prompt_tokens\":{prompt_tokens},\"output_tokens\":{output_tokens}}}"))
}

/// Read a whole reply: (status, body bytes, instants in ms since `since` of each chunk).
pub async fn drain(reply: UpstreamResponse, since: Instant) -> (u16, Vec<u8>, Vec<u128>) {
    let status = reply.status.as_u16();
    let mut body = Vec::new();
    let mut instants = Vec::new();
    let mut stream = reply.body;
    while let Some(item) = stream.next().await {
        match item {
            Ok(b) => {
                instants.push(since.elapsed().as_millis());
                body.extend_from_slice(&b);
            }
            Err(_) => break,
        }
    }
    (status, body, instants)
}

pub async fn text_of(t: &dyn UpstreamTransport, req: UpstreamRequest) -> (u16, String) {
    let reply = t.send(req).await.ok().expect("reply");
    let (status, body, _) = drain(reply, Instant::now()).await;
    (status, String::from_utf8_lossy(&body).to_string())
}

pub fn err_kind(e: UpstreamError) -> &'static str {
    match e {
        UpstreamError::Connect => "connect",
        UpstreamError::Reset => "reset",
    }
}

/// Wraps a transport and records the path of every request, so a test can show what was asked.
pub struct Recording {
    inner: Arc<dyn UpstreamTransport>,
    paths: Mutex<Vec<String>>,
}

impl Recording {
    pub fn new(inner: Arc<dyn UpstreamTransport>) -> Arc<Recording> {
        Arc::new(Recording { inner, paths: Mutex::default() })
    }
    pub fn paths(&self) -> Vec<String> {
        self.paths.lock().unwrap().clone()
    }
}

#[async_trait]
impl UpstreamTransport for Recording {
    async fn send(&self, req: UpstreamRequest) -> Result<UpstreamResponse, UpstreamError> {
        self.paths.lock().unwrap().push(req.uri.path().to_string());
        self.inner.send(req).await
    }
}

