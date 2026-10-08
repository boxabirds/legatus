//! Aliases: the pool a model name selects, the protocol union and the node model name (contract C12).
use crate::config::node::{EndpointProtocol, NodeSpec};
use crate::config::registry::*;
use crate::config::schema::join_path;
use legatus_common::ids::{AliasName, NodeId};
use std::collections::BTreeMap;
use yaml_serde::{Mapping, Value};

/// A set of endpoint protocols, one bit each.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ProtocolSet(u8);

const BIT_OPENAI_CHAT: u8 = 1;
const BIT_OPENAI_RESPONSES: u8 = 1 << 1;
const BIT_ANTHROPIC_MESSAGES: u8 = 1 << 2;

impl ProtocolSet {
    pub const EMPTY: ProtocolSet = ProtocolSet(0);

    fn bit(p: EndpointProtocol) -> u8 {
        match p {
            EndpointProtocol::OpenaiChat => BIT_OPENAI_CHAT,
            EndpointProtocol::OpenaiResponses => BIT_OPENAI_RESPONSES,
            EndpointProtocol::AnthropicMessages => BIT_ANTHROPIC_MESSAGES,
        }
    }
    pub fn of(p: EndpointProtocol) -> ProtocolSet {
        ProtocolSet(ProtocolSet::bit(p))
    }
    pub fn union(self, other: ProtocolSet) -> ProtocolSet {
        ProtocolSet(self.0 | other.0)
    }
    pub fn contains(&self, p: EndpointProtocol) -> bool {
        self.0 & ProtocolSet::bit(p) != 0
    }
    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AliasSpec {
    pub name: AliasName,
    pub nodes: Vec<NodeId>,
    pub protocols: ProtocolSet,
    pub key_headers: Vec<String>,
    pub hash_fallback: bool,
    pub description: Option<String>,
    /// Range check is story 165.
    pub hold_limit_s: Option<u32>,
    /// The model name each listed node expects, copied from the node at load.
    node_models: Vec<(NodeId, String)>,
}

/// The pool of one alias, imported by stories 131 and 150.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AliasPool {
    pub alias: AliasName,
    pub nodes: Vec<NodeId>,
}

impl AliasSpec {
    /// The model name that `node` expects (never the alias name). A node that is not listed
    /// cannot occur, because the load refuses it; the fallback is an empty string.
    pub fn node_model(&self, node: &NodeId) -> &str {
        self.node_models.iter().find(|(id, _)| id == node).map(|(_, model)| model.as_str()).unwrap_or_default()
    }

