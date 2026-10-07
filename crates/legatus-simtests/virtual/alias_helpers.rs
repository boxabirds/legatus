//! Helpers for the alias tests (story 153).
use legatus_proxy::config::alias::{read_aliases, AliasTable};
use legatus_proxy::config::node::read_nodes;
use legatus_proxy::config::read::parse_registry_text;
use legatus_proxy::config::registry::*;
use legatus_proxy::config::validate::validate_registry;

pub const PATH_LABEL: &str = "registry.yaml";

/// One node: name, engine, model (YAML scalar text) and the endpoint protocols (empty for none).
pub fn node_yaml(name: &str, engine: &str, model: &str, protocols: &[&str]) -> String {
    let endpoints: Vec<String> = protocols.iter().map(|p| format!("{{ protocol: {p}, base_url: \"http://{name}:8080\" }}")).collect();
    let responses = if protocols.contains(&"openai-responses") { "    responses: true\n" } else { "" };
    format!("  {name}:\n    engine: {{ name: {engine} }}\n    model: {model}\n    endpoints: [ {} ]\n{responses}", endpoints.join(", "))
}

pub fn registry(nodes: &[String], aliases_body: &str) -> String {
    format!("version: 1\nnodes:\n{}aliases:\n{aliases_body}", nodes.concat())
}

pub fn errors(text: &str) -> Vec<String> {
    validate_registry(&parse_registry_text(text, PATH_LABEL).expect("valid structured text")).errors.iter().map(|e| e.to_string()).collect()
}

pub fn table(text: &str) -> Option<AliasTable> {
    let doc = parse_registry_text(text, PATH_LABEL).unwrap();
    let typed = read_nodes(&doc.0, &mut ValidationReport::default());
    read_aliases(&doc.0, &typed, &mut ValidationReport::default())
}
