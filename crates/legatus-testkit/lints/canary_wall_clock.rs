// Canary: must fail the lint (wall clock type).
pub fn now() -> std::time::SystemTime { std::time::SystemTime::now() }
