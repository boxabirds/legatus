//! Story 121 unit tests: the byte-faithful model rewrite, the top-level splice, headers, step order.
use bytes::Bytes;
use http::header::{HeaderName, HeaderValue};
use http::HeaderMap;
use legatus_proxy::protocol::chat::{PipelineStep, PIPELINE_STEP_COUNT, STEP_ORDER};
use legatus_proxy::protocol::headers::{forward_headers, HOP_BY_HOP_HEADERS};
use legatus_proxy::protocol::rewrite::*;

const PI_103_BODY: &str = include_str!("../fixtures/bodies/pi_103_chat.json");
const PI_HAND_BODY: &str = include_str!("../fixtures/bodies/pi_chat.json");
const NODE_MODEL: &str = "qwen3.8-27b-q4";

fn bytes(text: &str) -> Bytes {
    Bytes::from(text.to_string())
}

fn rewritten(body: &str, node_model: &str) -> String {
    let body = bytes(body);
    let peek = peek_request(&body).expect("peek");
    String::from_utf8(rewrite_model(&body, &peek, node_model).to_vec()).unwrap()
}

/// The first and last bytes where two texts differ; everything outside is identical.
fn differing_span(a: &str, b: &str) -> (usize, usize) {
    let prefix = a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count();
    let suffix = a.bytes().rev().zip(b.bytes().rev()).take_while(|(x, y)| x == y).count().min(a.len().min(b.len()) - prefix);
    (prefix, suffix)
}

#[test]
fn tc01_the_alias_is_replaced_and_every_other_byte_is_equal_on_a_recorded_pi_body() {
    let out = rewritten(PI_103_BODY, NODE_MODEL);
    assert_eq!(out, PI_103_BODY.replacen("\"model\":\"local-coder\"", &format!("\"model\":\"{NODE_MODEL}\""), 1));
    let (prefix, suffix) = differing_span(PI_103_BODY, &out);
    // Everything before and after the differing span is identical, and the span in the original
    // lies inside the alias (the common letters of the two names may shrink it).
    let original_span = &PI_103_BODY[prefix..PI_103_BODY.len() - suffix];
    assert!("local-coder".contains(original_span), "the original differs only inside the alias: {original_span:?}");
    assert_eq!(out.len() - PI_103_BODY.len(), NODE_MODEL.len() - "local-coder".len());
    let peek = peek_request(&bytes(PI_103_BODY)).unwrap();
    assert_eq!(peek.model(), Some("local-coder"));
}

#[test]
fn tc02_a_model_key_inside_a_tool_parameter_is_untouched() {
    let body = "{\"tools\":[{\"function\":{\"parameters\":{\"properties\":{\"model\":{\"type\":\"string\"}}}}}],\"model\":\"alias\",\"stream\":true}";
    let out = rewritten(body, "node");
    assert_eq!(out, body.replace("\"model\":\"alias\"", "\"model\":\"node\""));
    assert!(out.contains("\"properties\":{\"model\":{\"type\":\"string\"}}"));
}

#[test]
fn tc03_unknown_fields_order_escapes_and_white_space_are_identical() {
    let body = "{\n  \"zeta\": [1, 2.5e3, null, true],\n  \"model\" :  \"alias\" ,\n  \"na\\u00efve\": \"caf\\u00e9 \\\"quoted\\\" \\\\ \\n\",\n  \"unknown\": {\"a\": {\"b\": []}}\n}\n";
    let out = rewritten(body, "n");
    assert_eq!(out, body.replace("\"alias\"", "\"n\""));
}

#[test]
fn tc03_a_key_written_with_an_escape_is_still_the_model_key() {
    let body = "{\"mod\\u0065l\":\"alias\",\"x\":1}";
    let peek = peek_request(&bytes(body)).unwrap();
    assert_eq!(peek.model(), Some("alias"));
    assert_eq!(rewritten(body, "n"), "{\"mod\\u0065l\":\"n\",\"x\":1}");
}

#[test]
fn tc03_the_model_value_is_decoded_for_the_lookup_and_replaced_as_a_whole() {
    let body = "{\"model\":\"al\\u0069as\\ud83d\\ude00\"}";
    let peek = peek_request(&bytes(body)).unwrap();
    assert_eq!(peek.model(), Some("alias\u{1F600}"));
    assert_eq!(rewritten(body, "n"), "{\"model\":\"n\"}");
}

