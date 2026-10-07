//! Fault injection for the stub kit (story 162).
pub mod actions;
pub mod clock;
pub mod idle;
pub mod rules;
pub mod sink;

pub use actions::FaultedTransport;
pub use rules::{RuleEngine, SeededRng};
pub use sink::FaultySink;
