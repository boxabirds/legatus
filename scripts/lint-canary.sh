#!/usr/bin/env bash
# Lint canary (story 128). Usage: lint-canary.sh
# 1. Each canary source under crates/legatus-testkit/lints must FAIL clippy
#    with the proxy rules; ok_sample.rs must pass.
# 2. Every allow attribute for the clock, sleep, process or random lints in
#    legatus-proxy and legatus-common must sit in a module listed in
#    lint-optouts.txt and carry `reason = "..."`.
# Exit 0: all good. Exit 1: a rule did not reject its sample, or a bad opt-out
# (file and line printed). Env: ROOT overrides the repository root (tests).
set -u
ROOT="${ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
LINTS="$ROOT/crates/legatus-testkit/lints"
CLIPPY="${CLIPPY_TOML:-$ROOT/crates/legatus-proxy/clippy.toml}"
status=0

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/rand/src" "$work/probe/src"
cat > "$work/rand/Cargo.toml" <<'T'
[package]
name = "rand"
version = "0.0.0"
edition = "2021"
T
cat > "$work/rand/src/lib.rs" <<'T'
pub fn thread_rng() -> u32 { 4 }
pub fn random<T: From<u8>>() -> T { T::from(4) }
T
cat > "$work/probe/Cargo.toml" <<'T'
[package]
name = "probe"
version = "0.0.0"
edition = "2021"
[dependencies]
rand = { path = "../rand" }
T
cp "$CLIPPY" "$work/probe/clippy.toml"

run_sample() {
  { echo '#![deny(clippy::disallowed_types, clippy::disallowed_methods)]'; cat "$1"; } > "$work/probe/src/lib.rs"
  (cd "$work/probe" && CARGO_TARGET_DIR="$work/target" cargo clippy --quiet >/dev/null 2>&1)
}

for sample in "$LINTS"/canary_*.rs; do
  [ -e "$sample" ] || { echo "no canary samples in $LINTS"; status=1; break; }
  if run_sample "$sample"; then
    echo "FAIL $sample: the lint did not reject it"; status=1
  else
    echo "ok   $(basename "$sample") rejected"
  fi
done
if [ -e "$LINTS/ok_sample.rs" ]; then
  if run_sample "$LINTS/ok_sample.rs"; then echo "ok   ok_sample.rs accepted"
  else echo "FAIL $LINTS/ok_sample.rs: the lint rejected the control sample"; status=1; fi
fi

# Opt-out check.
OPTOUTS="$ROOT/lint-optouts.txt"
[ -r "$OPTOUTS" ] || { echo "cannot read $OPTOUTS"; exit 2; }
for crate in legatus-proxy legatus-common; do
  while IFS=: read -r file line text; do
    [ -n "$file" ] || continue
    rel="${file#$ROOT/}"
    if ! grep -qE "^$rel \|" "$OPTOUTS"; then
      echo "FAIL $rel:$line: allow attribute in a module that is not in lint-optouts.txt"; status=1
    elif ! echo "$text" | grep -q 'reason *='; then
      echo "FAIL $rel:$line: allow attribute without reason = \"...\""; status=1
    fi
  done < <(grep -rnE 'allow\((clippy::)?disallowed_(types|methods)' "$ROOT/crates/$crate/src" 2>/dev/null)
done
[ "$status" -eq 0 ] && echo "lint canary ok"
exit "$status"
