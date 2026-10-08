//! Story 174 unit tests: the cap, the estimate, the limit and the decision, the refusal text, the
//! truncation record, the adapter facts and the measurements of the real Ollama.
use axum::body::Body;
use http_body_util::BodyExt;
use legatus_common::engine::*;
use legatus_common::ids::NodeId;
use legatus_common::protocol::Protocol;
use legatus_proxy::config::settings::CATALOGUE;
use legatus_proxy::engine::estimate::TokenEstimator;
use legatus_proxy::engine::guard::*;
use legatus_proxy::engine::ollama::*;
use legatus_proxy::engine::EngineAdapter;
use legatus_proxy::protocol::errors::{forbidden_word_in, refuse, RefusalDetail, RefusalKind, WordList};

const CANARY_PROMPT: &str = "CANARY-PROMPT-7f3a";
const REAL_REPLIES: &str = include_str!("../../../specs/proxy/evidence/captures/ollama-replies-20261008T010716636Z.json");
const REAL_ESTIMATOR: &str = include_str!("../../../specs/proxy/evidence/captures/ollama-estimator-20261008T012000000Z.txt");

fn node() -> NodeId {
    NodeId("o1".to_string())
}

#[test]
fn tc01_no_declaration_gives_one_declared_four_gives_four_and_declared_zero_counts_as_one() {
    assert_eq!(ollama_cap(None, None), OllamaCap { cap: 1, warning: None });
    assert_eq!(ollama_cap(Some(4), None), OllamaCap { cap: 4, warning: None });
    assert_eq!(ollama_cap(Some(0), None).cap, 1);
    assert_eq!(OLLAMA_DEFAULT_SLOTS, 1);
}

#[test]
fn tc02_a_lower_measurement_lowers_the_cap_with_a_warning_and_a_higher_one_never_raises_it() {
    assert_eq!(ollama_cap(Some(4), Some(1)), OllamaCap { cap: 1, warning: Some(SlotsAboveMeasured { declared: 4, measured: 1 }) });
    assert_eq!(ollama_cap(Some(2), Some(2)), OllamaCap { cap: 2, warning: None });
    assert_eq!(ollama_cap(Some(1), Some(4)), OllamaCap { cap: 1, warning: None });
    assert_eq!(ollama_cap(Some(4), None).warning, None);
    assert_eq!(ollama_cap(None, Some(0)).cap, 1, "a cap is never below one");
}

#[test]
fn tc03_the_view_of_four_declared_and_one_measured_shows_cap_one_and_the_warning() {
    let view = cap_view(Some(4), Some(1), 0);
    assert_eq!(view.cap.cap, 1);
    assert_eq!(view.cap.warning, Some(SlotsAboveMeasured { declared: 4, measured: 1 }));
}

#[test]
fn tc04_with_three_requests_in_flight_the_view_shows_load_three_from_the_proxy_count() {
    let view = cap_view(Some(4), None, 3);
    assert_eq!((view.load, view.load_source), (3, LoadSource::ProxyCount));
}

#[test]
fn tc05_the_estimate_is_the_rounded_up_quotient_and_does_not_overflow() {
    let e = TokenEstimator { bytes_per_token: 3 };
    assert_eq!(e.estimate(0), 0);
    assert_eq!(e.estimate(9), 3);
    assert_eq!(e.estimate(10), 4);
    assert_eq!(e.estimate(1), 1);
    assert_eq!(e.estimate(usize::MAX), u32::MAX);
    assert_eq!(TokenEstimator { bytes_per_token: 0 }.estimate(7), 7, "a zero divisor counts as one");
}

#[test]
fn tc06_the_limit_is_context_times_ratio_and_an_estimate_equal_to_it_passes() {
    assert_eq!(usable_limit(4096, 1.0), 4096);
    assert_eq!(usable_limit(4096, 0.5), 2048);
    assert_eq!(usable_limit(8192, 1.0), 8192, "the context of one slot, whatever the slot count");
    assert_eq!(usable_limit(4097, 0.5), 2048, "rounded down");
    assert_eq!(usable_limit(4096, 0.0), 0);
    assert_eq!(decide(4096, 4096), GuardDecision::Allow);
    assert_eq!(decide(4097, 4096), GuardDecision::Refuse { limit_tokens: 4096, estimate_tokens: 4097 });
    assert_eq!(decide(0, 4096), GuardDecision::Allow);
    assert_eq!(decide(1, 0), GuardDecision::Refuse { limit_tokens: 0, estimate_tokens: 1 });
}

