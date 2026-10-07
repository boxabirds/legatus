//! Builds the `legatus` binary once per test process and gives its path. Every scaled test that
//! starts the binary uses this, so that parallel tests never run it while another one relinks it.
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

const BIN: &str = "legatus";

/// Always runs cargo once: a stale binary would test old code. The `test-hooks` feature adds the
/// warning check that one test switches on with an environment variable; it is inert otherwise.
pub fn legatus_binary() -> PathBuf {
    static BUILT: OnceLock<PathBuf> = OnceLock::new();
    BUILT
        .get_or_init(|| {
            let status = Command::new("cargo").args(["build", "-p", "legatus-proxy", "--bin", BIN, "--features", "test-hooks"]).status().expect("run cargo build");
            assert!(status.success(), "building {BIN} failed");
            let exe = std::env::current_exe().expect("current exe");
            exe.parent().and_then(|p| p.parent()).expect("target dir").join(BIN)
        })
        .clone()
}

/// The line the binary prints when it serves with the registry loaded.
pub const LISTENING_MARK: &str = "legatus listening on";

/// Read the error output of the child until the listening line (or the end of the output) and
/// return everything read. The port opens before the registry is loaded, so a test that wants the
/// load messages must wait for this line, not for the port.
pub fn read_until_listening(child: &mut std::process::Child) -> String {
    use std::io::BufRead;
    let Some(stderr) = child.stderr.take() else { return String::new() };
    let mut text = String::new();
    for line in std::io::BufReader::new(stderr).lines().map_while(Result::ok) {
        text.push_str(&line);
        text.push('\n');
        if line.contains(LISTENING_MARK) {
            break;
        }
    }
    text
}
