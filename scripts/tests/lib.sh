#!/usr/bin/env bash
# Tiny assert helpers for the script tests of story 128.
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PASSED=0; FAILED=0
ok()   { PASSED=$((PASSED + 1)); echo "ok   $1"; }
bad()  { FAILED=$((FAILED + 1)); echo "FAIL $1"; }
# expect_exit NAME CODE COMMAND...   (output kept in $OUT)
expect_exit() {
  local name="$1" want="$2"; shift 2
  OUT=$("$@" 2>&1); local got=$?
  if [ "$got" -eq "$want" ]; then ok "$name"; else bad "$name (exit $got, wanted $want)"; echo "$OUT" | tail -5; fi
}
expect_out() { # NAME PATTERN (against $OUT)
  if echo "$OUT" | grep -qE "$2"; then ok "$1"; else bad "$1 (no match for: $2)"; echo "$OUT" | tail -5; fi
}
reject_out() { # NAME PATTERN
  if echo "$OUT" | grep -qE "$2"; then bad "$1 (unexpected match: $2)"; else ok "$1"; fi
}
finish() { echo "passed $PASSED, failed $FAILED"; [ "$FAILED" -eq 0 ]; }
# fixture_copy DIR: copy the files the scripts read into a temp workspace
fixture_copy() {
  mkdir -p "$1"
  cp -R "$REPO/scripts" "$REPO/crates" "$REPO/Cargo.toml" "$REPO/Cargo.lock" "$REPO/PINS.md" \
        "$REPO/rust-toolchain.toml" "$REPO/clippy.toml" "$REPO/lint-optouts.txt" "$1/"
  rm -rf "$1/crates/"*/target
}
