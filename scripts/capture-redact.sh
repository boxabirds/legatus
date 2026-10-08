#!/usr/bin/env bash
# Capture commit gate (story 120). Usage: capture-redact.sh [check] [captures-dir]
# Refuses any capture file (*.json) in the captures folder that has no leak-scan stamp.
# Exit 0: every file is stamped (or the folder has no files). Exit 1: one line per refused
# file, with its name only (never its content). Exit 2: bad usage or unreadable folder.
set -u
ROOT="${ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
DIR_DEFAULT="$ROOT/specs/proxy/evidence/captures"
STAMP_PATTERN='"scan_stamp"[[:space:]]*:[[:space:]]*"leak-scan-v1:[0-9]+"'

case "${1:-check}" in
  check) shift || true ;;
  -*) echo "usage: capture-redact.sh [check] [captures-dir]" >&2; exit 2 ;;
esac
dir="${1:-${CAPTURES_DIR:-$DIR_DEFAULT}}"

if [ ! -d "$dir" ]; then
  echo "no captures folder at $dir: nothing to check"
  exit 0
fi
status=0
count=0
while IFS= read -r file; do
  count=$((count + 1))
  if ! grep -Eq "$STAMP_PATTERN" "$file"; then
    echo "refused: ${file#$dir/} has no leak-scan stamp"
    status=1
  fi
done < <(find "$dir" -maxdepth 1 -type f -name '*.json' | sort)
[ "$status" -eq 0 ] && echo "captures ok ($count files)"
exit "$status"
