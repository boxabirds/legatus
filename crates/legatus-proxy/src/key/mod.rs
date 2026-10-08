//! The conversation key (contract C30, story 126): one rule for reading it from the headers.
pub mod harness;
pub mod hasher;
pub mod sources;

use hasher::KEY_LEN_BYTES;

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ConversationKey(pub [u8; KEY_LEN_BYTES]);

impl std::fmt::Debug for ConversationKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ConversationKey(..)")
    }
}

/// How sure a key is that it names one conversation (the logic is story 180).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyClass {
    Strong,
    Derived,
    Weak,
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

pub mod lazy_body;

use crate::config::alias::AliasSpec;
use crate::config::routes::RouteSpec;
use harness::HarnessTable;
use legatus_common::protocol::Protocol;
use sources::KeyMap;

/// The key map of a request: the map of its route, else the alias shorthand, else the default of
/// the protocol (PROPOSED: a route map wins over the alias shorthand).
pub fn choose_map(routes: &[RouteSpec], path: &str, alias: &AliasSpec, protocol: Protocol, table: &HarnessTable) -> KeyMap {
    if let Some(route) = routes.iter().find(|r| r.path == path && !r.key_map.is_empty()) {
        return KeyMap::from_route(&route.key_map);
    }
    if !alias.key_headers.is_empty() {
        return KeyMap::from_alias(&alias.key_headers, alias.hash_fallback);
    }
    KeyMap::default_for(protocol, table)
}
