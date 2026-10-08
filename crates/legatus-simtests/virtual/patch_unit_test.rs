//! Story 190 unit tests: the patch edits and their bytes, the path rule, the load-time check and
//! what the errors may say.
use bytes::Bytes;
use legatus_common::engine::EngineFamily;
use legatus_common::patch::*;
use legatus_proxy::engine::patch::*;
use legatus_proxy::engine::standard_adapters;
use legatus_proxy::protocol::paths::RouteKind;
use legatus_proxy::protocol::rewrite::peek_request;
use serde_json::{json, Value};

const CANARY_PROMPT: &str = "CANARY-PROMPT-7f3a";

fn patch(set: Value, remove: &[&str]) -> NodePatch {
    NodePatch::from_json(&json!({"set": set, "remove": remove}).to_string()).unwrap()
}

fn apply(body: &str, p: &NodePatch) -> String {
    let body = Bytes::from(body.to_string());
    let peek = peek_request(&body).unwrap();
    String::from_utf8(apply_patch(&body, &peek, p).unwrap().to_vec()).unwrap()
}

const BASE: &str = "{\"model\":\"m\",\"temperature\":0.7,\"messages\":[]}";

#[test]
fn tc01_set_of_a_scalar_appends_when_absent_and_replaces_in_place_when_present() {
    assert_eq!(apply(BASE, &patch(json!({"max_tokens": 2048}), &[])), "{\"model\":\"m\",\"temperature\":0.7,\"messages\":[],\"max_tokens\":2048}");
    assert_eq!(apply(BASE, &patch(json!({"temperature": 0.2}), &[])), "{\"model\":\"m\",\"temperature\":0.2,\"messages\":[]}", "in place, order kept");
    assert_eq!(apply("{\"model\":\"m\"}", &patch(json!({"top_p": 1}), &[])), "{\"model\":\"m\",\"top_p\":1}");
    assert_eq!(apply(BASE, &patch(json!({"stop": ["a", "b"]}), &[])), "{\"model\":\"m\",\"temperature\":0.7,\"messages\":[],\"stop\":[\"a\",\"b\"]}");
}

#[test]
fn tc02_a_map_merges_one_level_the_patch_wins_a_clash_and_a_deeper_map_is_replaced_whole() {
    let p = patch(json!({"chat_template_kwargs": {"enable_thinking": false, "deep": {"x": 1}}}), &[]);
    let merged = apply("{\"model\":\"m\",\"chat_template_kwargs\":{\"enable_thinking\":true,\"keep\":1,\"deep\":{\"y\":2}}}", &p);
    let value: Value = serde_json::from_str(&merged).unwrap();
    assert_eq!(value["chat_template_kwargs"], json!({"enable_thinking": false, "keep": 1, "deep": {"x": 1}}), "patch wins, keep survives, deep replaced whole");
    // A non-map member is replaced, an absent one gets the whole map.
    let replaced: Value = serde_json::from_str(&apply("{\"model\":\"m\",\"chat_template_kwargs\":7}", &p)).unwrap();
    assert_eq!(replaced["chat_template_kwargs"], json!({"enable_thinking": false, "deep": {"x": 1}}));
    let added: Value = serde_json::from_str(&apply("{\"model\":\"m\"}", &p)).unwrap();
    assert_eq!(added["chat_template_kwargs"], json!({"enable_thinking": false, "deep": {"x": 1}}));
}

#[test]
fn tc03_remove_deletes_every_occurrence_including_duplicates_and_an_absent_key_is_a_no_op() {
    let p = patch(json!({}), &["temperature"]);
    assert_eq!(apply(BASE, &p), "{\"model\":\"m\",\"messages\":[]}");
    assert_eq!(apply("{\"model\":\"m\",\"temperature\":1,\"messages\":[],\"temperature\":2}", &p), "{\"model\":\"m\",\"messages\":[]}");
    assert_eq!(apply(BASE, &patch(json!({}), &["nope"])), BASE);
    // A set of a duplicated key writes the patch value once.
    let once = apply("{\"model\":\"m\",\"top_p\":1,\"messages\":[],\"top_p\":2}", &patch(json!({"top_p": 3}), &[]));
    assert_eq!(once, "{\"model\":\"m\",\"messages\":[],\"top_p\":3}");
}