#[test]
fn tc04_a_content_string_that_contains_a_model_key_is_untouched() {
    let body = "{\"messages\":[{\"role\":\"user\",\"content\":\"set \\\"model\\\": \\\"x\\\" and {\\\"model\\\":\\\"y\\\"}\"}],\"model\":\"alias\"}";
    let out = rewritten(body, "node");
    assert_eq!(out, body.replace("],\"model\":\"alias\"}", "],\"model\":\"node\"}"));
    assert!(out.contains("{\\\"model\\\":\\\"y\\\"}"));
}

#[test]
fn tc05_model_missing_not_a_string_and_duplicate_are_returned_without_a_panic() {
    assert_eq!(peek_request(&bytes("{\"messages\":[]}")).unwrap_err(), RewriteError::ModelMissing);
    assert_eq!(peek_request(&bytes("{}")).unwrap_err(), RewriteError::ModelMissing);
    for bad in ["7", "null", "true", "[\"a\"]", "{\"a\":1}"] {
        assert_eq!(peek_request(&bytes(&format!("{{\"model\":{bad}}}"))).unwrap_err(), RewriteError::ModelNotString, "{bad}");
    }
    assert_eq!(peek_request(&bytes("{\"model\":\"a\",\"x\":1,\"model\":\"b\"}")).unwrap_err(), RewriteError::DuplicateModel);
    assert_eq!(peek_request(&bytes("{\"model\":\"a\",\"mod\\u0065l\":\"b\"}")).unwrap_err(), RewriteError::DuplicateModel);
}

#[test]
fn tc06_not_object_for_arrays_and_scalars_invalid_json_for_bad_empty_and_one_byte_bodies() {
    for body in ["[]", "[{\"model\":\"a\"}]", "\"model\"", "12", "true", " null "] {
        assert_eq!(peek_request(&bytes(body)).unwrap_err(), RewriteError::NotObject, "{body:?}");
    }
    for body in ["", " ", "{", "x", "}", "{\"model\"", "{\"model\":", "{\"model\":\"a\"", "{\"model\":\"a\",}", "{\"model\" \"a\"}", "{\"model\":\"a\"} trailing", "{model:\"a\"}", "{\"model\":\"a\\q\"}", "{\"a\":[1,2}", "{\"a\":tru}"] {
        assert_eq!(peek_request(&bytes(body)).unwrap_err(), RewriteError::InvalidJson, "{body:?}");
    }
}

#[test]
fn tc07_empty_model_one_character_model_and_longer_and_shorter_node_names() {
    assert_eq!(rewritten("{\"model\":\"\"}", "n"), "{\"model\":\"n\"}");
    assert_eq!(rewritten("{\"model\":\"a\",\"k\":1}", "z"), "{\"model\":\"z\",\"k\":1}");
    assert_eq!(rewritten("{\"model\":\"a\"}", "a-very-long-node-model-name-with-quant-q4_k_m"), "{\"model\":\"a-very-long-node-model-name-with-quant-q4_k_m\"}");
    assert_eq!(rewritten("{\"model\":\"a-very-long-alias-name\"}", "n"), "{\"model\":\"n\"}");
    assert_eq!(rewritten("{\"model\":\"a\"}", ""), "{\"model\":\"\"}");
}

#[test]
fn tc07_a_node_name_with_a_quote_a_backslash_and_a_control_character_is_escaped() {
    let out = rewritten("{\"model\":\"a\"}", "ab\"c\\d\ne\u{1}");
    assert_eq!(out, "{\"model\":\"ab\\\"c\\\\d\\ne\\u0001\"}");
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(parsed["model"], "ab\"c\\d\ne\u{1}");
    assert_eq!(json_string("é€😀"), "\"é€😀\"", "non-ASCII stays as UTF-8");
}

#[test]
fn tc08_the_stream_flag_is_read_true_false_absent_and_not_a_boolean_without_touching_bytes() {
    assert_eq!(peek_request(&bytes("{\"model\":\"a\",\"stream\":true}")).unwrap().stream(), Some(true));
    assert_eq!(peek_request(&bytes("{\"stream\":false,\"model\":\"a\"}")).unwrap().stream(), Some(false));
    assert_eq!(peek_request(&bytes("{\"model\":\"a\"}")).unwrap().stream(), None);
    assert_eq!(peek_request(&bytes("{\"model\":\"a\",\"stream\":\"yes\"}")).unwrap().stream(), None);
    assert_eq!(peek_request(&bytes("{\"model\":\"a\",\"x\":{\"stream\":true}}")).unwrap().stream(), None, "only a top-level stream counts");
    let pi = peek_request(&bytes(PI_103_BODY)).unwrap();
    assert_eq!(pi.stream(), Some(true));
}

