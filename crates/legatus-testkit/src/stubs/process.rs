//! Process mode: the same engine and fault rules served over real sockets by `legatus-stub`.
use super::scenario::{FaultAction, FaultRule, Limit, Scenario, StubSpec};
use std::io::{BufRead, BufReader, Write};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

const STUB_BIN: &str = "legatus-stub";
const ADDR_PREFIX: &str = "addr=";

/// A running stub process; killed on drop.
pub struct StubProcess {
    pub addr: SocketAddr,
    child: Option<Child>,
}

impl Drop for StubProcess {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn binary_path() -> PathBuf {
    let exe = std::env::current_exe().expect("current exe");
    let debug_dir = exe.parent().and_then(|p| p.parent()).expect("target dir").to_path_buf();
    debug_dir.join(STUB_BIN)
}

fn ensure_built() -> PathBuf {
    let path = binary_path();
    if !path.exists() {
        let status = Command::new("cargo")
            .args(["build", "-p", "legatus-testkit", "--bin", STUB_BIN])
            .status()
            .expect("run cargo build for the stub");
        assert!(status.success(), "building {STUB_BIN} failed");
    }
    path
}

/// A refuse-always rule means nobody listens: reserve a port and close it.
fn refuses_always(spec: &StubSpec, rules: &[FaultRule]) -> bool {
    rules.iter().any(|r| r.stub == spec.name && r.action == FaultAction::Refuse && r.limit == Limit::Always && r.when.path.is_none())
}

pub async fn start_process(spec: &StubSpec, rules: &[FaultRule]) -> StubProcess {
    if refuses_always(spec, rules) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("reserve a port");
        let addr = listener.local_addr().expect("reserved addr");
        drop(listener);
        return StubProcess { addr, child: None };
    }
    let bin = ensure_built();
    let text = Scenario::render_stub(spec, rules);
    let mut child = Command::new(bin).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().expect("spawn the stub");
    child.stdin.take().expect("stub stdin").write_all(text.as_bytes()).expect("send the scenario");
    let stdout = child.stdout.take().expect("stub stdout");
    let line = tokio::task::spawn_blocking(move || {
        let mut line = String::new();
        BufReader::new(stdout).read_line(&mut line).expect("read the stub address");
        line
    })
    .await
    .expect("join the reader");
    let addr: SocketAddr = line.trim().strip_prefix(ADDR_PREFIX).expect("address line").parse().expect("socket address");
    StubProcess { addr, child: Some(child) }
}
