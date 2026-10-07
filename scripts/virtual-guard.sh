#!/usr/bin/env bash
# Virtual guard (story 144). Usage: virtual-guard.sh
# Keeps real sockets, blocking calls and file I/O out of the virtual tier.
# Scanned: crates/legatus-simtests/virtual, crates/legatus-testkit/src/virt, and
# the proxy library without src/main.rs and src/net.
# Exit 0 clean. Exit 1 with name, file and line per hit. Exit 2 unreadable path.
# Env ROOT overrides the repository root (tests).
set -u
ROOT="${ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
FORBIDDEN='TcpListener|TcpStream|UdpSocket|UnixListener|UnixStream|spawn_blocking|tokio::fs|std::fs'
status=0
for dir in crates/legatus-simtests/virtual crates/legatus-testkit/src/virt crates/legatus-proxy/src; do
  [ -d "$ROOT/$dir" ] || { echo "cannot read $ROOT/$dir" >&2; exit 2; }
done
hits=$(grep -rnE --include='*.rs' "$FORBIDDEN" \
  "$ROOT/crates/legatus-simtests/virtual" \
  "$ROOT/crates/legatus-testkit/src/virt" \
  "$ROOT/crates/legatus-proxy/src" \
  | grep -vE "^$ROOT/crates/legatus-proxy/src/(main\.rs|net/)" || true)
if [ -n "$hits" ]; then
  echo "$hits" | while IFS=: read -r file line text; do
    name=$(echo "$text" | grep -oE "$FORBIDDEN" | head -1)
    echo "forbidden $name at ${file#$ROOT/}:$line"
  done
  status=1
fi
[ "$status" -eq 0 ] && echo "virtual guard clean"
exit "$status"
