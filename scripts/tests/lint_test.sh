#!/usr/bin/env bash
# TC-06 to TC-10 and TC-17 of story 128: each rule rejects its sample.
source "$(dirname "$0")/lib.sh"
CANARY="scripts/lint-canary.sh"
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
cd "$REPO"

expect_exit "baseline canary passes" 0 "$REPO/$CANARY"
for s in clock wall_clock sleep command rand; do expect_out "canary_$s rejected" "ok   canary_$s.rs rejected"; done
expect_out "TC-06 monotonic clock rejected (proxy rules)" "canary_clock.rs rejected"
expect_out "TC-07 wall clock rejected" "canary_wall_clock.rs rejected"
expect_out "TC-08 thread sleep rejected" "canary_sleep.rs rejected"
expect_out "TC-17 process spawn rejected" "canary_command.rs rejected"
expect_out "TC-17 unseeded randomness rejected" "canary_rand.rs rejected"

# Boundary: a use on the first line and on the last line of a file
mk() { fixture_copy "$1"; rm -f "$1"/crates/legatus-testkit/lints/canary_*.rs; }
mk "$tmp/edge"
printf 'pub fn a() -> std::time::Instant { std::time::Instant::now() }\npub fn pad() {}\n' > "$tmp/edge/crates/legatus-testkit/lints/canary_first.rs"
printf 'pub fn pad() {}\n\n\npub fn z() { std::thread::sleep(std::time::Duration::ZERO) }' > "$tmp/edge/crates/legatus-testkit/lints/canary_last.rs"
ROOT="$tmp/edge" expect_exit "boundary: use on first and last line rejected" 0 "$tmp/edge/$CANARY"

# TC-10: remove a rule, the canary must fail
grep -v 'thread::sleep' crates/legatus-proxy/clippy.toml > "$tmp/clippy_no_sleep.toml"
CLIPPY_TOML="$tmp/clippy_no_sleep.toml" expect_exit "TC-10 removing a rule fails the canary" 1 "$REPO/$CANARY"
expect_out "TC-10 names the sample that passed" "canary_sleep.rs: the lint did not reject it"

# TC-09: opt-outs
fixture_copy "$tmp/opt"; for d in net key; do mkdir -p "$tmp/opt/crates/legatus-proxy/src/$d"; done
good='#[allow(clippy::disallowed_types, reason = "production wall clock")]\npub struct W;\n'
printf "$good" > "$tmp/opt/crates/legatus-proxy/src/net/wall_system.rs"
printf '#[allow(clippy::disallowed_methods, reason = "os randomness")]\npub struct S;\n' > "$tmp/opt/crates/legatus-proxy/src/key/secret.rs"
ROOT="$tmp/opt" expect_exit "TC-09 the two listed modules with a reason pass" 0 "$tmp/opt/$CANARY"
mkdir -p "$tmp/opt/crates/legatus-testkit/src"
printf '#[allow(clippy::disallowed_types, reason = "test kit may read the clock")]\npub struct T;\n' > "$tmp/opt/crates/legatus-testkit/src/clockish.rs"
ROOT="$tmp/opt" expect_exit "TC-09 a testkit module with a written opt-out passes" 0 "$tmp/opt/$CANARY"
printf '#[allow(clippy::disallowed_types, reason = "because")]\npub struct U;\n' > "$tmp/opt/crates/legatus-proxy/src/unlisted.rs"
ROOT="$tmp/opt" expect_exit "TC-09 an unlisted proxy module fails" 1 "$tmp/opt/$CANARY"
expect_out "TC-09 file and line are printed" "unlisted.rs:1: allow attribute in a module that is not in lint-optouts.txt"
rm "$tmp/opt/crates/legatus-proxy/src/unlisted.rs"
printf '#[allow(clippy::disallowed_types)]\npub struct W;\n' > "$tmp/opt/crates/legatus-proxy/src/net/wall_system.rs"
ROOT="$tmp/opt" expect_exit "TC-09 a listed module without reason fails" 1 "$tmp/opt/$CANARY"
expect_out "TC-09 reason is demanded" "without reason"
finish
