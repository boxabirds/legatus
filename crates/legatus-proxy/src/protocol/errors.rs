//! The refusal catalogue (contract C23, story 160): one table of every refusal the proxy makes on
//! its own, one renderer per protocol shape, the hold-status rule, the Retry-After rule and the
//! guard for words that make a harness retry. Other stories supply triggers and call `refuse`;
//! none of them writes a refusal body itself.
use crate::config::settings::HoldLimitStatus;
use axum::body::Body;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::Response;
use legatus_common::protocol::Protocol;
use serde_json::json;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefusalKind {
    ModelNotFound,
    ModelMissing,
    InvalidBody,
    BodyTooLarge,
    ProtocolNotServed,
    ResponsesNotServed,
    PathNotServed,
    ContextLengthExceeded,
    PreviousResponseNotSupported,
    InvalidClientToken,
    CapacityWaitExpired,
    QueueFull,
    NodeConnectFailed,
    NoNodeAvailable,
    Starting,
}

impl RefusalKind {
    /// Every kind, in table order.
    pub const ALL: [RefusalKind; 15] = [
        RefusalKind::ModelNotFound,
        RefusalKind::ModelMissing,
        RefusalKind::InvalidBody,
        RefusalKind::BodyTooLarge,
        RefusalKind::ProtocolNotServed,
        RefusalKind::ResponsesNotServed,
        RefusalKind::PathNotServed,
        RefusalKind::ContextLengthExceeded,
        RefusalKind::PreviousResponseNotSupported,
        RefusalKind::InvalidClientToken,
        RefusalKind::CapacityWaitExpired,
        RefusalKind::QueueFull,
        RefusalKind::NodeConnectFailed,
        RefusalKind::NoNodeAvailable,
        RefusalKind::Starting,
    ];

    fn row(&self) -> &'static Row {
        &TABLE[*self as usize]
    }

    /// The stable machine-readable code in the body.
    pub fn code(&self) -> &'static str {
        self.row().code
    }

    /// The fixed words of the message. The context kind has placeholders filled by `refuse`.
    pub fn template(&self) -> &'static str {
        self.row().template
    }

    /// The status of a kind that has one; `None` for the hold kinds, which take `hold_status_for`.
    pub fn fixed_status(&self) -> Option<u16> {
        self.row().status
    }

    /// The type in the OpenAI shape.
    pub fn openai_type(&self) -> &'static str {
        self.row().openai_type
    }

    /// The type in the Messages shape; `None` for a kind that has no Messages form.
    pub fn messages_type(&self) -> Option<&'static str> {
        self.row().messages_type
    }

    /// True for a refusal after which a harness must stop (never retry).
    pub fn must_stop(&self) -> bool {
        self.row().must_stop
    }

    /// The two hold kinds, the only ones `refuse_retry` takes.
    pub fn is_hold(&self) -> bool {
        matches!(self, RefusalKind::CapacityWaitExpired | RefusalKind::QueueFull)
    }

    /// The status of the response when the caller gives none.
    pub fn status(&self) -> StatusCode {
        let code = self.fixed_status().unwrap_or(HOLD_STATUS_DEFAULT);
        StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
    }
}

/// One row of the catalogue.
struct Row {
    status: Option<u16>,
    code: &'static str,
    openai_type: &'static str,
    messages_type: Option<&'static str>,
    must_stop: bool,
    template: &'static str,
}

const STATUS_BAD_REQUEST: u16 = 400;
const STATUS_UNAUTHORIZED: u16 = 401;
const STATUS_NOT_FOUND: u16 = 404;
const STATUS_TOO_LARGE: u16 = 413;
const STATUS_BAD_GATEWAY: u16 = 502;
const STATUS_UNAVAILABLE: u16 = 503;
/// The status of a hold refusal when nothing else is said.
const HOLD_STATUS_DEFAULT: u16 = STATUS_UNAVAILABLE;
const STATUS_OVERLOADED_MESSAGES: u16 = 529;

const OPENAI_INVALID_REQUEST: &str = "invalid_request_error";
const MESSAGES_OVERLOADED: &str = "overloaded_error";
/// The text of every retry body in the Messages shape (PROPOSED: Messages harnesses retry on it).
const MESSAGES_RETRY_TEXT: &str = "Overloaded";

/// The context message for a request that is too long (it holds the pattern `maximum context
/// length is N tokens` that pi reads, and the word context).
const CONTEXT_TEMPLATE: &str = "This model's maximum context length is {max_tokens} tokens. However, your request has {used_tokens} tokens. Please reduce the length of the messages.";
const CONTEXT_TEMPLATE_PLAIN: &str = "This model's maximum context length was exceeded. Please reduce the length of the messages.";

