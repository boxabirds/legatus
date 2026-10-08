//! Which request headers go on to the node (story 121).
use http::header::{CONNECTION, CONTENT_LENGTH, HOST};
use http::HeaderMap;

/// Headers of one hop (RFC 9110 section 7.6.1), removed from the request to the node.
pub const HOP_BY_HOP_HEADERS: [&str; 8] = ["connection", "keep-alive", "proxy-authenticate", "proxy-authorization", "te", "trailer", "transfer-encoding", "upgrade"];

/// The header set for the node: every end-to-end header unchanged (name, value and order of
/// repeated values), the hop headers removed with every header the `connection` value names.
/// `host` and `content-length` are also dropped here: the sender sets `host` for the node and the
/// transport sets the length of the final body. Credentials and `x-legatus-*` are story 152.
pub fn forward_headers(inbound: &HeaderMap) -> HeaderMap {
    let named_by_connection: Vec<String> = inbound
        .get_all(CONNECTION)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(',').map(|t| t.trim().to_ascii_lowercase()).collect::<Vec<_>>())
        .filter(|t| !t.is_empty())
        .collect();
    let mut out = HeaderMap::with_capacity(inbound.len());
    for (name, value) in inbound.iter() {
        let lower = name.as_str();
        if name == HOST || name == CONTENT_LENGTH || HOP_BY_HOP_HEADERS.contains(&lower) || named_by_connection.iter().any(|n| n == lower) {
            continue;
        }
        out.append(name.clone(), value.clone());
    }
    out
}

/// The header set for the harness: the node's reply headers with only the hop-by-hop headers (and
/// those named by `connection`) removed. Every other name and value passes as sent; none is added.
pub fn strip_reply_headers(node_headers: &HeaderMap) -> HeaderMap {
    let named_by_connection: Vec<String> = node_headers
        .get_all(CONNECTION)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(',').map(|t| t.trim().to_ascii_lowercase()).collect::<Vec<_>>())
        .filter(|t| !t.is_empty())
        .collect();
    let mut out = HeaderMap::with_capacity(node_headers.len());
    for (name, value) in node_headers.iter() {
        let lower = name.as_str();
        if HOP_BY_HOP_HEADERS.contains(&lower) || named_by_connection.iter().any(|n| n == lower) {
            continue;
        }
        out.append(name.clone(), value.clone());
    }
    out
}
