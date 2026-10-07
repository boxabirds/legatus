//! Fleets for tests: a registry in text, turned into the deps of a router.
use legatus_proxy::config::typed::{Registry, RegistryHandle};
use legatus_proxy::{RouterDeps, Seams};
use std::sync::Arc;

/// The alias that `ONE_NODE_FLEET` serves.
pub const FLEET_ALIAS: &str = "local-coder";
/// The model name that the node of `ONE_NODE_FLEET` expects (not the alias).
pub const FLEET_NODE_MODEL: &str = "qwen-node";
/// The address of the node; the fake transport ignores it, the real one needs a real server.
pub const FLEET_NODE_URL: &str = "http://node.invalid";

/// A body that the one-node fleet routes.
pub const FLEET_REQUEST_BODY: &str = "{\"model\":\"local-coder\",\"messages\":[]}";

/// One chat node `n1` and one alias `local-coder`.
pub fn one_node_registry_text(node_url: &str) -> String {
    format!(
        "version: 1\nnodes:\n  n1:\n    engine: {{ name: llama-server }}\n    model: {FLEET_NODE_MODEL}\n    endpoints: [ {{ protocol: openai-chat, base_url: \"{node_url}\" }} ]\naliases:\n  {FLEET_ALIAS}: {{ nodes: [n1] }}\n"
    )
}

/// Deps whose registry is the given text. Panics when the text is not a valid registry.
pub fn deps_from_text(seams: Seams, text: &str) -> RouterDeps {
    let registry = Registry::from_text(text).expect("a valid registry for the test");
    RouterDeps::for_test(seams).with_registry(Arc::new(RegistryHandle::new(Arc::new(registry))))
}

/// Deps for the one-node fleet.
pub fn one_node_deps(seams: Seams) -> RouterDeps {
    deps_from_text(seams, &one_node_registry_text(FLEET_NODE_URL))
}