#[tokio::test]
async fn tc08_the_refusal_states_the_limit_and_the_size_names_the_context_and_holds_no_forbidden_word() {
    for protocol in [Protocol::OpenAiChat, Protocol::AnthropicMessages] {
        let response = refuse(RefusalKind::ContextLengthExceeded, protocol, &RefusalDetail { max_tokens: Some(4096), used_tokens: Some(5200), retry_after_s: None });
        assert_eq!(response.status(), 400);
        let body: Body = response.into_body();
        let text = String::from_utf8(body.collect().await.unwrap().to_bytes().to_vec()).unwrap();
        assert!(text.contains("4096") && text.contains("5200") && text.to_lowercase().contains("context"), "{text}");
        assert_eq!(forbidden_word_in(WordList::Stop, &text), None, "{text}");
    }
}

#[test]
fn tc09_an_estimate_ten_percent_off_flips_the_decision_at_the_band_edge_and_every_false_pass_is_caught() {
    let limit = 4096u32;
    let mut false_passes = 0;
    for true_tokens in (limit + 1)..=(limit * 11 / 10) {
        let low = (f64::from(true_tokens) * 0.9) as u32;
        match decide(low, limit) {
            GuardDecision::Allow => {
                false_passes += 1;
                // The engine cuts the prompt to about half the context; the reply shows it.
                let seen = Some(limit / 2 + 2);
                assert!(detect_truncation(&node(), low, seen, 0.25).is_some(), "estimate {low} for {true_tokens} true tokens");
            }
            GuardDecision::Refuse { .. } => {}
        }
    }
    assert!(false_passes > 0, "the band does let some through, which is why the record exists");
    // An estimate ten percent too high refuses a prompt that fits, from true size limit/1.1 up.
    let fits = (f64::from(limit) / 1.1) as u32;
    assert_eq!(decide((f64::from(fits) * 1.1) as u32, limit), GuardDecision::Allow);
    assert!(matches!(decide((f64::from(fits + 50) * 1.1) as u32, limit), GuardDecision::Refuse { .. }));
}

fn measured_rows() -> Vec<(String, f64, f64)> {
    REAL_ESTIMATOR
        .lines()
        .filter(|l| !l.starts_with('#') && !l.starts_with("label"))
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            (f.len() == 6).then(|| (f[0].to_string(), f[3].parse().unwrap(), f[5].parse().unwrap()))
        })
        .collect()
}

#[test]
fn tc10_the_measured_bound_says_three_bytes_per_token_never_undercounts_and_over_counts_up_to_2_4_times() {
    let rows = measured_rows();
    assert_eq!(rows.len(), 5, "five text families were measured");
    let smallest = rows.iter().map(|r| r.1).fold(f64::MAX, f64::min);
    assert!(smallest >= 3.0, "the densest sample still has at least 3 bytes per token: {smallest}");
    for (label, _, over) in &rows {
        assert!(*over >= 1.0, "{label}: the estimate must not be below the engine count");
        assert!(*over <= 2.5, "{label}");
    }
    let worst = rows.iter().map(|r| r.2).fold(0.0, f64::max);
    assert!((2.3..=2.5).contains(&worst), "{worst}");
    let setting = CATALOGUE.iter().find(|d| d.name == "ollama_bytes_per_token").expect("the setting row");
    assert!(format!("{:?}", setting.default).contains('3'), "the default divisor stays 3, the value the measurement supports: {:?}", setting.default);
}

#[test]
fn tc11_a_cut_is_recorded_only_when_the_reply_count_is_more_than_a_quarter_below_the_estimate() {
    assert!(detect_truncation(&node(), 3000, Some(2050), 0.25).is_some());
    assert_eq!(detect_truncation(&node(), 3000, Some(2250), 0.25), None, "exactly 25 percent does not record");
    assert!(detect_truncation(&node(), 3000, Some(2249), 0.25).is_some());
    assert_eq!(detect_truncation(&node(), 3000, Some(3100), 0.25), None, "above the estimate");
    assert_eq!(detect_truncation(&node(), 3000, None, 0.25), None, "no count");
    assert_eq!(detect_truncation(&node(), 0, Some(0), 0.25), None, "estimate of zero");
    let record = detect_truncation(&node(), 3000, Some(2050), 0.25).unwrap();
    assert_eq!((record.prompt_tokens_sent, record.prompt_tokens_seen), (3000, 2050));
    assert!((record.ratio - 0.3167).abs() < 0.001, "{}", record.ratio);
}

