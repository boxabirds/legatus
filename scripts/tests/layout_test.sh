#!/usr/bin/env bash
# TC-01 and TC-02 of story 128: workspace layout and the single binary.
source "$(dirname "$0")/lib.sh"
WANT="legatus-admin legatus-common legatus-node-agent legatus-proxy legatus-simtests legatus-testkit"
members() { (cd "$1" && cargo metadata --no-deps --format-version 1 | python3 -c "import json,sys;print(' '.join(sorted(p['name'] for p in json.load(sys.stdin)['packages'])))"); }
bins() { (cd "$1" && cargo metadata --no-deps --format-version 1 | python3 -c "import json,sys;d=json.load(sys.stdin);print(' '.join(t['name'] for p in d['packages'] if p['name']=='legatus-proxy' for t in p['targets'] if 'bin' in t['kind']))"); }

[ "$(members "$REPO")" = "$WANT" ] && ok "TC-01 exactly the six members" || bad "TC-01 members: $(members "$REPO")"

tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
fixture_copy "$tmp/missing"; rm -rf "$tmp/missing/crates/legatus-admin"
got=$(members "$tmp/missing"); missing=""
for m in $WANT; do case " $got " in *" $m "*) ;; *) missing="$missing $m";; esac; done
[ "$missing" = " legatus-admin" ] && ok "TC-01 a missing member is found and named:$missing" || bad "TC-01 missing member not named (got '$missing')"

fixture_copy "$tmp/extra"; mkdir -p "$tmp/extra/crates/legatus-extra/src"
printf '[package]\nname="legatus-extra"\nversion="0.0.0"\nedition="2021"\n' > "$tmp/extra/crates/legatus-extra/Cargo.toml"; echo >"$tmp/extra/crates/legatus-extra/src/lib.rs"
[ "$(members "$tmp/extra")" != "$WANT" ] && ok "TC-01 an extra member is detected" || bad "TC-01 extra member not detected"

[ "$(bins "$REPO")" = "legatus" ] && ok "TC-02 the proxy has exactly one binary, legatus" || bad "TC-02 proxy binaries: $(bins "$REPO")"
(cd "$REPO" && cargo build -p legatus-proxy >/dev/null 2>&1) && [ -x "$REPO/target/debug/legatus" ] && ok "TC-02 build yields target/debug/legatus" || bad "TC-02 no legatus binary"

fixture_copy "$tmp/second"; mkdir -p "$tmp/second/crates/legatus-proxy/src/bin"; echo 'fn main(){}' > "$tmp/second/crates/legatus-proxy/src/bin/other.rs"
[ "$(bins "$tmp/second")" != "legatus" ] && ok "TC-02 a second proxy binary is detected" || bad "TC-02 second binary not detected"
finish
