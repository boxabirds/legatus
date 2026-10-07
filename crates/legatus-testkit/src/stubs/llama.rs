//! Multi-slot llama-server stub: load pages, early headers, context errors, sleep, slot choice.
use super::common::*;
use super::scenario::StubSpec;
use async_trait::async_trait;
use legatus_proxy::upstream::transport::{UpstreamError, UpstreamRequest, UpstreamResponse, UpstreamTransport};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Notify;
use tokio::time::Instant;

/// Prefix similarity above this picks the similar slot; exactly this value does not.
pub const SLOT_SIMILARITY_THRESHOLD: f64 = 0.1;

struct Slot {
    busy: bool,
    last_used: u64,
}

struct State {
    slots: Vec<Slot>,
    deferred: u32,
    prefill_running: u32,
    clock: u64,
    asleep: bool,
    last_activity: Instant,
    last_slot: Option<usize>,
}

pub struct MultiSlotLlamaStub {
    spec: StubSpec,
    state: Arc<Mutex<State>>,
    freed: Arc<Notify>,
    log: StubLog,
}

impl MultiSlotLlamaStub {
    pub fn new(spec: StubSpec) -> MultiSlotLlamaStub {
        let slots = (0..spec.slots).map(|_| Slot { busy: false, last_used: 0 }).collect();
        MultiSlotLlamaStub {
            state: Arc::new(Mutex::new(State { slots, deferred: 0, prefill_running: 0, clock: 0, asleep: false, last_activity: Instant::now(), last_slot: None })),
            freed: Arc::new(Notify::new()),
            log: StubLog::default(),
            spec,
        }
    }
    pub fn log(&self) -> StubLog {
        self.log.clone()
    }
    /// The slot used by the most recent request that got one.
    pub fn last_slot(&self) -> Option<usize> {
        self.state.lock().ok().and_then(|s| s.last_slot)
    }
    /// Put the engine to sleep (sleep mode idle).
    pub fn force_sleep(&self) {
        if let Ok(mut s) = self.state.lock() {
            s.asleep = true;
        }
    }
    pub fn is_asleep(&self) -> bool {
        self.state.lock().map(|s| s.asleep).unwrap_or(false)
    }
    /// Context of one slot: ctx_total / slots with an explicit slot count, ctx_total when unified.
    pub fn slot_ctx(&self) -> u32 {
        if self.spec.kv_unified { self.spec.ctx_total } else { self.spec.ctx_total / self.spec.slots.max(1) }
    }
    fn idle_sleep_check(&self) {
        if !self.spec.sleep_on {
            return;
        }
        if let Ok(mut s) = self.state.lock() {
            let idle = s.last_activity.elapsed() >= Duration::from_secs(u64::from(self.spec.keep_alive_s));
            if idle && s.slots.iter().all(|x| !x.busy) {
                s.asleep = true;
            }
        }
    }
    async fn wake(&self) {
        let was_asleep = self.state.lock().map(|s| s.asleep).unwrap_or(false);
        if was_asleep {
            self.log.push("/slots", "wake");
            tokio::time::sleep(Duration::from_millis(self.spec.speed.load_time_ms)).await;
            if let Ok(mut s) = self.state.lock() {
                s.asleep = false;
                s.last_activity = Instant::now();
            }
        }
    }
    fn page(&self, path: &str) -> Option<UpstreamResponse> {
        let s = self.state.lock().ok()?;
        Some(match path {
            "/health" => reply(200, "{\"status\":\"ok\"}"),
            "/props" => reply(200, &format!("{{\"total_slots\":{}}}", self.spec.slots)),
            "/metrics" if self.spec.metrics_on => {
                let processing = s.slots.iter().filter(|x| x.busy).count();
                reply(200, &format!("llamacpp:requests_processing {processing}\nllamacpp:requests_deferred {}\n", s.deferred))
            }
            "/metrics" => reply(404, "{\"error\":\"not found\"}"),
            _ => return None,
        })
    }
    fn slots_page(&self) -> UpstreamResponse {
        let n_ctx = self.slot_ctx();
        let items: Vec<String> = self
            .state
            .lock()
            .map(|s| s.slots.iter().enumerate().map(|(i, x)| format!("{{\"id\":{i},\"is_processing\":{},\"n_ctx\":{n_ctx}}}", x.busy)).collect())
            .unwrap_or_default();
        reply(200, &format!("[{}]", items.join(",")))
    }
}