    pub fn pool(&self) -> AliasPool {
        AliasPool { alias: self.name.clone(), nodes: self.nodes.clone() }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolveError {
    UnknownModel,
}

/// Name to alias, read-only after the load. A reload builds a new table and swaps it whole.
#[derive(Clone, Debug, Default)]
pub struct AliasTable {
    by_name: BTreeMap<String, AliasSpec>,
}

impl AliasTable {
    /// Exact, case sensitive (PROPOSED). Writes no log line and never echoes the value.
    pub fn resolve(&self, model: &str) -> Result<&AliasSpec, ResolveError> {
        self.by_name.get(model).ok_or(ResolveError::UnknownModel)
    }

    /// Alias names in sorted order.
    pub fn names(&self) -> Vec<&AliasName> {
        self.by_name.values().map(|a| &a.name).collect()
    }

    /// Every alias in name order.
    pub fn iter(&self) -> impl Iterator<Item = &AliasSpec> {
        self.by_name.values()
    }

    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }
}

pub const TEXT_EMPTY_ALIAS: &str = "An alias needs at least one node.";
pub const TEXT_UNKNOWN_NODE: &str = "Name does not exist.";
pub const TEXT_NAME_TWICE: &str = "Two entries use this name.";
pub const TEXT_NO_COMMON_PROTOCOL: &str = "The nodes of this alias serve no common protocol.";

/// An alias hold limit as stored: a negative value becomes 0 and a value above `u32::MAX`
/// becomes `u32::MAX`, so that the range check of story 165 refuses both.
fn alias_hold_limit(v: &Value) -> u32 {
    match v {
        Value::Number(n) if n.as_u64().is_some() => u32::try_from(n.as_u64().unwrap_or(0)).unwrap_or(u32::MAX),
        _ => 0,
    }
}

/// The union of the endpoint protocols of the given nodes.
pub fn alias_protocols(nodes: &[&NodeSpec]) -> ProtocolSet {
    nodes.iter().flat_map(|n| n.endpoints.iter()).fold(ProtocolSet::EMPTY, |set, e| set.union(ProtocolSet::of(e.protocol)))
}

fn read_alias(name: &str, map: &Mapping, node_names: &[String], typed: &[NodeSpec], out: &mut ValidationReport) -> Option<AliasSpec> {
    let errors_before = out.errors.len();
    let path = join_path("aliases", name);
    let nodes_path = join_path(&path, "nodes");
    let listed = map.get("nodes").and_then(Value::as_sequence)?; // a missing or wrong-kind list is reported by the shape walk
    if listed.is_empty() {
        out.errors.push(error_fixed(ErrorCode::EmptyAlias, &nodes_path, TEXT_EMPTY_ALIAS));
    }
    let mut ids: Vec<NodeId> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for (index, entry) in listed.iter().enumerate() {
        let entry_path = join_path(&nodes_path, &index.to_string());
        let Some(node) = entry.as_str() else {
            out.errors.push(error_wrong_type(&entry_path, "string", FoundKind::of(entry)));
            continue;
        };
        if seen.contains(&node) {
            out.errors.push(error_fixed(ErrorCode::DuplicateName, &entry_path, TEXT_NAME_TWICE));
            continue;
        }
        seen.push(node);
        if !node_names.iter().any(|n| n == node) {
            out.errors.push(error_fixed(ErrorCode::UnknownRef, &entry_path, TEXT_UNKNOWN_NODE));
            continue;
        }
        ids.push(NodeId(node.to_string()));
    }
    let members: Vec<&NodeSpec> = ids.iter().filter_map(|id| typed.iter().find(|n| &n.name == id)).collect();
    let protocols = alias_protocols(&members);
    if !members.is_empty() && protocols.is_empty() {
        out.errors.push(error_fixed(ErrorCode::NoCommonProtocol, &path, TEXT_NO_COMMON_PROTOCOL));
    }
    let affinity = map.get("affinity").and_then(Value::as_mapping);
    let mut key_headers = Vec::new();
    if let Some(list) = affinity.and_then(|a| a.get("key_headers")).and_then(Value::as_sequence) {
        for (index, item) in list.iter().enumerate() {
            match item.as_str() {
                Some(h) => key_headers.push(h.to_string()),
                None => out.errors.push(error_wrong_type(&join_path(&join_path(&join_path(&path, "affinity"), "key_headers"), &index.to_string()), "string", FoundKind::of(item))),
            }
        }
    }
    if out.errors.len() != errors_before || members.len() != ids.len() {
        return None;
    }
    Some(AliasSpec {
        name: AliasName(name.to_string()),
        nodes: ids,
        protocols,
        key_headers,
        hash_fallback: affinity.and_then(|a| a.get("hash_fallback")).and_then(Value::as_bool).unwrap_or(true),
        description: map.get("description").and_then(Value::as_str).map(str::to_string),
        hold_limit_s: map.get("hold_limit_s").map(alias_hold_limit),
        node_models: members.iter().map(|n| (n.name.clone(), n.model.clone())).collect(),
    })
}

/// Read every alias against the node names of the file and the typed nodes. Every alias is
/// checked even after an earlier error. `None` when any alias raised an error.
pub fn read_aliases(tree: &Value, typed: &[NodeSpec], out: &mut ValidationReport) -> Option<AliasTable> {
    let root = tree.as_mapping()?;
    let aliases = root.get("aliases").and_then(Value::as_mapping)?;
    let node_names: Vec<String> = root.get("nodes").and_then(Value::as_mapping).map(|m| m.keys().filter_map(|k| k.as_str().map(str::to_string)).collect()).unwrap_or_default();
    let errors_before = out.errors.len();
    let mut table = AliasTable::default();
    for (key, value) in aliases {
        let (Some(name), Some(map)) = (key.as_str(), value.as_mapping()) else { continue };
        if let Some(alias) = read_alias(name, map, &node_names, typed, out) {
            table.by_name.insert(name.to_string(), alias);
        }
    }
    (out.errors.len() == errors_before && table.len() == aliases.len()).then_some(table)
}
