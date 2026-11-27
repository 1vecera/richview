#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
route=${1:?pass selected route A or B}; after 00-bind-full; after 01-select-parts; inputs
jq -e --arg c "$FINAL" --arg t "$TREE" '.commit==$c and .tree==$t' "$E/receipts/$R.promotion.json" >/dev/null
node "$S/select-route.mjs" bind "$route" "$FINAL"
guard; C=$(node "$S/allocate.mjs" "$ROOT" "$route" "$FINAL" "$BASE" "$HERE"); save CANDIDATE "$C"; check_cut
printf '%s %s\n' "$C" "$FINAL" > "$HERE/candidate-cut"; finish
