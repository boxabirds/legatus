//! Story 127 unit tests: the adapter registry, version status, the unknown adapter, the reuse
//! reader and the node view. Pure decisions over small values, no engine process.
use legatus_common::engine::*;
use legatus_common::protocol::Protocol;
use legatus_proxy::config::typed::Registry;
use legatus_proxy::engine::reuse::extract_reuse;
use legatus_proxy::engine::unknown::UnknownAdapter;
use legatus_proxy::engine::version::{detect_version_change, version_status};
use legatus_proxy::engine::{family_of, AdapterRegistry, EngineAdapter, HealthProbe, PlaceRelease};
use std::sync::Arc;

const CANARY_PROMPT: &str = "CANARY-PROMPT-7f3a";
const LARGEST: u64 = u64::MAX;

fn view(json: &str) -> UsageView<'_> {
    UsageView { json_tail: json.as_bytes(), protocol: Protocol::OpenAiChat, stream: false }
}

fn registry_text(nodes: &[(&str, &str, Option<&str>)]) -> String {
    let mut text = String::from("version: 1\nnodes:\n");
    for (name, engine, version) in nodes {
        let version = version.map(|v| format!(", version: \"{v}\"")).unwrap_or_default();
        text += &format!("  {name}:\n    engine: {{ name: {engine}{version} }}\n    model: m\n    endpoints: [ {{ protocol: openai-chat, base_url: \"http://node.invalid\" }} ]\n");
    }
    text += "aliases:\n  a: { nodes: [";
    text += &nodes.iter().map(|n| n.0).collect::<Vec<_>>().join(", ");
    text += "] }\n";
    text
}

fn node_of(engine: &str) -> legatus_proxy::config::node::NodeSpec {
    let registry = Registry::from_text(&registry_text(&[("n1", engine, None)])).expect("valid registry");
    registry.nodes[0].clone()
}

struct Marked(EngineFamily);
impl EngineAdapter for Marked {
    fn family(&self) -> EngineFamily {
        self.0
    }
    fn cap_source(&self) -> CapSource {
        CapSource::PropsTotalSlots
    }
    fn load_signal(&self) -> LoadSignal {
        LoadSignal::SlotsEndpoint
    }
    fn reuse_fields(&self, _: &UsageView<'_>, _: ReuseProbeState) -> ReuseReading {
        ReuseReading::Unknown(UnknownReason::FieldAbsent)
    }
    fn overflow_behaviour(&self) -> OverflowBehaviour {
        OverflowBehaviour::Error400
    }
    fn allowed_flags(&self) -> &'static [FlagSpec] {
        &[]
    }
}

#[test]
fn tc01_family_of_maps_the_six_names_and_everything_else_to_unknown() {
    for (name, family) in [
        ("llama-server", EngineFamily::LlamaServer),
        ("ollama", EngineFamily::Ollama),
        ("mlx_lm", EngineFamily::MlxLm),
        ("vllm", EngineFamily::Vllm),
        ("sglang", EngineFamily::Sglang),
        ("gufo", EngineFamily::Gufo),
    ] {
        assert_eq!(family_of(name), family, "{name}");
    }
    for name in ["lmstudio", "openai-compatible", "openai-hosted", "anthropic-hosted", "", "Ollama", "LLAMA-SERVER", "something-new"] {
        assert_eq!(family_of(name), EngineFamily::Unknown, "{name:?}");
    }
}

#[test]
fn tc02_with_nothing_registered_every_node_gets_the_unknown_adapter() {
    let registry = AdapterRegistry::new();
    for engine in ["llama-server", "ollama", "lmstudio", "openai-hosted"] {
        assert_eq!(registry.for_node(&node_of(engine)).family(), EngineFamily::Unknown, "{engine}");
    }
}

#[test]
fn tc03_a_registered_adapter_serves_its_family_only_and_a_second_registration_replaces_it() {
    let mut registry = AdapterRegistry::new();
    registry.register(Arc::new(Marked(EngineFamily::Ollama)));
    assert_eq!(registry.for_node(&node_of("ollama")).cap_source(), CapSource::PropsTotalSlots);
    assert_eq!(registry.for_node(&node_of("llama-server")).cap_source(), CapSource::FixedOne);
    assert_eq!(registry.for_node(&node_of("lmstudio")).cap_source(), CapSource::FixedOne);
    struct Second;
    impl EngineAdapter for Second {
        fn family(&self) -> EngineFamily {
            EngineFamily::Ollama
        }
        fn cap_source(&self) -> CapSource {
            CapSource::DeclaredSlots { default: 4 }
        }
        fn load_signal(&self) -> LoadSignal {
            LoadSignal::OwnInFlightOnly
        }
        fn reuse_fields(&self, _: &UsageView<'_>, _: ReuseProbeState) -> ReuseReading {
            ReuseReading::Unknown(UnknownReason::FieldAbsent)
        }
        fn overflow_behaviour(&self) -> OverflowBehaviour {
            OverflowBehaviour::Unknown
        }
        fn allowed_flags(&self) -> &'static [FlagSpec] {
            &[]
        }
    }
    registry.register(Arc::new(Second));
    assert_eq!(registry.for_node(&node_of("ollama")).cap_source(), CapSource::DeclaredSlots { default: 4 });
}

