//! The context guard for engines that silently cut a prompt that is too long, and the record of a
//! cut that happened anyway. The guard decides; the refusal text and status are `refuse` of the
//! protocol module (`ContextLengthExceeded`).
use crate::config::node::NodeSpec;
use crate::config::settings::EffectiveSettings;
use crate::engine::estimate::TokenEstimator;
use crate::engine::EngineAdapter;
use crate::obs::log_sink::{LogRecord, LogSink, SystemRecord, TruncationFields};
use crate::stream::tap::{ResponseTap, StreamEnd};
use legatus_common::engine::{OverflowBehaviour, UsageView};
use legatus_common::event::SystemEventKind;
use legatus_common::ids::NodeId;
use std::sync::Arc;

/// `floor(per_slot_context * ratio)` (the setting `ollama_truncation_limit_ratio`, default 1.0).
pub fn usable_limit(per_slot_context: u32, ratio: f64) -> u32 {
    let limit = (f64::from(per_slot_context) * ratio).floor();
    if limit <= 0.0 { 0 } else { limit as u32 }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuardDecision {
    Allow,
    Refuse { limit_tokens: u32, estimate_tokens: u32 },
}

/// Refuse only when the estimate is above the limit; equal passes.
pub fn decide(estimate_tokens: u32, limit_tokens: u32) -> GuardDecision {
    if estimate_tokens > limit_tokens {
        GuardDecision::Refuse { limit_tokens, estimate_tokens }
    } else {
        GuardDecision::Allow
    }
}

/// The estimate of the body that is sent to the node, with the divisor of the settings.
pub fn estimate_sent(settings: &EffectiveSettings, body_len: usize) -> u32 {
    TokenEstimator { bytes_per_token: settings.ollama_bytes_per_token }.estimate(body_len)
}

/// The decision for one request to one node. Only an engine that silently cuts is guarded, and
/// only when the registry gave the context of one slot. The context of one slot is used even when
/// the node runs several slots, because the engine keeps its context per slot.
pub fn context_decision(adapter: &dyn EngineAdapter, node: &NodeSpec, body_len: usize, settings: &EffectiveSettings) -> GuardDecision {
    if adapter.overflow_behaviour() != OverflowBehaviour::SilentTruncate {
        return GuardDecision::Allow;
    }
    let Some(context) = node.context_per_slot else { return GuardDecision::Allow };
    let estimate = estimate_sent(settings, body_len);
    decide(estimate, usable_limit(context, settings.ollama_truncation_limit_ratio))
}

/// A cut that the reply shows: the engine counted fewer prompt tokens than were sent.
#[derive(Clone, Debug, PartialEq)]
pub struct TruncationRecord {
    pub node: NodeId,
    pub prompt_tokens_sent: u32,
    pub prompt_tokens_seen: u32,
    /// The observed shortfall: (sent - seen) / sent (PROPOSED).
    pub ratio: f32,
}

/// Record when `seen` is more than `report_ratio` below `sent` (`truncation_report_ratio`, 0.25).
/// Exactly at the ratio does not record. A missing count, a count above the estimate and an
/// estimate of 0 never record.
pub fn detect_truncation(node: &NodeId, sent: u32, seen: Option<u32>, report_ratio: f64) -> Option<TruncationRecord> {
    let seen = seen?;
    if sent == 0 || f64::from(seen) >= f64::from(sent) * (1.0 - report_ratio) {
        return None;
    }
    Some(TruncationRecord { node: node.clone(), prompt_tokens_sent: sent, prompt_tokens_seen: seen, ratio: (f64::from(sent - seen) / f64::from(sent)) as f32 })
}

const PROMPT_TOKENS_KEYS: [&[u8]; 2] = [b"\"prompt_tokens\"", b"\"input_tokens\""];

/// The prompt token count in the tail of a reply: the last `usage.prompt_tokens` or
/// `usage.input_tokens`. The tail may start in the middle of the JSON, so this finds the key in
/// the bytes instead of parsing the whole object.
pub fn reported_prompt_tokens(view: &UsageView<'_>) -> Option<u32> {
    let bytes = view.json_tail;
    PROMPT_TOKENS_KEYS
        .iter()
        .filter_map(|key| bytes.windows(key.len()).rposition(|w| w == *key).map(|at| (at, key.len())))
        .max_by_key(|(at, _)| *at)
        .and_then(|(at, len)| {
            let rest = bytes[at + len..].trim_ascii_start();
            let rest = rest.strip_prefix(b":")?.trim_ascii_start();
            let digits: &[u8] = &rest[..rest.iter().take_while(|b| b.is_ascii_digit()).count()];
            std::str::from_utf8(digits).ok()?.parse().ok()
        })
}

/// How much of the end of a reply is kept to find the usage object.
pub const REPLY_TAIL_BYTES: usize = 4096;

/// Watches one reply of a guarded node and writes one `truncation_detected` event when the cut shows.
/// The reply is not changed. Counts and the node name reach the event; no text does.
pub struct TruncationTap {
    node: NodeId,
    sent: u32,
    report_ratio: f64,
    sink: Arc<dyn LogSink>,
    tail: Vec<u8>,
    protocol: legatus_common::protocol::Protocol,
    stream: bool,
}

impl TruncationTap {
    pub fn new(node: NodeId, sent: u32, report_ratio: f64, sink: Arc<dyn LogSink>, protocol: legatus_common::protocol::Protocol, stream: bool) -> TruncationTap {
        TruncationTap { node, sent, report_ratio, sink, tail: Vec::with_capacity(REPLY_TAIL_BYTES), protocol, stream }
    }
}

impl ResponseTap for TruncationTap {
    fn on_chunk(&mut self, chunk: &[u8]) {
        let keep = chunk.len().min(REPLY_TAIL_BYTES);
        let overflow = (self.tail.len() + keep).saturating_sub(REPLY_TAIL_BYTES);
        self.tail.drain(..overflow.min(self.tail.len()));
        self.tail.extend_from_slice(&chunk[chunk.len() - keep..]);
    }

    fn on_end(&mut self, end: StreamEnd) {
        if end != StreamEnd::Complete {
            return;
        }
        let view = UsageView { json_tail: &self.tail, protocol: self.protocol, stream: self.stream };
        if let Some(record) = detect_truncation(&self.node, self.sent, reported_prompt_tokens(&view), self.report_ratio) {
            let fields = TruncationFields { node: record.node, prompt_tokens_sent: record.prompt_tokens_sent, prompt_tokens_seen: record.prompt_tokens_seen, ratio: record.ratio };
            let _ = self.sink.offer(LogRecord::System(SystemRecord { kind: SystemEventKind::TruncationDetected, truncation: Some(fields) }));
        }
    }
}
