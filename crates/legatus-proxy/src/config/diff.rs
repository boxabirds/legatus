//! What changed between two registries (story 179).
use crate::config::node::NodeSpec;
use crate::config::typed::Registry;
use legatus_common::ids::{AliasName, NodeId};

/// A node present in both registries whose record differs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeChange {
    pub name: NodeId,
    /// One of the five keys of PRX-REG-024 changed: engine version, model, context, patch or
    /// flags. Story 147 probes the node again.
    pub probe_key_changed: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RegistryDiff {
    pub nodes_added: Vec<NodeId>,
    pub nodes_removed: Vec<NodeId>,
    pub nodes_changed: Vec<NodeChange>,
    /// Aliases added, removed or changed.
    pub aliases_changed: Vec<AliasName>,
    pub machines_changed: Vec<String>,
    pub routes_changed: bool,
    /// Names of the settings whose effective value differs.
    pub settings_changed: Vec<&'static str>,
    /// Start-only settings whose declared value, waiting for a restart, differs from before.
    pub restart_pending_changed: Vec<&'static str>,
    /// Places that hold a secret reference whose reference text differs (never a value).
    pub secret_refs_changed: Vec<String>,
}

impl RegistryDiff {
    pub fn is_empty(&self) -> bool {
        *self == RegistryDiff::default()
    }
}

fn probe_key_differs(old: &NodeSpec, new: &NodeSpec) -> bool {
    old.engine_version != new.engine_version
        || old.model != new.model
        || old.context_per_slot != new.context_per_slot
        || old.on_overflow != new.on_overflow
        || old.patch != new.patch
        || old.engine_flags != new.engine_flags
}

/// Compare two registries. Pure; names come out sorted.
pub fn diff_registries(old: &Registry, new: &Registry) -> RegistryDiff {
    let mut diff = RegistryDiff::default();
    for node in &new.nodes {
        match old.nodes.iter().find(|n| n.name == node.name) {
            None => diff.nodes_added.push(node.name.clone()),
            Some(before) if before != node => diff.nodes_changed.push(NodeChange { name: node.name.clone(), probe_key_changed: probe_key_differs(before, node) }),
            Some(_) => {}
        }
        let before_ref = old.nodes.iter().find(|n| n.name == node.name).and_then(|n| n.auth_key_ref.as_ref());
        if before_ref != node.auth_key_ref.as_ref() && (before_ref.is_some() || node.auth_key_ref.is_some()) {
            diff.secret_refs_changed.push(format!("nodes.{}.auth.key_ref", node.name));
        }
    }
    for node in &old.nodes {
        if !new.nodes.iter().any(|n| n.name == node.name) {
            diff.nodes_removed.push(node.name.clone());
            if node.auth_key_ref.is_some() {
                diff.secret_refs_changed.push(format!("nodes.{}.auth.key_ref", node.name));
            }
        }
    }
    for alias in new.aliases.iter() {
        if old.aliases.resolve(&alias.name.0).ok().is_none_or(|before| before != alias) {
            diff.aliases_changed.push(alias.name.clone());
        }
    }
    for alias in old.aliases.iter() {
        if new.aliases.resolve(&alias.name.0).is_err() {
            diff.aliases_changed.push(alias.name.clone());
        }
    }
    for machine in &new.machines {
        if old.machines.iter().find(|m| m.name == machine.name).is_none_or(|before| before != machine) {
            diff.machines_changed.push(machine.name.clone());
        }
    }
    for machine in &old.machines {
        if !new.machines.iter().any(|m| m.name == machine.name) {
            diff.machines_changed.push(machine.name.clone());
        }
    }
    diff.routes_changed = old.routes != new.routes;
    let (before, after) = (old.settings.values(), new.settings.values());
    for (name, value) in &after {
        if before.iter().find(|(n, _)| n == name).map(|(_, v)| v) != Some(value) {
            diff.settings_changed.push(name);
        }
    }
    let (pending_before, pending_after) = (old.settings.pending_restart(), new.settings.pending_restart());
    for (name, value) in pending_after {
        if pending_before.iter().find(|(n, _)| n == name).map(|(_, v)| v) != Some(value) {
            diff.restart_pending_changed.push(name);
        }
    }
    for (name, _) in pending_before {
        if !pending_after.iter().any(|(n, _)| n == name) {
            diff.restart_pending_changed.push(name);
        }
    }
    if before.iter().find(|(n, _)| *n == "client_tokens_ref") != after.iter().find(|(n, _)| *n == "client_tokens_ref") {
        diff.secret_refs_changed.push("settings.client_tokens_ref".to_string());
    }
    diff.nodes_added.sort();
    diff.nodes_removed.sort();
    diff.nodes_changed.sort_by(|a, b| a.name.cmp(&b.name));
    diff.aliases_changed.sort();
    diff.aliases_changed.dedup();
    diff.machines_changed.sort();
    diff.machines_changed.dedup();
    diff.secret_refs_changed.sort();
    diff.secret_refs_changed.dedup();
    diff
}
