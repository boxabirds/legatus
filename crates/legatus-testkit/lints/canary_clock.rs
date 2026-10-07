// Canary: must fail the lint (monotonic clock type).
pub fn now() -> std::time::Instant { std::time::Instant::now() }
