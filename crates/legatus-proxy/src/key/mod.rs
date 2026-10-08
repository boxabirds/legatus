//! The conversation key (contract C30, story 126): one rule for reading it from the headers.
pub mod harness;
pub mod hasher;
pub mod sources;
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
