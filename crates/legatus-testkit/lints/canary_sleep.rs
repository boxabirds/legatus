// Canary: must fail the lint (blocking sleep).
pub fn nap() { std::thread::sleep(std::time::Duration::from_millis(1)); }
