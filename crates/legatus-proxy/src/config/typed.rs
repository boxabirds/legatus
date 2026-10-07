//! The typed registry that the request path reads, and the handle that swaps it whole (story 121).
use crate::config::alias::AliasTable;
use crate::config::node::{MachineSpec, NodeSpec};
use crate::config::read::read_registry_text;
use crate::config::registry::{LoadedRegistry, RegistryError};
use crate::config::routes::RouteSpec;
use crate::config::settings::EffectiveSettings;
use std::sync::{Arc, RwLock};
use yaml_serde::Value;

/// One row of the `harnesses` list (contract C10).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HarnessRowSpec {
    pub name: String,
    pub session_header: Option<String>,
    pub agent_header: Option<String>,
    pub never_key_headers: Vec<String>,
    pub title_alias: Option<String>,
    pub user_agent_prefix: Option<String>,
}

fn text(map: &yaml_serde::Mapping, key: &str) -> Option<String> {
    map.get(key).and_then(Value::as_str).map(str::to_string)
}

/// Read the harness rows. Rows are already checked by the shape walk; a row without a name is skipped.
pub fn read_harness_rows(tree: &Value) -> Vec<HarnessRowSpec> {
    let Some(rows) = tree.as_mapping().and_then(|root| root.get("harnesses")).and_then(Value::as_sequence) else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(Value::as_mapping)
        .filter_map(|row| {
            Some(HarnessRowSpec {
                name: text(row, "name")?,
                session_header: text(row, "session_header"),
                agent_header: text(row, "agent_header"),
                never_key_headers: row.get("never_key_headers").and_then(Value::as_sequence).map(|l| l.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default(),
                title_alias: text(row, "title_alias"),
                user_agent_prefix: text(row, "user_agent_prefix"),
            })
        })
        .collect()
}

/// Everything the request path needs from a loaded registry file.
#[derive(Clone, Debug)]
pub struct Registry {
    pub generation: u64,
    pub settings: EffectiveSettings,
    pub machines: Vec<MachineSpec>,
    pub nodes: Vec<NodeSpec>,
    pub aliases: AliasTable,
    pub routes: Vec<RouteSpec>,
    pub harness_rows: Vec<HarnessRowSpec>,
}

impl Registry {
    pub fn from_loaded(l: &LoadedRegistry, generation: u64) -> Registry {
        Registry {
            generation,
            settings: l.settings.clone(),
            machines: l.machines.clone(),
            nodes: l.nodes.clone(),
            aliases: l.aliases.clone(),
            routes: l.routes.clone(),
            harness_rows: read_harness_rows(&l.raw.0),
        }
    }

    /// Load from registry text, for tests and tools. Generation 1.
    pub fn from_text(text: &str) -> Result<Registry, Vec<RegistryError>> {
        read_registry_text(text, "registry").map(|l| Registry::from_loaded(&l, 1))
    }

    pub fn node(&self, name: &legatus_common::ids::NodeId) -> Option<&NodeSpec> {
        self.nodes.iter().find(|n| &n.name == name)
    }
}

/// The registry in use. A reload (story 179) stores a new `Registry` whole; a request takes one
/// snapshot at its start and keeps it to its end.
pub struct RegistryHandle {
    current: RwLock<Arc<Registry>>,
}

impl RegistryHandle {
    pub fn new(registry: Arc<Registry>) -> RegistryHandle {
        RegistryHandle { current: RwLock::new(registry) }
    }

    /// A fleet with no node and no alias: every model name is unknown.
    pub fn empty() -> RegistryHandle {
        let loaded = read_registry_text("version: 1\nnodes: {}\naliases: {}\n", "empty").expect("the empty fleet is valid");
        RegistryHandle::new(Arc::new(Registry::from_loaded(&loaded, 0)))
    }

    pub fn snapshot(&self) -> Arc<Registry> {
        match self.current.read() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    pub fn store(&self, registry: Arc<Registry>) {
        match self.current.write() {
            Ok(mut guard) => *guard = registry,
            Err(poisoned) => *poisoned.into_inner() = registry,
        }
    }
}