#[test]
fn tc05_ollama_0_35_1_is_tested_and_0_40_0_is_untested_with_the_found_version() {
    assert_eq!(version_status(EngineFamily::Ollama, Some("0.35.1")), EngineVersionStatus::Tested);
    assert_eq!(version_status(EngineFamily::Ollama, Some("0.40.0")), EngineVersionStatus::Untested { found: "0.40.0".into() });
}

#[test]
fn tc06_llama_server_builds_are_tested_and_a_v_prefix_and_padding_are_ignored() {
    for tested in ["0.5.0", "b11146", "b11459", "v0.5.0", "  b11146  ", " v0.5.0 "] {
        assert_eq!(version_status(EngineFamily::LlamaServer, Some(tested)), EngineVersionStatus::Tested, "{tested:?}");
    }
    assert_eq!(version_status(EngineFamily::LlamaServer, Some("v0.6.0")), EngineVersionStatus::Untested { found: "v0.6.0".into() });
    assert_eq!(version_status(EngineFamily::MlxLm, Some("0.32.0")), EngineVersionStatus::Tested);
    assert_eq!(version_status(EngineFamily::Ollama, Some("0.5.0")), EngineVersionStatus::Untested { found: "0.5.0".into() }, "a version tested for another family is untested here");
}

#[test]
fn tc07_every_declared_version_of_the_families_without_a_list_is_untested() {
    for family in [EngineFamily::Vllm, EngineFamily::Sglang, EngineFamily::Gufo, EngineFamily::Unknown] {
        assert_eq!(version_status(family, Some("1.2.3")), EngineVersionStatus::Untested { found: "1.2.3".into() }, "{family:?}");
    }
}

#[test]
fn tc08_an_absent_version_is_unknown_for_every_family_and_so_is_an_empty_one() {
    for family in [EngineFamily::LlamaServer, EngineFamily::Ollama, EngineFamily::MlxLm, EngineFamily::Vllm, EngineFamily::Sglang, EngineFamily::Gufo, EngineFamily::Unknown] {
        assert_eq!(version_status(family, None), EngineVersionStatus::Unknown);
        assert_eq!(version_status(family, Some("")), EngineVersionStatus::Unknown);
        assert_eq!(version_status(family, Some("  ")), EngineVersionStatus::Unknown);
        assert_eq!(version_status(family, Some("v")), EngineVersionStatus::Unknown);
    }
}

#[test]
fn tc09_the_node_view_shows_name_version_and_status_and_an_untested_node_still_serves() {
    let text = registry_text(&[("o", "ollama", Some("0.35.1")), ("v", "vllm", Some("0.31.0")), ("x", "llama-server", None)]);
    let registry = Registry::from_text(&text).expect("valid registry");
    let views = legatus_proxy::config::node::node_config_views(&registry.nodes, &registry.warnings);
    let by = |n: &str| views.iter().find(|v| v.name.0 == n).unwrap();
    assert_eq!((by("o").engine, by("o").engine_version.as_deref(), by("o").engine_version_status.clone()), ("ollama", Some("0.35.1"), EngineVersionStatus::Tested));
    assert_eq!((by("v").engine, by("v").engine_version.as_deref(), by("v").engine_version_status.clone()), ("vllm", Some("0.31.0"), EngineVersionStatus::Untested { found: "0.31.0".into() }));
    assert_eq!((by("x").engine_version.as_deref(), by("x").engine_version_status.clone()), (None, EngineVersionStatus::Unknown));
    assert_eq!(registry.aliases.iter().count(), 1, "the registry still routes: the untested node is not refused");
    // After a reload that changes the version of the Ollama node, one calibration reason exists.
    let reloaded = Registry::from_text(&registry_text(&[("o", "ollama", Some("0.40.0")), ("v", "vllm", Some("0.31.0")), ("x", "llama-server", None)])).expect("valid registry");
    let reasons: Vec<CalibrationReason> = registry
        .nodes
        .iter()
        .zip(&reloaded.nodes)
        .filter_map(|(before, after)| detect_version_change(before.engine_version.as_deref(), after.engine_version.as_deref()))
        .collect();
    assert_eq!(reasons, vec![CalibrationReason::EngineVersionChanged { from: "0.35.1".into(), to: "0.40.0".into() }]);
    let shown = format!("{views:?}");
    assert!(!shown.contains("http://") && !shown.contains("key"), "{shown}");
}

