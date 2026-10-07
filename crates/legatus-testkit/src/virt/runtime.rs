//! The runtime every virtual test runs on: one thread, time enabled, starts paused.
pub fn virtual_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .start_paused(true)
        .build()
        .expect("build the paused runtime")
}
