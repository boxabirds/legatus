//! The refusal seam: five kinds here. Story 160 replaces it with the full catalogue.
use axum::body::Body;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::Response;
use legatus_common::protocol::Protocol;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefusalKind {
    ModelMissing,
    InvalidBody,
    ModelNotFound,
    NodeConnectFailed,
    Starting,
}

impl RefusalKind {
    pub fn status(&self) -> StatusCode {
        match self {
            RefusalKind::ModelMissing | RefusalKind::InvalidBody => StatusCode::BAD_REQUEST,
            RefusalKind::ModelNotFound => StatusCode::NOT_FOUND,
            RefusalKind::NodeConnectFailed => StatusCode::BAD_GATEWAY,
            RefusalKind::Starting => StatusCode::SERVICE_UNAVAILABLE,
        }
    }

    /// The code in the error body.
    pub fn code(&self) -> &'static str {
        match self {
            RefusalKind::ModelMissing => "model_missing",
            RefusalKind::InvalidBody => "invalid_body",
            RefusalKind::ModelNotFound => "model_not_found",
            RefusalKind::NodeConnectFailed => "node_connect_failed",
            RefusalKind::Starting => "starting",
        }
    }

    /// Fixed words. No text of the request is ever copied into a refusal.
    fn message(&self) -> &'static str {
        match self {
            RefusalKind::ModelMissing => "The request has no model name.",
            RefusalKind::InvalidBody => "The request body is not a JSON object with a model name.",
            RefusalKind::ModelNotFound => "No model with that name is served here.",
            RefusalKind::NodeConnectFailed => "The node did not answer.",
            RefusalKind::Starting => "The proxy is starting.",
        }
    }
}

/// Extra facts of a refusal; story 160 extends it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RefusalDetail {
    pub retry_after_s: Option<u32>,
}

const ERROR_TYPE_INVALID_REQUEST: &str = "invalid_request_error";
const ERROR_TYPE_SERVER: &str = "server_error";

/// Render a refusal in the error shape of the protocol of the request.
pub fn refuse(kind: RefusalKind, protocol: Protocol, detail: &RefusalDetail) -> Response {
    let error_type = if kind.status().is_client_error() { ERROR_TYPE_INVALID_REQUEST } else { ERROR_TYPE_SERVER };
    let body = match protocol {
        Protocol::AnthropicMessages => format!("{{\"type\":\"error\",\"error\":{{\"type\":\"{error_type}\",\"message\":\"{}\"}}}}", kind.message()),
        _ => format!("{{\"error\":{{\"message\":\"{}\",\"type\":\"{error_type}\",\"code\":\"{}\"}}}}", kind.message(), kind.code()),
    };
    let mut response = Response::new(Body::from(body));
    *response.status_mut() = kind.status();
    response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
    if let Some(seconds) = detail.retry_after_s {
        if let Ok(value) = HeaderValue::from_str(&seconds.to_string()) {
            response.headers_mut().insert(header::RETRY_AFTER, value);
        }
    }
    response
}
