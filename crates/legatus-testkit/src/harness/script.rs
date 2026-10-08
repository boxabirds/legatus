//! What each real harness does, as data. Every number is from the fixtures table of spec 13
//! (FIX-020 to FIX-025 and FIX-324 to FIX-327); anything the table does not say is listed in
//! `assumptions` and is not used as a fact.
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HarnessKind {
    Pi,
    ClaudeCode,
    OpenWebUi,
    Opencode,
    DeepSeek,
    Codex,
    Sdk,
    Headerless,
    RawHttp,
}

/// How a harness treats `Retry-After`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetryAfterPolicy {
    Ignores,
    /// Waits as told up to this long; a longer value fails at once.
    HonoursUpTo(Duration),
    /// Not said by the fixtures table for this harness.
    NotSpecified,
}

/// The kinds of request a raw HTTP client sends (FIX-025).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RawRequestKind {
    Malformed,
    Chunked,
    ExpectContinue,
    Oversized,
    Http10,
}

#[derive(Clone, Debug)]
pub struct HarnessScript {
    pub kind: HarnessKind,
    pub name: &'static str,
    /// The fixture rows it comes from.
    pub fixtures: &'static [&'static str],
    /// Headers of a main request: (name, value template). `{session}` is replaced by the session value.
    pub headers: Vec<(&'static str, &'static str)>,
    /// Names of the headers that carry a session or agent value.
    pub session_headers: Vec<&'static str>,
    /// Waits before each retry after the first attempt, when the table gives them.
    pub retry_gaps: Vec<Duration>,
    /// Total attempts, when the table gives a number.
    pub attempts: Option<u32>,
    /// Gives up when no byte has come in this time.
    pub give_up: Option<Duration>,
    /// Give-up times that differ by runtime (SDK clients: Node and Python).
    pub give_up_variants: Vec<(&'static str, Duration)>,
    pub retry_after: RetryAfterPolicy,
    /// Requests started at once (Open WebUI sub-agents).
    pub fan_out: Option<u32>,
    /// A request after which the session ends: the harness fails at once on this status.
    pub fails_at_once_on: Vec<u16>,
    /// A compaction call carries no session header (pi).
    pub compaction_without_session_header: bool,
    /// The kinds of raw request a raw client sends.
    pub raw_requests: Vec<RawRequestKind>,
    /// Facts the fixtures table does not give.
    pub assumptions: Vec<&'static str>,
}

const SECOND: Duration = Duration::from_secs(1);

fn secs(n: u64) -> Duration {
    SECOND * u32::try_from(n).unwrap_or(u32::MAX)
}

