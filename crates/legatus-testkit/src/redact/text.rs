//! Fillers for text values: the same number of bytes, drawn from a keyed hash.
use crate::redact::secret::RunKey;
use hmac::Mac;

/// How many times a filler is redrawn when another text already has it (a one-byte text has few).
pub const MAX_REDRAWS: u32 = 200;
/// The characters a filler is made of: printable ASCII, one byte each, so the byte length of the
/// original (multibyte or not) is kept exactly.
const FILLER_ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

/// A filler with exactly `text.len()` bytes. The same text and attempt give the same filler in one
/// run; another run key gives another filler.
pub fn filler_for(run: &RunKey, text: &str, attempt: u32) -> String {
    let mut out = String::with_capacity(text.len());
    let mut block = 0u32;
    while out.len() < text.len() {
        let mut mac = run.mac("filler", text.as_bytes());
        mac.update(&attempt.to_le_bytes());
        mac.update(&block.to_le_bytes());
        let digest = mac.finalize().into_bytes();
        for byte in digest.iter() {
            if out.len() == text.len() {
                break;
            }
            out.push(char::from(FILLER_ALPHABET[usize::from(*byte) % FILLER_ALPHABET.len()]));
        }
        block += 1;
    }
    out
}
