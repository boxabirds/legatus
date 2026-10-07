//! TC-16 of story 121: `UpstreamTransport::send` has exactly one call site, in `upstream/send.rs`.
use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Lines of the proxy library that call `send` on a transport.
fn send_calls(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    rust_files(root, &mut files);
    let mut calls = Vec::new();
    for path in files {
        let text = std::fs::read_to_string(&path).unwrap();
        for (n, line) in text.lines().enumerate() {
            let code = line.split("//").next().unwrap_or_default();
            // A call on something named transport, or the trait function by its path.
            let on_transport = code.split(".send(").next().is_some_and(|before| code.contains(".send(") && before.to_ascii_lowercase().ends_with("transport"));
            if on_transport || code.contains("Transport::send(") {
                calls.push(format!("{}:{}", path.strip_prefix(root).unwrap().display(), n + 1));
            }
        }
    }
    calls
}

#[test]
fn tc16_the_only_call_of_transport_send_is_in_upstream_send_rs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../legatus-proxy/src");
    let calls = send_calls(&root);
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert!(calls[0].starts_with("upstream/send.rs:"), "{calls:?}");
}

#[test]
fn tc16_the_scan_would_catch_a_second_call_site() {
    let dir = std::env::temp_dir().join(format!("legatus-scan-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("protocol")).unwrap();
    std::fs::write(dir.join("protocol/other.rs"), "async fn f(deps: &D) { deps.seams.transport.send(req).await; }\n").unwrap();
    assert_eq!(send_calls(&dir), vec!["protocol/other.rs:1".to_string()]);
}
