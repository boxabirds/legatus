//! Which path is which protocol (story 121 recognises the chat path; later stories add theirs).
use legatus_common::protocol::Protocol;

/// The chat completions path, matched exactly.
pub const CHAT_COMPLETIONS_PATH: &str = "/v1/chat/completions";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteKind {
    Chat,
    Messages,
    CountTokens,
    Responses,
    Models,
    Other,
}

/// The protocol and route of a request path. Only the exact chat path is recognised here.
pub fn recognise(path: &str) -> Option<(Protocol, RouteKind)> {
    match path {
        CHAT_COMPLETIONS_PATH => Some((Protocol::OpenAiChat, RouteKind::Chat)),
        _ => None,
    }
}
