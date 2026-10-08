//! Reading a conversation key from the headers of a request, in the order of a map. The first
//! source that gives a non-empty value wins; two sources are never merged, only the two headers
//! of one pair are joined. The raw value is never logged.
use crate::config::routes::{KeyMapEntry, KeySourceKind};
use crate::key::harness::HarnessTable;
use crate::key::hasher::{ConversationKey, KeyHasher, FIELD_SEPARATOR};
use crate::obs::log_sink::{LogRecord, LogSink, SystemRecord};
use http::HeaderMap;
use legatus_common::event::SystemEventKind;
use legatus_common::protocol::Protocol;
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// The body field a Responses client uses to name its conversation.
pub const PROMPT_CACHE_KEY_FIELD: &str = "prompt_cache_key";
pub const WARNING_KEY_HEADER_MISSING: &str = "key_header_missing";
pub const WARNING_KEY_HEADER_REPEAT: &str = "key_header_repeat";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HeaderRead {
    Absent,
    Present { value: Vec<u8>, repeated_differs: bool },
}

/// The first line of a header, trimmed of spaces and tabs at both ends; the name matches without
/// regard to case. A value empty after trimming is absent.
pub fn read_header(headers: &HeaderMap, name: &str) -> HeaderRead {
    let mut lines = headers.get_all(name).iter();
    let Some(first) = lines.next() else { return HeaderRead::Absent };
    let value = first.as_bytes().trim_ascii().to_vec();
    if value.is_empty() {
        return HeaderRead::Absent;
    }
    let repeated_differs = lines.any(|other| other.as_bytes().trim_ascii() != value.as_slice());
    HeaderRead::Present { value, repeated_differs }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeySource {
    Header(String),
    HeaderPair(String, String),
    BodyField(String),
    BodyHash,
    Credential,
    None,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceUsed {
    Header(String),
    HeaderPair(String, String),
    BodyField(String),
    BodyHash,
    Credential,
    None,
}

impl SourceUsed {
    /// The `key_source` of the request event: `header:<name>`, `derived` or `none` (the names of
    /// the other sources are fixed words).
    pub fn label(&self) -> String {
        match self {
            SourceUsed::Header(name) => format!("header:{name}"),
            SourceUsed::HeaderPair(session, _) => format!("header:{session}"),
            SourceUsed::BodyField(name) => format!("body_field:{name}"),
            SourceUsed::BodyHash => "derived".to_string(),
            SourceUsed::Credential => "credential".to_string(),
            SourceUsed::None => "none".to_string(),
        }
    }
}

/// What `resolve_key` found: the key and the source it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyResolution {
    pub key: Option<ConversationKey>,
    pub source: SourceUsed,
}

/// The sources of a route, read in list order. A map that the operator wrote warns once for a
/// header it names that a request lacks; the built-in default does not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyMap {
    pub sources: Vec<KeySource>,
    pub warn_missing: bool,
}

impl KeyMap {
    /// The default order, with header names from the harness table. Chat and Messages: the
    /// session header of each row in table order (a row with a session and an agent header is a
    /// pair), then the derived key, then the credential. Responses: session and thread header,
    /// the body field `prompt_cache_key`, the derived key, the credential.
    pub fn default_for(protocol: Protocol, table: &HarnessTable) -> KeyMap {
        let mut sources = Vec::new();
        if protocol == Protocol::OpenAiResponses {
            for (session, thread) in table.responses_headers() {
                sources.push(KeySource::Header(session));
                sources.push(KeySource::Header(thread));
            }
            sources.push(KeySource::BodyField(PROMPT_CACHE_KEY_FIELD.to_string()));
        } else {
            let pairs = table.pairs();
            for session in table.session_headers() {
                match pairs.iter().find(|(s, _)| *s == session) {
                    Some((s, a)) => sources.push(KeySource::HeaderPair(s.clone(), a.clone())),
                    None => sources.push(KeySource::Header(session)),
                }
            }
        }
        sources.push(KeySource::BodyHash);
        sources.push(KeySource::Credential);
        KeyMap { sources, warn_missing: false }
    }

    /// The alias shorthand: the listed headers, then the derived key when `hash_fallback` is on.
    pub fn from_alias(key_headers: &[String], hash_fallback: bool) -> KeyMap {
        let mut sources: Vec<KeySource> = key_headers.iter().map(|h| KeySource::Header(h.clone())).collect();
        if hash_fallback {
            sources.push(KeySource::BodyHash);
        }
        KeyMap { sources, warn_missing: true }
    }

    /// The key map of a route as the registry wrote it.
    pub fn from_route(entries: &[KeyMapEntry]) -> KeyMap {
        let sources = entries
            .iter()
            .filter_map(|e| match (e.kind, e.names.as_slice()) {
                (KeySourceKind::Header, [name]) => Some(KeySource::Header(name.clone())),
                (KeySourceKind::Header, names) if !names.is_empty() => None,
                (KeySourceKind::HeaderPair, [a, b]) => Some(KeySource::HeaderPair(a.clone(), b.clone())),
                (KeySourceKind::BodyField, [name]) => Some(KeySource::BodyField(name.clone())),
                (KeySourceKind::BodyHash, _) => Some(KeySource::BodyHash),
                (KeySourceKind::Credential, _) => Some(KeySource::Credential),
                (KeySourceKind::None, _) => Some(KeySource::None),
                _ => None,
            })
            .collect();
        KeyMap { sources, warn_missing: true }
    }
}

