//! The shared reader of the reused-token count in a reply. No adapter reads it any other way.
use legatus_common::engine::{ReuseFieldName, ReuseReading, UnknownReason, UsageView};
use serde_json::Value;

/// A server-sent event line starts with this; the JSON follows.
const SSE_DATA_PREFIX: &[u8] = b"data:";

fn path_of(field: ReuseFieldName) -> &'static [&'static str] {
    match field {
        ReuseFieldName::TimingsCacheN => &["timings", "cache_n"],
        ReuseFieldName::PromptTokensDetailsCachedTokens => &["usage", "prompt_tokens_details", "cached_tokens"],
        ReuseFieldName::InputTokensDetailsCachedTokens => &["usage", "input_tokens_details", "cached_tokens"],
    }
}

/// Read the named field. Absent gives `Unknown(FieldAbsent)`; a value that is not an unsigned
/// whole number (string, negative, fractional, null) gives `Unknown(FieldMalformed)`; 0 is a
/// valid reading (a cold turn). A tail that is not JSON holds no field.
pub fn extract_reuse(field: ReuseFieldName, view: &UsageView<'_>) -> ReuseReading {
    let tail = view.json_tail.trim_ascii();
    let tail = tail.strip_prefix(SSE_DATA_PREFIX).unwrap_or(tail);
    let Ok(root) = serde_json::from_slice::<Value>(tail) else {
        return ReuseReading::Unknown(UnknownReason::FieldAbsent);
    };
    let mut at = &root;
    for key in path_of(field) {
        match at.get(key) {
            Some(next) => at = next,
            None => return ReuseReading::Unknown(UnknownReason::FieldAbsent),
        }
    }
    match at.as_u64() {
        Some(cached_tokens) => ReuseReading::Reused { cached_tokens, field },
        None => ReuseReading::Unknown(UnknownReason::FieldMalformed),
    }
}
