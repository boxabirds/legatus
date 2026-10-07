//! Batching mlx_lm stub: admits up to decode_concurrency at once, no load pages, drops invalid
//! requests without a status, limited prompt cache, optional wedge.
use super::common::*;
use super::scenario::StubSpec;
use async_trait::async_trait;
use legatus_proxy::upstream::transport::{UpstreamError, UpstreamRequest, UpstreamResponse, UpstreamTransport};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Default)]
struct State {
    busy: u32,
    accepted: u32,
    cache: Vec<String>,
    cancelled: Vec<u64>,
    next_id: u64,
}

pub struct BatchingMlxStub {
    spec: StubSpec,
    state: Arc<Mutex<State>>,
    log: StubLog,
}

impl BatchingMlxStub {
    pub fn new(spec: StubSpec) -> BatchingMlxStub {
        BatchingMlxStub { spec, state: Arc::default(), log: StubLog::default() }
    }
    pub fn log(&self) -> StubLog {
        self.log.clone()
    }
    /// Requests counted busy (a cancelled request stays counted until its normal end).
    pub fn busy(&self) -> u32 {
        self.state.lock().map(|s| s.busy).unwrap_or(0)
    }
    pub fn cache_keys(&self) -> Vec<String> {
        self.state.lock().map(|s| s.cache.clone()).unwrap_or_default()
    }
    /// Cancel during prefill: the request is logged cancelled but stays busy until its normal end.
    pub fn cancel(&self, request_id: u64) {
        if let Ok(mut s) = self.state.lock() {
            s.cancelled.push(request_id);
        }
        self.log.push("/v1/chat/completions", format!("cancel {request_id}"));
    }
    /// Id the next admitted request will get.
    pub fn next_request_id(&self) -> u64 {
        self.state.lock().map(|s| s.next_id).unwrap_or(0)
    }
}

#[async_trait]
impl UpstreamTransport for BatchingMlxStub {
    async fn send(&self, req: UpstreamRequest) -> Result<UpstreamResponse, UpstreamError> {
        let path = req.uri.path().to_string();
        if matches!(path.as_str(), "/props" | "/metrics" | "/slots") {
            return Ok(reply(404, "{\"error\":\"not found\"}"));
        }
        let valid = json_body(&req).filter(|b| b.get("messages").is_some());
        let Some(body) = valid else {
            self.log.push(&path, "drop invalid");
            return Err(UpstreamError::Reset);
        };
        let wedged = {
            let s = self.state.lock().map_err(|_| UpstreamError::Reset)?;
            self.spec.wedge_after.is_some_and(|n| s.accepted >= n)
        };
        if wedged {
            self.log.push(&path, "wedged");
            std::future::pending::<()>().await;
        }
        let prompt_tokens = u32_field(&body, "prompt_tokens");
        let tokens = output_tokens(&body);
        let (first, interval, id) = {
            let mut s = self.state.lock().map_err(|_| UpstreamError::Reset)?;
            s.accepted += 1;
            s.busy += 1;
            let id = s.next_id;
            s.next_id += 1;
            if let Some(key) = body.get("cache_key").and_then(|k| k.as_str()) {
                s.cache.push(key.to_string());
                while s.cache.len() > self.spec.cache_entries as usize {
                    s.cache.remove(0);
                }
            }
            let first = self.spec.speed.first_token_ms(prompt_tokens, prompt_tokens, 1, 0);
            (first, self.spec.speed.token_interval_ms(s.busy), id)
        };
        self.log.push(&path, format!("admit {id}"));
        // The request ends at a fixed time; a cancel does not shorten it.
        let total_ms = first + interval * u64::from(tokens);
        let state = self.state.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(total_ms)).await;
            if let Ok(mut s) = state.lock() {
                s.busy -= 1;
            }
        });
        let body_stream = token_stream(first, tokens, Arc::new(move || interval), Box::new(()));
        Ok(UpstreamResponse { status: http::StatusCode::OK, headers: http::HeaderMap::new(), body: body_stream })
    }
}
