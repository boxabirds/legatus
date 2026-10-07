//! Story 136 unit tests: the load warnings, which never stop the load.
use super::node_helpers::*;
use legatus_proxy::config::registry::*;

fn mlx(responses: bool) -> String {
    registry(&node("mlx_lm", &format!("    responses: {responses}\n")), "")
}

#[test]
fn tc09_mlx_lm_with_responses_true_loads_with_one_warning_and_zero_errors() {
    let report = run(&mlx(true));
    assert!(report.errors.is_empty());
    let shown: Vec<String> = report.warnings.iter().map(|w| w.to_string()).collect();
    assert_eq!(shown, vec!["warning responses_engine_mismatch at nodes.n1.responses: The engine answers 404 on the Responses path."]);
}

#[test]
fn tc10_mlx_lm_with_responses_false_gives_no_warning() {
    assert!(run(&mlx(false)).warnings.is_empty());
    assert!(run(&registry(&node("mlx_lm", ""), "")).warnings.is_empty());
}

#[test]
fn tc11_vllm_without_prompt_tokens_details_warns_and_with_it_or_on_other_engines_does_not() {
    let without = warnings(&registry(&node("vllm", ""), ""));
    assert_eq!(without.len(), 1);
    assert!(without[0].starts_with("warning vllm_no_prompt_tokens_details at nodes.n1.cache.prompt_tokens_details"));
    for declared in ["true", "false"] {
        assert!(warnings(&registry(&node("vllm", &format!("    cache: {{ prompt_tokens_details: {declared} }}\n")), "")).is_empty(), "{declared}");
    }
    for engine in ["llama-server", "ollama", "sglang", "mlx_lm"] {
        assert!(warnings(&registry(&node(engine, ""), "")).is_empty(), "{engine}");
    }
}

fn hosted(extra: &str) -> String {
    format!("version: 1\nnodes:\n  h:\n    engine: {{ name: openai-hosted }}\n    model: m\n    endpoints: [ {{ protocol: openai-chat, base_url: \"https://api.openai.com\" }} ]\n{extra}aliases: {{}}\n")
}

#[test]
fn tc12_a_hosted_node_with_slots_or_warm_capacity_warns_once_and_never_errors() {
    let slots = run(&hosted("    slots: 4\n"));
    assert!(slots.errors.is_empty());
    assert_eq!(slots.warnings.iter().map(|w| w.to_string()).collect::<Vec<_>>(), vec!["warning hosted_ignores_slots at nodes.h.slots: A hosted node ignores slots and warm capacity."]);
    let warm = run(&hosted("    warm_capacity: 4\n"));
    assert_eq!(warm.warnings.len(), 1);
    assert_eq!(warm.warnings[0].path, "nodes.h.warm_capacity");
    let both = run(&hosted("    slots: 4\n    warm_capacity: 6\n"));
    assert_eq!(both.warnings.len(), 1, "once per node");
    assert!(run(&hosted("")).warnings.is_empty());
    assert!(run(&hosted("    warm_capacity: auto\n")).warnings.is_empty());
}

#[test]
fn tc12_anthropic_hosted_is_hosted_too_and_a_local_node_with_slots_is_not_warned() {
    let text = hosted("    slots: 2\n").replace("openai-hosted", "anthropic-hosted").replace("openai-chat", "anthropic-messages");
    assert_eq!(run(&text).warnings.len(), 1);
    assert!(warnings(&registry(&node("llama-server", "    slots: 4\n"), "")).is_empty());
}

#[test]
fn tc22_speculative_decoding_with_prefix_caching_warns_flag_mismatch_and_either_alone_does_not() {
    let both = registry(&node("llama-server", "    engine_flags: { prefix_caching: true, speculative_decoding: true }\n"), "");
    let report = run(&both);
    assert!(report.errors.is_empty());
    assert_eq!(report.warnings.iter().map(|w| w.to_string()).collect::<Vec<_>>(), vec!["warning flag_mismatch at nodes.n1.engine_flags.speculative_decoding: Speculative decoding with prefix caching is untested."]);
    for one in ["prefix_caching: true", "speculative_decoding: true", "prefix_caching: true, speculative_decoding: false", "prefix_caching: true, speculative_decoding: unknown"] {
        assert!(warnings(&registry(&node("llama-server", &format!("    engine_flags: {{ {one} }}\n")), "")).is_empty(), "{one}");
    }
}

#[test]
fn tc19_three_warnings_and_no_error_return_the_record_and_all_three_warnings() {
    let text = "version: 1\nnodes:\n  a:\n    engine: { name: mlx_lm }\n    model: m\n    endpoints: [ { protocol: openai-responses, base_url: \"http://a\" } ]\n    responses: true\n  b:\n    engine: { name: vllm }\n    model: m\n    endpoints: [ { protocol: openai-chat, base_url: \"http://b\" } ]\n  c:\n    engine: { name: openai-hosted }\n    model: m\n    endpoints: [ { protocol: openai-chat, base_url: \"https://api.openai.com\" } ]\n    slots: 2\naliases: {}\n";
    let report = run(text);
    assert!(report.errors.is_empty());
    let codes: Vec<WarningCode> = report.warnings.iter().map(|w| w.code).collect();
    assert_eq!(codes, vec![WarningCode::ResponsesEngineMismatch, WarningCode::VllmNoPromptTokensDetails, WarningCode::HostedIgnoresSlots]);
}

#[test]
fn a_node_with_an_error_raises_no_warning_because_its_record_is_not_built() {
    let text = registry(&node("mlx_lm", "    responses: true\n    slots: 0\n"), "");
    let report = run(&text);
    assert_eq!(report.errors.len(), 1);
    assert!(report.warnings.is_empty());
}
