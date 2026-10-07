//! Helpers for the node tests (story 136): build registry text around one node and run the checks.
use legatus_proxy::config::read::parse_registry_text;
use legatus_proxy::config::registry::*;
use legatus_proxy::config::validate::validate_registry;

pub const PATH_LABEL: &str = "registry.yaml";

/// A registry with the given `nodes:` body (already indented by two spaces) and extra top-level text.
pub fn registry(nodes_body: &str, extra_top: &str) -> String {
    format!("version: 1\nnodes:\n{nodes_body}aliases: {{}}\n{extra_top}")
}

/// One node named `n1` on the given engine, with extra lines (indented four spaces by the caller).
pub fn node(engine: &str, extra: &str) -> String {
    format!(
        "  n1:\n    engine: {{ name: {engine} }}\n    model: m\n    endpoints: [ {{ protocol: openai-chat, base_url: \"http://h:8080\" }} ]\n{extra}"
    )
}

pub fn run(text: &str) -> ValidationReport {
    validate_registry(&parse_registry_text(text, PATH_LABEL).expect("valid structured text"))
}

pub fn errors(text: &str) -> Vec<String> {
    run(text).errors.iter().map(|e| e.to_string()).collect()
}

pub fn warnings(text: &str) -> Vec<String> {
    run(text).warnings.iter().map(|w| w.to_string()).collect()
}
