#!/usr/bin/env bash
# Pin check (story 128). Usage: pins.sh check
# Compares the pin table in PINS.md with Cargo.lock and rust-toolchain.toml.
# Exit 0: every pin equals its resolved version. Exit 1: drift, an empty
# evidence cell, or a table with zero rows (one line per problem).
# Exit 2: a file cannot be read, or bad usage.
# A pinned library that Cargo.lock does not contain yet (no member uses it)
# is compared with its exact "=" requirement in Cargo.toml instead, and a
# NOTE line says so. If neither has it, the pin fails.
set -u
PINS_FILE="${PINS_FILE:-PINS.md}"
LOCK_FILE="${LOCK_FILE:-Cargo.lock}"
TOOLCHAIN_FILE="${TOOLCHAIN_FILE:-rust-toolchain.toml}"
MANIFEST_FILE="${MANIFEST_FILE:-Cargo.toml}"

[ "${1:-}" = "check" ] || { echo "usage: pins.sh check" >&2; exit 2; }
for f in "$PINS_FILE" "$LOCK_FILE" "$TOOLCHAIN_FILE" "$MANIFEST_FILE"; do
  [ -r "$f" ] || { echo "cannot read $f" >&2; exit 2; }
done

rows=0
status=0
while IFS='|' read -r _ name version evidence _; do
  name=$(echo "$name" | sed -E 's/^ +//; s/ *\(.*\)//; s/ +$//')
  version=$(echo "$version" | sed -E 's/^ +//; s/ +$//')
  evidence=$(echo "$evidence" | sed -E 's/^ +//; s/ +$//')
  [ -n "$name" ] || continue
  rows=$((rows + 1))
  if [ -z "$evidence" ]; then
    echo "pin $name $version: evidence cell is empty"
    status=1
  fi
  if [ "$name" = "rustc" ]; then
    resolved=$(sed -nE 's/^channel *= *"([^"]+)".*/\1/p' "$TOOLCHAIN_FILE")
  else
    resolved=$(awk -v n="$name" '$0 == "name = \"" n "\"" {getline; gsub(/version = |"/, ""); print; exit}' "$LOCK_FILE")
  fi
  if [ -z "$resolved" ]; then
    if [ "$name" = "rustc" ]; then
      echo "pin $name $version: no channel in $TOOLCHAIN_FILE"; status=1
    else
      resolved=$(sed -nE "s/^$name *= *\\{ *version *= *\"=([^\"]+)\".*/\\1/p" "$MANIFEST_FILE" | head -1)
      if [ -z "$resolved" ]; then
        echo "pin $name $version: in neither $LOCK_FILE nor $MANIFEST_FILE"; status=1
      else
        echo "NOTE $name: not in $LOCK_FILE yet, compared with $MANIFEST_FILE"
        [ "$resolved" = "$version" ] || { echo "drift $name: pin $version, manifest $resolved"; status=1; }
      fi
    fi
  elif [ "$resolved" != "$version" ]; then
    echo "drift $name: pin $version, resolved $resolved"
    status=1
  fi
done < <(grep -E '^\| ' "$PINS_FILE" | grep -vE '^\| (Pin|---)' )

if [ "$rows" -eq 0 ]; then
  echo "no pin rows in $PINS_FILE"
  status=1
fi
[ "$status" -eq 0 ] && echo "pins ok ($rows rows)"
exit "$status"
