// Canary: must fail the lint (unseeded randomness).
pub fn roll() -> u32 { rand::random() }