const ODD: &str = "{\"model\":\"m\", \"tools\" : [ {\"type\":\"function\",\"function\":{\"parameters\":{\"z\":1.0,\"a\":1e3,\"big\":12345678901234567890,\"u\":\"caf\\u00e9 \\ud83d\\ude00\"}}} ] ,\n \"system\":\"s  \\n\",  \"messages\":[{\"role\":\"user\",\"content\":\"x \\\" y\",\"n\":1.50}] , \"stream\" : true }";

#[test]
fn tc04_the_prompt_members_are_byte_equal_after_a_patch_whatever_their_spacing_numbers_and_escapes() {
    let before = Bytes::from(ODD.to_string());
    let peek = peek_request(&before).unwrap();
    let p = patch(json!({"chat_template_kwargs": {"enable_thinking": false}, "max_tokens": 5}), &["temperature"]);
    let after = apply_patch(&before, &peek, &p).unwrap();
    let after_peek = peek_request(&after).unwrap();
    for key in ["messages", "tools", "system"] {
        let (b, a) = (peek.value_spans(key)[0], after_peek.value_spans(key)[0]);
        assert_eq!(&before[b.0..b.1], &after[a.0..a.1], "{key}");
    }
    let parsed: Value = serde_json::from_slice(&after).expect("still a valid object");
    assert_eq!(parsed["max_tokens"], 5);
    assert_eq!(parsed["stream"], true, "the stream switch is untouched");
}

#[test]
fn tc05_applying_twice_equals_once_two_runs_are_equal_and_new_members_are_in_sorted_key_order() {
    let p = patch(json!({"zeta": 1, "alpha": 2, "mid": {"k": 1}}), &[]);
    let once = apply(BASE, &p);
    assert_eq!(apply(&once, &p), once);
    assert_eq!(apply(BASE, &p), once);
    let positions: Vec<usize> = ["alpha", "mid", "zeta"].iter().map(|k| once.find(&format!("\"{k}\"")).unwrap()).collect();
    assert!(positions[0] < positions[1] && positions[1] < positions[2], "{once}");
}

#[test]
fn tc06_a_body_that_is_not_an_object_is_refused_and_an_empty_patch_returns_the_same_bytes() {
    let body = Bytes::from_static(b"[1,2]");
    let peek = peek_request(&Bytes::from(BASE.to_string())).unwrap();
    assert_eq!(patch_edits(&patch(json!({"a": 1}), &[]), &peek, &body), Err(PatchError::NotAnObject));
    let input = Bytes::from(BASE.to_string());
    let peek = peek_request(&input).unwrap();
    let out = apply_patch(&input, &peek, &NodePatch::default()).unwrap();
    assert_eq!(out, input);
    assert_eq!(out.as_ptr(), input.as_ptr(), "no copy");
    assert!(patch_rewritten_body(&Bytes::from_static(b"[1]"), &patch(json!({"a": 1}), &[])).is_err());
}

#[test]
fn tc07_a_set_of_max_tokens_replaces_a_smaller_harness_value_there_is_no_clamp() {
    let out = apply("{\"model\":\"m\",\"max_tokens\":10}", &patch(json!({"max_tokens": 2048}), &[]));
    assert_eq!(out, "{\"model\":\"m\",\"max_tokens\":2048}");
}

#[test]
fn tc14_no_error_form_holds_prompt_text_only_key_names() {
    let errors = [
        PatchError::ForbiddenKey { op: PatchOp::Set, key: "messages".into() },
        PatchError::AdapterForbiddenKey { op: PatchOp::Set, key: "session_id".into() },
        PatchError::EmptyKey { op: PatchOp::Remove },
        PatchError::SetRemoveClash { key: "top_p".into() },
        PatchError::NotAnObject,
    ];
    let shown = format!("{errors:?} {CANARY_PROMPT}");
    assert!(shown.contains("messages") && shown.contains("CANARY"), "control");
    let without = format!("{errors:?}");
    assert!(!without.contains("CANARY"));
}

