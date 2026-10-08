//! Read the request body up to a limit (story 160). Nothing past the limit is read.
use axum::body::Body;
use bytes::{Bytes, BytesMut};
use http_body_util::BodyExt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyReadError {
    /// The declared length, or the bytes read so far, are above the limit.
    TooLarge,
    /// The client closed the connection during the read.
    Aborted,
}

/// Read the whole body if it is at most `limit` bytes. A declared length above the limit is
/// refused before any byte is read; a body of unknown length stops at the first byte over.
pub async fn read_body_limited(body: Body, declared_len: Option<u64>, limit: u64) -> Result<Bytes, BodyReadError> {
    if declared_len.is_some_and(|len| len > limit) {
        return Err(BodyReadError::TooLarge);
    }
    let mut body = body;
    let mut collected = BytesMut::new();
    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(|_| BodyReadError::Aborted)?;
        if let Ok(data) = frame.into_data() {
            if u64::try_from(collected.len() + data.len()).map_or(true, |total| total > limit) {
                return Err(BodyReadError::TooLarge);
            }
            collected.extend_from_slice(&data);
        }
    }
    Ok(collected.freeze())
}