#[test]
fn tc10_a_calibration_reason_exists_only_for_two_known_different_versions() {
    let changed = CalibrationReason::EngineVersionChanged { from: "0.35.1".into(), to: "0.40.0".into() };
    assert_eq!(detect_version_change(Some("0.35.1"), Some("0.40.0")), Some(changed));
    assert_eq!(detect_version_change(Some("0.35.1"), Some("0.35.1")), None);
    assert_eq!(detect_version_change(Some("v0.35.1"), Some(" 0.35.1 ")), None, "padding and v are not a change");
    assert_eq!(detect_version_change(None, Some("0.35.1")), None);
    assert_eq!(detect_version_change(Some("0.35.1"), None), None);
    assert_eq!(detect_version_change(None, None), None);
    let twice = [detect_version_change(Some("1"), Some("2")), detect_version_change(Some("2"), Some("3"))];
    assert_eq!(twice.iter().flatten().count(), 2);
}

#[test]
fn tc11_the_unknown_adapter_has_cap_one_own_in_flight_unknown_overflow_and_no_flags() {
    assert_eq!(UNKNOWN_ENGINE_CAP, 1);
    let a = UnknownAdapter;
    assert_eq!((a.family(), a.cap_source(), a.load_signal(), a.overflow_behaviour()), (EngineFamily::Unknown, CapSource::FixedOne, LoadSignal::OwnInFlightOnly, OverflowBehaviour::Unknown));
    assert!(a.allowed_flags().is_empty());
}

const BOTH_FIELDS: &str = "{\"timings\":{\"cache_n\":5484},\"usage\":{\"prompt_tokens_details\":{\"cached_tokens\":300}}}";

#[test]
fn tc12_unprobed_the_unknown_adapter_reads_unknown_even_when_fields_are_present() {
    assert_eq!(UnknownAdapter.reuse_fields(&view(BOTH_FIELDS), ReuseProbeState::Unprobed), ReuseReading::Unknown(UnknownReason::NotYetProbed));
}

#[test]
fn tc13_probed_with_a_field_the_unknown_adapter_reads_that_field_only() {
    let probe = ReuseProbeState::Probed { field: Some(ReuseFieldName::PromptTokensDetailsCachedTokens) };
    assert_eq!(UnknownAdapter.reuse_fields(&view(BOTH_FIELDS), probe), ReuseReading::Reused { cached_tokens: 300, field: ReuseFieldName::PromptTokensDetailsCachedTokens });
    let only_timings = "{\"timings\":{\"cache_n\":5484}}";
    assert_eq!(UnknownAdapter.reuse_fields(&view(only_timings), probe), ReuseReading::Unknown(UnknownReason::FieldAbsent));
}

#[test]
fn tc14_probed_with_none_the_unknown_adapter_reads_none_found_even_when_a_field_is_present() {
    assert_eq!(UnknownAdapter.reuse_fields(&view(BOTH_FIELDS), ReuseProbeState::Probed { field: None }), ReuseReading::Unknown(UnknownReason::NoneFound));
}

#[test]
fn tc15_the_three_fields_are_read_and_zero_is_a_reading() {
    let reused = |field, json: &str| extract_reuse(field, &view(json));
    assert_eq!(reused(ReuseFieldName::TimingsCacheN, BOTH_FIELDS), ReuseReading::Reused { cached_tokens: 5484, field: ReuseFieldName::TimingsCacheN });
    assert_eq!(reused(ReuseFieldName::PromptTokensDetailsCachedTokens, BOTH_FIELDS), ReuseReading::Reused { cached_tokens: 300, field: ReuseFieldName::PromptTokensDetailsCachedTokens });
    let input = "{\"usage\":{\"input_tokens_details\":{\"cached_tokens\":12}}}";
    assert_eq!(reused(ReuseFieldName::InputTokensDetailsCachedTokens, input), ReuseReading::Reused { cached_tokens: 12, field: ReuseFieldName::InputTokensDetailsCachedTokens });
    assert_eq!(reused(ReuseFieldName::TimingsCacheN, "{\"timings\":{\"cache_n\":0}}"), ReuseReading::Reused { cached_tokens: 0, field: ReuseFieldName::TimingsCacheN });
    let sse = format!("data: {BOTH_FIELDS}\n");
    assert_eq!(reused(ReuseFieldName::TimingsCacheN, &sse), ReuseReading::Reused { cached_tokens: 5484, field: ReuseFieldName::TimingsCacheN });
}

