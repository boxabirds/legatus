//! Story 136 unit tests: flag defaults, contradictions, warm capacity and the cache flags.
use super::node_helpers::*;
use legatus_proxy::config::node::*;
use legatus_proxy::config::read::parse_registry_text;
use legatus_proxy::config::registry::*;

fn nodes_of(text: &str) -> Vec<NodeSpec> {
    let doc = parse_registry_text(text, PATH_LABEL).unwrap();
    read_nodes(&doc.0, &mut ValidationReport::default())
}

#[test]
fn tc01_with_no_flags_every_default_holds() {
    let n = &nodes_of(&registry(&node("llama-server", ""), ""))[0];
    assert_eq!((n.responses, n.stateful_responses, n.ignores_previous_response_id), (false, false, false));
    assert_eq!(n.warm_capacity, WarmCapacitySetting::Auto);
    assert_eq!((n.engine_flags.prefix_caching, n.engine_flags.speculative_decoding), (TriState::Unknown, TriState::Unknown));
    assert_eq!((n.affinity_mode, n.always_on, n.cache_kind), (AffinityModeSetting::Auto, true, CacheKind::Unknown));
}

#[test]
fn tc02_one_alias_with_responses_true_and_false_nodes_keeps_each_flag_as_declared() {
    let text = format!(
        "version: 1\nnodes:\n  a:\n    engine: {{ name: llama-server }}\n    model: m\n    endpoints: [ {{ protocol: openai-responses, base_url: \"http://a\" }} ]\n    responses: true\n  b:\n    engine: {{ name: ollama }}\n    model: m\n    endpoints: [ {{ protocol: openai-chat, base_url: \"http://b\" }} ]\n    responses: false\naliases:\n  both: {{ nodes: [a, b] }}\n"
    );
    assert!(errors(&text).is_empty(), "{:?}", errors(&text));
    let nodes = nodes_of(&text);
    assert_eq!((nodes[0].name.0.as_str(), nodes[0].responses), ("a", true));
    assert_eq!((nodes[1].name.0.as_str(), nodes[1].responses), ("b", false));
}

#[test]
fn tc06_an_openai_responses_address_without_the_flag_is_one_bad_responses_flag() {
    let text = "version: 1\nnodes:\n  n1:\n    engine: { name: vllm }\n    model: m\n    endpoints: [ { protocol: openai-responses, base_url: \"http://h\" } ]\n    cache: { prompt_tokens_details: true }\naliases: {}\n";
    assert_eq!(errors(text), vec!["registry error bad_responses_flag at nodes.n1.responses: Responses flags do not agree."]);
    assert!(errors(&text.replace("model: m\n", "model: m\n    responses: true\n")).is_empty());
}

#[test]
fn tc07_stateful_and_ignores_both_true_is_one_error_and_either_alone_is_accepted() {
    let both = registry(&node("ollama", "    stateful_responses: true\n    ignores_previous_response_id: true\n"), "");
    assert_eq!(errors(&both), vec!["registry error bad_responses_flag at nodes.n1.stateful_responses: Responses flags do not agree."]);
    assert!(errors(&registry(&node("ollama", "    stateful_responses: true\n"), "")).is_empty());
    assert!(errors(&registry(&node("ollama", "    ignores_previous_response_id: true\n"), "")).is_empty());
}

#[test]
fn tc06_tc07_no_node_list_comes_back_on_error_for_the_registry_but_the_other_nodes_are_still_checked() {
    let text = format!(
        "version: 1\nnodes:\n  bad:\n    engine: {{ name: ollama }}\n    model: m\n    endpoints: [ {{ protocol: openai-responses, base_url: \"http://h\" }} ]\n  worse:\n    engine: {{ name: ollama }}\n    model: m\n    endpoints: [ {{ protocol: openai-chat, base_url: \"http://h\" }} ]\n    stateful_responses: true\n    ignores_previous_response_id: true\naliases: {{}}\n"
    );
    let found = errors(&text);
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found[0].contains("nodes.bad.responses") && found[1].contains("nodes.worse.stateful_responses"));
}