#[test]
fn tc09_turn_1_and_turn_30_give_the_same_edit() {
    let body_for_turn = |turns: usize| {
        let messages: Vec<String> = (0..turns).map(|i| format!("{{\"role\":\"user\",\"content\":\"turn {i}\"}}")).collect();
        format!("{{\"model\":\"local-coder\",\"messages\":[{}],\"stream\":true}}", messages.join(","))
    };
    for turns in [1, 2, 10, 30] {
        let body = body_for_turn(turns);
        assert_eq!(rewritten(&body, NODE_MODEL), body.replacen("local-coder", NODE_MODEL, 1), "turn {turns}");
    }
}

#[test]
fn tc10_no_stream_options_in_none_out_and_pis_own_stream_options_are_kept() {
    let without = "{\"model\":\"a\",\"messages\":[],\"stream\":true}";
    assert!(!rewritten(without, "n").contains("stream_options"));
    let out = rewritten(PI_103_BODY, "n");
    assert!(out.contains("\"stream_options\":{\"include_usage\":true}"), "what pi sent is what the node gets");
    assert_eq!(out.matches("stream_options").count(), PI_103_BODY.matches("stream_options").count());
    let hand = rewritten(PI_HAND_BODY, "n");
    assert_eq!(hand.matches("stream_options").count(), PI_HAND_BODY.matches("stream_options").count());
}

fn edit(body: &str, edits: &[TopLevelEdit]) -> String {
    let body = bytes(body);
    let peek = peek_request(&body).unwrap();
    String::from_utf8(splice_top_level(&body, &peek, edits).to_vec()).unwrap()
}

fn raw(text: &str) -> Bytes {
    bytes(text)
}

#[test]
fn splice_replaces_inserts_and_removes_top_level_members_only() {
    let body = "{\"model\":\"m\",\"a\":1,\"nested\":{\"a\":2},\"b\":[1,2]}";
    assert_eq!(edit(body, &[TopLevelEdit::Replace { key: "a".into(), raw_value: raw("{\"x\":0}") }]), "{\"model\":\"m\",\"a\":{\"x\":0},\"nested\":{\"a\":2},\"b\":[1,2]}");
    assert_eq!(edit(body, &[TopLevelEdit::Insert { key: "c".into(), raw_value: raw("true") }]), "{\"model\":\"m\",\"a\":1,\"nested\":{\"a\":2},\"b\":[1,2],\"c\":true}");
    assert_eq!(edit(body, &[TopLevelEdit::Remove { key: "a".into() }]), "{\"model\":\"m\",\"nested\":{\"a\":2},\"b\":[1,2]}");
    assert_eq!(edit(body, &[TopLevelEdit::Remove { key: "b".into() }]), "{\"model\":\"m\",\"a\":1,\"nested\":{\"a\":2}}");
    assert_eq!(edit(body, &[TopLevelEdit::Remove { key: "model".into() }]).len() < body.len(), true);
    assert_eq!(edit(body, &[TopLevelEdit::Remove { key: "absent".into() }]), body);
}

#[test]
fn splice_keeps_white_space_and_handles_the_first_last_and_only_member() {
    let body = "{\n  \"model\": \"m\",\n  \"a\": 1\n}\n";
    assert_eq!(edit(body, &[TopLevelEdit::Insert { key: "z".into(), raw_value: raw("0") }]), "{\n  \"model\": \"m\",\n  \"a\": 1,\"z\":0\n}\n");
    assert_eq!(edit(body, &[TopLevelEdit::Remove { key: "model".into() }]), "{\n  \"a\": 1\n}\n");
    assert_eq!(edit(body, &[TopLevelEdit::Remove { key: "a".into() }]), "{\n  \"model\": \"m\"\n}\n");
    assert_eq!(edit("{\"model\":\"m\"}", &[TopLevelEdit::Remove { key: "model".into() }]), "{}");
    let only = edit("{\"model\":\"m\"}", &[TopLevelEdit::Insert { key: "k".into(), raw_value: raw("1") }]);
    assert_eq!(only, "{\"model\":\"m\",\"k\":1}");
}

