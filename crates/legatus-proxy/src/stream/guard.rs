//! The response guard interface. Declared here once; story 135 implements it with its `SecretMatcher`.

/// What the guard found in one chunk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanResult {
    Clean,
    /// The chunk holds a hosted node key: it is not sent and the connection is closed abruptly.
    Found,
}

/// Per reply: created by `copy_response`, filled by the guard for a key split across chunks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GuardCarry {
    pub tail: Vec<u8>,
}

/// The one stage of the reply path that is not read-only. Every method defaults to doing nothing,
/// and while `is_active()` is false the relay calls none of them.
pub trait ResponseGuard: Send + Sync {
    /// True when a hosted node key is loaded.
    fn is_active(&self) -> bool {
        false
    }
    /// Remove a reply header whose value holds a hosted node key.
    fn scrub_headers(&self, _headers: &mut http::HeaderMap) {}
    /// Look at one chunk of the reply body.
    fn scan_chunk(&self, _carry: &mut GuardCarry, _chunk: &[u8]) -> ScanResult {
        ScanResult::Clean
    }
}

/// The guard of a registry with no hosted node key.
pub struct NoGuard;

impl ResponseGuard for NoGuard {}
