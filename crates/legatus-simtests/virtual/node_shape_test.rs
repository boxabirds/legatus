//! Story 136 unit tests: node and machine shape, slots, machine references.
use super::node_helpers::*;
use legatus_proxy::config::node::*;
use legatus_proxy::config::read::parse_registry_text;
use legatus_proxy::config::registry::*;

fn nodes_of(text: &str) -> Vec<NodeSpec> {
    let doc = parse_registry_text(text, PATH_LABEL).unwrap();
    read_nodes(&doc.0, &mut ValidationReport::default())
}

#[test]
fn tc03_slots_0_gives_one_bad_slots_at_the_node_and_no_node_record() {
    let text = registry(&node("llama-server", "    slots: 0\n"), "");
    assert_eq!(errors(&text), vec!["registry error bad_slots at nodes.n1.slots: Slots must be a whole number of 1 or more, or auto for an engine that reports its slots."]);
    assert!(nodes_of(&text).is_empty(), "a node with an error is not returned");
}

#[test]
fn tc03_negative_and_text_slots_are_refused_too() {
    assert_eq!(errors(&registry(&node("llama-server", "    slots: -2\n"), "")).len(), 1);
    assert_eq!(errors(&registry(&node("llama-server", "    slots: many\n"), "")), vec!["registry error bad_type at nodes.n1.slots: Expected a whole number or auto, found string."]);
}

#[test]
fn tc04_slots_1_is_accepted() {
    let text = registry(&node("ollama", "    slots: 1\n"), "");
    assert!(errors(&text).is_empty());
    assert_eq!(nodes_of(&text)[0].slots, Some(Slots::Count(1)));
}

#[test]
fn tc05_slots_auto_is_accepted_on_llama_server_and_refused_on_ollama() {
    assert!(errors(&registry(&node("llama-server", "    slots: auto\n"), "")).is_empty());
    assert_eq!(errors(&registry(&node("ollama", "    slots: auto\n"), "")).len(), 1);
    assert!(errors(&registry(&node("ollama", "    slots: auto\n"), ""))[0].starts_with("registry error bad_slots at nodes.n1.slots"));
    for engine in ["mlx_lm", "vllm", "sglang", "gufo", "lmstudio", "openai-compatible"] {
        assert_eq!(errors(&registry(&node(engine, "    slots: auto\n"), "")).len(), 1, "{engine}");
    }
}

#[test]
fn tc14_a_name_outside_the_list_and_a_bad_url_each_give_one_error_with_the_field_path() {
    assert_eq!(errors(&registry(&node("llamaa-server", ""), "")), vec!["registry error bad_type at nodes.n1.engine.name: Value is not one of the allowed values."]);
    let protocol = "  n1:\n    engine: { name: ollama }\n    model: m\n    endpoints: [ { protocol: openai-chatt, base_url: \"http://h\" } ]\n";
    assert_eq!(errors(&registry(protocol, "")), vec!["registry error bad_type at nodes.n1.endpoints.0.protocol: Value is not one of the allowed values."]);
    let overflow = node("ollama", "    context: { per_slot: 100, on_overflow: truncate }\n");
    assert_eq!(errors(&registry(&overflow, "")), vec!["registry error bad_type at nodes.n1.context.on_overflow: Value is not one of the allowed values."]);
    let ftp = "  n1:\n    engine: { name: ollama }\n    model: m\n    endpoints: [ { protocol: openai-chat, base_url: \"ftp://h\" } ]\n";
    assert_eq!(errors(&registry(ftp, "")), vec!["registry error bad_url at nodes.n1.endpoints.0.base_url: Address is not a valid http or https URL."]);
    let no_host = ftp.replace("ftp://h", "not a url");
    assert_eq!(errors(&registry(&no_host, "")).len(), 1);
    for good in ["http://h", "https://h:8443/v1", "http://10.0.0.5:8081"] {
        assert!(errors(&registry(&ftp.replace("ftp://h", good), "")).is_empty(), "{good}");
    }
}

