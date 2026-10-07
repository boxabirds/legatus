//! The one call site of the node transport (story 121). A source scan test enforces that no
//! other function in the workspace library calls `UpstreamTransport::send`.
use crate::upstream::transport::{UpstreamError, UpstreamRequest, UpstreamResponse, UpstreamTransport};
use legatus_common::ids::NodeId;

/// Send one request to one node. Makes no retry and sets no timeout (PRX-SCOPE-006, 007).
/// `Connect` means no head arrived; a node error status is a response, not an error.
pub async fn send_to_node(transport: &dyn UpstreamTransport, _node: &NodeId, request: UpstreamRequest) -> Result<UpstreamResponse, UpstreamError> {
    transport.send(request).await
}
