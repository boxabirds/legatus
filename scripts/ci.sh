#!/usr/bin/env bash
# One command to build, lint and test everything (story 128).
# Usage: ci.sh. Exit 0 pass, 1 fail. Stops at the first failing step.
# Steps: toolchain, pins, build, clippy (with the lint canary), virtual-guard,
# unit tests, virtual tier, capture-redact, lint-ste, scaled tier.
# A step whose script belongs to a later story (virtual-guard.sh story 144,
# capture-redact.sh story 120, lint-ste.sh no owner yet) is skipped with a
# warning while the script is missing.
set -u
ROOT="${ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
cd "$ROOT" || exit 1
total_start=$SECONDS

step() { # name, command...
  local name="$1"; shift
  local start=$SECONDS
  if "$@" > "${TMPDIR:-/tmp}/ci-step.log" 2>&1; then
    echo "pass  $name ($((SECONDS - start)) s)"
  else
    echo "FAIL  $name ($((SECONDS - start)) s)"
    tail -n 30 "${TMPDIR:-/tmp}/ci-step.log"
    echo "ci failed at step: $name"
    exit 1
  fi
}

optional() { # name, script
  local name="$1" script="$2"
  if [ -x "scripts/$script" ]; then
    step "$name" "scripts/$script"
  else
    echo "skip  $name (scripts/$script not present yet)"
  fi
}

toolchain() {
  local pinned installed
  pinned=$(sed -nE 's/^channel *= *"([^"]+)".*/\1/p' rust-toolchain.toml)
  installed=$(rustc --version | awk '{print $2}')
  [ "$pinned" = "$installed" ] || { echo "toolchain: pinned $pinned, installed $installed"; return 1; }
}
lint() { cargo clippy --workspace --all-targets -- -D warnings && scripts/lint-canary.sh; }

[ -d target ] || echo "no build output yet: building everything first"
step toolchain toolchain
step pins scripts/pins.sh check
step build cargo build --workspace
step clippy lint
optional virtual-guard virtual-guard.sh
step unit-tests cargo test --workspace
step virtual-tier scripts/test.sh virtual
optional capture-redact capture-redact.sh
optional lint-ste lint-ste.sh
step scaled-tier scripts/test.sh scaled
echo "ci passed in $((SECONDS - total_start)) s"
