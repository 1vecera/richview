#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after 14-private-prod; inputs; check_prod; clean_tree; require_signup_unchanged
go=$(go_text site)
test "$(hashfile "$HERE/reviewed-dry.json")" = "$(get DRY_SHA)"
cmp "$SOUT/dry-run.json" "$HERE/reviewed-dry.json"
test "$(get BEFORE_WORKER)" = "$OLD_WORKER"; test ! -e "$SOUT/rollback.json"
git diff --exit-code "$BASE" "$FINAL" -- cloudflare/ wrangler.toml
attempt deploy; guard
node "$S/worker-change.mjs" deploy "$ROOT" "$(dirname "$SITE")" "$SITEWT" "$SOUT" "$FINAL" "$R" "$M" "$OLD_WORKER" "$HERE" "$C" "$go"
jq -e --arg c "$FINAL" --arg r "$R" --arg w "$OLD_WORKER" '.commit==$c and .receipt==$r and .result=="pass" and .http=="pass" and .before.version==$w and .version!=$w' "$SOUT/deploy.json" >/dev/null
save NEW_WORKER "$(jq -er .version "$SOUT/deploy.json")"
finish
