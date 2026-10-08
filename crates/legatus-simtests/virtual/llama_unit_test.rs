//! Story 151 unit tests: properties, slot count, slot context, metrics, reuse, idle policy and
//! error classification of the llama-server adapter.
use legatus_common::engine::*;
use legatus_common::protocol::Protocol;
use legatus_proxy::config::node::Slots;
use legatus_proxy::engine::llama_server::*;
use legatus_proxy::engine::{AdapterRegistry, EngineAdapter};
use std::time::Duration;

const CANARY_PROMPT: &str = "CANARY-PROMPT-7f3a";

#[test]
fn tc01_props_reads_one_two_and_four_slots_and_refuses_a_missing_string_zero_or_negative_count() {
    for n in [1u32, 2, 4] {
        assert_eq!(parse_props(format!("{{\"total_slots\":{n},\"model_alias\":\"x\"}}").as_bytes()), Ok(n));
    }
    assert_eq!(parse_props(b"{\"model_alias\":\"x\"}"), Err(PropsError::MissingField));
    assert_eq!(parse_props(b"not json"), Err(PropsError::MissingField));
    assert_eq!(parse_props(b"{\"total_slots\":\"4\"}"), Err(PropsError::NotAnInteger));
    assert_eq!(parse_props(b"{\"total_slots\":-1}"), Err(PropsError::NotAnInteger));
    assert_eq!(parse_props(b"{\"total_slots\":2.5}"), Err(PropsError::NotAnInteger));
    assert_eq!(parse_props(b"{\"total_slots\":0}"), Err(PropsError::Zero));
}

const REAL_PROPS_CAPTURE: &str = include_str!("../../../specs/proxy/evidence/captures/llama-props-20261008T005552780Z.json");

#[test]
fn tc01_the_real_props_capture_gives_two_slots() {
    let capture: serde_json::Value = serde_json::from_str(REAL_PROPS_CAPTURE).unwrap();
    let body = serde_json::to_vec(&capture["requests"][0]["body"]).unwrap();
    assert_eq!(parse_props(&body), Ok(2), "the redacted capture of /props from llama-server 0.5.0 run with -np 2");
}

#[test]
fn tc02_the_smaller_of_registry_and_engine_wins_and_a_registry_count_never_raises_the_cap() {
    let e = |slots, source| EffectiveSlots { slots, source };
    assert_eq!(effective_slots(Slots::Auto, Some(4)), e(4, SlotsSource::Props));
    assert_eq!(effective_slots(Slots::Count(4), Some(2)), e(2, SlotsSource::Props));
    assert_eq!(effective_slots(Slots::Count(2), Some(4)), e(2, SlotsSource::Registry));
    assert_eq!(effective_slots(Slots::Count(3), Some(3)), e(3, SlotsSource::Registry));
    assert_eq!(effective_slots(Slots::Auto, None), e(LLAMA_SERVER_DEFAULT_SLOTS, SlotsSource::DefaultUnverified));
    assert_eq!(effective_slots(Slots::Count(3), None), e(3, SlotsSource::Registry));
    assert_eq!(LLAMA_SERVER_DEFAULT_SLOTS, 4);
}

fn context(per_slot: Option<u32>, total: Option<u32>, np: Option<u32>, unified: Option<bool>) -> Option<u32> {
    slot_context(per_slot, total, np, unified).unwrap().tokens
}

#[test]
fn tc04_slot_context_divides_the_total_by_the_parallel_count_unless_unified() {
    assert_eq!(context(None, Some(16384), Some(4), None), Some(4096));
    assert_eq!(context(None, Some(16384), Some(4), Some(false)), Some(4096));
    assert_eq!(context(None, Some(16385), Some(4), None), Some(4096), "rounded down");
    assert_eq!(context(None, Some(16384), Some(1), None), Some(16384));
    assert_eq!(context(None, Some(16384), Some(4), Some(true)), Some(16384), "unified");
    assert_eq!(context(None, Some(16384), None, None), Some(16384), "no -np");
    assert_eq!(context(Some(2048), Some(16384), Some(4), None), Some(2048), "declared wins");
    assert_eq!(context(None, None, Some(4), None), None);
    assert_eq!(context(None, None, None, None), None);
    assert_eq!(slot_context(None, Some(16384), Some(0), None), Err(SlotContextError::ZeroParallel));
    assert_eq!(slot_context(None, Some(4096), Some(2), None).unwrap().source, Some(SlotContextSource::TotalOverParallel));
}

#[test]
fn tc06_metrics_are_read_with_and_without_the_prefix_and_comments_are_ignored() {
    let with = "# HELP llamacpp:requests_processing Number of requests processing\n# TYPE llamacpp:requests_processing gauge\nllamacpp:requests_processing 3\n# HELP llamacpp:requests_deferred x\nllamacpp:requests_deferred 2\nllamacpp:n_tokens_max 419\n";
    assert_eq!(parse_metrics(with), Ok(MetricsReading { processing: 3, deferred: 2 }));
    assert_eq!(parse_metrics("requests_processing 0\nrequests_deferred 0\n"), Ok(MetricsReading { processing: 0, deferred: 0 }));
    assert_eq!(parse_metrics("llamacpp:requests_processing 1.0\nllamacpp:requests_deferred 0\n"), Ok(MetricsReading { processing: 1, deferred: 0 }));
    assert_eq!(parse_metrics("llamacpp:requests_processing 3\n"), Err(MetricsError::MissingCounter));
    assert_eq!(parse_metrics(""), Err(MetricsError::MissingCounter));
    assert_eq!(parse_metrics("llamacpp:requests_processing many\nllamacpp:requests_deferred 0\n"), Err(MetricsError::NotANumber));
    assert_eq!(parse_metrics("llamacpp:requests_processing -1\nllamacpp:requests_deferred 0\n"), Err(MetricsError::NotANumber));
}

