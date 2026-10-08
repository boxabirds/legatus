//! Keyed tokens for credentials, cookies, sessions and user names. Equal values give equal tokens
//! inside one run; the key is gone at the end of the run, so a second run gives other tokens.
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

const KEY_BYTES: usize = 32;
const SPLITMIX_INCREMENT: u64 = 0x9E37_79B9_7F4A_7C15;
const SPLITMIX_MIX_ONE: u64 = 0xBF58_476D_1CE4_E5B9;
const SPLITMIX_MIX_TWO: u64 = 0x94D0_49BB_1331_11EB;
/// How many bytes of the keyed hash a token shows (as hex digits twice this).
const TOKEN_HASH_BYTES: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecretKind {
    Credential,
    Cookie,
    Session,
    User,
}

impl SecretKind {
    fn label(&self) -> &'static str {
        match self {
            SecretKind::Credential => "cred",
            SecretKind::Cookie => "cookie",
            SecretKind::Session => "session",
            SecretKind::User => "user",
        }
    }
}

/// The key of one run. Not `Clone`, not `Serialize`, overwritten when dropped.
pub struct RunKey([u8; KEY_BYTES]);

impl RunKey {
    /// A key derived from a seed (tests use fixed seeds; the capture tool draws one from the time).
    pub fn from_seed(seed: u64) -> RunKey {
        let mut state = seed;
        let mut bytes = [0u8; KEY_BYTES];
        for chunk in bytes.chunks_mut(8) {
            state = state.wrapping_add(SPLITMIX_INCREMENT);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(SPLITMIX_MIX_ONE);
            z = (z ^ (z >> 27)).wrapping_mul(SPLITMIX_MIX_TWO);
            z ^= z >> 31;
            chunk.copy_from_slice(&z.to_le_bytes());
        }
        RunKey(bytes)
    }

    pub(crate) fn mac(&self, label: &str, data: &[u8]) -> HmacSha256 {
        let mut mac = HmacSha256::new_from_slice(&self.0).expect("HMAC accepts a key of any length");
        mac.update(label.as_bytes());
        mac.update(&[0]);
        mac.update(data);
        mac
    }

    /// True when every byte is zero (the key was discarded). For tests of the discard.
    pub fn is_zeroed(&self) -> bool {
        self.0.iter().all(|b| *b == 0)
    }
}

impl RunKey {
    /// Overwrite the key. Called when the key is dropped; public so a test can see the effect.
    pub fn discard(&mut self) {
        // black_box keeps the optimiser from dropping the overwrite of memory that is about to be freed.
        self.0.fill(0);
        std::hint::black_box(&self.0);
    }
}

impl Drop for RunKey {
    fn drop(&mut self) {
        self.discard();
    }
}

impl std::fmt::Debug for RunKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RunKey(..)")
    }
}

/// A token for a secret value: its kind and a keyed hash, equal in equal out.
pub fn token_for(run: &RunKey, kind: SecretKind, value: &str) -> String {
    let digest = run.mac(kind.label(), value.as_bytes()).finalize().into_bytes();
    let hex: String = digest.iter().take(TOKEN_HASH_BYTES).map(|b| format!("{b:02x}")).collect();
    format!("tok-{}-{hex}", kind.label())
}