fn pick_slot(s: &State, similar: Option<usize>, similarity: f64) -> Option<usize> {
    if similarity > SLOT_SIMILARITY_THRESHOLD {
        if let Some(i) = similar {
            if s.slots.get(i).is_some_and(|x| !x.busy) {
                return Some(i);
            }
        }
    }
    s.slots.iter().enumerate().filter(|(_, x)| !x.busy).min_by_key(|(_, x)| x.last_used).map(|(i, _)| i)
}

#[async_trait]
impl UpstreamTransport for MultiSlotLlamaStub {
    async fn send(&self, req: UpstreamRequest) -> Result<UpstreamResponse, UpstreamError> {
        let path = req.uri.path().to_string();
        self.idle_sleep_check();
        if path == "/slots" {
            self.wake().await;
            return Ok(self.slots_page());
        }
        if let Some(page) = self.page(&path) {
            return Ok(page);
        }
        let Some(body) = json_body(&req) else {
            self.log.push(&path, "500 malformed json");
            return Ok(reply(500, "{\"error\":{\"code\":500,\"type\":\"server_error\",\"message\":\"parse error\"}}"));
        };
        self.wake().await;
        let prompt_tokens = u32_field(&body, "prompt_tokens");
        let n_ctx = self.slot_ctx();
        if prompt_tokens > n_ctx {
            self.log.push(&path, "400 exceed_context_size_error");
            return Ok(reply(400, &format!("{{\"error\":{{\"code\":400,\"type\":\"exceed_context_size_error\",\"message\":\"prompt {prompt_tokens} exceeds context {n_ctx}\",\"n_prompt_tokens\":{prompt_tokens},\"n_ctx\":{n_ctx}}}}}")));
        }
        let similar = body.get("similar_slot").and_then(|x| x.as_u64()).map(|x| x as usize);
        let similarity = body.get("prefix_similarity").and_then(|x| x.as_f64()).unwrap_or(0.0);
        let (state, freed, log, speed) = (self.state.clone(), self.freed.clone(), self.log.clone(), self.spec.speed.clone());
        let tokens = output_tokens(&body);
        let (state_end, freed_end) = (state.clone(), freed.clone());
        let body_stream = {
            // Headers go out at once; the slot is taken when the stream is first polled.
            let interval_state = state.clone();
            let interval_speed = speed.clone();
            let slot_holder: Arc<Mutex<Option<usize>>> = Arc::default();
            let slot_end = slot_holder.clone();
            let guard = OnDrop(move || {
                if let (Ok(mut s), Ok(slot)) = (state_end.lock(), slot_end.lock()) {
                    if let Some(i) = *slot {
                        s.slots[i].busy = false;
                    }
                }
                freed_end.notify_waiters();
            });
            let inner = token_stream(0, tokens, Arc::new(move || {
                let running = interval_state.lock().map(|s| s.slots.iter().filter(|x| x.busy).count() as u32).unwrap_or(1);
                interval_speed.token_interval_ms(running)
            }), Box::new(guard));
            let path2 = path.clone();
            let acquire = async move {
                loop {
                    let notified = freed.notified();
                    tokio::pin!(notified);
                    notified.as_mut().enable();
                    {
                        let mut s = state.lock().expect("stub state");
                        if let Some(i) = pick_slot(&s, similar, similarity) {
                            s.clock += 1;
                            let stamp = s.clock;
                            s.slots[i].busy = true;
                            s.slots[i].last_used = stamp;
                            s.last_slot = Some(i);
                            s.prefill_running += 1;
                            s.last_activity = Instant::now();
                            *slot_holder.lock().expect("slot holder") = Some(i);
                            log.push(&path2, format!("slot={i}"));
                            break;
                        }
                        s.deferred += 1;
                    }
                    notified.await;
                    if let Ok(mut s) = state.lock() {
                        s.deferred = s.deferred.saturating_sub(1);
                    }
                }
                // Requests that start at the same instant share the prefill compute.
                tokio::task::yield_now().await;
                let (first, running_prefill) = {
                    let s = state.lock().expect("stub state");
                    let r = s.prefill_running.max(1);
                    (speed.first_token_ms(prompt_tokens, prompt_tokens, r, 0), r)
                };
                let _ = running_prefill;
                tokio::time::sleep(Duration::from_millis(first)).await;
                if let Ok(mut s) = state.lock() {
                    s.prefill_running = s.prefill_running.saturating_sub(1);
                }
            };
            Box::pin(futures_util::StreamExt::flatten(futures_util::stream::once(async move {
                acquire.await;
                inner
            })))
        };
        Ok(UpstreamResponse { status: http::StatusCode::OK, headers: http::HeaderMap::new(), body: body_stream })
    }
}
