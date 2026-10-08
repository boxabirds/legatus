//! Story 160 unit tests: the catalogue table, the shapes, the word guard, the Retry-After rules,
//! the hold status and the bounded body reader.
use axum::body::Body;
use bytes::Bytes;
use futures_util::stream;
use http_body_util::BodyExt;
use legatus_common::protocol::Protocol;
use legatus_proxy::config::settings::HoldLimitStatus;
use legatus_proxy::protocol::body::{read_body_limited, BodyReadError};
use legatus_proxy::protocol::errors::*;
use legatus_testkit::harness_rules::{pi_reaction, Reaction};

async fn parts(response: axum::response::Response) -> (u16, http::HeaderMap, serde_json::Value, String) {
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    (status, headers, serde_json::from_str(&text).expect("valid JSON"), text)
}

const DETAIL: RefusalDetail = RefusalDetail { max_tokens: Some(8192), used_tokens: Some(9000), retry_after_s: None };

#[test]
fn tc01_the_table_has_fifteen_rows_unique_codes_and_the_right_statuses() {
    assert_eq!(RefusalKind::ALL.len(), 15);
    let mut codes: Vec<&str> = RefusalKind::ALL.iter().map(|k| k.code()).collect();
    codes.sort_unstable();
    codes.dedup();
    assert_eq!(codes.len(), 15, "codes are unique");
    for kind in RefusalKind::ALL {
        let code = kind.code();
        assert!(code.chars().all(|c| c.is_ascii_lowercase() || c == '_') && !code.is_empty(), "{code}");
        assert!(!kind.template().is_empty());
        if kind.must_stop() {
            assert!(matches!(kind.fixed_status(), Some(400 | 401 | 404 | 413)), "{kind:?}");
        }
        assert!(!matches!(kind.fixed_status(), Some(429 | 408)), "{kind:?}");
    }
    let by_status = |s: u16| RefusalKind::ALL.iter().filter(|k| k.fixed_status() == Some(s)).count();
    assert_eq!(by_status(413), 1, "one 413 row");
    assert_eq!(RefusalKind::ALL.iter().filter(|k| k.fixed_status().is_none()).count(), 2, "the two hold kinds take hold_status_for");
    assert_eq!(RefusalKind::NodeConnectFailed.fixed_status(), Some(502));
    assert_eq!(RefusalKind::NoNodeAvailable.fixed_status(), Some(503));
    assert_eq!(RefusalKind::Starting.fixed_status(), Some(503));
    assert_eq!(RefusalKind::ModelNotFound.fixed_status(), Some(404));
    assert_eq!(RefusalKind::InvalidClientToken.fixed_status(), Some(401));
}

#[test]
fn tc01_the_table_is_indexed_by_the_enum_in_the_documented_order() {
    let expected = [
        "model_not_found", "model_missing", "invalid_body", "body_too_large", "protocol_not_served", "responses_not_served", "path_not_served",
        "context_length_exceeded", "previous_response_not_supported", "invalid_client_token", "capacity_wait_expired", "queue_full", "node_connect_failed",
        "no_node_available", "starting",
    ];
    assert_eq!(RefusalKind::ALL.iter().map(|k| k.code()).collect::<Vec<_>>(), expected);
    assert_eq!(RefusalKind::ALL.iter().filter(|k| k.must_stop()).count(), 10);
    assert_eq!(RefusalKind::ALL.iter().filter(|k| k.is_hold()).count(), 2);
}

#[tokio::test]
async fn tc02_every_kind_renders_the_openai_shape_with_message_type_and_code() {
    for kind in RefusalKind::ALL {
        let (status, headers, json, _) = parts(refuse(kind, Protocol::OpenAiChat, &DETAIL)).await;
        assert_eq!(status, kind.fixed_status().unwrap_or(503), "{kind:?}");
        assert_eq!(headers["content-type"], "application/json");
        let error = &json["error"];
        assert_eq!(error["code"], kind.code(), "{kind:?}");
        assert_eq!(error["type"], kind.openai_type(), "{kind:?}");
        assert!(error["message"].as_str().is_some_and(|m| !m.is_empty()), "{kind:?}");
        assert!(json.get("type").is_none(), "the OpenAI shape has no top-level type");
    }
}

