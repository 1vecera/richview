#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after 21-push; load_keys SEED
refs=$(public_refs); test "$refs" = "$(printf '%s\trefs/heads/main' "$SEED")"; test "$(gh api repos/1vecera/richview/releases/latest --jq .tag_name)" = "$PREV_TAG"
test "$(hashfile "$HERE/stage.tsv")" = "$(get ASSETS_HASH)"; test "$(hashfile "$NOTES")" = "$(get NOTES_HASH)"
cat "$HERE/stage.tsv" "$NOTES"; node "$S/windows/cost.mjs" "$WIN" public | tee "$HERE/public-walk-budget.json"
finish
