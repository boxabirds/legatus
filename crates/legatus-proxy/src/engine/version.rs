//! The declared engine version of a node: tested, untested or unknown, and a change between two
//! declared versions. An untested or unknown version never stops traffic.
use legatus_common::engine::{CalibrationReason, EngineFamily, EngineVersionStatus};

pub const LLAMA_SERVER_TESTED: &[&str] = &["0.5.0", "b11146", "b11459"];
pub const OLLAMA_TESTED: &[&str] = &["0.35.1"];
pub const MLX_LM_TESTED: &[&str] = &["0.32.0"];

/// The versions the notes proved. Every other family has none (PROPOSED), so any declared
/// version of it shows as untested.
pub fn tested_versions(family: EngineFamily) -> &'static [&'static str] {
    match family {
        EngineFamily::LlamaServer => LLAMA_SERVER_TESTED,
        EngineFamily::Ollama => OLLAMA_TESTED,
        EngineFamily::MlxLm => MLX_LM_TESTED,
        _ => &[],
    }
}

/// White space and one leading `v` do not count in a comparison. Empty text is no version.
fn normal(version: &str) -> &str {
    let trimmed = version.trim();
    trimmed.strip_prefix('v').unwrap_or(trimmed)
}

fn known(version: Option<&str>) -> Option<&str> {
    version.map(normal).filter(|v| !v.is_empty())
}

pub fn version_status(family: EngineFamily, declared: Option<&str>) -> EngineVersionStatus {
    match known(declared) {
        None => EngineVersionStatus::Unknown,
        Some(version) if tested_versions(family).contains(&version) => EngineVersionStatus::Tested,
        Some(_) => EngineVersionStatus::Untested { found: declared.map(str::trim).unwrap_or_default().to_string() },
    }
}

/// A reason exists only when both versions are known and differ. First identification at join is
/// the job of the join probe (story 147).
pub fn detect_version_change(previous: Option<&str>, now: Option<&str>) -> Option<CalibrationReason> {
    let (from, to) = (known(previous)?, known(now)?);
    (from != to).then(|| CalibrationReason::EngineVersionChanged { from: from.to_string(), to: to.to_string() })
}
