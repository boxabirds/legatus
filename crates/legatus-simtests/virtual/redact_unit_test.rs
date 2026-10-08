//! Story 120 unit tests: redaction, tokens and the leak scan.
use legatus_testkit::capture::{Capture, CapturedRequest};
use legatus_testkit::markers::{CANARY_API_KEY, CANARY_PROMPT, SECRET_SESSION};
use legatus_testkit::redact::scan::{leak_scan, scan_and_stamp, LeakFound, LEAK_SCAN_MIN_CHARS};
use legatus_testkit::redact::secret::{token_for, RunKey, SecretKind};
use legatus_testkit::redact::text::filler_for;
use legatus_testkit::redact::{parse_raw, RedactError, Redactor};
use serde_json::{json, Value};

const SEED_ONE: u64 = 11;
const SEED_TWO: u64 = 12;
const MAX_SEED: u64 = u64::MAX;

fn request(headers: Vec<(&str, &str)>, body: Value) -> CapturedRequest {
    CapturedRequest { method: "POST".into(), path: "/v1/chat/completions".into(), headers: headers.into_iter().map(|(n, v)| (n.to_string(), v.to_string())).collect(), body, response_status: Some(200) }
}

fn pi_like() -> Capture {
    let body = json!({
        "model": "local-coder",
        "messages": [
            {"role": "system", "content": "You are an expert coding assistant operating inside pi"},
            {"role": "user", "content": [{"type": "text", "text": format!("{CANARY_PROMPT} explain this repository in detail please")}]},
            {"role": "assistant", "content": null, "tool_calls": [{"id": "call_1", "type": "function", "function": {"name": "read", "arguments": "{\"path\":\"/home/someone/src/main.rs\"}"}}]},
            {"role": "tool", "tool_call_id": "call_1", "content": "fn main() { println!(\"a long line of source code that came from a tool result\"); }"}
        ],
        "stream": true,
        "max_completion_tokens": 1024,
        "usage": {"prompt_tokens": 4321, "completion_tokens": 77},
        "prompt_cache_key": SECRET_SESSION,
        "user": "alice@example.com",
        "tools": [{"type": "function", "function": {"name": "read", "description": "Read file contents from the disk of the machine", "parameters": {"type": "object", "properties": {}}}}]
    });
    Capture::new(
        "pi",
        "1.0.3",
        "2026-10-08T00:00:00Z",
        vec![request(
            vec![
                ("Authorization", &format!("Bearer {CANARY_API_KEY}")),
                ("Cookie", "session=abc123def456; theme=dark"),
                ("x-session-affinity", SECRET_SESSION),
                ("content-type", "application/json"),
                ("user-agent", "pi (darwin 25.6.0; arm64)"),
                ("x-something-new", "an unknown header value that must be treated as text"),
            ],
            body,
        )],
    )
}

fn all_strings(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(s) => out.push(s.clone()),
        Value::Array(a) => a.iter().for_each(|v| all_strings(v, out)),
        Value::Object(m) => m.values().for_each(|v| all_strings(v, out)),
        _ => {}
    }
}

fn shape(value: &Value) -> Value {
    match value {
        Value::Object(m) => Value::Object(m.iter().map(|(k, v)| (k.clone(), shape(v))).collect()),
        Value::Array(a) => Value::Array(a.iter().map(shape).collect()),
        Value::String(_) => Value::String(String::new()),
        other => other.clone(),
    }
}

#[test]
fn tc01_structure_key_names_array_lengths_and_token_counts_are_unchanged() {
    let raw = pi_like();
    let out = Redactor::new_run(SEED_ONE).redact(&raw).unwrap();
    assert_eq!(out.requests.len(), 1);
    assert_eq!(shape(&out.requests[0].body), shape(&raw.requests[0].body), "same keys, same array lengths, same numbers");
    assert_eq!(out.requests[0].body["usage"], json!({"prompt_tokens": 4321, "completion_tokens": 77}));
    assert_eq!(out.requests[0].body["max_completion_tokens"], 1024);
    assert_eq!(out.requests[0].body["model"], "local-coder", "the model alias is structure");
    assert_eq!(out.requests[0].body["messages"][1]["role"], "user");
    assert_eq!(out.requests[0].body["tools"][0]["type"], "function");
    assert_eq!((out.harness.as_str(), out.harness_version.as_str(), out.requests[0].response_status), ("pi", "1.0.3", Some(200)));
    assert_eq!(out.requests[0].headers.iter().find(|(n, _)| n == "content-type").unwrap().1, "application/json");
}