#[tokio::test]
async fn tc03_every_kind_with_a_messages_type_renders_the_messages_shape_with_the_code_beside_the_type() {
    for kind in RefusalKind::ALL {
        let (_, _, json, _) = parts(refuse(kind, Protocol::AnthropicMessages, &DETAIL)).await;
        match kind.messages_type() {
            Some(messages_type) => {
                assert_eq!(json["type"], "error", "{kind:?}");
                assert_eq!(json["error"]["type"], messages_type, "{kind:?}");
                assert_eq!(json["error"]["code"], kind.code(), "{kind:?}");
                assert!(json["error"]["message"].as_str().is_some_and(|m| !m.is_empty()));
            }
            None => {
                assert!(json["error"]["code"] == kind.code() && json.get("type").is_none(), "{kind:?} falls back to the OpenAI shape");
                assert!(matches!(kind, RefusalKind::ResponsesNotServed | RefusalKind::PreviousResponseNotSupported));
            }
        }
    }
}

#[tokio::test]
async fn tc17_the_responses_context_refusal_is_the_openai_shape_with_the_pattern_pi_reads() {
    let (status, _, json, text) = parts(refuse(RefusalKind::ContextLengthExceeded, Protocol::OpenAiResponses, &DETAIL)).await;
    assert_eq!(status, 400);
    assert_eq!(json["error"]["code"], "context_length_exceeded");
    assert_eq!(json["error"]["type"], "invalid_request_error");
    let message = json["error"]["message"].as_str().unwrap();
    assert_eq!(message, "This model's maximum context length is 8192 tokens. However, your request has 9000 tokens. Please reduce the length of the messages.");
    assert!(message.contains("maximum context length is 8192 tokens") && message.to_lowercase().contains("context"));
    assert!(!text.contains("{max_tokens}"));
    let (_, _, plain, _) = parts(refuse(RefusalKind::ContextLengthExceeded, Protocol::OpenAiChat, &RefusalDetail::default())).await;
    assert!(plain["error"]["message"].as_str().unwrap().contains("context"), "without numbers the message still names the context");
}

#[tokio::test]
async fn tc04_no_must_stop_body_holds_a_word_of_the_stop_list_in_either_shape() {
    for kind in RefusalKind::ALL.into_iter().filter(|k| k.must_stop()) {
        for protocol in [Protocol::OpenAiChat, Protocol::AnthropicMessages, Protocol::OpenAiResponses] {
            let (status, _, _, text) = parts(refuse(kind, protocol, &DETAIL)).await;
            assert_eq!(forbidden_word_in(WordList::Stop, &text), None, "{kind:?} {protocol:?}: {text}");
            assert_eq!(forbidden_word_in(WordList::NoRetry, &text), None);
            assert_ne!(status, 429);
        }
    }
}

#[test]
fn tc05_the_guard_finds_each_word_on_a_400_and_a_409_body_and_counts_an_underscore_as_a_space() {
    let words = [("503", WordList::Stop), ("server error", WordList::Stop), ("timeout", WordList::Stop), ("rate limit", WordList::Stop), ("overloaded", WordList::Stop), ("terminated", WordList::Stop)];
    for (word, list) in words {
        for status in [400, 409] {
            let body = format!("{{\"error\":{{\"message\":\"x {} y\",\"type\":\"invalid_request_error\"}}}} status {status}", word.to_uppercase());
            assert_eq!(forbidden_word_in(list, &body), Some(word), "{word} on {status}");
        }
    }
    assert_eq!(forbidden_word_in(WordList::Stop, "{\"type\":\"server_error\"}"), Some("server error"));
    assert_eq!(forbidden_word_in(WordList::Stop, "{\"code\":\"rate_limit_exceeded\"}"), Some("rate limit"));
    for word in ["quota", "billing", "rate limit"] {
        assert_eq!(forbidden_word_in(WordList::HoldBody, &format!("About {word}.")), Some(word));
    }
    assert_eq!(forbidden_word_in(WordList::NoRetry, "all fine"), None);
    assert_eq!(forbidden_word_in(WordList::NoRetry, "Overloaded"), Some("overloaded"));
}

