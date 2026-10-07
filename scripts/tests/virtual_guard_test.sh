#!/usr/bin/env bash
# TC-21, TC-22 and TC-23 of story 144: the virtual guard.
source "$(dirname "$0")/lib.sh"
GUARD="scripts/virtual-guard.sh"
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
cd "$REPO"

mk() { rm -rf "$tmp/ws"; fixture_copy "$tmp/ws"; }
check() { # name, forbidden snippet
  mk
  printf 'pub fn f() { let _ = %s; }\n' "$2" > "$tmp/ws/crates/legatus-simtests/virtual/bad.rs"
  ROOT="$tmp/ws" expect_exit "TC-21 $1 exits 1" 1 "$tmp/ws/$GUARD"
  expect_out "TC-21 $1 is named with file and line" "forbidden .*at crates/legatus-simtests/virtual/bad.rs:1"
}
check "TcpStream" "std::net::TcpStream::connect"
check "TcpListener" "std::net::TcpListener::bind"
check "UdpSocket" "std::net::UdpSocket::bind"
check "UnixListener" "tokio::net::UnixListener::bind"
check "spawn_blocking" "tokio::task::spawn_blocking"
check "tokio::fs" "tokio::fs::read"
check "std::fs" "std::fs::read"

mk; mv "$tmp/ws/crates/legatus-testkit/src/virt" "$tmp/ws/crates/legatus-testkit/src/virt.gone"
ROOT="$tmp/ws" expect_exit "TC-21 an unreadable path exits 2" 2 "$tmp/ws/$GUARD"

mk
expect_exit "TC-22 a clean tree exits 0" 0 env ROOT="$tmp/ws" "$tmp/ws/$GUARD"
printf 'fn x() { let _ = std::net::TcpStream::connect("a"); let _ = std::fs::read("b"); }\n' > "$tmp/ws/crates/legatus-proxy/src/net/extra.rs"
expect_exit "TC-22 src/net is ignored (no false hit)" 0 env ROOT="$tmp/ws" "$tmp/ws/$GUARD"
printf 'fn main() { let _ = std::fs::read("b"); }\n' >> "$tmp/ws/crates/legatus-proxy/src/main.rs"
expect_exit "TC-22 src/main.rs is ignored (no false hit)" 0 env ROOT="$tmp/ws" "$tmp/ws/$GUARD"

expect_exit "TC-23 the real workspace passes the guard" 0 "$REPO/$GUARD"
expect_exit "TC-23 the virtual tier then runs" 0 cargo test -p legatus-simtests --features tier-virtual
finish