#[test]
fn tc08_warm_capacity_below_equal_auto_and_zero() {
    let below = registry(&node("llama-server", "    slots: 4\n    warm_capacity: 3\n"), "");
    assert_eq!(errors(&below), vec!["registry error bad_warm_capacity at nodes.n1.warm_capacity: Warm capacity cannot be lower than the slots."]);
    assert!(errors(&registry(&node("llama-server", "    slots: 4\n    warm_capacity: 4\n"), "")).is_empty());
    assert!(errors(&registry(&node("llama-server", "    slots: 4\n    warm_capacity: 9\n"), "")).is_empty());
    assert!(errors(&registry(&node("llama-server", "    slots: 4\n    warm_capacity: auto\n"), "")).is_empty());
    assert_eq!(
        errors(&registry(&node("llama-server", "    warm_capacity: 0\n"), "")),
        vec!["registry error bad_warm_capacity at nodes.n1.warm_capacity: Warm capacity must be auto or a whole number of 1 or more."]
    );
    assert_eq!(errors(&registry(&node("llama-server", "    warm_capacity: sometimes\n"), "")), vec!["registry error bad_type at nodes.n1.warm_capacity: Expected a whole number or auto, found string."]);
}

#[test]
fn tc08_warm_capacity_is_not_compared_when_slots_are_auto_or_unset_or_the_node_is_hosted() {
    assert!(errors(&registry(&node("llama-server", "    slots: auto\n    warm_capacity: 1\n"), "")).is_empty());
    assert!(errors(&registry(&node("ollama", "    warm_capacity: 1\n"), "")).is_empty());
    let hosted = "version: 1\nnodes:\n  h:\n    engine: { name: anthropic-hosted }\n    model: m\n    endpoints: [ { protocol: anthropic-messages, base_url: \"https://api.anthropic.com\" } ]\n    slots: 4\n    warm_capacity: 2\naliases: {}\n";
    assert!(errors(hosted).is_empty());
}

#[test]
fn tc13_the_cache_flags_take_true_false_and_unknown_and_nothing_else() {
    for (text, want) in [("true", TriState::True), ("false", TriState::False), ("unknown", TriState::Unknown)] {
        let n = &nodes_of(&registry(&node("vllm", &format!("    cache: {{ prompt_tokens_details: true }}\n    engine_flags: {{ prefix_caching: {text}, speculative_decoding: {text} }}\n")), ""))[0];
        assert_eq!((n.engine_flags.prefix_caching, n.engine_flags.speculative_decoding), (want, want), "{text}");
    }
    let maybe = registry(&node("vllm", "    cache: { prompt_tokens_details: true }\n    engine_flags: { prefix_caching: maybe }\n"), "");
    assert_eq!(errors(&maybe), vec!["registry error bad_type at nodes.n1.engine_flags.prefix_caching: Expected true, false or unknown, found string."]);
    let number = registry(&node("vllm", "    cache: { prompt_tokens_details: true }\n    engine_flags: { speculative_decoding: 3 }\n"), "");
    assert_eq!(errors(&number), vec!["registry error bad_type at nodes.n1.engine_flags.speculative_decoding: Expected true, false or unknown, found integer."]);
}

#[test]
fn tc13_a_flag_that_is_not_a_boolean_is_a_bad_type_by_kind() {
    assert_eq!(errors(&registry(&node("ollama", "    responses: yes please\n"), "")), vec!["registry error bad_type at nodes.n1.responses: Expected boolean, found string."]);
}

#[test]
fn tc13_a_negative_count_is_refused_by_kind_and_range() {
    assert_eq!(
        errors(&registry(&node("llama-server", "    engine_flags: { np: -1 }\n"), "")),
        vec!["registry error bad_type at nodes.n1.engine_flags.np: Expected a whole number of 0 or more that fits in 32 bits, found integer."]
    );
}

#[test]
fn tc20_the_node_view_shows_each_node_with_flags_unknown_as_unknown_and_its_warnings_only() {
    let text = "version: 1\nnodes:\n  a:\n    engine: { name: mlx_lm }\n    model: m\n    endpoints: [ { protocol: openai-responses, base_url: \"http://a\" } ]\n    responses: true\n    engine_flags: { prefix_caching: true }\n  b:\n    engine: { name: ollama }\n    model: m\n    endpoints: [ { protocol: openai-chat, base_url: \"http://b\" } ]\naliases: {}\n";
    let report = run(text);
    assert!(report.errors.is_empty());
    let doc = parse_registry_text(text, PATH_LABEL).unwrap();
    let nodes = read_nodes(&doc.0, &mut ValidationReport::default());
    let views = node_config_views(&nodes, &report.warnings);
    assert_eq!(views.len(), 2);
    assert_eq!((views[0].name.0.as_str(), views[0].engine, views[0].responses, views[0].prefix_caching, views[0].speculative_decoding), ("a", "mlx_lm", true, TriState::True, TriState::Unknown));
    assert_eq!(views[0].warnings, vec![WarningCode::ResponsesEngineMismatch]);
    assert_eq!((views[1].name.0.as_str(), views[1].responses, views[1].prefix_caching.as_str()), ("b", false, "unknown"));
    assert!(views[1].warnings.is_empty());
}
