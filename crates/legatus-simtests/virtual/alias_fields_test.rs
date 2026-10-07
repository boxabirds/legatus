//! Story 153 unit tests: alias fields, errors and the protocol union.
use super::alias_helpers::*;
use legatus_proxy::config::node::EndpointProtocol;

const CHAT: &[&str] = &["openai-chat"];

fn two_chat_nodes() -> Vec<String> {
    vec![node_yaml("a", "ollama", "qwen", CHAT), node_yaml("b", "llama-server", "qwen-b", CHAT)]
}

#[test]
fn tc03_two_aliases_that_share_one_node_both_resolve_and_list_it() {
    let text = registry(&two_chat_nodes(), "  one: { nodes: [a] }\n  two: { nodes: [a, b] }\n");
    assert!(errors(&text).is_empty());
    let t = table(&text).expect("table");
    assert_eq!(t.resolve("one").unwrap().nodes.iter().map(|n| n.0.as_str()).collect::<Vec<_>>(), vec!["a"]);
    assert_eq!(t.resolve("two").unwrap().nodes.iter().map(|n| n.0.as_str()).collect::<Vec<_>>(), vec!["a", "b"]);
}

#[test]
fn tc04_a_chat_only_node_and_a_messages_only_node_give_both_protocols() {
    let nodes = vec![node_yaml("a", "ollama", "m", CHAT), node_yaml("b", "anthropic-hosted", "c", &["anthropic-messages"])];
    let t = table(&registry(&nodes, "  x: { nodes: [a, b] }\n")).unwrap();
    let set = t.resolve("x").unwrap().protocols;
    assert!(set.contains(EndpointProtocol::OpenaiChat) && set.contains(EndpointProtocol::AnthropicMessages));
    assert!(!set.contains(EndpointProtocol::OpenaiResponses));
}

#[test]
fn tc05_a_chat_node_plus_a_chat_and_responses_node_give_chat_and_responses() {
    let nodes = vec![node_yaml("a", "ollama", "m", CHAT), node_yaml("b", "llama-server", "m", &["openai-chat", "openai-responses"])];
    let t = table(&registry(&nodes, "  x: { nodes: [a, b] }\n")).unwrap();
    let set = t.resolve("x").unwrap().protocols;
    assert!(set.contains(EndpointProtocol::OpenaiChat) && set.contains(EndpointProtocol::OpenaiResponses));
    assert!(!set.is_empty());
}

#[test]
fn tc06_an_alias_whose_nodes_list_no_endpoint_is_no_common_protocol_and_not_empty_alias() {
    let nodes = vec![node_yaml("a", "ollama", "m", &[]), node_yaml("b", "ollama", "m", &[])];
    let found = errors(&registry(&nodes, "  x: { nodes: [a, b] }\n"));
    assert_eq!(found, vec!["registry error no_common_protocol at aliases.x: The nodes of this alias serve no common protocol."]);
    assert!(table(&registry(&nodes, "  x: { nodes: [a, b] }\n")).is_none(), "no table on error");
}

#[test]
fn tc06_a_protocols_field_in_the_file_is_an_unknown_field() {
    let text = registry(&two_chat_nodes(), "  x: { nodes: [a], protocols: [openai-chat] }\n");
    assert_eq!(errors(&text), vec!["registry error unknown_field at aliases.x.protocols: Field is not part of the schema."]);
}

#[test]
fn tc07_no_nodes_an_unknown_node_and_a_repeated_node_each_give_their_code_with_the_alias_path() {
    let text = registry(&two_chat_nodes(), "  empty: { nodes: [] }\n  ghost: { nodes: [a, nope] }\n  twice: { nodes: [a, a] }\n  fine: { nodes: [b] }\n");
    assert_eq!(
        errors(&text),
        vec![
            "registry error empty_alias at aliases.empty.nodes: An alias needs at least one node.",
            "registry error unknown_ref at aliases.ghost.nodes.1: Name does not exist.",
            "registry error duplicate_name at aliases.twice.nodes.1: Two entries use this name.",
        ]
    );
    assert!(table(&text).is_none());
}