#[test]
fn splice_applies_several_edits_in_order_and_keeps_valid_json() {
    let body = "{\"model\":\"m\",\"a\":1,\"b\":2}";
    let out = edit(
        body,
        &[
            TopLevelEdit::Replace { key: "a".into(), raw_value: raw("\"much longer value\"") },
            TopLevelEdit::Remove { key: "b".into() },
            TopLevelEdit::Insert { key: "c".into(), raw_value: raw("[1,2,3]") },
            TopLevelEdit::Insert { key: "a".into(), raw_value: raw("0") },
        ],
    );
    assert_eq!(out, "{\"model\":\"m\",\"a\":0,\"c\":[1,2,3]}");
    serde_json::from_str::<serde_json::Value>(&out).unwrap();
}

fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (name, value) in pairs {
        map.append(HeaderName::from_bytes(name.as_bytes()).unwrap(), HeaderValue::from_str(value).unwrap());
    }
    map
}

#[test]
fn tc11_hop_headers_and_the_headers_named_by_connection_are_removed_and_a_custom_header_stays() {
    let inbound = headers(&[
        ("connection", "keep-alive, x-private"),
        ("keep-alive", "timeout=5"),
        ("proxy-authenticate", "x"),
        ("proxy-authorization", "y"),
        ("te", "trailers"),
        ("trailer", "z"),
        ("transfer-encoding", "chunked"),
        ("upgrade", "websocket"),
        ("x-private", "only this hop"),
        ("x-custom", "stays"),
        ("authorization", "Bearer local"),
        ("accept-encoding", "gzip, deflate"),
        ("host", "proxy.local"),
        ("content-length", "123"),
    ]);
    let out = forward_headers(&inbound);
    for name in HOP_BY_HOP_HEADERS {
        assert!(!out.contains_key(name), "{name} removed");
    }
    assert!(!out.contains_key("x-private"), "named by connection");
    assert_eq!(out["x-custom"], "stays");
    assert_eq!(out["authorization"], "Bearer local", "credentials are story 152, not decided here");
    assert_eq!(out["accept-encoding"], "gzip, deflate", "not touched");
    assert!(!out.contains_key("host") && !out.contains_key("content-length"), "set for the node and the final body by the sender");
}

#[test]
fn tc11_repeated_headers_keep_every_value_in_order_and_the_recorded_pi_headers_survive() {
    let out = forward_headers(&headers(&[("x-multi", "one"), ("x-multi", "two"), ("x-multi", "three")]));
    let values: Vec<&str> = out.get_all("x-multi").iter().map(|v| v.to_str().unwrap()).collect();
    assert_eq!(values, vec!["one", "two", "three"]);
    let recorded: Vec<(String, String)> = serde_json::from_str(include_str!("../fixtures/bodies/pi_103_chat_headers.json")).unwrap();
    let pairs: Vec<(&str, &str)> = recorded.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let forwarded = forward_headers(&headers(&pairs));
    let hop = ["connection", "keep-alive", "transfer-encoding", "te", "upgrade", "trailer"];
    for (name, value) in &recorded {
        if hop.contains(&name.as_str()) {
            assert!(!forwarded.contains_key(name.as_str()), "{name}");
        } else {
            assert_eq!(forwarded[name.as_str()], value.as_str(), "{name}");
        }
    }
    assert!(!forwarded.contains_key("connection"), "pi sends keep-alive; the hop is not forwarded");
}

#[test]
fn tc15_step_order_has_sixteen_distinct_entries_in_the_documented_order() {
    assert_eq!(PIPELINE_STEP_COUNT, 16);
    assert_eq!(STEP_ORDER.len(), 16);
    let mut seen: Vec<PipelineStep> = Vec::new();
    for step in STEP_ORDER {
        assert!(!seen.contains(&step), "{step:?} twice");
        seen.push(step);
    }
    assert_eq!(
        STEP_ORDER,
        [
            PipelineStep::AcceptAndAuth, PipelineStep::RecogniseProtocol, PipelineStep::ReadBody, PipelineStep::AliasLookup, PipelineStep::ComputeKey,
            PipelineStep::TableLookup, PipelineStep::Place, PipelineStep::Hold, PipelineStep::HoldRefuse, PipelineStep::PatchAndRewrite,
            PipelineStep::ContextCheck, PipelineStep::Send, PipelineStep::CopyResponse, PipelineStep::CacheFeedback, PipelineStep::TableUpdate,
            PipelineStep::WriteEvent,
        ]
    );
}
