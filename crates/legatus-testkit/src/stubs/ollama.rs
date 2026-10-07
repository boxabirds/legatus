//! Serial Ollama stub: one slot, FIFO queue, headers at service start, truncation, unload.
use super::common::*;
use super::scenario::StubSpec;
use async_trait::async_trait;
use legatus_proxy::upstream::transport::{UpstreamError, UpstreamRequest, UpstreamResponse, UpstreamTransport};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio::time::Instant;

struct State {
    waiting: u32,
    running: u32,
    loaded_until: Option<Instant>,
}

pub struct SerialOllamaStub {
    spec: StubSpec,
    slot: Arc<Semaphore>,
    state: Arc<Mutex<State>>,
    log: StubLog,
}

impl SerialOllamaStub {
    pub fn new(spec: StubSpec) -> SerialOllamaStub {
        SerialOllamaStub {
            spec,
            slot: Arc::new(Semaphore::new(1)),
            state: Arc::new(Mutex::new(State { waiting: 0, running: 0, loaded_until: None })),
            log: StubLog::default(),
        }
    }
    pub fn log(&self) -> StubLog {
        self.log.clone()
    }
    fn is_loaded(&self) -> bool {
        self.state.lock().map(|s| s.loaded_until.is_some_and(|t| Instant::now() < t)).unwrap_or(false)
    }
    fn keep_alive(&self) -> Duration {
        Duration::from_secs(u64::from(self.spec.keep_alive_s))
    }
    /// The process page: model, memory, expiry; no queue or slot field.
    fn process_page(&self) -> UpstreamResponse {
        let body = if self.is_loaded() {
            let expires_in_ms = self.state.lock().ok().and_then(|s| s.loaded_until).map(|t| t.saturating_duration_since(Instant::now()).as_millis()).unwrap_or(0);
            format!("{{\"models\":[{{\"name\":\"{}\",\"size_vram\":4294967296,\"expires_in_ms\":{expires_in_ms}}}]}}", self.spec.model)
        } else {
            "{\"models\":[]}".to_string()
        };
        reply(200, &body)
    }
}

#[async_trait]
impl UpstreamTransport for SerialOllamaStub {
    async fn send(&self, req: UpstreamRequest) -> Result<UpstreamResponse, UpstreamError> {
        let path = req.uri.path().to_string();
        if path == "/api/ps" {
            return Ok(self.process_page());
        }
        let Some(body) = json_body(&req) else {
            self.log.push(&path, "400 invalid json");
            return Ok(reply(400, "{\"error\":\"invalid json\"}"));
        };
        if let Some(model) = body.get("model").and_then(|m| m.as_str()) {
            if model != self.spec.model {
                self.log.push(&path, "404 unknown model");
                return Ok(reply(404, "{\"error\":\"model not found\"}"));
            }
        }
        let prompt_tokens = u32_field(&body, "prompt_tokens");
        {
            let mut s = self.state.lock().map_err(|_| UpstreamError::Reset)?;
            if self.spec.max_queue > 0 && s.running + s.waiting >= self.spec.max_queue {
                drop(s);
                self.log.push(&path, "503 queue full");
                return Ok(reply(503, "{\"error\":\"server busy\"}"));
            }
            s.waiting += 1;
        }
        let permit = self.slot.clone().acquire_owned().await.map_err(|_| UpstreamError::Reset)?;
        let was_loaded = self.is_loaded();
        {
            let mut s = self.state.lock().map_err(|_| UpstreamError::Reset)?;
            s.waiting -= 1;
            s.running += 1;
        }
        if !was_loaded {
            self.log.push(&path, "load");
            tokio::time::sleep(Duration::from_millis(self.spec.speed.load_time_ms)).await;
        }
        let ctx_limit = f64::from(self.spec.ctx_total) * f64::from(self.spec.truncate_ratio);
        if f64::from(prompt_tokens) > ctx_limit {
            self.log.push(&path, "truncated");
        }
        self.log.push(&path, "serve");
        let first = self.spec.speed.first_token_ms(prompt_tokens, prompt_tokens, 1, 0);
        let speed = self.spec.speed.clone();
        let (state, keep_alive) = (self.state.clone(), self.keep_alive());
        let guard = OnDrop(move || {
            if let Ok(mut s) = state.lock() {
                s.running -= 1;
                s.loaded_until = Some(Instant::now() + keep_alive);
            }
            let _ = &permit;
        });
        let stream = token_stream(first, output_tokens(&body), Arc::new(move || speed.token_interval_ms(1)), Box::new(guard));
        Ok(UpstreamResponse { status: http::StatusCode::OK, headers: http::HeaderMap::new(), body: stream })
    }
}