fn view(json: &str) -> UsageView<'_> {
    UsageView { json_tail: json.as_bytes(), protocol: Protocol::OpenAiChat, stream: false }
}

#[test]
fn tc11_reuse_is_timings_cache_n_and_the_stored_length_is_ignored() {
    let a = LlamaServerAdapter;
    let probe = ReuseProbeState::Unprobed;
    let reused = |n| ReuseReading::Reused { cached_tokens: n, field: ReuseFieldName::TimingsCacheN };
    assert_eq!(a.reuse_fields(&view("{\"tokens_cached\":6300,\"timings\":{\"cache_n\":5484,\"prompt_n\":516}}"), probe), reused(5484));
    assert_eq!(a.reuse_fields(&view("{\"tokens_cached\":6300}"), probe), ReuseReading::Unknown(UnknownReason::FieldAbsent));
    assert_eq!(a.reuse_fields(&view("{\"timings\":{\"cache_n\":0}}"), probe), reused(0));
    assert_eq!(a.reuse_fields(&view("{\"timings\":{\"cache_n\":\"5\"}}"), probe), ReuseReading::Unknown(UnknownReason::FieldMalformed));
}

#[test]
fn tc13_the_adapter_facts_and_it_ignores_the_probe_state() {
    let a = LlamaServerAdapter;
    assert_eq!((a.family(), a.cap_source(), a.load_signal(), a.overflow_behaviour()), (EngineFamily::LlamaServer, CapSource::PropsTotalSlots, LoadSignal::MetricsEndpoint, OverflowBehaviour::Error400));
    assert!(a.allowed_flags().iter().any(|f| f.flag == EngineFlag::Np) && a.allowed_flags().iter().any(|f| f.flag == EngineFlag::KvUnified));
    let tail = view("{\"timings\":{\"cache_n\":7}}");
    let expected = a.reuse_fields(&tail, ReuseProbeState::Unprobed);
    assert_eq!(a.reuse_fields(&tail, ReuseProbeState::Probed { field: None }), expected);
    assert_eq!(a.reuse_fields(&tail, ReuseProbeState::Probed { field: Some(ReuseFieldName::InputTokensDetailsCachedTokens) }), expected);
    // The trait defaults of the shell are kept.
    assert!(a.drop_is_node_signal() && a.advisories().is_empty() && a.forbidden_added_fields().is_empty());
}

#[test]
fn the_standard_registry_serves_llama_server_with_this_adapter_and_others_with_the_unknown_one() {
    let registry = AdapterRegistry::standard();
    assert_eq!(registry.for_family(EngineFamily::LlamaServer).cap_source(), CapSource::PropsTotalSlots);
    assert_eq!(registry.for_family(EngineFamily::Ollama).cap_source(), CapSource::FixedOne);
}

#[test]
fn tc14_a_connection_idle_at_most_four_seconds_is_reused_and_a_longer_one_is_not() {
    let policy = IdlePolicy::default();
    assert_eq!(policy.max_idle, Duration::from_secs(4));
    assert!(policy.reusable(Duration::from_millis(3999)));
    assert!(policy.reusable(Duration::from_millis(4000)));
    assert!(!policy.reusable(Duration::from_millis(4001)));
    assert!(!IdlePolicy { max_idle: Duration::ZERO }.reusable(Duration::from_millis(1)));
    assert!(IdlePolicy { max_idle: Duration::ZERO }.reusable(Duration::ZERO));
}

#[test]
fn tc18_the_context_error_and_a_json_500_are_request_faults_and_everything_else_is_not() {
    assert_eq!(classify_error(400, Some("exceed_context_size_error")), ErrorClass::RequestFault);
    assert_eq!(classify_error(500, Some("server_error")), ErrorClass::RequestFault);
    assert_eq!(classify_error(500, None), ErrorClass::NotClassified);
    assert_eq!(classify_error(400, Some("invalid_request_error")), ErrorClass::NotClassified);
    assert_eq!(classify_error(400, None), ErrorClass::NotClassified);
    for status in [401, 404, 429, 502, 503, 504] {
        assert_eq!(classify_error(status, Some("exceed_context_size_error")), ErrorClass::NotClassified, "{status}");
    }
    let body = b"{\"error\":{\"code\":400,\"type\":\"exceed_context_size_error\",\"message\":\"x\"}}";
    assert_eq!(error_type_of(body).as_deref(), Some("exceed_context_size_error"));
    assert_eq!(error_type_of(b"{\"error\":\"plain text\"}"), None);
    assert_eq!(error_type_of(b"nope"), None);
}

#[test]
fn tc19_no_debug_form_holds_text_from_an_error_body_or_a_prompt() {
    let body = format!("{{\"error\":{{\"type\":\"server_error\",\"message\":\"{CANARY_PROMPT}\"}}}}");
    let code = error_type_of(body.as_bytes());
    let shown = format!("{:?} {:?} {:?} {:?}", classify_error(500, code.as_deref()), LoadReading::OwnCountOnly, LoadPoller::new("http://node.invalid").interval, code);
    assert!(!shown.contains("CANARY"), "{shown}");
    let reading = LlamaServerAdapter.reuse_fields(&view(&format!("{{\"content\":\"{CANARY_PROMPT}\",\"timings\":{{\"cache_n\":1}}}}")), ReuseProbeState::Unprobed);
    assert!(!format!("{reading:?}").contains("CANARY"));
}
