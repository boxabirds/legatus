//! Idle connection close of the stub process and the stub-half check (story 162).
//! The proxy-side check (no reuse of a connection idle longer than `upstream_idle_reuse_max_s`)
//! is built and tested in story 151.
use crate::stubs::StubProcess;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdleError {
    /// The client reused a connection that the stub had closed.
    Reused,
}

/// The idle limit a well-behaved client applies before it stops reusing a connection (PROPOSED,
/// `upstream_idle_reuse_max_s` of story 165).
pub const CLIENT_IDLE_REUSE_MAX_S: u32 = 4;

const PROBE_REQUEST: &[u8] = b"GET /health HTTP/1.1\r\nhost: stub\r\ncontent-length: 0\r\n\r\n";
const READ_BUFFER_BYTES: usize = 4096;

async fn exchange(conn: &mut TcpStream) -> std::io::Result<bool> {
    conn.write_all(PROBE_REQUEST).await?;
    let mut buf = vec![0u8; READ_BUFFER_BYTES];
    let n = conn.read(&mut buf).await?;
    Ok(n > 0)
}

/// A client that follows the reuse rule: after `idle_s` idle seconds it opens a fresh connection
/// when `idle_s` reaches the reuse limit, and reuses the old one otherwise. Passes when the
/// request on the connection it picks succeeds.
pub async fn assert_fresh_connection_after_idle(stub: &StubProcess, idle_s: u32) -> Result<(), IdleError> {
    let mut first = TcpStream::connect(stub.addr).await.map_err(|_| IdleError::Reused)?;
    exchange(&mut first).await.map_err(|_| IdleError::Reused)?;
    tokio::time::sleep(Duration::from_secs(u64::from(idle_s))).await;
    let mut conn = if idle_s >= CLIENT_IDLE_REUSE_MAX_S {
        TcpStream::connect(stub.addr).await.map_err(|_| IdleError::Reused)?
    } else {
        first
    };
    match exchange(&mut conn).await {
        Ok(true) => Ok(()),
        _ => Err(IdleError::Reused),
    }
}

/// A careless client that always reuses its connection after `idle_s`. Fails with `Reused` when
/// the stub has closed the connection in the meantime.
pub async fn reuse_after_idle(stub: &StubProcess, idle_s: u32) -> Result<(), IdleError> {
    let mut conn = TcpStream::connect(stub.addr).await.map_err(|_| IdleError::Reused)?;
    exchange(&mut conn).await.map_err(|_| IdleError::Reused)?;
    tokio::time::sleep(Duration::from_secs(u64::from(idle_s))).await;
    match exchange(&mut conn).await {
        Ok(true) => Ok(()),
        _ => Err(IdleError::Reused),
    }
}