#[test]
fn tc07_a_repeated_alias_name_is_duplicate_name_at_the_aliases_map() {
    let text = registry(&two_chat_nodes(), "  x: { nodes: [a] }\n  x: { nodes: [b] }\n");
    let error = legatus_proxy::config::read::parse_registry_text(&text, PATH_LABEL).expect_err("two x keys");
    assert_eq!((error.code, error.path.as_str()), (legatus_proxy::config::registry::ErrorCode::DuplicateName, "aliases"));
}

#[test]
fn tc07_an_alias_naming_a_node_that_has_its_own_error_adds_no_second_error() {
    let nodes = vec![node_yaml("a", "ollama", "m", CHAT).replace("    model: m\n", "    model: m\n    slots: 0\n")];
    let found = errors(&registry(&nodes, "  x: { nodes: [a] }\n"));
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("bad_slots at nodes.a.slots"));
}

#[test]
fn tc08_alias_fields_are_read_with_their_defaults_and_order() {
    let text = registry(&two_chat_nodes(), "  x:\n    nodes: [a]\n    affinity: { key_headers: [x-session-affinity, x-claude-code-session-id] }\n    description: the coder pool\n    hold_limit_s: 120\n  y: { nodes: [b] }\n");
    assert!(errors(&text).is_empty());
    let t = table(&text).unwrap();
    let x = t.resolve("x").unwrap();
    assert_eq!(x.key_headers, vec!["x-session-affinity", "x-claude-code-session-id"]);
    assert_eq!((x.hash_fallback, x.description.as_deref(), x.hold_limit_s), (true, Some("the coder pool"), Some(120)));
    let y = t.resolve("y").unwrap();
    assert_eq!((y.hash_fallback, y.key_headers.len(), y.description.clone(), y.hold_limit_s), (true, 0, None, None));
    let off = registry(&two_chat_nodes(), "  x: { nodes: [a], affinity: { hash_fallback: false } }\n");
    assert!(!table(&off).unwrap().resolve("x").unwrap().hash_fallback);
}

#[test]
fn tc08_a_misspelled_field_and_a_wrong_kind_are_reported_with_their_paths() {
    let text = registry(&two_chat_nodes(), "  x:\n    nodes: [a]\n    descripton: typo\n    affinity: { hash_fallback: sometimes, key_hedaers: [] }\n");
    assert_eq!(
        errors(&text),
        vec![
            "registry error bad_type at aliases.x.affinity.hash_fallback: Expected boolean, found string.",
            "registry error unknown_field at aliases.x.affinity.key_hedaers: Field is not part of the schema.",
            "registry error unknown_field at aliases.x.descripton: Field is not part of the schema.",
        ]
    );
    assert_eq!(errors(&registry(&two_chat_nodes(), "  x: { nodes: [a, 7] }\n")), vec!["registry error bad_type at aliases.x.nodes.1: Expected string, found integer."]);
    assert_eq!(errors(&registry(&two_chat_nodes(), "  x: {}\n")), vec!["registry error missing_key at aliases.x.nodes: Required field is missing."]);
}

#[test]
fn tc14_an_alias_with_20_nodes_builds_and_keeps_the_declared_order() {
    let names: Vec<String> = (0..20).map(|i| format!("n{:02}", 19 - i)).collect();
    let nodes: Vec<String> = names.iter().map(|n| node_yaml(n, "ollama", n, CHAT)).collect();
    let text = registry(&nodes, &format!("  big: {{ nodes: [{}] }}\n", names.join(", ")));
    assert!(errors(&text).is_empty());
    let t = table(&text).unwrap();
    let alias = t.resolve("big").unwrap();
    assert_eq!(alias.nodes.iter().map(|n| n.0.clone()).collect::<Vec<_>>(), names);
}
