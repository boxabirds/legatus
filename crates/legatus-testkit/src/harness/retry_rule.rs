//! Stub copies of how pi, opencode and Claude Code decide between retry and stop for an error
//! reply. The word list is the specification's (PRX-PROTO-032); for pi the real patterns read
//! from its source are used (`harness_rules`); the exact opencode and Claude Code patterns are an
//! ASSUMPTION until read from their sources.
use crate::harness::script::HarnessKind;
use crate::harness_rules::{pi_reaction, Reaction};
use legatus_proxy::protocol::errors::{forbidden_word_in, WordList};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetryVerdict {
    Retry,
    Stop,
}

/// Claude Code waits as told by `Retry-After` up to this long; more fails at once.
pub const CLAUDE_CODE_RETRY_AFTER_MAX: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, Debug)]
pub struct RetryRule {
    kind: HarnessKind,
}

/// Is the body in the Messages error shape (`{"type":"error",...}`)?
fn is_messages_body(body_text: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(body_text).ok().is_some_and(|v| v.get("type").and_then(|t| t.as_str()) == Some("error"))
}

fn status_retries(status: u16) -> bool {
    matches!(status, 408 | 429) || (500..=599).contains(&status)
}

/// The rule of a harness, or `None` for a kind that has no rule here.
pub fn retry_rule_for(kind: HarnessKind) -> Option<RetryRule> {
    matches!(kind, HarnessKind::Pi | HarnessKind::Opencode | HarnessKind::ClaudeCode).then_some(RetryRule { kind })
}

impl RetryRule {
    /// Decide for an error reply: the status, the body text and an optional `Retry-After`.
    pub fn classify(&self, status: u16, body_text: &str, retry_after: Option<Duration>) -> RetryVerdict {
        let verdict = match self.kind {
            HarnessKind::Pi | HarnessKind::Opencode => {
                if is_messages_body(body_text) {
                    // On a Messages body the status decides (408, 429 and 5xx retry; 400 and 409 stop).
                    status_retries(status)
                } else if self.kind == HarnessKind::Pi {
                    pi_reaction(status, body_text) == Reaction::Retry
                } else {
                    forbidden_word_in(WordList::Stop, body_text).is_some()
                }
            }
            HarnessKind::ClaudeCode => {
                if retry_after.is_some_and(|d| d > CLAUDE_CODE_RETRY_AFTER_MAX) {
                    false
                } else {
                    body_text.to_ascii_lowercase().contains("overloaded_error") || status_retries(status)
                }
            }
            _ => false,
        };
        if verdict {
            RetryVerdict::Retry
        } else {
            RetryVerdict::Stop
        }
    }
}
