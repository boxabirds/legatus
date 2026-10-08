//! Story 120 unit tests: harness scripts as data, retry rules, and the conversation generator.
use legatus_testkit::conversation::{ConversationGen, GenMode, Turn};
use legatus_testkit::harness::retry_rule::CLAUDE_CODE_RETRY_AFTER_MAX;
use legatus_testkit::harness::{retry_rule_for, script_for, HarnessKind, HarnessScript, RawRequestKind, RetryAfterPolicy, RetryVerdict};
use std::time::Duration;

const ALL_KINDS: [HarnessKind; 9] = [
    HarnessKind::Pi,
    HarnessKind::ClaudeCode,
    HarnessKind::OpenWebUi,
    HarnessKind::Opencode,
    HarnessKind::DeepSeek,
    HarnessKind::Codex,
    HarnessKind::Sdk,
    HarnessKind::Headerless,
    HarnessKind::RawHttp,
];
const MESSAGES_429: &str = "{\"type\":\"error\",\"error\":{\"type\":\"rate_limit_error\",\"message\":\"slow down\"}}";
const MESSAGES_400: &str = "{\"type\":\"error\",\"error\":{\"type\":\"invalid_request_error\",\"message\":\"bad\"}}";
const SECS: fn(u64) -> Duration = Duration::from_secs;
const TURNS: usize = 12;
const SEED: u64 = 7;
const OTHER_SEED: u64 = 8;
const EDIT_POSITION: u32 = 100;
const OVERSIZED_BYTES: usize = 4096;

fn turns(seed: u64, mode: GenMode) -> Vec<Turn> {
    let mut gen = ConversationGen::new(seed, mode);
    let mut out: Vec<Turn> = Vec::new();
    for _ in 0..TURNS {
        let next = gen.next_turn(out.last());
        out.push(next);
    }
    out
}

fn common_prefix(a: &[u32], b: &[u32]) -> u32 {
    u32::try_from(a.iter().zip(b).take_while(|(x, y)| x == y).count()).unwrap()
}

#[test]
fn tc09_pi_sends_its_session_header_waits_2_4_8_and_gives_up_at_299() {
    let pi = script_for(HarnessKind::Pi);
    assert_eq!(pi.request_headers("S1"), vec![("x-session-affinity".to_string(), "S1".to_string())]);
    assert_eq!(pi.attempt_timeline(), vec![SECS(0), SECS(2), SECS(6), SECS(14)]);
    assert_eq!(pi.attempts, Some(4));
    assert_eq!(pi.give_up, Some(SECS(299)));
    assert_eq!(pi.retry_after, RetryAfterPolicy::Ignores);
    assert!(pi.compaction_without_session_header);
}

#[test]
fn tc12_every_kind_has_a_script_with_fixture_rows_and_assumptions_where_the_table_is_silent() {
    for kind in ALL_KINDS {
        let s = script_for(kind);
        assert_eq!(s.kind, kind);
        assert!(!s.fixtures.is_empty() && s.fixtures.iter().all(|f| f.starts_with("FIX-")), "{}", s.name);
    }
    assert!(!script_for(HarnessKind::ClaudeCode).assumptions.is_empty(), "the 11 beta headers are not in the table");
    assert!(!script_for(HarnessKind::Opencode).assumptions.is_empty());
    assert!(script_for(HarnessKind::Pi).assumptions.is_empty(), "pi is fully in the table");
}

#[test]
fn tc13_claude_code_honours_retry_after_up_to_60_seconds_and_only_then() {
    let s = script_for(HarnessKind::ClaudeCode);
    assert_eq!(s.give_up, Some(SECS(360)));
    assert_eq!(s.wait_for_retry_after(SECS(30)), Some(SECS(30)));
    assert_eq!(s.wait_for_retry_after(SECS(60)), Some(SECS(60)));
    assert_eq!(s.wait_for_retry_after(SECS(61)), None);
    assert_eq!(s.session_headers.len(), 2);
    let pi = script_for(HarnessKind::Pi);
    assert_eq!(pi.wait_for_retry_after(SECS(1)), None, "pi ignores it");
}

#[test]
fn tc14_opencode_has_nine_attempts_a_300_second_limit_and_two_session_headers() {
    let s = script_for(HarnessKind::Opencode);
    assert_eq!((s.attempts, s.give_up), (Some(9), Some(SECS(300))));
    assert_eq!(s.session_headers, vec!["x-session-affinity", "x-session-id"]);
    assert!(s.retry_gaps.is_empty(), "the gaps are not in the table, so none are invented");
}

#[test]
fn tc15_codex_fails_at_once_on_429_and_sends_three_session_headers() {
    let s = script_for(HarnessKind::Codex);
    assert_eq!(s.fails_at_once_on, vec![429]);
    assert_eq!(s.session_headers.len(), 3);
    let headers = s.request_headers("abc");
    assert!(headers.iter().any(|(n, v)| n == "session-id" && v == "abc"));
}

#[test]
fn tc16_open_webui_fans_out_twenty_and_sdk_has_node_and_python_limits() {
    assert_eq!(script_for(HarnessKind::OpenWebUi).fan_out, Some(20));
    let sdk = script_for(HarnessKind::Sdk);
    assert_eq!(sdk.give_up_variants, vec![("node", SECS(301)), ("python", SECS(600))]);
}

