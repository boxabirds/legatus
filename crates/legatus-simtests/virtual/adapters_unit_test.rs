//! Story 183 unit tests: the mlx_lm, vLLM, SGLang and gufo adapters, the reuse gate, the floor, the
//! advisories, the forbidden patch keys, the Responses gap table and the node view.
use legatus_common::engine::*;
use legatus_common::protocol::Protocol;
use legatus_proxy::config::node::{node_config_views, CacheKind};
use legatus_proxy::config::typed::Registry;
use legatus_proxy::engine::advisory::*;
use legatus_proxy::engine::gufo::GufoAdapter;
use legatus_proxy::engine::mlx_lm::MlxLmAdapter;
use legatus_proxy::engine::responses_gaps::responses_gaps;
use legatus_proxy::engine::sglang::SglangAdapter;
use legatus_proxy::engine::vllm::{VllmAdapter, VLLM_REUSE_MEASURE_FLOOR_TOKENS};
use legatus_proxy::engine::{standard_adapters, EngineAdapter, PlaceRelease};

const CANARY_PROMPT: &str = "CANARY-PROMPT-7f3a";
const DETAILS: ReuseFieldName = ReuseFieldName::PromptTokensDetailsCachedTokens;

fn view(json: &str) -> UsageView<'_> {
    UsageView { json_tail: json.as_bytes(), protocol: Protocol::OpenAiChat, stream: false }
}

#[test]
fn tc01_mlx_lm_facts_and_overrides() {
    let a = MlxLmAdapter;
    assert_eq!((a.family(), a.cap_source(), a.load_signal(), a.overflow_behaviour()), (EngineFamily::MlxLm, CapSource::DeclaredSlots { default: 1 }, LoadSignal::OwnInFlightOnly, OverflowBehaviour::Unbounded));
    assert_eq!((a.place_release(), a.drop_is_node_signal()), (PlaceRelease::AtNodeEnd, false));
    let tail = view("{\"usage\":{\"prompt_tokens_details\":{\"cached_tokens\":12}}}");
    assert_eq!(a.reuse_fields(&tail, ReuseProbeState::Unprobed), ReuseReading::Reused { cached_tokens: 12, field: DETAILS });
    assert!(a.allowed_flags().is_empty());
}

fn node(engine: &str, extra: &str) -> legatus_proxy::config::node::NodeSpec {
    let text = format!("version: 1\nnodes:\n  n1:\n    engine: {{ name: {engine} }}\n    model: m\n    endpoints: [ {{ protocol: openai-chat, base_url: \"http://node.invalid\" }} ]\n{extra}aliases:\n  a: {{ nodes: [n1] }}\n");
    Registry::from_text(&text).expect("valid registry").nodes[0].clone()
}

#[test]
fn tc02_mlx_lm_cap_is_one_without_a_declaration_and_the_declared_count_otherwise() {
    // The cap comes from the cap source: the default is 1 and a declaration is the registry's slots.
    let a = MlxLmAdapter;
    assert_eq!(a.cap_source(), CapSource::DeclaredSlots { default: 1 });
    assert_eq!(node("mlx_lm", "").slots, None);
    assert_eq!(node("mlx_lm", "    slots: 8\n").slots, Some(legatus_proxy::config::node::Slots::Count(8)));
}

#[test]
fn tc05_every_mlx_lm_node_has_the_memory_advisory_and_no_other_engine_does() {
    let registry = standard_adapters();
    for family in [EngineFamily::MlxLm] {
        assert_eq!(registry.for_family(family).advisories(), [AdvisoryCode::MlxMemoryGrowth]);
    }
    for family in [EngineFamily::LlamaServer, EngineFamily::Ollama, EngineFamily::Sglang, EngineFamily::Gufo, EngineFamily::Unknown] {
        assert!(!registry.for_family(family).advisories().contains(&AdvisoryCode::MlxMemoryGrowth), "{family:?}");
    }
    let facts = AdvisoryFacts { cache_kind: CacheKind::Dense, responses: false, declared_prompt_tokens_details: None };
    let got = node_advisories(&MlxLmAdapter, &facts, ReuseProbeState::Unprobed);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].code, AdvisoryCode::MlxMemoryGrowth);
}

