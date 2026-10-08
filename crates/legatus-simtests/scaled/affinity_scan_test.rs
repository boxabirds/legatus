//! Story 143 TC-14: the table writes no file and reads none. The source of the affinity module
//! names no file or socket API, and the planted control proves the scan can fail.
use crate::source_scan::scan_for_banned_patterns;
use std::path::Path;

const FILE_APIS: &[&str] = &["std::fs", "tokio::fs", "File::", "OpenOptions", "TcpStream", "TcpListener"];
const AFFINITY_DIR: &str = "crates/legatus-proxy/src/affinity";

#[test]
fn tc14_the_affinity_source_names_no_file_api() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let found = scan_for_banned_patterns(&root, &[AFFINITY_DIR], FILE_APIS);
    let shown: Vec<String> = found.iter().map(|v| format!("{}:{} {}", v.file.display(), v.line, v.pattern)).collect();
    assert!(found.is_empty(), "{shown:?}");
}

#[test]
fn tc14_a_planted_file_write_is_found() {
    let dir = std::env::temp_dir().join(format!("legatus-affinity-scan-{}", std::process::id()));
    let src = dir.join(AFFINITY_DIR);
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("bad.rs"), "fn a() {}\nfn b() { let _ = std::fs::write(\"x\", \"y\"); }\n").unwrap();
    let found = scan_for_banned_patterns(&dir, &[AFFINITY_DIR], FILE_APIS);
    assert_eq!((found.len(), found[0].line), (1, 2));
}
