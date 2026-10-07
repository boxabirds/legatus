//! Run the client steps of a scenario against its stubs, in memory (virtual time) or as processes.
use super::scenario::{Scenario, Step};
use super::{process::start_process, start_in_memory};
use crate::faults::SeededRng;
use bytes::Bytes;
use http::{HeaderMap, Method};
use http_body_util::{BodyExt, Full};
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use legatus_proxy::upstream::transport::{UpstreamError, UpstreamRequest, UpstreamTransport};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;

/// Status value of a step whose connection was refused.
pub const STATUS_CONNECT_ERROR: u16 = 0;
/// Status value of a step whose connection was reset before a status.
pub const STATUS_RESET: u16 = 1;
/// Largest seeded arrival offset added to a step instant.
pub const ARRIVAL_JITTER_MS: u64 = 50;
const OUTPUT_TOKENS_PER_STEP: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepResult {
    pub index: usize,
    pub at_ms: u64,
    pub stub: String,
    pub status: u16,
    pub body: Vec<u8>,
    pub first_byte_ms: u64,
}

fn step_body(step: &Step) -> Bytes {
    Bytes::from(format!(
        "{{\"messages\":[],\"prompt_tokens\":{},\"output_tokens\":{OUTPUT_TOKENS_PER_STEP}}}",
        step.request.body_tokens
    ))
}

/// Arrival time of every step: the written instant plus a seeded whole-millisecond offset when a
/// seed is set.
pub fn arrival_times(scenario: &Scenario) -> Vec<u64> {
    let mut rng = SeededRng::new(scenario.seed);
    scenario
        .steps
        .iter()
        .map(|s| s.at_ms + if scenario.seed == 0 { 0 } else { rng.arrival_offset(ARRIVAL_JITTER_MS).ms() })
        .collect()
}

pub async fn run_in_memory(scenario: &Scenario) -> Vec<StepResult> {
    let started = Instant::now();
    let transports: BTreeMap<String, Arc<dyn UpstreamTransport>> =
        scenario.stubs.iter().map(|s| (s.name.clone(), start_in_memory(s, &scenario.faults))).collect();
    let arrivals = arrival_times(scenario);
    let mut handles = Vec::new();
    for (index, step) in scenario.steps.iter().enumerate() {
        let transport = transports[&step.stub].clone();
        let step = step.clone();
        let at = arrivals[index];
        handles.push(tokio::spawn(async move {
            tokio::time::sleep_until(started + Duration::from_millis(at)).await;
            let request = UpstreamRequest {
                method: Method::POST,
                uri: format!("http://stub{}", step.request.path).parse().expect("step path"),
                headers: HeaderMap::new(),
                body: step_body(&step),
            };
            let (status, body, first_byte) = match transport.send(request).await {
                Err(UpstreamError::Connect) => (STATUS_CONNECT_ERROR, Vec::new(), started.elapsed()),
                Err(UpstreamError::Reset) => (STATUS_RESET, Vec::new(), started.elapsed()),
                Ok(reply) => {
                    let mut body = Vec::new();
                    let mut first = None;
                    let mut stream = reply.body;
                    while let Some(item) = futures_util::StreamExt::next(&mut stream).await {
                        first.get_or_insert(started.elapsed());
                        match item {
                            Ok(b) => body.extend_from_slice(&b),
                            Err(_) => break,
                        }
                    }
                    (reply.status.as_u16(), body, first.unwrap_or_else(|| started.elapsed()))
                }
            };
            StepResult { index, at_ms: at, stub: step.stub, status, body, first_byte_ms: u64::try_from(first_byte.as_millis()).unwrap_or(u64::MAX) }
        }));
    }
    let mut results = Vec::new();
    for h in handles {
        results.push(h.await.expect("step task"));
    }
    results
}

/// Same scenario with every stub as a process on real sockets (real time; keep instants small).
pub async fn run_process(scenario: &Scenario) -> Vec<StepResult> {
    let mut procs = BTreeMap::new();
    for spec in &scenario.stubs {
        procs.insert(spec.name.clone(), start_process(spec, &scenario.faults).await);
    }
    let started = Instant::now();
    let client: Client<_, Full<Bytes>> = Client::builder(TokioExecutor::new()).build_http();
    let arrivals = arrival_times(scenario);
    let mut results = Vec::new();
    for (index, step) in scenario.steps.iter().enumerate() {
        tokio::time::sleep_until(started + Duration::from_millis(arrivals[index])).await;
        let addr = procs[&step.stub].addr;
        let request = http::Request::builder()
            .method(Method::POST)
            .uri(format!("http://{addr}{}", step.request.path))
            .body(Full::new(step_body(step)))
            .expect("request");
        let (status, body) = match client.request(request).await {
            Err(e) if e.is_connect() => (STATUS_CONNECT_ERROR, Vec::new()),
            Err(_) => (STATUS_RESET, Vec::new()),
            Ok(reply) => {
                let status = reply.status().as_u16();
                match reply.into_body().collect().await {
                    Ok(c) => (status, c.to_bytes().to_vec()),
                    Err(_) => (STATUS_RESET, Vec::new()),
                }
            }
        };
        results.push(StepResult { index, at_ms: arrivals[index], stub: step.stub.clone(), status, body, first_byte_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX) });
    }
    results
}
