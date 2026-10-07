//! The one executable of the proxy, named `legatus`.
#![deny(clippy::disallowed_types, clippy::disallowed_methods)]

fn main() -> std::process::ExitCode {
    legatus_proxy::lifecycle::start::main_entry(std::env::args().collect())
}
