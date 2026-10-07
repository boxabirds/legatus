// Canary: must fail the lint in the proxy crate (process spawn).
pub fn spawn() { let _ = std::process::Command::new("true"); }