#[test]
fn tc14_cache_kind_and_affinity_mode_outside_the_list_are_refused_and_listed_values_pass() {
    assert_eq!(errors(&registry(&node("vllm", "    cache: { kind: warm, prompt_tokens_details: true }\n"), "")), vec!["registry error bad_type at nodes.n1.cache.kind: Value is not one of the allowed values."]);
    assert_eq!(errors(&registry(&node("vllm", "    affinity_mode: maybe\n    cache: { prompt_tokens_details: true }\n"), "")), vec!["registry error bad_type at nodes.n1.affinity_mode: Value is not one of the allowed values."]);
    for kind in ["dense", "hybrid", "unknown"] {
        assert!(errors(&registry(&node("vllm", &format!("    cache: {{ kind: {kind}, prompt_tokens_details: true }}\n")), "")).is_empty(), "{kind}");
    }
}

#[test]
fn tc15_a_missing_machine_is_unknown_ref_and_no_machine_and_a_known_machine_are_valid() {
    assert_eq!(errors(&registry(&node("ollama", "    machine: mini\n"), "")), vec!["registry error unknown_ref at nodes.n1.machine: Machine is not defined."]);
    assert!(errors(&registry(&node("ollama", ""), "")).is_empty());
    assert!(errors(&registry(&node("ollama", "    machine: mini\n"), "machines:\n  mini: { host: mini.local, mem_gb: 16 }\n")).is_empty());
}

#[test]
fn tc15_a_repeated_node_name_is_duplicate_name() {
    let text = format!("{}{}", registry(&node("ollama", ""), ""), "");
    let repeated = text.replacen("aliases: {}\n", &format!("{}aliases: {{}}\n", node("ollama", "")), 1);
    let error = parse_registry_text(&repeated, PATH_LABEL).expect_err("two n1 keys in one map");
    assert_eq!((error.code, error.path.as_str()), (ErrorCode::DuplicateName, "nodes"));
    assert!(error.text.starts_with("A name is used twice. At line "), "{}", error.text);
    assert!(!error.text.contains("n1"), "the repeated name is not echoed in the text");
}

#[test]
fn tc15_machines_are_typed_and_a_bad_mem_gb_is_reported() {
    let text = registry(&node("ollama", ""), "machines:\n  mini: { host: mini.local, mem_gb: 16, agent: \"http://mini.local:9100\" }\n  big: { mem_gb: lots }\n");
    assert_eq!(errors(&text), vec!["registry error bad_type at machines.big.mem_gb: Expected number, found string."]);
    let doc = parse_registry_text(&registry(&node("ollama", ""), "machines:\n  mini: { host: mini.local, mem_gb: 16.5 }\n"), PATH_LABEL).unwrap();
    let machines = read_machines(&doc.0, &mut ValidationReport::default());
    assert_eq!(machines, vec![MachineSpec { name: "mini".into(), host: Some("mini.local".into()), mem_gb: Some(16.5), agent: None }]);
}

#[test]
fn tc16_silent_truncate_without_per_slot_is_context_missing_and_with_it_is_valid() {
    assert_eq!(
        errors(&registry(&node("ollama", "    context: { on_overflow: silent_truncate }\n"), "")),
        vec!["registry error context_missing at nodes.n1.context.per_slot: Context size per slot is required when the engine silently truncates."]
    );
    assert!(errors(&registry(&node("ollama", "    context: { per_slot: 8192, on_overflow: silent_truncate }\n"), "")).is_empty());
    assert!(errors(&registry(&node("ollama", "    context: { on_overflow: error_400 }\n"), "")).is_empty());
}

#[test]
fn tc17_unknown_fields_inside_a_node_a_machine_and_nested_maps_give_the_full_path() {
    let text = registry(&node("ollama", "    bogus: 1\n    engine_flags: { jinjaa: true }\n    cache: { kindd: dense }\n"), "machines:\n  m1: { hostt: x }\n");
    assert_eq!(
        errors(&text),
        vec![
            "registry error unknown_field at machines.m1.hostt: Field is not part of the schema.",
            "registry error unknown_field at nodes.n1.bogus: Field is not part of the schema.",
            "registry error unknown_field at nodes.n1.cache.kindd: Field is not part of the schema.",
            "registry error unknown_field at nodes.n1.engine_flags.jinjaa: Field is not part of the schema.",
        ]
    );
}

