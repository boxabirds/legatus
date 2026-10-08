//! Story 174 TC-15: no code of the proxy writes a keep-alive value into a request.
use crate::source_scan::scan_for_banned_patterns;
use std::path::Path;

/// A string literal that names the request field. The registry setting `keep_alive_s` and the
/// flag name in `config/` are different words and are not matched.
const KEEP_ALIVE_FIELD: &str = "\"keep_alive\"";

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn tc15_the_proxy_source_names_no_keep_alive_request_field() {
    let found = scan_for_banned_patterns(&root(), &["crates/legatus-proxy/src"], &[KEEP_ALIVE_FIELD]);
    let shown: Vec<String> = found.iter().map(|v| format!("{}:{}", v.file.display(), v.line)).collect();
    assert!(found.is_empty(), "keep_alive is written at {shown:?}");
}

#[test]
fn tc15_a_planted_write_is_found() {
    let dir = std::env::temp_dir().join(format!("legatus-keepalive-{}", std::process::id()));
    let src = dir.join("crates/legatus-proxy/src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("bad.rs"), "fn a() {}\nlet f = \"keep_alive\";\n").unwrap();
    let found = scan_for_banned_patterns(&dir, &["crates/legatus-proxy/src"], &[KEEP_ALIVE_FIELD]);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].line, 2);
}
