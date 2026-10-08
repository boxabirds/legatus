//! Story 151 TC-10: the llama-server adapter source never asks for the slots status page, which
//! wakes a sleeping engine.
use std::path::Path;

#[test]
fn tc10_the_adapter_source_holds_no_slots_page_path() {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("../legatus-proxy/src/engine/llama_server.rs");
    let text = std::fs::read_to_string(&file).expect("the adapter source");
    let page = format!("/{}", "slots");
    let hits: Vec<usize> = text.lines().enumerate().filter(|(_, l)| l.contains(&page)).map(|(i, _)| i + 1).collect();
    assert!(hits.is_empty(), "the slots page is named at lines {hits:?}");
    // The whole engine folder is held to the same rule.
    let dir = file.parent().unwrap();
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let source = std::fs::read_to_string(entry.path()).unwrap_or_default();
        assert!(!source.contains(&page), "{} names the slots page", entry.path().display());
    }
}

#[test]
fn tc10_the_check_would_catch_a_planted_path() {
    let page = format!("/{}", "slots");
    let planted = format!("let url = format!(\"{{base}}{page}\");");
    assert!(planted.contains(&page));
}
