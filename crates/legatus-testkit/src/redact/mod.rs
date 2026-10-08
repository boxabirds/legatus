//! Redaction of captures (story 120): text becomes equal-length fillers, credentials and session
//! values become keyed tokens, the run key is discarded, and a scan proves nothing leaked.
pub mod scan;
pub mod secret;
pub mod text;
pub mod write;

use crate::capture::{Capture, CapturedRequest};
use secret::{RunKey, SecretKind};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;

/// A string longer than this many bytes is refused (the run stops and writes nothing).
pub const MAX_TEXT_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RedactError {
    /// A value that is not text (raw bytes of a capture); the field path only.
    NotUtf8(String),
    TooLarge(String),
}

impl std::fmt::Display for RedactError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RedactError::NotUtf8(path) => write!(f, "value is not text at {path}"),
            RedactError::TooLarge(path) => write!(f, "value is too large at {path}"),
        }
    }
}

impl std::error::Error for RedactError {}

/// Parse the bytes of a raw capture. A capture that is not UTF-8 JSON is refused with a path.
pub fn parse_raw(bytes: &[u8]) -> Result<Capture, RedactError> {
    let text = std::str::from_utf8(bytes).map_err(|_| RedactError::NotUtf8("(file)".to_string()))?;
    serde_json::from_str(text).map_err(|_| RedactError::NotUtf8("(file)".to_string()))
}

/// Body keys whose string values are structure, not text: they stay as recorded because the
/// router and the harness scripts read them.
const STRUCTURAL_BODY_KEYS: [&str; 8] = ["model", "role", "type", "object", "finish_reason", "stop_reason", "event", "tool_choice"];
/// Body keys that hold a session, a user or another identifier tied to a person.
const SECRET_BODY_KEYS: [(&str, SecretKind); 9] = [
    ("session_id", SecretKind::Session),
    ("prompt_cache_key", SecretKind::Session),
    ("thread_id", SecretKind::Session),
    ("conversation_id", SecretKind::Session),
    ("previous_response_id", SecretKind::Session),
    ("user", SecretKind::User),
    ("user_id", SecretKind::User),
    ("safety_identifier", SecretKind::User),
    ("api_key", SecretKind::Credential),
];
/// Header names that hold a credential, a cookie or a session value.
const SECRET_HEADERS: [(&str, SecretKind); 17] = [
    ("authorization", SecretKind::Credential),
    ("proxy-authorization", SecretKind::Credential),
    ("x-api-key", SecretKind::Credential),
    ("api-key", SecretKind::Credential),
    ("cookie", SecretKind::Cookie),
    ("set-cookie", SecretKind::Cookie),
    ("x-session-affinity", SecretKind::Session),
    ("x-session-id", SecretKind::Session),
    ("session-id", SecretKind::Session),
    ("thread-id", SecretKind::Session),
    ("x-claude-code-session-id", SecretKind::Session),
    ("x-claude-code-agent-id", SecretKind::Session),
    ("x-claude-code-parent-agent-id", SecretKind::Session),
    ("x-codex-window-id", SecretKind::Session),
    ("x-openwebui-chat-id", SecretKind::Session),
    ("openai-organization", SecretKind::User),
    ("x-openwebui-user-id", SecretKind::User),
];
/// Header names that are structure: kept as recorded.
const STRUCTURAL_HEADERS: [&str; 12] = ["content-type", "accept", "accept-encoding", "accept-language", "content-length", "connection", "host", "user-agent", "transfer-encoding", "expect", "anthropic-version", "openai-beta"];

pub(crate) fn secret_kind_of_header(name: &str) -> Option<SecretKind> {
    let lower = name.to_ascii_lowercase();
    SECRET_HEADERS.iter().find(|(n, _)| *n == lower).map(|(_, k)| *k)
}

pub(crate) fn header_is_structural(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    STRUCTURAL_HEADERS.contains(&lower.as_str()) || lower.starts_with("x-stainless-") || lower.starts_with("anthropic-")
}

pub(crate) fn secret_kind_of_body_key(key: &str) -> Option<SecretKind> {
    SECRET_BODY_KEYS.iter().find(|(k, _)| *k == key).map(|(_, kind)| *kind)
}

pub(crate) fn body_key_is_structural(key: &str) -> bool {
    STRUCTURAL_BODY_KEYS.contains(&key)
}