impl HarnessScript {
    fn base(kind: HarnessKind, name: &'static str, fixtures: &'static [&'static str]) -> HarnessScript {
        HarnessScript {
            kind,
            name,
            fixtures,
            headers: Vec::new(),
            session_headers: Vec::new(),
            retry_gaps: Vec::new(),
            attempts: None,
            give_up: None,
            give_up_variants: Vec::new(),
            retry_after: RetryAfterPolicy::NotSpecified,
            fan_out: None,
            fails_at_once_on: Vec::new(),
            compaction_without_session_header: false,
            raw_requests: Vec::new(),
            assumptions: Vec::new(),
        }
    }

    /// The instants, from the first attempt, at which the harness sends its attempts when every
    /// attempt fails at once (only when the table gives the gaps).
    pub fn attempt_timeline(&self) -> Vec<Duration> {
        let mut at = Duration::ZERO;
        let mut out = vec![at];
        for gap in &self.retry_gaps {
            at += *gap;
            out.push(at);
        }
        out
    }

    /// The headers of a main request with the session value filled in.
    pub fn request_headers(&self, session: &str) -> Vec<(String, String)> {
        self.headers.iter().map(|(n, v)| ((*n).to_string(), v.replace("{session}", session))).collect()
    }

    /// What this harness does with a `Retry-After`: the wait, or `None` for "fails at once or
    /// ignores it" (the caller reads `retry_after`).
    pub fn wait_for_retry_after(&self, value: Duration) -> Option<Duration> {
        match self.retry_after {
            RetryAfterPolicy::HonoursUpTo(limit) if value <= limit => Some(value),
            _ => None,
        }
    }

    /// The bytes a raw client sends for one kind of request (FIX-025).
    pub fn raw_request(kind: RawRequestKind, oversized_bytes: usize) -> Vec<u8> {
        let body = "{\"model\":\"local-coder\",\"messages\":[]}";
        match kind {
            RawRequestKind::Malformed => b"POST /v1/chat/completions HTTP/1.1\r\nHost: proxy\r\nContent-Length: not-a-number\r\n\r\n{".to_vec(),
            RawRequestKind::Chunked => format!("POST /v1/chat/completions HTTP/1.1\r\nHost: proxy\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{body}\r\n0\r\n\r\n", body.len()).into_bytes(),
            RawRequestKind::ExpectContinue => format!("POST /v1/chat/completions HTTP/1.1\r\nHost: proxy\r\nExpect: 100-continue\r\nContent-Length: {}\r\n\r\n{body}", body.len()).into_bytes(),
            RawRequestKind::Oversized => {
                let pad = "x".repeat(oversized_bytes);
                let big = format!("{{\"model\":\"local-coder\",\"pad\":\"{pad}\"}}");
                format!("POST /v1/chat/completions HTTP/1.1\r\nHost: proxy\r\nContent-Length: {}\r\n\r\n{big}", big.len()).into_bytes()
            }
            RawRequestKind::Http10 => format!("POST /v1/chat/completions HTTP/1.0\r\nContent-Length: {}\r\n\r\n{body}", body.len()).into_bytes(),
        }
    }
}

/// The script of one harness.
pub fn script_for(kind: HarnessKind) -> HarnessScript {
    match kind {
        HarnessKind::Pi => {
            let mut s = HarnessScript::base(kind, "pi", &["FIX-020", "FIX-260"]);
            s.headers = vec![("x-session-affinity", "{session}")];
            s.session_headers = vec!["x-session-affinity"];
            s.retry_gaps = vec![secs(2), secs(4), secs(8)];
            s.attempts = Some(4);
            s.give_up = Some(secs(299));
            s.retry_after = RetryAfterPolicy::Ignores;
            s.compaction_without_session_header = true;
            s
        }
        HarnessKind::ClaudeCode => {
            let mut s = HarnessScript::base(kind, "claude-code", &["FIX-021"]);
            s.headers = vec![("x-claude-code-session-id", "{session}"), ("x-claude-code-agent-id", "{session}-agent"), ("anthropic-beta", "{eleven beta flags}")];
            s.session_headers = vec!["x-claude-code-session-id", "x-claude-code-agent-id"];
            s.give_up = Some(secs(360));
            s.retry_after = RetryAfterPolicy::HonoursUpTo(secs(60));
            s.assumptions = vec!["The names of the 11 beta headers are not in the table; the value above is a placeholder.", "The retry gaps of Claude Code are not in the table."];
            s
        }
        HarnessKind::OpenWebUi => {
            let mut s = HarnessScript::base(kind, "open-webui", &["FIX-022", "FIX-266"]);
            s.headers = vec![("x-openwebui-chat-id", "{session}")];
            s.session_headers = vec!["x-openwebui-chat-id"];
            s.fan_out = Some(20);
            s.assumptions = vec!["Docs only: the sub-agent requests carry the parent model; retry behaviour is not in the table."];
            s
        }
        HarnessKind::Opencode => {
            let mut s = HarnessScript::base(kind, "opencode", &["FIX-324"]);
            s.headers = vec![("x-session-affinity", "{session}"), ("x-session-id", "{session}")];
            s.session_headers = vec!["x-session-affinity", "x-session-id"];
            s.give_up = Some(secs(300));
            s.attempts = Some(9);
            s.retry_after = RetryAfterPolicy::Ignores;
            s.assumptions = vec!["The gaps between the 9 attempts are not in the table.", "The session value starts with ses_ (the table says one ses_ value)."];
            s
        }
        HarnessKind::DeepSeek => {
            let mut s = HarnessScript::base(kind, "deepseek-harness", &["FIX-325"]);
            s.give_up = Some(secs(299));
            s.attempts = Some(7);
            s.retry_after = RetryAfterPolicy::Ignores;
            s.assumptions = vec!["It sends a title call and the main request at session start; no session header."];
            s
        }
        HarnessKind::Codex => {
            let mut s = HarnessScript::base(kind, "codex", &["FIX-326"]);
            s.headers = vec![("session-id", "{session}"), ("thread-id", "{session}-thread"), ("x-codex-window-id", "{session}-window")];
            s.session_headers = vec!["session-id", "thread-id", "x-codex-window-id"];
            s.fails_at_once_on = vec![429];
            s.attempts = Some(30);
            s.assumptions = vec!["The body carries prompt_cache_key and an environment item first in input; 30 is the number of requests the table says, not an attempt limit."];
            s
        }
        HarnessKind::Sdk => {
            let mut s = HarnessScript::base(kind, "sdk-clients", &["FIX-327"]);
            s.give_up_variants = vec![("node", secs(301)), ("python", secs(600))];
            s.give_up = Some(secs(301));
            s.attempts = Some(3);
            s.retry_after = RetryAfterPolicy::NotSpecified;
            s.assumptions = vec!["Retry-After is honoured as the table in spec 04 says; the table is not repeated here."];
            s
        }
        HarnessKind::Headerless => {
            let mut s = HarnessScript::base(kind, "headerless", &["FIX-024", "FIX-268"]);
            s.session_headers = Vec::new();
            s
        }
        HarnessKind::RawHttp => {
            let mut s = HarnessScript::base(kind, "raw-http", &["FIX-025"]);
            s.raw_requests = vec![RawRequestKind::Malformed, RawRequestKind::Chunked, RawRequestKind::ExpectContinue, RawRequestKind::Oversized, RawRequestKind::Http10];
            s
        }
    }
}
