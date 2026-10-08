//! The secret of the keyed hash for this process. PROPOSED: story 170 keeps one persistent secret
//! file; until then each start draws a new one from the system, so keys do not survive a restart.
use crate::key::hasher::{KeySecret, SECRET_LEN_BYTES};
use std::io::Read;

const SYSTEM_RANDOM: &str = "/dev/urandom";

/// Thirty-two bytes from the system's random source. If it cannot be read the secret is the
/// process id and the clock, hashed: weaker, but it still differs from start to start.
pub fn process_secret() -> KeySecret {
    let mut bytes = [0u8; SECRET_LEN_BYTES];
    let read = std::fs::File::open(SYSTEM_RANDOM).and_then(|mut f| f.read_exact(&mut bytes));
    if read.is_err() {
        use sha2::{Digest, Sha256};
        use crate::time::WallClock;
        let now = crate::net::wall_system::SystemWallClock.now().unix_ms;
        let mut hasher = Sha256::new();
        hasher.update(std::process::id().to_le_bytes());
        hasher.update(now.to_le_bytes());
        bytes.copy_from_slice(&hasher.finalize());
    }
    KeySecret::from_bytes(bytes)
}
