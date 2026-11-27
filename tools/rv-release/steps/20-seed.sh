#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after 19-kit
SEED=$(sed -n 's/^public=//p' "$NEXT/manifest"); [[ $SEED =~ ^[0-9a-f]{40}$ ]]
cat "$HERE/stage.tsv" "$L/seed-${SEED:0:8}.md"
test -s "$L/stage-${FINAL:0:8}.done"
for os in macos linux; do grep -Fq 'PASS installer, CLI, worker protocol and uninstall' "$L/verify-$os-${FINAL:0:8}.log"; done
test "$(ref prod)" = "$FINAL"; test "$(sed -n 's/^tested=//p' "$NEXT/manifest")" = "$FINAL"
sh "$KIT/check-seed-history.sh" "$NEXT"
test "$(git -C "$NEXT/checkout" rev-list --parents -1 HEAD)" = "$SEED $BEFORE"
test "$(git -C "$NEXT/checkout" rev-parse 'HEAD^{tree}')" = "$TREE"
refs=$(public_refs); test "$refs" = "$(printf '%s\trefs/heads/main' "$BEFORE")"
printf '%s\n' "$refs" > "$HERE/public-before.tsv"
save SEED "$SEED"
printf "%s\n" "Read the release kit, seed scan and Windows handover before exposing the tree."
finish