#[test]
fn tc02_each_text_gets_a_filler_of_equal_byte_length_ascii_multibyte_empty_and_one_byte() {
    let key = RunKey::from_seed(SEED_ONE);
    for text in ["", "a", "hello world", "caf\u{e9} \u{20ac} \u{1F600} \u{4e2d}\u{6587}", &"x".repeat(1000)] {
        let filler = filler_for(&key, text, 0);
        assert_eq!(filler.len(), text.len(), "{text:?}");
        assert!(filler.is_ascii());
    }
    let redactor = Redactor::new_run(SEED_ONE);
    let mut raw = pi_like();
    raw.requests[0].body["messages"][0]["content"] = json!("caf\u{e9} \u{20ac}\u{1F600}");
    let out = redactor.redact(&raw).unwrap();
    assert_eq!(out.requests[0].body["messages"][0]["content"].as_str().unwrap().len(), "caf\u{e9} \u{20ac}\u{1F600}".len());
}

#[test]
fn tc03_equal_strings_give_equal_fillers_and_different_strings_different_ones() {
    let mut raw = pi_like();
    raw.requests[0].body["messages"] = json!([
        {"role": "user", "content": "the same sentence repeated again"},
        {"role": "assistant", "content": "the same sentence repeated again"},
        {"role": "user", "content": "a different sentence of equal size"}
    ]);
    let out = Redactor::new_run(SEED_ONE).redact(&raw).unwrap();
    let contents: Vec<&str> = out.requests[0].body["messages"].as_array().unwrap().iter().map(|m| m["content"].as_str().unwrap()).collect();
    assert_eq!(contents[0], contents[1]);
    assert_ne!(contents[0], contents[2]);
    assert_ne!(contents[0], "the same sentence repeated again");
    // One-byte texts: every distinct one gets its own filler.
    let singles: Vec<String> = ('a'..='z').map(|c| c.to_string()).collect();
    let mut raw = pi_like();
    raw.requests[0].body["messages"] = Value::Array(singles.iter().map(|s| json!({"role": "user", "content": s})).collect());
    let out = Redactor::new_run(SEED_ONE).redact(&raw).unwrap();
    let mut fillers: Vec<&str> = out.requests[0].body["messages"].as_array().unwrap().iter().map(|m| m["content"].as_str().unwrap()).collect();
    fillers.sort_unstable();
    fillers.dedup();
    assert_eq!(fillers.len(), singles.len());
}

#[test]
fn tc04_credentials_cookies_sessions_and_users_become_keyed_tokens_equal_in_equal_out() {
    let raw = pi_like();
    let out = Redactor::new_run(SEED_ONE).redact(&raw).unwrap();
    let header = |name: &str| out.requests[0].headers.iter().find(|(n, _)| n == name).unwrap().1.clone();
    assert!(header("authorization").starts_with("tok-cred-"));
    assert!(header("cookie").starts_with("tok-cookie-"));
    assert!(header("x-session-affinity").starts_with("tok-session-"));
    assert_eq!(out.requests[0].body["prompt_cache_key"], header("x-session-affinity").replace("tok-session-", "tok-session-"), "the same session value gives the same token in the header and the body");
    assert!(out.requests[0].body["user"].as_str().unwrap().starts_with("tok-user-"));
    assert!(header("x-something-new") != "an unknown header value that must be treated as text", "an unknown header is text, never safe");
    assert_eq!(header("x-something-new").len(), "an unknown header value that must be treated as text".len());
    let key = RunKey::from_seed(SEED_ONE);
    assert_eq!(token_for(&key, SecretKind::Session, "s"), token_for(&key, SecretKind::Session, "s"));
    assert_ne!(token_for(&key, SecretKind::Session, "s"), token_for(&key, SecretKind::Session, "t"));
    assert_ne!(token_for(&key, SecretKind::Session, "s"), token_for(&key, SecretKind::User, "s"));
}

#[test]
fn tc05_a_second_run_gives_other_tokens_and_a_discarded_key_is_zero() {
    let raw = pi_like();
    let one = Redactor::new_run(SEED_ONE).redact(&raw).unwrap();
    let two = Redactor::new_run(SEED_TWO).redact(&raw).unwrap();
    assert_ne!(one.requests[0].headers, two.requests[0].headers);
    assert_ne!(one.requests[0].body["messages"][1], two.requests[0].body["messages"][1]);
    let again = Redactor::new_run(SEED_ONE).redact(&raw).unwrap();
    assert_eq!(one, again, "the same key is the same run");
    let mut key = RunKey::from_seed(MAX_SEED);
    assert!(!key.is_zeroed());
    key.discard();
    assert!(key.is_zeroed(), "no key survives the run");
    assert_eq!(format!("{key:?}"), "RunKey(..)", "the key is not printable");
}

fn capture_with(raw_text: &str, out_text: &str) -> (Capture, Capture) {
    let make = |text: &str| Capture::new("h", "1", "t", vec![request(vec![], json!({"model": "m", "messages": [{"role": "user", "content": text}]}))]);
    (make(raw_text), make(out_text))
}

