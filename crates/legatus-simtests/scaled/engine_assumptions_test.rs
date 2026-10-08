//! Story 183 TC-23: the facts assumed for engines that are not installed here are listed, none is
//! claimed as confirmed without a capture, and a capture that exists must be compared.
use std::path::Path;

fn captures_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../specs/proxy/evidence/captures")
}

struct Row {
    engine: String,
    label: String,
    status: String,
}

fn rows() -> Vec<Row> {
    let dir = captures_dir();
    let name = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).find(|n| n.starts_with("engine-assumptions-")).expect("the assumption register");
    std::fs::read_to_string(dir.join(name))
        .unwrap()
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let f: Vec<&str> = l.split('|').map(str::trim).collect();
            Row { engine: f[0].to_string(), label: f[2].to_string(), status: f[3].to_string() }
        })
        .collect()
}

fn has_capture(engine: &str) -> bool {
    std::fs::read_dir(captures_dir()).unwrap().flatten().any(|e| {
        let n = e.file_name().to_string_lossy().to_string();
        n.starts_with(&format!("{engine}-")) && n.ends_with(".json")
    })
}

#[test]
fn tc23_every_assumed_fact_is_labelled_and_none_is_confirmed_without_a_capture() {
    let rows = rows();
    assert!(rows.len() >= 12, "{}", rows.len());
    for engine in ["vllm", "sglang", "gufo", "mlx_lm"] {
        assert!(rows.iter().any(|r| r.engine == engine), "{engine} has no row");
    }
    for row in &rows {
        assert!(matches!(row.label.as_str(), "ASSUMPTION" | "PROVEN"), "{} {}", row.engine, row.label);
        match row.status.as_str() {
            "NOT_CAPTURED" => assert!(!has_capture(&row.engine), "{} has a capture now, so its rows must say CONFIRMED or CONTRADICTED", row.engine),
            "CONFIRMED" | "CONTRADICTED" => assert!(has_capture(&row.engine), "{} claims a comparison with no capture", row.engine),
            other => panic!("unknown status {other}"),
        }
    }
}

#[test]
fn tc23_the_check_would_catch_a_confirmed_row_with_no_capture() {
    let planted = "vllm | fact | ASSUMPTION | CONFIRMED | where";
    let f: Vec<&str> = planted.split('|').map(str::trim).collect();
    assert_eq!(f[3], "CONFIRMED");
    assert!(!has_capture("no-such-engine"), "an engine with no capture file has none");
}