#[test]
fn tc17_a_node_without_model_or_endpoints_is_missing_key_and_the_spec_only_fields_are_accepted() {
    assert_eq!(
        errors("version: 1\nnodes:\n  n1:\n    engine: { name: ollama }\naliases: {}\n"),
        vec!["registry error missing_key at nodes.n1.endpoints: Required field is missing.", "registry error missing_key at nodes.n1.model: Required field is missing."]
    );
    let accepted = node("llama-server", "    patch: { set: { a: 1 }, remove: [b] }\n    auth: { scheme: bearer, key_ref: \"env:KEY\" }\n    always_on: false\n    cold_allowance_tokens: 1000\n    affinity_mode: off\n    paths: [ /v1/embeddings ]\n    quantisation: q4\n    engine_flags: { jinja: true, np: 4, ctx_checkpoints: 32, checkpoint_min_step: 8192, cache_ram_mib: 8192, num_parallel: 2, keep_alive_s: 60, kv_unified: true, prefix_caching: true, speculative_decoding: false, ctx_size: 262144, vllm_block_size: 16 }\n");
    assert!(errors(&registry(&accepted, "")).is_empty());
}

#[test]
fn tc17_a_fully_declared_node_is_read_with_every_field() {
    let text = registry(
        &node(
            "llama-server",
            "    quantisation: q4\n    machine: strix\n    slots: 4\n    warm_capacity: 8\n    paths: [ /v1/embeddings ]\n    context: { per_slot: 65536, on_overflow: error_400 }\n    cache: { kind: hybrid }\n    always_on: false\n    affinity_mode: on\n    cold_allowance_tokens: 2000\n    engine_flags: { np: 4, cache_ram_mib: 8192, prefix_caching: true }\n",
        ),
        "machines:\n  strix: { host: strix.local }\n",
    );
    let n = &nodes_of(&text)[0];
    assert_eq!((n.name.0.as_str(), n.engine, n.model.as_str()), ("n1", EngineName::LlamaServer, "m"));
    assert_eq!((n.slots, n.warm_capacity, n.context_per_slot, n.on_overflow), (Some(Slots::Count(4)), WarmCapacitySetting::Count(8), Some(65536), Some(OnOverflow::Error400)));
    assert_eq!((n.cache_kind, n.always_on, n.affinity_mode, n.cold_allowance_tokens), (CacheKind::Hybrid, false, AffinityModeSetting::On, Some(2000)));
    assert_eq!((n.engine_flags.np, n.engine_flags.cache_ram_mib, n.engine_flags.prefix_caching), (Some(4), Some(8192), TriState::True));
    assert_eq!((n.machine.as_deref(), n.quantisation.as_deref(), n.paths.clone()), (Some("strix"), Some("q4"), vec!["/v1/embeddings".to_string()]));
    assert_eq!(n.endpoints, vec![Endpoint { protocol: EndpointProtocol::OpenaiChat, base_url: "http://h:8080".into() }]);
}

#[test]
fn tc18_a_canary_secret_in_the_auth_map_and_the_model_name_never_reaches_a_text() {
    let canary = "sk-ant-api03-Zk3QfN8vLm2PxTcR9wYbHd7sUe1AoJqGiV5nXtKzB4";
    let text = format!(
        "version: 1\nnodes:\n  n1:\n    engine: {{ name: anthropic-hosted }}\n    model: {canary}\n    endpoints: [ {{ protocol: anthropic-messages, base_url: \"https://api.anthropic.com\" }} ]\n    auth: {{ scheme: x-api-key, key_ref: {canary} }}\n    slots: 0\n    warm_capacity: {canary}\n    context: {{ on_overflow: {canary} }}\naliases: {{}}\n"
    );
    let report = run(&text);
    assert!(!report.errors.is_empty());
    for e in &report.errors {
        assert!(!e.to_string().contains("sk-ant"), "{e}");
    }
    for w in &report.warnings {
        assert!(!w.to_string().contains("sk-ant"), "{w}");
    }
    let doc = parse_registry_text(&text, PATH_LABEL).unwrap();
    for view in node_config_views(&read_nodes(&doc.0, &mut ValidationReport::default()), &report.warnings) {
        assert!(!format!("{view:?}").contains("sk-ant"));
    }
}