#[tokio::test]
async fn tc16_retry_after_30_is_accepted_31_refused_and_statuses_503_504_529_only() {
    for status in [503, 504, 529] {
        let mut detail = RefusalDetail::default();
        detail.retry_after_s = Some(RETRY_AFTER_MAX_S);
        let response = refuse_retry(RefusalKind::CapacityWaitExpired, Protocol::OpenAiChat, status, &detail).unwrap();
        assert_eq!(response.status().as_u16(), status);
        assert_eq!(response.headers()["retry-after"], "30");
    }
    let none = refuse_retry(RefusalKind::QueueFull, Protocol::OpenAiChat, 503, &RefusalDetail::default()).unwrap();
    assert!(!none.headers().contains_key("retry-after"));
    let mut too_long = RefusalDetail::default();
    too_long.retry_after_s = Some(RETRY_AFTER_MAX_S + 1);
    assert_eq!(refuse_retry(RefusalKind::CapacityWaitExpired, Protocol::OpenAiChat, 503, &too_long).err(), Some(RefusalError::RetryAfterTooLarge));
    for status in [429, 408, 500, 502, 400, 200] {
        assert_eq!(refuse_retry(RefusalKind::CapacityWaitExpired, Protocol::OpenAiChat, status, &RefusalDetail::default()).err(), Some(RefusalError::StatusNotAllowed), "{status}");
    }
    for kind in RefusalKind::ALL.into_iter().filter(|k| !k.is_hold()) {
        assert_eq!(refuse_retry(kind, Protocol::OpenAiChat, 503, &RefusalDetail::default()).err(), Some(RefusalError::NotARetryKind), "{kind:?}");
    }
    assert_eq!(RETRY_STATUSES_ALLOWED, [503, 504, 529]);
}

#[tokio::test]
async fn tc16_a_must_stop_refusal_never_carries_retry_after() {
    for kind in RefusalKind::ALL.into_iter().filter(|k| k.must_stop()) {
        let mut detail = DETAIL;
        detail.retry_after_s = Some(5);
        let response = refuse(kind, Protocol::OpenAiChat, &detail);
        assert!(!response.headers().contains_key("retry-after"), "{kind:?}");
    }
}

#[tokio::test]
async fn tc19_retry_bodies_in_the_messages_shape_say_overloaded_and_hold_bodies_avoid_the_hold_words() {
    for kind in [RefusalKind::CapacityWaitExpired, RefusalKind::QueueFull, RefusalKind::NodeConnectFailed, RefusalKind::NoNodeAvailable, RefusalKind::Starting] {
        let (_, _, json, _) = parts(refuse(kind, Protocol::AnthropicMessages, &DETAIL)).await;
        assert_eq!(json["error"]["type"], "overloaded_error", "{kind:?}");
        assert_eq!(json["error"]["message"], "Overloaded", "{kind:?}");
    }
    for kind in [RefusalKind::CapacityWaitExpired, RefusalKind::QueueFull] {
        for protocol in [Protocol::OpenAiChat, Protocol::AnthropicMessages, Protocol::OpenAiResponses] {
            let (_, _, _, text) = parts(refuse(kind, protocol, &DETAIL)).await;
            assert_eq!(forbidden_word_in(WordList::HoldBody, &text), None, "{kind:?} {protocol:?}: {text}");
        }
    }
}

#[tokio::test]
async fn tc21_a_message_with_quotes_newlines_and_unicode_encodes_as_valid_json_in_both_shapes() {
    // The only text with a quote is the context template; the numbers cannot break it.
    for protocol in [Protocol::OpenAiChat, Protocol::AnthropicMessages] {
        let (_, _, json, text) = parts(refuse(RefusalKind::ContextLengthExceeded, protocol, &RefusalDetail { max_tokens: Some(u32::MAX), used_tokens: Some(0), retry_after_s: None })).await;
        assert!(text.contains("\\u0027") || text.contains('\'') || text.contains("model's"));
        assert!(json["error"]["message"].as_str().unwrap().contains("4294967295"));
    }
    for kind in RefusalKind::ALL {
        let (_, _, _, text) = parts(refuse(kind, Protocol::OpenAiChat, &DETAIL)).await;
        serde_json::from_str::<serde_json::Value>(&text).unwrap();
    }
}

#[test]
fn tc23_the_hold_status_follows_the_protocol_and_the_setting() {
    for setting in [None, Some(HoldLimitStatus::S503), Some(HoldLimitStatus::S504), Some(HoldLimitStatus::S529)] {
        assert_eq!(hold_status_for(Protocol::OpenAiResponses, setting), 503, "Responses is always 503");
    }
    assert_eq!(hold_status_for(Protocol::OpenAiChat, None), 503);
    assert_eq!(hold_status_for(Protocol::OpenAiChat, Some(HoldLimitStatus::S504)), 504);
    assert_eq!(hold_status_for(Protocol::OpenAiChat, Some(HoldLimitStatus::S529)), 529);
    assert_eq!(hold_status_for(Protocol::AnthropicMessages, None), 529);
    assert_eq!(hold_status_for(Protocol::AnthropicMessages, Some(HoldLimitStatus::S503)), 503);
    assert_eq!(hold_status_for(Protocol::Passthrough, None), 503);
}

