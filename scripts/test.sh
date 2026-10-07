#!/usr/bin/env bash
# Tier runner (story 128). Usage: test.sh [virtual|scaled|real]
# No argument: virtual then scaled, never real (PROPOSED default).
# Exit 0 pass, 1 a test failed, 2 unknown tier, 3 real tier not configured.
set -u
ROOT="${ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
cd "$ROOT" || exit 1

run_tier() {
  echo "== tier $1"
  cargo test -p legatus-simtests --features "tier-$1" || { echo "tier $1 failed"; return 1; }
}

check_real() {
  if [ -z "${LEGATUS_REAL_ENGINES:-}" ]; then
    echo "real tier not configured. Missing: LEGATUS_REAL_ENGINES (a list of engine=host:port entries)." >&2
    return 3
  fi
}

case "${1:-}" in
  "")      run_tier virtual || exit 1; run_tier scaled || exit 1 ;;
  virtual) run_tier virtual || exit 1 ;;
  scaled)  run_tier scaled || exit 1 ;;
  real)    check_real || exit 3; run_tier real || exit 1 ;;
  *)       echo "unknown tier '$1'. Valid tiers: virtual, scaled, real" >&2; exit 2 ;;
esac
exit 0
