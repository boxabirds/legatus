//! The conversation key and the keyed hash that makes it (contract C30). A key is a fixed-length
//! digest, so a header of any length gives a key of one length and the value cannot be read back.
use hmac::{Hmac, Mac};
use sha2::Sha256;

pub use crate::key::{ConversationKey, KeyClass};

/// Length of a conversation key in bytes (the digest is cut to this).
pub const KEY_LEN_BYTES: usize = 16;
/// Length of the secret in bytes (spec 03 section 4.4).
pub const SECRET_LEN_BYTES: usize = 32;
/// The version of the key rule; story 170 builds its canonical text on it.
pub const KEY_VERSION_TAG: &str = "v1";
/// Joins the two values of a header pair; a byte that does not occur in a header value.
pub const FIELD_SEPARATOR: u8 = 0x1F;

/// The secret of the keyed hash. Its Debug form shows nothing.
pub struct KeySecret([u8; SECRET_LEN_BYTES]);

impl KeySecret {
    /// Story 170 gives the bytes from its secret file; tests give fixed bytes.
    pub fn from_bytes(bytes: [u8; SECRET_LEN_BYTES]) -> KeySecret {
        KeySecret(bytes)
    }
}

impl std::fmt::Debug for KeySecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("KeySecret(..)")
    }
}

type HmacSha256 = Hmac<Sha256>;

/// HMAC-SHA-256 over the domain label, a separator and the value, cut to `KEY_LEN_BYTES`.
pub struct KeyHasher {
    mac: HmacSha256,
}

impl KeyHasher {
    pub fn new(secret: &KeySecret) -> KeyHasher {
        // A 32-byte key is always accepted by HMAC.
        let mac = HmacSha256::new_from_slice(&secret.0).unwrap_or_else(|_| unreachable!("HMAC takes a key of any length"));
        KeyHasher { mac }
    }

    pub fn hash(&self, domain: &[u8], data: &[u8]) -> ConversationKey {
        let mut mac = self.mac.clone();
        mac.update(KEY_VERSION_TAG.as_bytes());
        mac.update(&[FIELD_SEPARATOR]);
        mac.update(domain);
        mac.update(&[FIELD_SEPARATOR]);
        mac.update(data);
        let digest = mac.finalize().into_bytes();
        let mut key = [0u8; KEY_LEN_BYTES];
        key.copy_from_slice(&digest[..KEY_LEN_BYTES]);
        ConversationKey(key)
    }
}