#[test]
fn tc17_headerless_sends_no_session_header_and_deepseek_gives_up_at_299_after_7_attempts() {
    assert!(script_for(HarnessKind::Headerless).session_headers.is_empty());
    assert!(script_for(HarnessKind::Headerless).request_headers("x").is_empty());
    let d = script_for(HarnessKind::DeepSeek);
    assert_eq!((d.attempts, d.give_up), (Some(7), Some(SECS(299))));
}

#[test]
fn tc18_raw_client_has_five_request_kinds_each_with_its_own_bytes() {
    let s = script_for(HarnessKind::RawHttp);
    assert_eq!(s.raw_requests.len(), 5);
    let mut seen: Vec<Vec<u8>> = s.raw_requests.iter().map(|k| HarnessScript::raw_request(*k, OVERSIZED_BYTES)).collect();
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), 5);
    let big = HarnessScript::raw_request(RawRequestKind::Oversized, OVERSIZED_BYTES);
    assert!(big.len() > OVERSIZED_BYTES);
    let chunked = String::from_utf8(HarnessScript::raw_request(RawRequestKind::Chunked, 0)).unwrap();
    assert!(chunked.contains("Transfer-Encoding: chunked") && chunked.ends_with("0\r\n\r\n"));
    assert!(String::from_utf8(HarnessScript::raw_request(RawRequestKind::Http10, 0)).unwrap().contains("HTTP/1.0"));
}

#[test]
fn tc19_retry_rule_exists_for_pi_opencode_and_claude_code_only() {
    for kind in [HarnessKind::Pi, HarnessKind::Opencode, HarnessKind::ClaudeCode] {
        assert!(retry_rule_for(kind).is_some());
    }
    for kind in [HarnessKind::Codex, HarnessKind::Sdk, HarnessKind::Headerless, HarnessKind::RawHttp, HarnessKind::OpenWebUi, HarnessKind::DeepSeek] {
        assert!(retry_rule_for(kind).is_none(), "{kind:?}");
    }
}

#[test]
fn tc20_on_a_messages_body_pi_and_opencode_decide_by_status() {
    for kind in [HarnessKind::Pi, HarnessKind::Opencode] {
        let rule = retry_rule_for(kind).unwrap();
        assert_eq!(rule.classify(429, MESSAGES_429, None), RetryVerdict::Retry, "{kind:?}");
        assert_eq!(rule.classify(503, MESSAGES_429, None), RetryVerdict::Retry);
        assert_eq!(rule.classify(400, MESSAGES_400, None), RetryVerdict::Stop);
        assert_eq!(rule.classify(409, MESSAGES_400, None), RetryVerdict::Stop);
    }
}

#[test]
fn tc23_claude_code_stops_when_retry_after_is_over_60_seconds() {
    let rule = retry_rule_for(HarnessKind::ClaudeCode).unwrap();
    assert_eq!(CLAUDE_CODE_RETRY_AFTER_MAX, SECS(60));
    assert_eq!(rule.classify(529, MESSAGES_429, Some(SECS(60))), RetryVerdict::Retry);
    assert_eq!(rule.classify(529, MESSAGES_429, Some(SECS(61))), RetryVerdict::Stop);
    assert_eq!(rule.classify(503, "{}", None), RetryVerdict::Retry);
    assert_eq!(rule.classify(400, MESSAGES_400, None), RetryVerdict::Stop);
    assert_eq!(rule.classify(200, "{\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\"}}", None), RetryVerdict::Retry);
}

#[test]
fn tc24_the_same_seed_gives_the_same_conversation_and_another_seed_a_different_one() {
    assert_eq!(turns(SEED, GenMode::Append), turns(SEED, GenMode::Append));
    assert_ne!(turns(SEED, GenMode::Append), turns(OTHER_SEED, GenMode::Append));
    let mut announced = Vec::new();
    let gen = ConversationGen::with_optional_seed(None, GenMode::Append, &mut |m| announced.push(m));
    assert_eq!(announced, vec!["conversation seed: 1".to_string()]);
    assert_eq!(gen.seed(), 1);
    let mut quiet = Vec::new();
    ConversationGen::with_optional_seed(Some(SEED), GenMode::Append, &mut |m| quiet.push(m));
    assert!(quiet.is_empty());
}

#[test]
fn tc25_expected_d_is_exactly_the_shared_prefix_in_every_mode() {
    for mode in [GenMode::Append, GenMode::ToolCall, GenMode::Compaction, GenMode::Cancel, GenMode::EditAt { position: EDIT_POSITION }] {
        let all = turns(SEED, mode);
        assert_eq!(all[0].expected_d, 0);
        for pair in all.windows(2) {
            assert_eq!(pair[1].expected_d, common_prefix(&pair[0].tokens, &pair[1].tokens), "{mode:?}");
        }
    }
    let side = turns(SEED, GenMode::SideRequest);
    assert!(side.iter().skip(1).all(|t| t.expected_d == 0));
    let appended = turns(SEED, GenMode::Append);
    assert!(appended.windows(2).all(|p| p[1].tokens.len() > p[0].tokens.len()));
}
