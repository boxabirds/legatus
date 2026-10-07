//! Story 153 unit tests: exact lookup, the unknown model and the node model name.
use super::alias_helpers::*;
use legatus_common::ids::NodeId;
use legatus_proxy::config::alias::ResolveError;
use legatus_proxy::protocol::route::{route_by_model, Refusal, MODEL_NOT_FOUND_CODE};

const CHAT: &[&str] = &["openai-chat"];
const PI_CHAT_BODY: &str = include_str!("../fixtures/bodies/pi_chat.json");
const CLAUDE_BODY: &str = include_str!("../fixtures/bodies/claude_messages.json");

fn node_id(name: &str) -> NodeId {
    NodeId(name.to_string())
}

#[test]
fn tc01_an_exact_name_resolves_to_the_alias() {
    let t = table(&registry(&[node_yaml("a", "ollama", "qwen", CHAT)], "  local-coder: { nodes: [a] }\n")).unwrap();
    let alias = t.resolve("local-coder").unwrap();
    assert_eq!(alias.name.0, "local-coder");
    assert_eq!(alias.pool().nodes, vec![node_id("a")]);
    assert_eq!(alias.pool().alias.0, "local-coder");
    assert_eq!(t.names().iter().map(|n| n.0.as_str()).collect::<Vec<_>>(), vec!["local-coder"]);
}

#[test]
fn tc02_a_different_case_a_trailing_space_a_prefix_and_the_empty_string_are_unknown() {
    let t = table(&registry(&[node_yaml("a", "ollama", "qwen", CHAT)], "  local-coder: { nodes: [a] }\n")).unwrap();
    for wrong in ["Local-Coder", "local-coder ", " local-coder", "local", "local-coder-2", "", "local\u{2010}coder", "ｌocal-coder"] {
        assert_eq!(t.resolve(wrong).unwrap_err(), ResolveError::UnknownModel, "{wrong:?}");
    }
}

#[test]
fn tc15_the_error_value_holds_no_text_of_the_model_value() {
    let t = table(&registry(&[node_yaml("a", "ollama", "qwen", CHAT)], "  x: { nodes: [a] }\n")).unwrap();
    let canary = "sk-ant-api03-Zk3QfN8vLm2PxTcR9wYbHd7sUe1AoJqGiV5nXtKzB4";
    let error = t.resolve(canary).unwrap_err();
    assert_eq!(format!("{error:?}"), "UnknownModel");
    let refusal = route_by_model(&t, canary).unwrap_err();
    assert_eq!(format!("{refusal:?}"), "ModelNotFound");
    assert_eq!((refusal, refusal.code()), (Refusal::ModelNotFound, MODEL_NOT_FOUND_CODE));
    assert_eq!(MODEL_NOT_FOUND_CODE, "model_not_found");
}

#[test]
fn tc09_node_model_is_the_model_of_that_node_and_not_the_alias_name() {
    let t = table(&registry(&[node_yaml("strix", "llama-server", "qwen3.8-27b-q4", CHAT)], "  local-coder: { nodes: [strix] }\n")).unwrap();
    assert_eq!(t.resolve("local-coder").unwrap().node_model(&node_id("strix")), "qwen3.8-27b-q4");
}

#[test]
fn tc10_two_nodes_with_different_model_names_each_give_their_own() {
    let nodes = vec![node_yaml("strix", "llama-server", "qwen3.8-27b-q4", CHAT), node_yaml("mini", "ollama", "qwen3:4b", CHAT)];
    let t = table(&registry(&nodes, "  pool: { nodes: [strix, mini] }\n")).unwrap();
    let alias = t.resolve("pool").unwrap();
    assert_eq!(alias.node_model(&node_id("strix")), "qwen3.8-27b-q4");
    assert_eq!(alias.node_model(&node_id("mini")), "qwen3:4b");
}

#[test]
fn tc11_a_node_in_two_aliases_gives_the_same_model_name_in_both() {
    let nodes = vec![node_yaml("strix", "llama-server", "qwen3.8-27b-q4", CHAT)];
    let t = table(&registry(&nodes, "  one: { nodes: [strix] }\n  two: { nodes: [strix] }\n")).unwrap();
    assert_eq!(t.resolve("one").unwrap().node_model(&node_id("strix")), t.resolve("two").unwrap().node_model(&node_id("strix")));
}

/// Test-local stand-in for the byte-exact splice of story 121: replace the value of the
/// top-level `model` string and leave every other byte alone.
fn write_model(body: &str, old: &str, new_model: &str) -> String {
    let needle = format!("\"model\":\"{old}\"");
    assert!(body.contains(&needle));
    let escaped = new_model.replace('\\', "\\\\").replace('"', "\\\"");
    body.replacen(&needle, &format!("\"model\":\"{escaped}\""), 1)
}

#[test]
fn tc12_the_name_returned_is_the_name_written_into_a_pi_chat_and_a_claude_messages_body() {
    let nodes = vec![node_yaml("strix", "llama-server", "qwen3.8-27b-q4", CHAT), node_yaml("claude", "anthropic-hosted", "claude-example-model", &["anthropic-messages"])];
    let t = table(&registry(&nodes, "  local-coder: { nodes: [strix] }\n  frontier: { nodes: [claude] }\n")).unwrap();
    let pi = write_model(PI_CHAT_BODY, "local-coder", t.resolve("local-coder").unwrap().node_model(&node_id("strix")));
    assert_eq!(pi, PI_CHAT_BODY.replacen("\"model\":\"local-coder\"", "\"model\":\"qwen3.8-27b-q4\"", 1));
    let claude = write_model(CLAUDE_BODY, "frontier", t.resolve("frontier").unwrap().node_model(&node_id("claude")));
    assert_eq!(claude, CLAUDE_BODY.replacen("\"model\":\"frontier\"", "\"model\":\"claude-example-model\"", 1));
    let parsed: serde_json::Value = serde_json::from_str(&pi).unwrap();
    assert_eq!(parsed["model"], "qwen3.8-27b-q4");
    assert_eq!(parsed["messages"], serde_json::from_str::<serde_json::Value>(PI_CHAT_BODY).unwrap()["messages"], "no other value changed");
}

#[test]
fn tc13_a_model_name_with_a_quote_and_a_backslash_is_returned_unchanged() {
    let node = node_yaml("odd", "ollama", "'ab\"c\\d'", CHAT);
    let t = table(&registry(&[node], "  x: { nodes: [odd] }\n")).unwrap();
    let name = t.resolve("x").unwrap().node_model(&node_id("odd"));
    assert_eq!(name, "ab\"c\\d");
    let written = write_model(PI_CHAT_BODY, "local-coder", name);
    let parsed: serde_json::Value = serde_json::from_str(&written).expect("still valid JSON");
    assert_eq!(parsed["model"], "ab\"c\\d");
}
