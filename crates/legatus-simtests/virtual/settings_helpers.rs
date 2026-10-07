//! Helpers for the settings tests (story 165).
use legatus_proxy::config::alias::{read_aliases, AliasTable};
use legatus_proxy::config::node::read_nodes;
use legatus_proxy::config::read::parse_registry_text;
use legatus_proxy::config::registry::*;
use legatus_proxy::config::settings::{read_settings, EffectiveSettings};
use legatus_proxy::config::validate::validate_registry;

pub const PATH_LABEL: &str = "registry.yaml";

/// A valid registry whose `settings` map holds `listen` plus the given lines (indented two spaces),
/// and `alias_extra` on the alias `x`.
pub fn registry_with(settings_lines: &str, alias_extra: &str) -> String {
    format!(
        "version: 1\nsettings:\n  listen: 127.0.0.1:8080\n{settings_lines}nodes:\n  a:\n    engine: {{ name: ollama }}\n    model: m\n    endpoints: [ {{ protocol: openai-chat, base_url: \"http://a\" }} ]\naliases:\n  x: {{ nodes: [a]{alias_extra} }}\n"
    )
}

pub fn registry(settings_lines: &str) -> String {
    registry_with(settings_lines, "")
}

pub fn report(text: &str) -> ValidationReport {
    validate_registry(&parse_registry_text(text, PATH_LABEL).expect("valid structured text"))
}

pub fn errors(text: &str) -> Vec<String> {
    report(text).errors.iter().map(|e| e.to_string()).collect()
}

pub fn warnings(text: &str) -> Vec<String> {
    report(text).warnings.iter().map(|w| w.to_string()).collect()
}

/// The effective settings of a registry text, and the errors raised while reading them.
pub fn effective(text: &str) -> (Option<EffectiveSettings>, Vec<String>) {
    let doc = parse_registry_text(text, PATH_LABEL).unwrap();
    let typed = read_nodes(&doc.0, &mut ValidationReport::default());
    let aliases: AliasTable = read_aliases(&doc.0, &typed, &mut ValidationReport::default()).unwrap_or_default();
    let mut out = ValidationReport::default();
    let settings = read_settings(doc.0.as_mapping().and_then(|r| r.get("settings")), &aliases, &mut out);
    (settings, out.errors.iter().map(|e| e.to_string()).collect())
}

/// Defaults with no settings map at all.
pub fn defaults() -> EffectiveSettings {
    read_settings(None, &AliasTable::default(), &mut ValidationReport::default()).expect("defaults are valid")
}
