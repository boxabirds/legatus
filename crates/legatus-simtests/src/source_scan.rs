//! Source scans for the proxy: no routing module names an engine, and no code path controls an
//! engine (story 127). The scans read real files and belong to the scaled tier.
use std::path::{Path, PathBuf};

/// Directories of the proxy source where engine names are allowed.
pub const ENGINE_NAME_ALLOWED_DIRS: &[&str] = &["engine", "config"];
/// Engine names that routing code must not hold (matched without regard to case).
pub const ENGINE_NAMES: &[&str] = &["llama", "ollama", "mlx", "vllm", "sglang", "gufo"];
/// Code paths that would make the proxy manage an engine (PROPOSED list). Reading `/api/ps` and
/// the model list is allowed because it only reads.
pub const BANNED_PATTERNS: &[&str] = &[
    "std::process::Command",
    "tokio::process::Command",
    "std::env::set_var",
    "/api/pull",
    "/api/delete",
    "/api/create",
    "/models/load",
    "/models/unload",
    "docker",
    "launchctl",
    "systemctl",
];
/// Source folders scanned for engine control, relative to the repository root. The test kit is out.
pub const SCANNED_DIRS: &[&str] = &["crates/legatus-proxy/src", "crates/legatus-node-agent/src"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Violation {
    pub file: PathBuf,
    pub line: usize,
    pub pattern: String,
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn scan_files(files: &[PathBuf], patterns: &[&str], ignore_case: bool) -> Vec<Violation> {
    let mut found = Vec::new();
    for file in files {
        let Ok(text) = std::fs::read_to_string(file) else { continue };
        for (index, line) in text.lines().enumerate() {
            let line_text = if ignore_case { line.to_lowercase() } else { line.to_string() };
            if let Some(pattern) = patterns.iter().find(|p| line_text.contains(**p)) {
                found.push(Violation { file: file.clone(), line: index + 1, pattern: (*pattern).to_string() });
            }
        }
    }
    found
}

/// Every banned pattern in the Rust sources under each of `dirs` (relative to `root`).
pub fn scan_for_banned_patterns(root: &Path, dirs: &[&str], patterns: &[&str]) -> Vec<Violation> {
    let mut files = Vec::new();
    for dir in dirs {
        rust_files(&root.join(dir), &mut files);
    }
    scan_files(&files, patterns, false)
}

/// Every engine name in a proxy source folder outside the allowed folders. `src` is the folder
/// that holds the module folders (`engine`, `config`, `protocol`, ...) and `main.rs`.
pub fn scan_for_engine_names(src: &Path, allowed: &[&str], names: &[&str]) -> Vec<Violation> {
    let mut files = Vec::new();
    let Ok(entries) = std::fs::read_dir(src) else { return Vec::new() };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if path.is_dir() {
            if !allowed.contains(&name.as_str()) {
                rust_files(&path, &mut files);
            }
        } else if path.extension().is_some_and(|e| e == "rs") {
            files.push(path);
        }
    }
    scan_files(&files, names, true)
}
