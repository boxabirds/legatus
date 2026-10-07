#!/usr/bin/env bash
# TC-13, TC-14 and TC-18 of story 128: tier argument handling and step order.
source "$(dirname "$0")/lib.sh"
cd "$REPO"
unset LEGATUS_REAL_ENGINES
expect_exit "TC-14 unknown tier exits 2" 2 scripts/test.sh nonsense
expect_out "TC-14 lists the valid tiers" "Valid tiers: virtual, scaled, real"
expect_exit "TC-14 real with no engines exits 3" 3 scripts/test.sh real
expect_out "TC-14 prints what is missing" "Missing: LEGATUS_REAL_ENGINES"
reject_out "TC-14 runs nothing" "== tier"
expect_exit "boundary: one valid argument (virtual)" 0 scripts/test.sh virtual
expect_out "TC-12 virtual runs the virtual tier" "== tier virtual"
reject_out "TC-12 virtual does not run scaled (negative)" "== tier scaled"
expect_exit "boundary: zero arguments" 0 scripts/test.sh
expect_out "TC-13 default runs virtual" "== tier virtual"
expect_out "TC-13 default runs scaled" "== tier scaled"
reject_out "TC-13 default never runs real (negative)" "== tier real"

# TC-18: step order of ci.sh as printed
expect_exit "ci passes on the real workspace" 0 scripts/ci.sh
order=$(echo "$OUT" | grep -E '^(pass|skip|FAIL)' | awk '{print $2}' | tr '\n' ' ')
want="toolchain pins build clippy virtual-guard unit-tests virtual-tier capture-redact lint-ste scaled-tier "
[ "$order" = "$want" ] && ok "TC-18 step order: $order" || bad "TC-18 step order '$order'"
expect_out "TC-18 a missing script is skipped with a warning" "skip  lint-ste"
finish
