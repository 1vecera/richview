#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after 00-bind-full; after 01-select-parts; test "$(cat "$L/FINAL")" = "$FINAL"; test ! -e "$NEXT"
sh "$KIT/next-release.sh" --source "$ROOT" --commit "$FINAL" --public-url "$PUBLIC" --parent "$BEFORE" --name "$(git show -s --format=%an "$BEFORE")" --email "$(git show -s --format=%ae "$BEFORE")" --output "$NEXT"
seed=$(sed -n 's/^public=//p' "$NEXT/manifest"); test "$(git -C "$NEXT/checkout" rev-list --parents -1 HEAD)" = "$seed $BEFORE"
test "$(git -C "$NEXT/checkout" rev-parse 'HEAD^{tree}')" = "$TREE"; git diff --no-renames --binary "$BEFORE" "$FINAL" > "$L/delta-${FINAL:0:8}.patch"
git diff --no-renames --stat=240 "$BEFORE" "$FINAL" > "$L/delta-${FINAL:0:8}-stat.txt"
bash "$S/kit/scan-final.sh" "$ROOT/tmp/team/launch" "$NEXT" "$L/scan-${seed:0:8}" "$PREV/scan-${BEFORE:0:8}-fonts.json" > "$L/scan-${seed:0:8}.log"
node "$S/kit/review-seed.cjs" "$ROOT" "$L" "$FINAL" "$PREV/scan-${BEFORE:0:8}-private-scan.json" > "$HERE/seed-counts.json"
cat "$L/seed-${seed:0:8}.md"; read -r -p 'After reviewing public delta and all scan findings, type the seed SHA: ' v; test "$v" = "$seed"; save SEED "$seed"
finish
