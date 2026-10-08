//! Story 127 scans of the real proxy source: no routing module names an engine (TC-04) and no
//! code path controls an engine (TC-20). A planted violation in a temp tree must be reported.
use crate::source_scan::*;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn temp_tree(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("legatus-scan-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn tc04_no_routing_module_of_the_proxy_names_an_engine() {
    let src = repo_root().join("crates/legatus-proxy/src");
    let found = scan_for_engine_names(&src, ENGINE_NAME_ALLOWED_DIRS, ENGINE_NAMES);
    let shown: Vec<String> = found.iter().map(|v| format!("{}:{} {}", v.file.display(), v.line, v.pattern)).collect();
    assert!(found.is_empty(), "engine names outside engine/ and config/: {shown:?}");
}

#[test]
fn tc04_a_planted_engine_name_is_reported_with_file_and_line_and_an_allowed_dir_is_not() {
    let src = temp_tree("names");
    std::fs::create_dir_all(src.join("protocol")).unwrap();
    std::fs::create_dir_all(src.join("engine")).unwrap();
    std::fs::create_dir_all(src.join("config")).unwrap();
    std::fs::write(src.join("protocol/route.rs"), "fn ok() {}\nlet engine = \"Ollama\";\n").unwrap();
    std::fs::write(src.join("engine/mod.rs"), "// llama-server and vllm live here\n").unwrap();
    std::fs::write(src.join("config/node.rs"), "const E: &str = \"sglang\";\n").unwrap();
    let found = scan_for_engine_names(&src, ENGINE_NAME_ALLOWED_DIRS, ENGINE_NAMES);
    assert_eq!(found.len(), 1);
    assert!(found[0].file.ends_with("protocol/route.rs"));
    assert_eq!((found[0].line, found[0].pattern.as_str()), (2, "llama"), "Ollama contains llama, which is listed first");
}

#[test]
fn tc20_the_proxy_source_holds_no_engine_control_pattern() {
    let found = scan_for_banned_patterns(&repo_root(), SCANNED_DIRS, BANNED_PATTERNS);
    let shown: Vec<String> = found.iter().map(|v| format!("{}:{} {}", v.file.display(), v.line, v.pattern)).collect();
    assert!(found.is_empty(), "engine control found: {shown:?}");
}

#[test]
fn tc20_a_planted_violation_is_reported_with_file_line_and_pattern() {
    let root = temp_tree("control");
    let dir = root.join("crates/legatus-proxy/src");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("bad.rs"), "fn a() {}\nfn b() { std::process::Command::new(\"x\"); }\n// reads /api/ps only\n").unwrap();
    std::fs::write(dir.join("unload.rs"), "fn a() {}\nfn b() {}\nconst P: &str = \"/models/unload\";\n").unwrap();
    std::fs::write(dir.join("good.rs"), "let path = \"/api/ps\";\nlet models = \"/v1/models\";\n").unwrap();
    let found = scan_for_banned_patterns(&root, &["crates/legatus-proxy/src"], BANNED_PATTERNS);
    assert_eq!(found.len(), 2, "{found:?}");
    let bad = found.iter().find(|v| v.file.ends_with("bad.rs")).unwrap();
    assert_eq!((bad.line, bad.pattern.as_str()), (2, "std::process::Command"));
    let unload = found.iter().find(|v| v.file.ends_with("unload.rs")).unwrap();
    assert_eq!((unload.line, unload.pattern.as_str()), (3, "/models/unload"));
    for pattern in ["tokio::process::Command", "std::env::set_var", "/api/pull", "/api/delete", "/api/create", "/models/load", "/models/unload", "docker", "launchctl", "systemctl"] {
        assert!(BANNED_PATTERNS.contains(&pattern), "{pattern}");
    }
}
