//! Story 120 scaled tests: the redact binary and its exit codes, the commit gate script and the
//! create-new file rule.
use legatus_proxy::time::{WallClock, WallTime};
use legatus_proxy::CHAT_COMPLETIONS_PATH;
use legatus_testkit::capture::Capture;
use legatus_testkit::redact::write::write_capture;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::Command;

const EXIT_LEAK: i32 = 1;
const EXIT_USAGE: i32 = 2;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn capture_bin() -> PathBuf {
    let status = Command::new(env!("CARGO")).args(["build", "-q", "-p", "legatus-testkit", "--bin", "legatus-capture"]).current_dir(root()).status().unwrap();
    assert!(status.success());
    root().join("target/debug/legatus-capture")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("legatus-capture-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn raw_json(text: &str) -> String {
    json!({
        "schema": 1, "harness": "pi", "harness_version": "1.0.3", "captured_at": "2026-10-08T00:00:00Z",
        "requests": [{"method": "POST", "path": CHAT_COMPLETIONS_PATH,
            "headers": [["authorization", "Bearer sk-test-0000"], ["x-session-affinity", "SECRET-SESSION-123"]],
            "body": {"model": "local-coder", "messages": [{"role": "user", "content": text}]}, "response_status": 200}]
    })
    .to_string()
}

fn run(bin: &Path, raw: &Path, out: &Path) -> (i32, String, String) {
    let o = Command::new(bin).args(["redact", raw.to_str().unwrap(), out.to_str().unwrap(), "pi-run"]).output().unwrap();
    (o.status.code().unwrap(), String::from_utf8_lossy(&o.stdout).into(), String::from_utf8_lossy(&o.stderr).into())
}

fn files_in(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into()).collect();
    v.sort();
    v
}

#[test]
fn tc07_a_clean_redaction_writes_a_stamped_file_and_hides_every_planted_marker() {
    let (dir, out) = (scratch("clean"), scratch("clean-out"));
    let raw = dir.join("raw.json");
    std::fs::write(&raw, raw_json("CANARY-PROMPT-7f3a tell me about the repository")).unwrap();
    let (code, stdout, _) = run(&capture_bin(), &raw, &out);
    assert_eq!(code, 0);
    let written = PathBuf::from(stdout.trim());
    let text = std::fs::read_to_string(&written).unwrap();
    assert!(text.contains("leak-scan-v1:16"));
    for marker in ["CANARY-PROMPT-7f3a", "SECRET-SESSION-123", "sk-test-0000"] {
        assert!(!text.contains(marker), "{marker}");
    }
    assert!(written.file_name().unwrap().to_string_lossy().starts_with("pi-run-"));
}

#[test]
fn tc07_a_raw_file_that_is_not_utf8_exits_2_and_writes_nothing() {
    let (dir, out) = (scratch("bad"), scratch("bad-out"));
    let raw = dir.join("raw.json");
    std::fs::write(&raw, [0xff, 0xfe, b'{']).unwrap();
    let (code, _, stderr) = run(&capture_bin(), &raw, &out);
    assert_eq!(code, EXIT_USAGE);
    assert!(files_in(&out).is_empty());
    assert!(!stderr.contains("CANARY"));
    let o = Command::new(capture_bin()).output().unwrap();
    assert_eq!(o.status.code(), Some(EXIT_USAGE), "no arguments is a usage error");
    let _ = EXIT_LEAK;
}

fn gate(dir: &Path) -> (i32, String) {
    let o = Command::new("bash").arg(root().join("scripts/capture-redact.sh")).arg("check").arg(dir).output().unwrap();
    (o.status.code().unwrap(), String::from_utf8_lossy(&o.stdout).into())
}

#[test]
fn tc08_the_gate_refuses_an_unstamped_file_by_name_and_passes_a_stamped_one() {
    let dir = scratch("gate");
    assert_eq!(gate(&dir).0, 0, "an empty folder passes");
    std::fs::write(dir.join("good.json"), "{\"scan_stamp\": \"leak-scan-v1:16\", \"requests\": []}").unwrap();
    assert_eq!(gate(&dir).0, 0);
    std::fs::write(dir.join("bad.json"), "{\"requests\": [], \"note\": \"CANARY-PROMPT-7f3a\"}").unwrap();
    let (code, out) = gate(&dir);
    assert_eq!(code, 1);
    assert!(out.contains("bad.json") && !out.contains("CANARY") && !out.contains("good.json"), "{out}");
    assert_eq!(gate(&dir.join("missing")).0, 0, "no folder means nothing to check");
}

/// A clock that does not move, so two writes get the same name.
struct FixedWall;

impl WallClock for FixedWall {
    fn now(&self) -> WallTime {
        WallTime { unix_ms: FIXED_UNIX_MS }
    }
}

const FIXED_UNIX_MS: i64 = 1_791_000_000_000;

#[test]
fn tc10_an_existing_file_is_refused_and_stays_byte_identical() {
    let out = scratch("exists");
    let wall = FixedWall;
    let mut capture = Capture::new("pi", "1.0.3", "t", vec![]);
    capture.scan_stamp = Some("leak-scan-v1:16".to_string());
    let first = write_capture(&out, "pi-run", &capture, &wall).unwrap();
    let before = std::fs::read(&first).unwrap();
    let again = write_capture(&out, "pi-run", &capture, &wall);
    assert!(again.is_err(), "the same millisecond gives the same name, and create-new refuses it");
    assert_eq!(std::fs::read(&first).unwrap(), before);
    assert_eq!(files_in(&out).len(), 1);
}

