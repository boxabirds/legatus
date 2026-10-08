//! The leak scan: after redaction, no run of `min_len` or more characters of the raw text may
//! still be in the output. A hit gives a field path and a length, never the text.
use crate::capture::{Capture, SCAN_STAMP_PREFIX};
use crate::redact::sensitive_strings;
use std::collections::HashSet;

/// PROPOSED: the specification gives no length; 16 is long enough to skip common words and short
/// enough to catch a key.
pub const LEAK_SCAN_MIN_CHARS: usize = 16;

/// A shared run was found. `path` is the field of the output, `len` the length of the run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeakFound {
    pub path: String,
    pub len: usize,
}

impl std::fmt::Display for LeakFound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "leak of {} characters at {}", self.len, self.path)
    }
}

impl std::error::Error for LeakFound {}

/// Every window of `min_len` characters of every text the redaction was meant to remove.
fn raw_windows(raw: &Capture, min_len: usize) -> HashSet<Vec<char>> {
    let mut windows = HashSet::new();
    for (_, text) in sensitive_strings(raw) {
        let chars: Vec<char> = text.chars().collect();
        if chars.len() >= min_len {
            for w in chars.windows(min_len) {
                windows.insert(w.to_vec());
            }
        }
    }
    windows
}

/// Check the output against the raw capture. Strings the redaction keeps as structure (model
/// names, roles, content types) are not part of the raw side.
pub fn leak_scan(raw: &Capture, out: &Capture, min_len: usize) -> Result<(), LeakFound> {
    let windows = raw_windows(raw, min_len);
    if windows.is_empty() {
        return Ok(());
    }
    for (path, text) in all_strings(out) {
        let chars: Vec<char> = text.chars().collect();
        if chars.len() < min_len {
            continue;
        }
        let mut run = 0usize;
        let mut longest = 0usize;
        for (i, w) in chars.windows(min_len).enumerate() {
            if windows.contains(w) {
                run = if run == 0 { min_len } else { run + 1 };
                longest = longest.max(run);
            } else {
                run = 0;
            }
            let _ = i;
        }
        if longest >= min_len {
            return Err(LeakFound { path, len: longest });
        }
    }
    Ok(())
}

/// Every string value of the output capture with its path (keys are structure and are skipped).
fn all_strings(c: &Capture) -> Vec<(String, String)> {
    fn walk(value: &serde_json::Value, path: &str, out: &mut Vec<(String, String)>) {
        match value {
            serde_json::Value::String(s) => out.push((path.to_string(), s.clone())),
            serde_json::Value::Array(items) => items.iter().enumerate().for_each(|(i, v)| walk(v, &format!("{path}.{i}"), out)),
            serde_json::Value::Object(map) => map.iter().for_each(|(k, v)| walk(v, &format!("{path}.{k}"), out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for (i, request) in c.requests.iter().enumerate() {
        for (name, value) in &request.headers {
            out.push((format!("requests.{i}.headers.{name}"), value.clone()));
        }
        walk(&request.body, &format!("requests.{i}.body"), &mut out);
    }
    out
}

/// Scan, and when clean return the output with its scan stamp set. Only a stamped capture can be
/// written to the captures folder.
pub fn scan_and_stamp(raw: &Capture, out: &Capture, min_len: usize) -> Result<Capture, LeakFound> {
    leak_scan(raw, out, min_len)?;
    let mut stamped = out.clone();
    stamped.scan_stamp = Some(format!("{SCAN_STAMP_PREFIX}{min_len}"));
    Ok(stamped)
}