/// Where a string in a capture counts as text to protect: its path and its text.
pub(crate) fn sensitive_strings(capture: &Capture) -> Vec<(String, &str)> {
    let mut found = Vec::new();
    for (i, request) in capture.requests.iter().enumerate() {
        for (name, value) in &request.headers {
            if !header_is_structural(name) {
                found.push((format!("requests.{i}.headers.{}", name.to_ascii_lowercase()), value.as_str()));
            }
        }
        collect_body(&request.body, &format!("requests.{i}.body"), None, &mut found);
    }
    found
}

fn collect_body<'a>(value: &'a Value, path: &str, key: Option<&str>, out: &mut Vec<(String, &'a str)>) {
    match value {
        Value::String(text) => {
            if !key.is_some_and(body_key_is_structural) {
                out.push((path.to_string(), text.as_str()));
            }
        }
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                collect_body(item, &format!("{path}.{i}"), key, out);
            }
        }
        Value::Object(map) => {
            for (k, v) in map {
                collect_body(v, &format!("{path}.{k}"), Some(k), out);
            }
        }
        _ => {}
    }
}

/// One redaction run. The key lives only here and is dropped (zeroised) with the redactor.
pub struct Redactor {
    key: RunKey,
    fillers: Mutex<HashMap<String, String>>,
}

impl Redactor {
    /// A new run. `rng_seed` makes the key reproducible for tests; a real run draws it from the time.
    pub fn new_run(rng_seed: u64) -> Redactor {
        Redactor { key: RunKey::from_seed(rng_seed), fillers: Mutex::new(HashMap::new()) }
    }

    fn filler(&self, text: &str) -> String {
        let mut cache = match self.fillers.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(existing) = cache.get(text) {
            return existing.clone();
        }
        let mut attempt = 0u32;
        loop {
            let candidate = text::filler_for(&self.key, text, attempt);
            // Different texts must get different fillers; a text of one byte has few possibilities,
            // so a taken filler is redrawn.
            if !cache.values().any(|v| *v == candidate) || attempt >= text::MAX_REDRAWS {
                cache.insert(text.to_string(), candidate.clone());
                return candidate;
            }
            attempt += 1;
        }
    }

    fn redact_value(&self, value: &Value, path: &str, key: Option<&str>) -> Result<Value, RedactError> {
        Ok(match value {
            Value::String(text) => {
                if text.len() > MAX_TEXT_BYTES {
                    return Err(RedactError::TooLarge(path.to_string()));
                }
                match key {
                    _ if key.is_some_and(body_key_is_structural) => Value::String(text.clone()),
                    Some(k) if secret_kind_of_body_key(k).is_some() => Value::String(secret::token_for(&self.key, secret_kind_of_body_key(k).unwrap_or(SecretKind::Session), text)),
                    _ => Value::String(self.filler(text)),
                }
            }
            Value::Array(items) => Value::Array(items.iter().enumerate().map(|(i, v)| self.redact_value(v, &format!("{path}.{i}"), key)).collect::<Result<_, _>>()?),
            Value::Object(map) => Value::Object(map.iter().map(|(k, v)| Ok((k.clone(), self.redact_value(v, &format!("{path}.{k}"), Some(k))?))).collect::<Result<_, RedactError>>()?),
            other => other.clone(),
        })
    }

    /// Redact a whole capture: the same shape, every text replaced, every credential tokenised.
    pub fn redact(&self, raw: &Capture) -> Result<Capture, RedactError> {
        let mut requests = Vec::new();
        for (i, request) in raw.requests.iter().enumerate() {
            let headers = request
                .headers
                .iter()
                .map(|(name, value)| {
                    let lower = name.to_ascii_lowercase();
                    let redacted = if let Some(kind) = secret_kind_of_header(name) {
                        secret::token_for(&self.key, kind, value)
                    } else if header_is_structural(name) {
                        value.clone()
                    } else {
                        // An unknown header is text, never safe.
                        self.filler(value)
                    };
                    (lower, redacted)
                })
                .collect();
            requests.push(CapturedRequest {
                method: request.method.clone(),
                path: request.path.clone(),
                headers,
                body: self.redact_value(&request.body, &format!("requests.{i}.body"), None)?,
                response_status: request.response_status,
            });
        }
        Ok(Capture { schema: raw.schema, harness: raw.harness.clone(), harness_version: raw.harness_version.clone(), captured_at: raw.captured_at.clone(), requests, scan_stamp: None })
    }
}
