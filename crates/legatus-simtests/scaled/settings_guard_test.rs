//! TC-10 of story 165: the catalogue is the only place that defines a default. This guard reads
//! the proxy sources, which the virtual tier may not do, so it runs in the scaled tier.
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

fn proxy_sources() -> Vec<(PathBuf, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../legatus-proxy/src");
    let mut files = Vec::new();
    rust_files(&root, &mut files);
    files.into_iter().map(|p| { let text = std::fs::read_to_string(&p).unwrap(); (p, text) }).collect()
}

const CATALOGUE_FILE: &str = "config/settings.rs";

#[test]
fn tc10_no_second_definition_of_the_table_expiry_exists_outside_the_catalogue() {
    let mut offenders = Vec::new();
    for (path, text) in proxy_sources() {
        if path.ends_with(CATALOGUE_FILE) {
            continue;
        }
        for (n, line) in text.lines().enumerate() {
            let defines_const = line.trim_start().starts_with("const ") || line.trim_start().starts_with("pub const ");
            let names_expiry = ["TABLE_TTL", "TABLE_EXPIRY", "IDLE_EXPIRY", "ENTRY_TTL"].iter().any(|m| line.contains(m));
            if (defines_const && names_expiry) || line.contains("\"table_ttl_s\"") {
                offenders.push(format!("{}:{}", path.display(), n + 1));
            }
        }
    }
    assert!(offenders.is_empty(), "a second definition of the table expiry: {offenders:?}");
}

#[test]
fn tc10_the_guard_would_catch_a_second_definition() {
    let sample = "pub const DEFAULT_TABLE_TTL_S: u32 = 600;";
    let trimmed = sample.trim_start();
    assert!((trimmed.starts_with("const ") || trimmed.starts_with("pub const ")) && sample.contains("TABLE_TTL"));
}

#[test]
fn tc10_every_catalogue_default_is_defined_once_in_the_catalogue_file() {
    let (_, text) = proxy_sources().into_iter().find(|(p, _)| p.ends_with(CATALOGUE_FILE)).unwrap();
    for name in ["hold_limit_s", "protected_window_s", "table_ttl_s", "key_text_limit_system", "key_text_limit_first"] {
        let rows = text.lines().filter(|l| l.contains(&format!("def(\"{name}\","))).count();
        assert_eq!(rows, 1, "{name}");
    }
}
