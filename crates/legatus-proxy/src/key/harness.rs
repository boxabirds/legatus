//! Which harness sent a request, by the names of the headers it uses for a session. One table,
//! built in or read from the registry key `harnesses:`. A harness that is not in the table is
//! `unknown`, never a refusal. The built-in rows are ASSUMPTIONS from the recorded headers of
//! spike C; Open WebUI was never measured.
use crate::config::typed::HarnessRowSpec;
use http::HeaderMap;
use legatus_common::ids::AliasName;

/// How much of `user-agent` is looked at when matching a prefix (PROPOSED).
pub const USER_AGENT_PREFIX_SCAN_BYTES: usize = 256;

/// Headers no default map reads as a conversation: they change every request or are a request id.
pub const FIXED_NEVER_KEY_HEADERS: [&str; 6] = ["x-codex-window-id", "x-client-request-id", "x-stainless-retry-count", "x-claude-code-parent-agent-id", "x-codex-turn-metadata", "x-codex-parent-thread-id"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HarnessLabel {
    Known(String),
    Unknown,
}

impl HarnessLabel {
    pub fn as_str(&self) -> &str {
        match self {
            HarnessLabel::Known(name) => name,
            HarnessLabel::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Debug)]
struct Row {
    spec: HarnessRowSpec,
    /// The second id header of a Responses harness (Codex `thread-id`).
    thread_header: Option<String>,
}

#[derive(Clone, Debug)]
pub struct HarnessTable {
    rows: Vec<Row>,
}

fn row(name: &str, session: Option<&str>, agent: Option<&str>, thread: Option<&str>, prefix: Option<&str>) -> Row {
    Row {
        spec: HarnessRowSpec {
            name: name.to_string(),
            session_header: session.map(str::to_string),
            agent_header: agent.map(str::to_string),
            never_key_headers: Vec::new(),
            title_alias: None,
            user_agent_prefix: prefix.map(str::to_string),
        },
        thread_header: thread.map(str::to_string),
    }
}

impl Default for HarnessTable {
    /// The built-in rows, in the order of the default key map: pi, opencode, Claude Code, Codex,
    /// Open WebUI. DeepSeek Harness and the SDK clients send no session header and are matched by
    /// their user agent only.
    fn default() -> Self {
        HarnessTable {
            rows: vec![
                row("pi", Some("x-session-affinity"), None, None, Some("pi")),
                row("opencode", Some("x-session-id"), None, None, Some("opencode")),
                row("claude-code", Some("x-claude-code-session-id"), Some("x-claude-code-agent-id"), None, Some("claude-cli")),
                row("codex", Some("session-id"), None, Some("thread-id"), Some("codex")),
                row("open-webui", Some("x-openwebui-chat-id"), None, None, None),
                row("deepseek-harness", None, None, None, Some("deepseek")),
                row("sdk", None, None, None, Some("openai/")),
            ],
        }
    }
}

fn has(headers: &HeaderMap, name: &str) -> bool {
    headers.get(name).is_some_and(|v| !v.as_bytes().trim_ascii().is_empty())
}

impl HarnessTable {
    pub fn from_specs(specs: &[HarnessRowSpec]) -> HarnessTable {
        HarnessTable { rows: specs.iter().map(|spec| Row { spec: spec.clone(), thread_header: None }).collect() }
    }

    /// (1) the first row whose session or agent header is present; (2) else the first row whose
    /// user-agent prefix starts the first 256 bytes of `user-agent`, ignoring case; (3) else unknown.
    pub fn classify(&self, headers: &HeaderMap) -> HarnessLabel {
        let by_header = self.rows.iter().find(|r| r.spec.session_header.as_deref().is_some_and(|h| has(headers, h)) || r.spec.agent_header.as_deref().is_some_and(|h| has(headers, h)));
        if let Some(r) = by_header {
            return HarnessLabel::Known(r.spec.name.clone());
        }
        let agent = headers.get("user-agent").map(|v| v.as_bytes()).unwrap_or_default();
        let scanned = String::from_utf8_lossy(&agent[..agent.len().min(USER_AGENT_PREFIX_SCAN_BYTES)]).to_ascii_lowercase();
        let by_agent = self.rows.iter().find(|r| r.spec.user_agent_prefix.as_deref().is_some_and(|p| !p.is_empty() && scanned.starts_with(&p.to_ascii_lowercase())));
        by_agent.map(|r| HarnessLabel::Known(r.spec.name.clone())).unwrap_or(HarnessLabel::Unknown)
    }

    /// The session header of each row, in row order, without repeats.
    pub fn session_headers(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for h in self.rows.iter().filter_map(|r| r.spec.session_header.clone()) {
            if !out.contains(&h) {
                out.push(h);
            }
        }
        out
    }

    pub fn agent_headers(&self) -> Vec<String> {
        self.rows.iter().filter_map(|r| r.spec.agent_header.clone()).collect()
    }

    /// The `(session, agent)` header pairs of rows that have both, in row order.
    pub fn pairs(&self) -> Vec<(String, String)> {
        self.rows.iter().filter_map(|r| Some((r.spec.session_header.clone()?, r.spec.agent_header.clone()?))).collect()
    }

    /// The `(session, thread)` header names of a Responses harness.
    pub fn responses_headers(&self) -> Vec<(String, String)> {
        self.rows.iter().filter_map(|r| Some((r.spec.session_header.clone()?, r.thread_header.clone()?))).collect()
    }

    /// The headers of the rows' own lists and the fixed list.
    pub fn never_key_headers(&self) -> Vec<String> {
        let mut out: Vec<String> = FIXED_NEVER_KEY_HEADERS.iter().map(|h| (*h).to_string()).collect();
        for h in self.rows.iter().flat_map(|r| r.spec.never_key_headers.iter()) {
            let h = h.to_ascii_lowercase();
            if !out.contains(&h) {
                out.push(h);
            }
        }
        out
    }

    /// Does this harness send title or side calls on the given alias (story 180 asks).
    pub fn is_title_alias(&self, label: &HarnessLabel, alias: &AliasName) -> bool {
        let HarnessLabel::Known(name) = label else { return false };
        self.rows.iter().any(|r| &r.spec.name == name && r.spec.title_alias.as_deref() == Some(alias.0.as_str()))
    }
}
