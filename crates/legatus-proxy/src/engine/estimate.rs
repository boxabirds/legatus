//! The size of a prompt in tokens, estimated from the length of the request body only. No
//! tokenizer: constant time, and it over-counts JSON overhead (conservative).
//!
//! Measured on Ollama 0.40.0 with qwen3:1.7b (specs/proxy/evidence/captures/ollama-estimator-*.txt):
//! at 3 bytes per token the estimate was 1.31 to 2.40 times the engine's count and never below it.

/// The setting `ollama_bytes_per_token` (story 165) holds the divisor; 3 is the PROPOSED default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TokenEstimator {
    pub bytes_per_token: u32,
}

impl TokenEstimator {
    /// `ceil(body_len / bytes_per_token)`, saturating at `u32::MAX`. A divisor of 0 counts as 1.
    pub fn estimate(&self, body_len: usize) -> u32 {
        let divisor = u64::from(self.bytes_per_token.max(1));
        let tokens = (body_len as u64).div_ceil(divisor);
        u32::try_from(tokens).unwrap_or(u32::MAX)
    }
}