/// Indexed by `RefusalKind as usize`; a unit test checks the order.
static TABLE: [Row; 15] = [
    Row { status: Some(STATUS_NOT_FOUND), code: "model_not_found", openai_type: OPENAI_INVALID_REQUEST, messages_type: Some("not_found_error"), must_stop: true, template: "No model with that name is served here." },
    Row { status: Some(STATUS_BAD_REQUEST), code: "model_missing", openai_type: OPENAI_INVALID_REQUEST, messages_type: Some(OPENAI_INVALID_REQUEST), must_stop: true, template: "The request has no model name." },
    Row { status: Some(STATUS_BAD_REQUEST), code: "invalid_body", openai_type: OPENAI_INVALID_REQUEST, messages_type: Some(OPENAI_INVALID_REQUEST), must_stop: true, template: "The request body is not a JSON object with a model name." },
    Row { status: Some(STATUS_TOO_LARGE), code: "body_too_large", openai_type: OPENAI_INVALID_REQUEST, messages_type: Some("request_too_large"), must_stop: true, template: "The request body is too large." },
    Row { status: Some(STATUS_NOT_FOUND), code: "protocol_not_served", openai_type: OPENAI_INVALID_REQUEST, messages_type: Some("not_found_error"), must_stop: true, template: "No node of this model serves this protocol." },
    Row { status: Some(STATUS_NOT_FOUND), code: "responses_not_served", openai_type: OPENAI_INVALID_REQUEST, messages_type: None, must_stop: true, template: "No node of this model serves the Responses API." },
    Row { status: Some(STATUS_NOT_FOUND), code: "path_not_served", openai_type: OPENAI_INVALID_REQUEST, messages_type: Some("not_found_error"), must_stop: true, template: "This path is not served." },
    Row { status: Some(STATUS_BAD_REQUEST), code: "context_length_exceeded", openai_type: OPENAI_INVALID_REQUEST, messages_type: Some(OPENAI_INVALID_REQUEST), must_stop: true, template: CONTEXT_TEMPLATE },
    Row { status: Some(STATUS_BAD_REQUEST), code: "previous_response_not_supported", openai_type: OPENAI_INVALID_REQUEST, messages_type: None, must_stop: true, template: "The node of this model does not support previous_response_id." },
    Row { status: Some(STATUS_UNAUTHORIZED), code: "invalid_client_token", openai_type: "authentication_error", messages_type: Some("authentication_error"), must_stop: true, template: "The client token is not valid." },
    Row { status: None, code: "capacity_wait_expired", openai_type: "capacity_wait_expired", messages_type: Some(MESSAGES_OVERLOADED), must_stop: false, template: "The wait for a free node ended. Try again later." },
    Row { status: None, code: "queue_full", openai_type: "capacity_wait_expired", messages_type: Some(MESSAGES_OVERLOADED), must_stop: false, template: "The queue for this model is full. Try again later." },
    Row { status: Some(STATUS_BAD_GATEWAY), code: "node_connect_failed", openai_type: "bad_gateway", messages_type: Some(MESSAGES_OVERLOADED), must_stop: false, template: "The node did not answer." },
    Row { status: Some(STATUS_UNAVAILABLE), code: "no_node_available", openai_type: "unavailable", messages_type: Some(MESSAGES_OVERLOADED), must_stop: false, template: "No node is available for this model." },
    Row { status: Some(STATUS_UNAVAILABLE), code: "starting", openai_type: "unavailable", messages_type: Some(MESSAGES_OVERLOADED), must_stop: false, template: "The proxy is starting." },
];

/// Extra facts of a refusal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RefusalDetail {
    /// For the context message (story 174).
    pub max_tokens: Option<u32>,
    pub used_tokens: Option<u32>,
    pub retry_after_s: Option<u32>,
}

/// The longest Retry-After the proxy sends (PROPOSED, DEC-064 pending).
pub const RETRY_AFTER_MAX_S: u32 = 30;
/// The statuses a hold refusal may use. 429 and 408 are never allowed.
pub const RETRY_STATUSES_ALLOWED: [u16; 3] = [503, 504, 529];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefusalError {
    NotARetryKind,
    StatusNotAllowed,
    RetryAfterTooLarge,
}

/// The message of a refusal: the template with the numbers of the detail. Never a request value.
fn message_for(kind: RefusalKind, detail: &RefusalDetail) -> String {
    match (kind, detail.max_tokens, detail.used_tokens) {
        (RefusalKind::ContextLengthExceeded, Some(max), Some(used)) => CONTEXT_TEMPLATE.replace("{max_tokens}", &max.to_string()).replace("{used_tokens}", &used.to_string()),
        (RefusalKind::ContextLengthExceeded, _, _) => CONTEXT_TEMPLATE_PLAIN.to_string(),
        _ => kind.template().to_string(),
    }
}