#[test]
fn tc16_the_stored_length_field_is_never_reuse() {
    let only = "{\"tokens_cached\":9000}";
    for field in [ReuseFieldName::TimingsCacheN, ReuseFieldName::PromptTokensDetailsCachedTokens, ReuseFieldName::InputTokensDetailsCachedTokens] {
        assert_eq!(extract_reuse(field, &view(only)), ReuseReading::Unknown(UnknownReason::FieldAbsent));
    }
    let both = "{\"tokens_cached\":9000,\"timings\":{\"cache_n\":5484}}";
    assert_eq!(extract_reuse(ReuseFieldName::TimingsCacheN, &view(both)), ReuseReading::Reused { cached_tokens: 5484, field: ReuseFieldName::TimingsCacheN });
}

#[test]
fn tc17_a_usage_without_a_cache_field_and_a_reply_without_usage_are_field_absent() {
    let absent = ReuseReading::Unknown(UnknownReason::FieldAbsent);
    assert_eq!(extract_reuse(ReuseFieldName::PromptTokensDetailsCachedTokens, &view("{\"usage\":{\"prompt_tokens\":5}}")), absent);
    assert_eq!(extract_reuse(ReuseFieldName::PromptTokensDetailsCachedTokens, &view("{\"choices\":[]}")), absent);
    assert_eq!(extract_reuse(ReuseFieldName::TimingsCacheN, &view("")), absent);
    assert_eq!(extract_reuse(ReuseFieldName::TimingsCacheN, &view("not json")), absent);
    assert_eq!(extract_reuse(ReuseFieldName::TimingsCacheN, &view("{\"timings\":\"x\"}")), absent);
}

#[test]
fn tc18_a_field_that_is_not_an_unsigned_whole_number_is_malformed_and_the_largest_u64_is_reused() {
    let malformed = ReuseReading::Unknown(UnknownReason::FieldMalformed);
    for bad in ["\"5\"", "-5", "5.5", "null", "true", "[]", "{}"] {
        let json = format!("{{\"timings\":{{\"cache_n\":{bad}}}}}");
        assert_eq!(extract_reuse(ReuseFieldName::TimingsCacheN, &view(&json)), malformed, "{bad}");
    }
    let json = format!("{{\"timings\":{{\"cache_n\":{LARGEST}}}}}");
    assert_eq!(extract_reuse(ReuseFieldName::TimingsCacheN, &view(&json)), ReuseReading::Reused { cached_tokens: LARGEST, field: ReuseFieldName::TimingsCacheN });
}

#[test]
fn tc19_the_reuse_field_names_are_exactly_three_and_none_is_the_stored_length() {
    // An exhaustive match: a new variant breaks the build until this test is read again.
    let all = [ReuseFieldName::TimingsCacheN, ReuseFieldName::PromptTokensDetailsCachedTokens, ReuseFieldName::InputTokensDetailsCachedTokens];
    for field in all {
        let _ = match field {
            ReuseFieldName::TimingsCacheN | ReuseFieldName::PromptTokensDetailsCachedTokens | ReuseFieldName::InputTokensDetailsCachedTokens => (),
        };
        assert!(!field.wire_name().contains("tokens_cached"), "{field:?}");
    }
    assert_eq!(all.iter().map(|f| f.wire_name()).collect::<Vec<_>>(), ["timings.cache_n", "usage.prompt_tokens_details.cached_tokens", "usage.input_tokens_details.cached_tokens"]);
}

#[test]
fn tc21_no_debug_output_holds_text_from_a_reply_body() {
    let body = format!("{{\"choices\":[{{\"message\":{{\"content\":\"{CANARY_PROMPT}\"}}}}],\"timings\":{{\"cache_n\":7}}}}");
    let v = view(&body);
    let reading = extract_reuse(ReuseFieldName::TimingsCacheN, &v);
    let shown = format!("{v:?} {reading:?} {:?}", UnknownAdapter.reuse_fields(&v, ReuseProbeState::Unprobed));
    assert!(!shown.contains("CANARY"), "{shown}");
    assert!(shown.contains("bytes"));
    let malformed = extract_reuse(ReuseFieldName::TimingsCacheN, &view(&format!("{{\"timings\":{{\"cache_n\":\"{CANARY_PROMPT}\"}}}}")));
    assert!(!format!("{malformed:?}").contains("CANARY"));
}

#[test]
fn tc22_the_unknown_adapter_keeps_the_seven_defaults() {
    let a = UnknownAdapter;
    assert_eq!(a.place_release(), PlaceRelease::AtClientEnd);
    let probe: HealthProbe = a.health_probe();
    assert_eq!(probe.path, "/v1/models");
    assert!((probe.parse)(200, b"{}").ok);
    assert!(!(probe.parse)(503, b"").ok);
    assert!(a.advisories().is_empty());
    assert!(!a.reuse_noisy_per_request());
    assert_eq!(a.reuse_measure_floor_tokens(), 0);
    assert!(a.forbidden_added_fields().is_empty());
    assert!(a.drop_is_node_signal());
}
