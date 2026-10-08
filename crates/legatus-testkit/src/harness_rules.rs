//! Stub copies of how real harnesses classify an HTTP error as "retry" or "stop". They are copies of
//! the rules in the harness sources, so a test can say what a real harness would do with a
//! refusal body. The real harnesses are run against the proxy in the real tier to prove the
//! copies match (story 160 tasks 9 and 10).
use regex::Regex;
use std::sync::OnceLock;

/// What a harness does with an error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reaction {
    Retry,
    Stop,
}

/// The retryable and non-retryable patterns of pi 1.0.3 (`@earendil-works/pi-ai`
/// `dist/utils/retry.js`, `RETRYABLE_PROVIDER_ERROR_PATTERN` and
/// `NON_RETRYABLE_PROVIDER_LIMIT_ERROR_PATTERN`), copied as written there.
const PI_RETRYABLE: &[&str] = &[
    "overloaded",
    "server_busy",
    "servers are currently busy",
    "currently experiencing high demand",
    "model is at capacity",
    "rate.?limit",
    "too many requests",
    "429",
    "500",
    "502",
    "503",
    "504",
    "520",
    "524",
    "service.?unavailable",
    "server.?error",
    "internal.?error",
    "provider.?returned.?error",
    "exceeded request buffer limit while retrying upstream",
    "network.?error",
    "connection.?error",
    "connection.?refused",
    "connection.?lost",
    "other side closed",
    "fetch failed",
    "getaddrinfo",
    "ENOTFOUND",
    "EAI_AGAIN",
    "upstream.?connect",
    "reset before headers",
    "socket hang up",
    "socket connection was closed",
    "timed? out",
    "timeout",
    "terminated",
    "websocket.?closed",
    "websocket.?error",
    "ended without",
    "stream ended before message_stop",
    "stream ended before a terminal response event",
    "http2 request did not get a response",
    "pending stream has been canceled",
    "retry delay",
    "you can retry your request",
    "try your request again",
    "please retry your request",
    "ResourceExhausted",
    "subscription_sharing_usage_unavailable",
];

const PI_NON_RETRYABLE: &[&str] = &[
    "GoUsageLimitError",
    "FreeUsageLimitError",
    "Monthly usage limit reached",
    "available balance",
    "insufficient_quota",
    "out of budget",
    "quota exceeded",
    "billing",
    "subscription_sharing_usage_limit_exceeded",
];

fn pi_patterns() -> &'static (Regex, Regex) {
    static PATTERNS: OnceLock<(Regex, Regex)> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        (
            Regex::new(&format!("(?i){}", PI_RETRYABLE.join("|"))).expect("pi retryable pattern"),
            Regex::new(&format!("(?i){}", PI_NON_RETRYABLE.join("|"))).expect("pi non-retryable pattern"),
        )
    })
}

/// The context-overflow pattern pi reads (`maximum context length is N tokens`); such an error is
/// handled by compaction and not retried.
fn pi_context_overflow(text: &str) -> bool {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"(?i)maximum context length is \d+ tokens|context.?length.?exceeded|prompt is too long").expect("context pattern")).is_match(text)
}

/// pi's reaction to an HTTP error: its message is the status and the `error.message` of the body
/// (the OpenAI SDK form), checked against the patterns above.
pub fn pi_reaction(status: u16, error_message: &str) -> Reaction {
    let text = format!("{status} {error_message}");
    let (retryable, non_retryable) = pi_patterns();
    if pi_context_overflow(&text) || non_retryable.is_match(&text) {
        return Reaction::Stop;
    }
    if retryable.is_match(&text) {
        Reaction::Retry
    } else {
        Reaction::Stop
    }
}