/// The part of the body that a key may use. A header key never touches it.
pub trait BodyKeySource {
    fn body_field(&self, name: &str) -> Option<Vec<u8>>;
    fn derived(&self) -> Option<ConversationKey>;
}

/// A body that offers nothing (an unreadable body, or a test that must not touch it).
pub struct NoBodyKey;

impl BodyKeySource for NoBodyKey {
    fn body_field(&self, _name: &str) -> Option<Vec<u8>> {
        None
    }
    fn derived(&self) -> Option<ConversationKey> {
        None
    }
}

/// One flag per warning per start, and the sink the first one goes to. Soft state: a new start
/// warns again, by design.
pub struct WarnOnce {
    seen: Mutex<HashSet<String>>,
    sink: Arc<dyn LogSink>,
    repeats: AtomicU64,
}

impl WarnOnce {
    pub fn new(sink: Arc<dyn LogSink>) -> WarnOnce {
        WarnOnce { seen: Mutex::new(HashSet::new()), sink, repeats: AtomicU64::new(0) }
    }

    /// True the first time this label is asked for.
    pub fn first_time(&self, label: &str) -> bool {
        self.seen.lock().map(|mut seen| seen.insert(label.to_string())).unwrap_or(false)
    }

    /// How many times a header sent two different lines (the metric of story 138).
    pub fn repeat_count(&self) -> u64 {
        self.repeats.load(Ordering::Relaxed)
    }

    fn warn(&self, code: &'static str, header: &str) {
        if self.first_time(&format!("{code}:{header}")) {
            let record = SystemRecord { code: Some(code), ..SystemRecord::new(SystemEventKind::Warning) };
            let _ = self.sink.offer(LogRecord::System(record));
        }
    }
}

fn pair_data(session: &[u8], agent: Option<&[u8]>) -> Vec<u8> {
    let mut data = session.to_vec();
    if let Some(agent) = agent {
        data.push(FIELD_SEPARATOR);
        data.extend_from_slice(agent);
    }
    data
}

/// Walk the map in order and stop at the first source that gives a value. The body is asked only
/// by a body source. A map that is empty gives no key.
pub fn resolve_key(map: &KeyMap, headers: &HeaderMap, body: &dyn BodyKeySource, hasher: &KeyHasher, warn: &WarnOnce, _table: &HarnessTable) -> KeyResolution {
    let missing = |name: &str| {
        if map.warn_missing {
            warn.warn(WARNING_KEY_HEADER_MISSING, name);
        }
    };
    let note_repeat = |name: &str, repeated: bool| {
        if repeated {
            warn.repeats.fetch_add(1, Ordering::Relaxed);
            warn.warn(WARNING_KEY_HEADER_REPEAT, name);
        }
    };
    for source in &map.sources {
        match source {
            KeySource::Header(name) => match read_header(headers, name) {
                HeaderRead::Present { value, repeated_differs } => {
                    note_repeat(name, repeated_differs);
                    let domain = format!("header:{}", name.to_ascii_lowercase());
                    return KeyResolution { key: Some(hasher.hash(domain.as_bytes(), &value)), source: SourceUsed::Header(name.clone()) };
                }
                HeaderRead::Absent => missing(name),
            },
            KeySource::HeaderPair(session_name, agent_name) => match read_header(headers, session_name) {
                HeaderRead::Present { value, repeated_differs } => {
                    note_repeat(session_name, repeated_differs);
                    let agent = match read_header(headers, agent_name) {
                        HeaderRead::Present { value, repeated_differs } => {
                            note_repeat(agent_name, repeated_differs);
                            Some(value)
                        }
                        HeaderRead::Absent => None,
                    };
                    let domain = format!("pair:{}", session_name.to_ascii_lowercase());
                    return KeyResolution { key: Some(hasher.hash(domain.as_bytes(), &pair_data(&value, agent.as_deref()))), source: SourceUsed::HeaderPair(session_name.clone(), agent_name.clone()) };
                }
                HeaderRead::Absent => missing(session_name),
            },
            KeySource::BodyField(name) => {
                if let Some(value) = body.body_field(name).map(|v| v.trim_ascii().to_vec()).filter(|v| !v.is_empty()) {
                    let domain = format!("body_field:{name}");
                    return KeyResolution { key: Some(hasher.hash(domain.as_bytes(), &value)), source: SourceUsed::BodyField(name.clone()) };
                }
            }
            KeySource::BodyHash => {
                if let Some(key) = body.derived() {
                    return KeyResolution { key: Some(key), source: SourceUsed::BodyHash };
                }
            }
            // The credential key is story 180's; the walk stops here and hands over.
            KeySource::Credential => return KeyResolution { key: None, source: SourceUsed::Credential },
            KeySource::None => return KeyResolution { key: None, source: SourceUsed::None },
        }
    }
    KeyResolution { key: None, source: SourceUsed::None }
}
