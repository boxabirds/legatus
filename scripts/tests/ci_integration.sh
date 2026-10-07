#!/usr/bin/env bash
# TC-11, TC-12, TC-13, TC-14, TC-16 and TC-18 of story 128 against the real
# workspace and real cargo. TC-16 (macOS and Linux) is run on each machine.
source "$(dirname "$0")/lib.sh"
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
fixture_copy "$tmp/ws"; cd "$tmp/ws"
export CARGO_TARGET_DIR="$tmp/ws/target"

[ ! -d target ] && ok "TC-11 state before: no build output" || bad "TC-11 build output existed"
expect_exit "TC-11 ci from a clean target folder passes" 0 ./scripts/ci.sh
expect_out "TC-11 prints the first-build notice" "no build output yet"
expect_out "TC-11 final pass result" "ci passed in"
[ -x target/debug/legatus ] && ok "TC-11 state after: build output present" || bad "TC-11 no binary after ci"
order=$(echo "$OUT" | grep -E '^(pass|skip|FAIL)' | awk '{print $2}' | tr '\n' ' ')
[ "$order" = "toolchain pins build clippy virtual-guard unit-tests virtual-tier capture-redact lint-ste scaled-tier " ] && ok "TC-18 steps in order" || bad "TC-18 order '$order'"

# stops at the first failing step: break the pins, later steps must not run
sed -i.bak 's/1\.53\.2/1.53.1/' PINS.md
expect_exit "TC-11 a failing step stops ci" 1 ./scripts/ci.sh
expect_out "TC-11 names the failing step" "ci failed at step: pins"
reject_out "TC-11 later steps did not run" "(pass|FAIL)  build"
mv PINS.md.bak PINS.md

unset LEGATUS_REAL_ENGINES
expect_exit "TC-12 test virtual" 0 ./scripts/test.sh virtual
reject_out "TC-12 no scaled test ran (negative)" "tier scaled"
expect_exit "TC-13 default run" 0 ./scripts/test.sh
reject_out "TC-13 never starts the real tier (negative)" "tier real"
expect_exit "TC-14 test real with no engines" 3 ./scripts/test.sh real
reject_out "TC-14 runs nothing" "Running|running [0-9]+ test"

# TC-16: every pin builds on this machine (macOS and Linux run this same script)
expect_exit "TC-16 pins file passes on $(uname -s)" 0 ./scripts/pins.sh check
expect_exit "TC-16 workspace builds on $(uname -s)" 0 cargo build --workspace
finish