#[test]
fn tc09_the_vllm_reuse_state_follows_the_declaration_and_a_probe_that_ran_beats_it() {
    let field = ReuseProbeState::Probed { field: Some(DETAILS) };
    let none = ReuseProbeState::Probed { field: None };
    assert_eq!(reuse_state_for_node(Some(true), ReuseProbeState::Unprobed), field);
    assert_eq!(reuse_state_for_node(Some(false), ReuseProbeState::Unprobed), ReuseProbeState::Unprobed);
    assert_eq!(reuse_state_for_node(None, ReuseProbeState::Unprobed), ReuseProbeState::Unprobed);
    assert_eq!(reuse_state_for_node(None, field), field, "a probe that found the field, no declaration");
    assert_eq!(reuse_state_for_node(Some(true), none), none, "a probe that found nothing beats the declaration");
}

#[test]
fn tc09_vllm_reads_only_a_field_that_is_declared_or_found() {
    let a = VllmAdapter;
    let with_field = view("{\"usage\":{\"prompt_tokens_details\":{\"cached_tokens\":300}}}");
    assert_eq!(a.reuse_fields(&with_field, ReuseProbeState::Unprobed), ReuseReading::Unknown(UnknownReason::NotYetProbed));
    assert_eq!(a.reuse_fields(&with_field, ReuseProbeState::Probed { field: None }), ReuseReading::Unknown(UnknownReason::NoneFound));
    assert_eq!(a.reuse_fields(&with_field, ReuseProbeState::Probed { field: Some(DETAILS) }), ReuseReading::Reused { cached_tokens: 300, field: DETAILS });
    let zero = view("{\"usage\":{\"prompt_tokens_details\":{\"cached_tokens\":0}}}");
    assert_eq!(a.reuse_fields(&zero, ReuseProbeState::Probed { field: Some(DETAILS) }), ReuseReading::Reused { cached_tokens: 0, field: DETAILS }, "a silent zero is a reading");
    let absent = view("{\"usage\":{\"prompt_tokens\":9}}");
    assert_eq!(a.reuse_fields(&absent, ReuseProbeState::Probed { field: Some(DETAILS) }), ReuseReading::Unknown(UnknownReason::FieldAbsent));
}

#[test]
fn tc12_prompts_below_528_tokens_are_left_out_of_the_vllm_measure() {
    assert_eq!(VLLM_REUSE_MEASURE_FLOOR_TOKENS, 528);
    let a = VllmAdapter;
    assert_eq!(a.reuse_measure_floor_tokens(), 528);
    for (tokens, counts) in [(479u32, false), (527, false), (528, true), (4096, true), (0, false)] {
        assert_eq!(counts_for_reuse_measure(&a, tokens), counts, "{tokens}");
    }
    assert!(counts_for_reuse_measure(&GufoAdapter, 0), "an adapter without a floor counts every prompt");
    assert!(a.reuse_noisy_per_request());
}

#[test]
fn tc13_the_hybrid_advisory_is_active_for_hybrid_without_a_reuse_field_only() {
    let facts = |kind| AdvisoryFacts { cache_kind: kind, responses: false, declared_prompt_tokens_details: None };
    let code = AdvisoryCode::VllmHybridNoReuseSignal;
    let field = ReuseProbeState::Probed { field: Some(DETAILS) };
    assert!(advisory_active(code, &facts(CacheKind::Hybrid), ReuseProbeState::Unprobed).is_some());
    assert!(advisory_active(code, &facts(CacheKind::Hybrid), ReuseProbeState::Probed { field: None }).is_some());
    assert!(advisory_active(code, &facts(CacheKind::Hybrid), field).is_none(), "hybrid with the declaration");
    assert!(advisory_active(code, &facts(CacheKind::Dense), ReuseProbeState::Unprobed).is_none());
    assert!(advisory_active(code, &facts(CacheKind::Unknown), ReuseProbeState::Unprobed).is_none());
    assert_eq!(VllmAdapter.advisories(), [code]);
}

#[test]
fn tc14_vllm_cap_is_the_declared_slots_or_one_and_overflow_is_unknown() {
    let a = VllmAdapter;
    assert_eq!((a.cap_source(), a.load_signal(), a.overflow_behaviour()), (CapSource::DeclaredSlots { default: 1 }, LoadSignal::OwnInFlightOnly, OverflowBehaviour::Unknown));
    assert_eq!(node("vllm", "    slots: 64\n").slots, Some(legatus_proxy::config::node::Slots::Count(64)));
}