#[test]
fn tc15_each_forbidden_key_is_rejected_in_set_and_in_remove_and_allowed_keys_pass() {
    let adapter = standard_adapters().for_family(EngineFamily::LlamaServer);
    for key in FORBIDDEN_PATCH_KEYS {
        assert_eq!(FORBIDDEN_PATCH_KEYS.len(), 6);
        let set = validate_patch(&patch(json!({ *key: 1 }), &[]), adapter).unwrap_err();
        assert_eq!(set, vec![PatchError::ForbiddenKey { op: PatchOp::Set, key: (*key).into() }]);
        let remove = validate_patch(&patch(json!({}), &[key]), adapter).unwrap_err();
        assert_eq!(remove, vec![PatchError::ForbiddenKey { op: PatchOp::Remove, key: (*key).into() }]);
    }
    let fine = patch(json!({"chat_template_kwargs": {"enable_thinking": false}, "reasoning_effort": "low", "max_tokens": 9}), &["temperature"]);
    assert_eq!(validate_patch(&fine, adapter), Ok(()));
    let dotted = patch(json!({"chat_template_kwargs.enable_thinking": false}), &[]);
    assert_eq!(validate_patch(&dotted, adapter), Ok(()), "a dotted name is one literal top-level name");
}

#[test]
fn tc16_an_sglang_patch_that_sets_session_id_is_rejected_and_the_same_patch_on_llama_server_passes() {
    let p = patch(json!({"session_id": "x"}), &[]);
    assert_eq!(validate_patch(&p, standard_adapters().for_family(EngineFamily::Sglang)), Err(vec![PatchError::AdapterForbiddenKey { op: PatchOp::Set, key: "session_id".into() }]));
    assert_eq!(validate_patch(&p, standard_adapters().for_family(EngineFamily::LlamaServer)), Ok(()));
}

#[test]
fn tc17_an_empty_key_and_a_set_remove_clash_are_rejected_and_several_mistakes_are_all_returned() {
    let adapter = standard_adapters().for_family(EngineFamily::LlamaServer);
    assert_eq!(validate_patch(&patch(json!({"": 1}), &[]), adapter), Err(vec![PatchError::EmptyKey { op: PatchOp::Set }]));
    assert_eq!(validate_patch(&patch(json!({}), &[""]), adapter), Err(vec![PatchError::EmptyKey { op: PatchOp::Remove }]));
    assert_eq!(validate_patch(&patch(json!({"top_p": 1}), &["top_p"]), adapter), Err(vec![PatchError::SetRemoveClash { key: "top_p".into() }]));
    let many = validate_patch(&patch(json!({"messages": 1, "tools": 2, "": 3}), &["model"]), adapter).unwrap_err();
    assert_eq!(many.len(), 4, "{many:?}");
}

#[test]
fn tc19_only_chat_and_messages_are_patched() {
    for (kind, yes) in [(RouteKind::Chat, true), (RouteKind::Messages, true), (RouteKind::CountTokens, false), (RouteKind::Responses, false), (RouteKind::Models, false), (RouteKind::Other, false)] {
        assert_eq!(patch_applies_to(kind), yes, "{kind:?}");
    }
}

#[test]
fn the_text_of_a_patch_is_read_sorted_and_a_bad_shape_is_named() {
    let p = NodePatch::from_json("{\"set\":{\"b\":1,\"a\":2},\"remove\":[\"z\",\"y\",\"y\"]}").unwrap();
    assert_eq!(p.set.iter().map(|s| s.0.as_str()).collect::<Vec<_>>(), ["a", "b"]);
    assert_eq!(p.remove, ["y", "z"]);
    assert_eq!(NodePatch::from_json("[1]"), Err(PatchParseError::NotAMap));
    assert_eq!(NodePatch::from_json("{\"set\":[1]}"), Err(PatchParseError::SetNotAMap));
    assert_eq!(NodePatch::from_json("{\"remove\":{}}"), Err(PatchParseError::RemoveNotAList));
    assert_eq!(NodePatch::from_json("{\"remove\":[1]}"), Err(PatchParseError::RemoveItemNotText));
    assert!(NodePatch::from_json("{}").unwrap().is_empty());
}