#[test]
fn the_reply_count_is_found_in_a_tail_that_starts_mid_json() {
    let view = |s: &'static str| UsageView { json_tail: s.as_bytes(), protocol: Protocol::OpenAiChat, stream: false };
    assert_eq!(reported_prompt_tokens(&view("...text\"},\"usage\":{\"prompt_tokens\":2050,\"completion_tokens\":4}}")), Some(2050));
    assert_eq!(reported_prompt_tokens(&view("{\"usage\":{\"input_tokens\": 77,\"output_tokens\":4}}")), Some(77));
    assert_eq!(reported_prompt_tokens(&view("{\"usage\":{\"completion_tokens\":4}}")), None);
    assert_eq!(reported_prompt_tokens(&view("{\"usage\":{\"prompt_tokens\":\"many\"}}")), None);
    assert_eq!(reported_prompt_tokens(&view("")), None);
}

#[test]
fn tc16_the_adapter_facts_and_reuse_from_the_chat_reply_only() {
    let a = OllamaAdapter;
    assert_eq!((a.family(), a.cap_source(), a.load_signal(), a.overflow_behaviour()), (EngineFamily::Ollama, CapSource::DeclaredSlots { default: 1 }, LoadSignal::OwnInFlightOnly, OverflowBehaviour::SilentTruncate));
    let flags: Vec<EngineFlag> = a.allowed_flags().iter().map(|f| f.flag).collect();
    assert_eq!(flags, vec![EngineFlag::NumParallel, EngineFlag::KeepAliveS]);
    let chat = UsageView { json_tail: b"{\"usage\":{\"prompt_tokens_details\":{\"cached_tokens\":310}}}", protocol: Protocol::OpenAiChat, stream: false };
    assert_eq!(a.reuse_fields(&chat, ReuseProbeState::Unprobed), ReuseReading::Reused { cached_tokens: 310, field: ReuseFieldName::PromptTokensDetailsCachedTokens });
    let messages = UsageView { json_tail: chat.json_tail, protocol: Protocol::AnthropicMessages, stream: false };
    assert_eq!(a.reuse_fields(&messages, ReuseProbeState::Unprobed), ReuseReading::Unknown(UnknownReason::FieldAbsent), "the Messages shape of Ollama was not measured");
}

#[test]
fn tc17_no_debug_form_holds_prompt_text() {
    let record = detect_truncation(&node(), 3000, Some(2050), 0.25).unwrap();
    let shown = format!("{:?} {:?} {:?} {CANARY_PROMPT}", GuardDecision::Refuse { limit_tokens: 1, estimate_tokens: 2 }, record, cap_view(Some(2), Some(1), 1));
    assert!(shown.contains("CANARY"), "control: the marker is in the string");
    let without = shown.replace(CANARY_PROMPT, "");
    assert!(!without.contains("CANARY") && without.contains("o1") && without.contains("3000"), "{without}");
}

fn captured_bodies() -> Vec<(String, serde_json::Value)> {
    let capture: serde_json::Value = serde_json::from_str(REAL_REPLIES).unwrap();
    capture["requests"].as_array().unwrap().iter().map(|r| (r["path"].as_str().unwrap().to_string(), r["body"].clone())).collect()
}

#[test]
fn tc18_the_captured_replies_give_the_reuse_the_adapter_produces_and_ps_holds_no_slot_or_busy_field() {
    let bodies = captured_bodies();
    let reply = |name: &str| bodies.iter().find(|(p, _)| p.ends_with(name)).unwrap().1.clone();
    let reuse = |body: serde_json::Value| {
        let bytes = serde_json::to_vec(&body).unwrap();
        OllamaAdapter.reuse_fields(&UsageView { json_tail: &bytes, protocol: Protocol::OpenAiChat, stream: false }, ReuseProbeState::Unprobed)
    };
    let cached = ReuseFieldName::PromptTokensDetailsCachedTokens;
    assert_eq!(reuse(reply("reply-cold")), ReuseReading::Reused { cached_tokens: 3, field: cached });
    assert_eq!(reuse(reply("reply-identical-second-turn")), ReuseReading::Reused { cached_tokens: 869, field: cached });
    let cut = reply("cut-to-2050");
    assert_eq!(cut["usage"]["prompt_tokens"], 2050, "the engine kept about half of a 4096 context");
    assert_eq!(reuse(cut), ReuseReading::Reused { cached_tokens: 2049, field: cached });
    let ps = reply("/api/ps");
    let model = &ps["models"][0];
    assert_eq!(model["context_length"], 4096, "/api/ps carries the loaded context length");
    let keys: Vec<&String> = model.as_object().unwrap().keys().collect();
    assert!(!keys.iter().any(|k| k.contains("slot") || k.contains("busy") || k.contains("parallel")), "{keys:?}");
}
