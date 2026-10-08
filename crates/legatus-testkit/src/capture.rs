//! The capture file format (contract C05): what a real harness sent, one request after another.
//! A committed capture is always redacted and carries a leak-scan stamp.
use serde::{Deserialize, Serialize};

/// The current schema number of the capture file.
pub const CAPTURE_SCHEMA: u32 = 1;
/// The text of a leak-scan stamp: the scan version and the minimum length it checked.
pub const SCAN_STAMP_PREFIX: &str = "leak-scan-v1:";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CapturedRequest {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: serde_json::Value,
    pub response_status: Option<u16>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Capture {
    pub schema: u32,
    pub harness: String,
    pub harness_version: String,
    pub captured_at: String,
    pub requests: Vec<CapturedRequest>,
    /// Set by a clean leak scan (`scan_and_stamp`); the commit gate refuses a file without it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scan_stamp: Option<String>,
}

/// The single name other stories import.
pub type CaptureRecord = Capture;

impl Capture {
    pub fn new(harness: &str, harness_version: &str, captured_at: &str, requests: Vec<CapturedRequest>) -> Capture {
        Capture { schema: CAPTURE_SCHEMA, harness: harness.to_string(), harness_version: harness_version.to_string(), captured_at: captured_at.to_string(), requests, scan_stamp: None }
    }
}