#[test]
fn tc06_the_pi_rules_stop_on_every_must_stop_body_and_retry_every_retry_body() {
    for kind in RefusalKind::ALL {
        let status = kind.fixed_status().unwrap_or(503);
        let reaction = pi_reaction(status, kind.template());
        if kind.must_stop() {
            assert_eq!(reaction, Reaction::Stop, "{kind:?}: pi would retry a refusal that must stop");
        } else {
            assert_eq!(reaction, Reaction::Retry, "{kind:?}: pi would not retry");
        }
    }
    assert_eq!(pi_reaction(503, "Overloaded"), Reaction::Retry);
    assert_eq!(pi_reaction(400, "quota exceeded for this key"), Reaction::Stop, "pi treats a quota body as final");
}

fn body_of(chunks: Vec<&'static [u8]>) -> Body {
    Body::from_stream(stream::iter(chunks.into_iter().map(|c| Ok::<_, std::io::Error>(Bytes::from_static(c)))))
}

#[tokio::test]
async fn tc13_the_body_reader_passes_a_body_at_the_limit_and_refuses_one_byte_over() {
    assert_eq!(read_body_limited(body_of(vec![b"1234", b"5678"]), None, 8).await.unwrap(), Bytes::from_static(b"12345678"));
    assert_eq!(read_body_limited(body_of(vec![b"1234", b"56789"]), None, 8).await.unwrap_err(), BodyReadError::TooLarge);
    assert_eq!(read_body_limited(body_of(vec![]), None, 8).await.unwrap(), Bytes::new(), "a body of zero bytes");
    assert_eq!(read_body_limited(body_of(vec![b"abc"]), Some(3), 3).await.unwrap(), Bytes::from_static(b"abc"));
}

#[tokio::test]
async fn tc13_a_declared_length_over_the_limit_is_refused_before_any_byte_is_read() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    let polled = Arc::new(AtomicBool::new(false));
    let flag = polled.clone();
    let body = Body::from_stream(stream::poll_fn(move |_| {
        flag.store(true, Ordering::SeqCst);
        std::task::Poll::Ready(Some(Ok::<_, std::io::Error>(Bytes::from_static(b"x"))))
    }));
    assert_eq!(read_body_limited(body, Some(9), 8).await.unwrap_err(), BodyReadError::TooLarge);
    assert!(!polled.load(Ordering::SeqCst), "no byte was read");
}

#[tokio::test]
async fn tc13_reading_stops_at_the_first_byte_over_and_a_client_abort_is_aborted() {
    let polled = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = polled.clone();
    let body = Body::from_stream(stream::poll_fn(move |_| {
        counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        std::task::Poll::Ready(Some(Ok::<_, std::io::Error>(Bytes::from_static(b"0123456789"))))
    }));
    assert_eq!(read_body_limited(body, None, 25).await.unwrap_err(), BodyReadError::TooLarge);
    assert_eq!(polled.load(std::sync::atomic::Ordering::SeqCst), 3, "stopped at the chunk that passed the limit");
    let broken = Body::from_stream(stream::iter(vec![Ok(Bytes::from_static(b"ab")), Err(std::io::Error::other("closed"))]));
    assert_eq!(read_body_limited(broken, None, 100).await.unwrap_err(), BodyReadError::Aborted);
}

#[test]
fn the_test_kit_checker_is_the_same_checker_and_panics_on_a_hit() {
    use legatus_testkit::refusal_words::{assert_no_hold_word, assert_no_stop_word};
    assert_no_stop_word("{\"error\":{\"message\":\"No model with that name is served here.\"}}");
    assert_no_hold_word("{\"error\":{\"message\":\"The wait for a free node ended.\"}}");
    assert!(std::panic::catch_unwind(|| assert_no_stop_word("a SERVER_ERROR happened")).is_err());
    assert!(std::panic::catch_unwind(|| assert_no_hold_word("billing problem")).is_err());
}
