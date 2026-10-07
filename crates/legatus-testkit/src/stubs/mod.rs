//! Stub kit (contract C03, story 162): scenario format, three engine stubs, fault wrapper, two run
//! modes (in memory and as a process).
pub mod common;
pub mod llama;
pub mod mlx;
pub mod ollama;
pub mod process;
pub mod run;
pub mod scenario;
pub mod speed;

use crate::faults::FaultedTransport;
use legatus_proxy::upstream::transport::UpstreamTransport;
use scenario::{FaultRule, StubKind, StubSpec};
use std::sync::Arc;

pub use process::{start_process, StubProcess};

/// Build the engine for a spec, without faults.
pub fn engine_for(spec: &StubSpec) -> Arc<dyn UpstreamTransport> {
    match spec.kind {
        StubKind::SerialOllama => Arc::new(ollama::SerialOllamaStub::new(spec.clone())),
        StubKind::MultiSlotLlama => Arc::new(llama::MultiSlotLlamaStub::new(spec.clone())),
        StubKind::BatchingMlx => Arc::new(mlx::BatchingMlxStub::new(spec.clone())),
    }
}

/// Start a stub in memory, wrapped with the fault rules that name it.
pub fn start_in_memory(spec: &StubSpec, rules: &[FaultRule]) -> Arc<dyn UpstreamTransport> {
    let mine: Vec<FaultRule> = rules.iter().filter(|r| r.stub == spec.name).cloned().collect();
    Arc::new(FaultedTransport::new(engine_for(spec), mine))
}
