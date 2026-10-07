//! The context of one request (story 121 declares the first fields; later stories add theirs).
use crate::protocol::paths::RouteKind;
use crate::protocol::rewrite::RequestPeek;
use legatus_common::ids::{AliasName, NodeId};
use legatus_common::protocol::Protocol;
use tokio::time::Instant;

pub struct RequestCtx {
    pub protocol: Protocol,
    pub route: RouteKind,
    /// The model value the harness sent: the alias.
    pub model: String,
    pub stream: bool,
    /// What `peek_request` found in the body as received.
    pub peek: RequestPeek,
    pub alias: AliasName,
    /// The node chosen at step 7.
    pub node: Option<NodeId>,
    pub started: Instant,
}