/// The body in the shape of the protocol. A kind with no Messages type uses the OpenAI shape.
fn body_for(kind: RefusalKind, protocol: Protocol, detail: &RefusalDetail) -> String {
    let message = message_for(kind, detail);
    let messages_shape = protocol == Protocol::AnthropicMessages;
    match (messages_shape, kind.messages_type()) {
        (true, Some(messages_type)) => {
            let text = if kind.must_stop() { message } else { MESSAGES_RETRY_TEXT.to_string() };
            json!({"type": "error", "error": {"type": messages_type, "message": text, "code": kind.code()}}).to_string()
        }
        _ => json!({"error": {"message": message, "type": kind.openai_type(), "code": kind.code()}}).to_string(),
    }
}

fn respond(status: u16, body: String, retry_after_s: Option<u32>, close: bool) -> Response {
    let mut response = Response::new(Body::from(body));
    *response.status_mut() = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
    if let Some(seconds) = retry_after_s {
        if let Ok(value) = HeaderValue::from_str(&seconds.to_string()) {
            response.headers_mut().insert(header::RETRY_AFTER, value);
        }
    }
    if close {
        response.headers_mut().insert(header::CONNECTION, HeaderValue::from_static("close"));
    }
    response
}

/// Render a refusal. The only renderer: a kind with a fixed status uses it, a hold kind uses the
/// default hold status (callers that know the setting use `refuse_retry`). A too-large body closes
/// the connection after the answer, so the unread rest is not parsed as another request.
pub fn refuse(kind: RefusalKind, protocol: Protocol, detail: &RefusalDetail) -> Response {
    let status = kind.fixed_status().unwrap_or(HOLD_STATUS_DEFAULT);
    respond(status, body_for(kind, protocol, detail), None, kind == RefusalKind::BodyTooLarge)
}

/// Render a hold refusal with the status the caller chose and an optional Retry-After. The proxy
/// never relies on the header; it is only a hint, at most `RETRY_AFTER_MAX_S` seconds.
pub fn refuse_retry(kind: RefusalKind, protocol: Protocol, status: u16, detail: &RefusalDetail) -> Result<Response, RefusalError> {
    if !kind.is_hold() {
        return Err(RefusalError::NotARetryKind);
    }
    if !RETRY_STATUSES_ALLOWED.contains(&status) {
        return Err(RefusalError::StatusNotAllowed);
    }
    if detail.retry_after_s.is_some_and(|s| s > RETRY_AFTER_MAX_S) {
        return Err(RefusalError::RetryAfterTooLarge);
    }
    Ok(respond(status, body_for(kind, protocol, detail), detail.retry_after_s, false))
}

/// The status of a hold refusal: 503 always on the Responses path (PRX-ADM-053); on chat the
/// setting, 503 when it is not set; on Messages the setting, 529 when it is not set (PROPOSED).
pub fn hold_status_for(protocol: Protocol, setting: Option<HoldLimitStatus>) -> u16 {
    match protocol {
        Protocol::OpenAiResponses => STATUS_UNAVAILABLE,
        Protocol::AnthropicMessages => setting.map(|s| s.code()).unwrap_or(STATUS_OVERLOADED_MESSAGES),
        Protocol::OpenAiChat | Protocol::Passthrough => setting.map(|s| s.code()).unwrap_or(STATUS_UNAVAILABLE),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WordList {
    /// Words a refusal that must stop may not hold.
    Stop,
    /// Words a refusal that must not be retried may not hold.
    NoRetry,
    /// Words a hold-limit or queue-full body may not hold.
    HoldBody,
}

const STOP_WORDS: [&str; 6] = ["503", "server error", "timeout", "rate limit", "overloaded", "terminated"];
const NO_RETRY_WORDS: [&str; 4] = ["server error", "overloaded", "timeout", "terminated"];
const HOLD_BODY_WORDS: [&str; 3] = ["quota", "billing", "rate limit"];

/// The first word of the list found in the text, case-insensitive, an underscore counting as a
/// space (so `server_error` matches `server error`). Meant for tests and for the test kit.
pub fn forbidden_word_in(list: WordList, text: &str) -> Option<&'static str> {
    let normal = text.to_lowercase().replace('_', " ");
    let words: &[&'static str] = match list {
        WordList::Stop => &STOP_WORDS,
        WordList::NoRetry => &NO_RETRY_WORDS,
        WordList::HoldBody => &HOLD_BODY_WORDS,
    };
    words.iter().copied().find(|w| normal.contains(w))
}
