#!/usr/bin/env bash
# TC-03, TC-04, TC-05 and TC-15 of story 128.
source "$(dirname "$0")/lib.sh"
PINS="$REPO/scripts/pins.sh"
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
cd "$REPO"

expect_exit "TC-03 equal pins exit 0" 0 "$PINS" check

sed 's/1\.53\.2/1.53.1/' PINS.md > "$tmp/drift.md"
PINS_FILE="$tmp/drift.md" expect_exit "TC-04 a drifted pin exits 1 (never 0)" 1 "$PINS" check
expect_out "TC-04 names tokio" "drift tokio"
printf '[[package]]\nname = "tokio"\nversion = "1.53.0"\n' > "$tmp/lock"
LOCK_FILE="$tmp/lock" expect_exit "TC-04 lock drift exits 1" 1 "$PINS" check
expect_out "TC-04 lock drift names tokio, pin and resolved" "drift tokio: pin 1.53.2, resolved 1.53.0"

sed -E 's/^(\| tower \| 0\.5\.3 \| )[^|]*/\1 /' PINS.md > "$tmp/noev.md"
PINS_FILE="$tmp/noev.md" expect_exit "TC-05 empty evidence exits 1" 1 "$PINS" check
expect_out "TC-05 empty evidence names the pin" "pin tower 0.5.3: evidence cell is empty"
echo "no table" > "$tmp/zero.md"
PINS_FILE="$tmp/zero.md" expect_exit "TC-05 zero rows exits 1" 1 "$PINS" check
head -n 8 PINS.md | grep -E '^\| (Pin|---)' > "$tmp/head.md"
PINS_FILE="$tmp/head.md" expect_exit "TC-05 header only (zero rows) exits 1" 1 "$PINS" check
PINS_FILE=/nonexistent expect_exit "TC-05 unreadable file exits 2" 2 "$PINS" check

# TC-15: wrong toolchain, run ci.sh against a fixture whose pin differs
fixture_copy "$tmp/ws"; printf '[toolchain]\nchannel = "1.0.0"\n' > "$tmp/ws/rust-toolchain.toml"
RUSTUP_TOOLCHAIN=1.94.0 ROOT="$tmp/ws" expect_exit "TC-15 wrong toolchain exits 1" 1 "$tmp/ws/scripts/ci.sh"
expect_out "TC-15 prints pinned and installed versions" "pinned 1\.0\.0, installed [0-9.]+"
finish