#[test]
fn tc06_the_scan_passes_at_15_shared_characters_and_fails_at_16_and_17() {
    let shared = "ABCDEFGHIJKLMNOPQ";
    for (len, ok) in [(15usize, true), (16, false), (17, false)] {
        let (raw, out) = capture_with(&format!("1{}2", &shared[..len]), &format!("3{}4", &shared[..len]));
        let result = leak_scan(&raw, &out, LEAK_SCAN_MIN_CHARS);
        assert_eq!(result.is_ok(), ok, "{len} shared characters");
        if let Err(LeakFound { path, len: found }) = result {
            assert_eq!(path, "requests.0.body.messages.0.content");
            assert!(found >= LEAK_SCAN_MIN_CHARS);
        }
    }
    assert_eq!(LEAK_SCAN_MIN_CHARS, 16);
}

#[test]
fn tc06_a_clean_redaction_passes_the_scan_and_gets_a_stamp() {
    let raw = pi_like();
    let out = Redactor::new_run(SEED_ONE).redact(&raw).unwrap();
    let stamped = scan_and_stamp(&raw, &out, LEAK_SCAN_MIN_CHARS).expect("no leak");
    assert_eq!(stamped.scan_stamp.as_deref(), Some("leak-scan-v1:16"));
    let mut text = Vec::new();
    for r in &stamped.requests {
        all_strings(&r.body, &mut text);
        text.extend(r.headers.iter().map(|(_, v)| v.clone()));
    }
    for marker in [CANARY_PROMPT, SECRET_SESSION, CANARY_API_KEY, "alice@example.com", "abc123def456"] {
        assert!(!text.iter().any(|t| t.contains(marker)), "{marker} survived");
    }
}

#[test]
fn tc06_the_scan_catches_an_unredacted_text_left_by_a_broken_redactor() {
    let raw = pi_like();
    let mut leaky = Redactor::new_run(SEED_ONE).redact(&raw).unwrap();
    leaky.requests[0].body["messages"][3]["content"] = raw.requests[0].body["messages"][3]["content"].clone();
    let error = leak_scan(&raw, &leaky, LEAK_SCAN_MIN_CHARS).unwrap_err();
    assert_eq!(error.path, "requests.0.body.messages.3.content");
    assert!(error.len >= LEAK_SCAN_MIN_CHARS);
}

#[test]
fn tc21_no_error_or_report_holds_a_planted_marker() {
    let raw = pi_like();
    let mut leaky = Redactor::new_run(SEED_ONE).redact(&raw).unwrap();
    leaky.requests[0].body["messages"][1]["content"][0]["text"] = raw.requests[0].body["messages"][1]["content"][0]["text"].clone();
    let leak = leak_scan(&raw, &leaky, LEAK_SCAN_MIN_CHARS).unwrap_err();
    let shown = format!("{leak} {leak:?}");
    let errors = [format!("{}", RedactError::NotUtf8("requests.0.body".into())), format!("{}", RedactError::TooLarge("requests.0.body".into())), shown];
    for text in errors {
        for marker in [CANARY_PROMPT, SECRET_SESSION, CANARY_API_KEY] {
            assert!(!text.contains(marker), "{text}");
        }
    }
    let parse_error = parse_raw(&[0xff, 0xfe, b'{']).unwrap_err();
    assert_eq!(parse_error, RedactError::NotUtf8("(file)".into()));
}

#[test]
fn tc22_an_empty_capture_and_a_one_request_capture_redact_without_error() {
    let empty = Capture::new("pi", "1", "t", vec![]);
    let out = Redactor::new_run(SEED_ONE).redact(&empty).unwrap();
    assert!(out.requests.is_empty());
    assert!(leak_scan(&empty, &out, LEAK_SCAN_MIN_CHARS).is_ok());
    let one = Capture::new("pi", "1", "t", vec![request(vec![], json!({"model": "m"}))]);
    assert_eq!(Redactor::new_run(0).redact(&one).unwrap().requests.len(), 1);
    assert_eq!(Redactor::new_run(MAX_SEED).redact(&one).unwrap().requests.len(), 1);
}

#[test]
fn a_string_over_the_size_limit_is_refused_with_its_path_and_nothing_else() {
    let huge = "x".repeat(legatus_testkit::redact::MAX_TEXT_BYTES + 1);
    let capture = Capture::new("pi", "1", "t", vec![request(vec![], json!({"model": "m", "messages": [{"role": "user", "content": huge}]}))]);
    let error = Redactor::new_run(SEED_ONE).redact(&capture).unwrap_err();
    assert_eq!(error, RedactError::TooLarge("requests.0.body.messages.0.content".into()));
}