#[test]
fn tc15_sglang_and_gufo_overrides() {
    let s = SglangAdapter;
    assert_eq!(s.reuse_fields(&view("{\"usage\":{\"prompt_tokens_details\":{\"cached_tokens\":256}}}"), ReuseProbeState::Unprobed), ReuseReading::Reused { cached_tokens: 256, field: DETAILS });
    assert!(s.reuse_noisy_per_request());
    assert_eq!(s.forbidden_added_fields(), ["session_id"]);
    let g = GufoAdapter;
    assert_eq!(g.reuse_fields(&view("{\"timings\":{\"cache_n\":40}}"), ReuseProbeState::Unprobed), ReuseReading::Reused { cached_tokens: 40, field: ReuseFieldName::TimingsCacheN });
    assert!(!g.reuse_noisy_per_request());
    assert_eq!((g.cap_source(), g.load_signal()), (CapSource::DeclaredSlots { default: 1 }, LoadSignal::OwnInFlightOnly));
    assert!(g.forbidden_added_fields().is_empty());
}

#[test]
fn tc17_a_patch_that_sets_session_id_on_an_sglang_node_is_listed_and_other_engines_and_keys_are_not() {
    assert_eq!(check_patch_keys(&SglangAdapter, &["temperature", "session_id", "top_p"]), vec!["session_id"]);
    assert!(check_patch_keys(&SglangAdapter, &["messages", "model"]).is_empty());
    assert!(check_patch_keys(standard_adapters().for_family(EngineFamily::LlamaServer), &["session_id"]).is_empty());
}

#[test]
fn tc20_every_listed_engine_has_labelled_sourced_gaps_and_an_unlisted_one_gets_the_unverified_note() {
    for name in ["llama-server", "ollama", "vllm", "sglang", "mlx_lm", "gufo"] {
        let gaps = responses_gaps(name);
        assert!(!gaps.is_empty(), "{name}");
        assert!(gaps.iter().all(|g| !g.text.is_empty() && !g.source.is_empty() && g.label != GapLabel::Unverified), "{name}");
    }
    assert!(responses_gaps("mlx_lm")[0].text.contains("404"));
    for name in ["lmstudio", "openai-compatible", "something-new", ""] {
        let gaps = responses_gaps(name);
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].label, GapLabel::Unverified, "{name}");
    }
}

#[test]
fn tc21_the_node_view_lists_gaps_only_for_nodes_that_serve_responses_and_holds_no_secret() {
    let text = "version: 1\nnodes:\n  a:\n    engine: { name: llama-server }\n    model: m\n    endpoints: [ { protocol: openai-responses, base_url: \"http://127.0.0.1:1\" } ]\n    responses: true\n  b:\n    engine: { name: vllm }\n    model: m\n    endpoints: [ { protocol: openai-responses, base_url: \"http://127.0.0.1:2\" } ]\n    responses: true\n  c:\n    engine: { name: sglang }\n    model: m\n    endpoints: [ { protocol: openai-responses, base_url: \"http://127.0.0.1:3\" } ]\n    responses: true\n  d:\n    engine: { name: mlx_lm }\n    model: m\n    endpoints: [ { protocol: openai-chat, base_url: \"http://127.0.0.1:4\" } ]\n    cache: { kind: hybrid }\naliases:\n  x: { nodes: [a, b, c, d] }\n";
    let registry = Registry::from_text(text).expect("valid registry");
    let views = node_config_views(&registry.nodes, &registry.warnings);
    let by = |n: &str| views.iter().find(|v| v.name.0 == n).unwrap();
    for name in ["a", "b", "c"] {
        assert!(!by(name).responses_gaps.is_empty(), "{name}");
    }
    assert!(by("d").responses_gaps.is_empty(), "the flag is false");
    assert_eq!(by("d").advisories.iter().map(|a| a.code).collect::<Vec<_>>(), vec![AdvisoryCode::MlxMemoryGrowth]);
    assert!(by("a").advisories.is_empty());
    let shown = format!("{views:?}");
    assert!(!shown.contains("127.0.0.1"), "no address in the view");
}

#[test]
fn tc22_no_debug_form_holds_prompt_text() {
    let facts = AdvisoryFacts { cache_kind: CacheKind::Hybrid, responses: true, declared_prompt_tokens_details: None };
    let adv = advisory_active(AdvisoryCode::VllmHybridNoReuseSignal, &facts, ReuseProbeState::Unprobed);
    let reading = VllmAdapter.reuse_fields(&view(&format!("{{\"content\":\"{CANARY_PROMPT}\",\"usage\":{{\"prompt_tokens_details\":{{\"cached_tokens\":1}}}}}}")), ReuseProbeState::Probed { field: Some(DETAILS) });
    let shown = format!("{adv:?} {:?} {reading:?}", responses_gaps("vllm"));
    assert!(!shown.contains("CANARY"), "{shown}");
}
