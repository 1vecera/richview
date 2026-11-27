#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after 49-archive; require_aws; check_prod
load_keys R
jq -e --arg c "$FINAL" --arg t "$TREE" '.commit==$c and .tree==$t' "$E/receipts/$R.prod-merge.json" >/dev/null
test "$(get OLD_SHA256)" = "$(jq -er .archiveSha256 "$STATE")"
bash "$S/provider-check.sh"; verify_signup open "$HERE/pre-approval-summary.txt"
sed -n '/^{/,/^}/p' "$HERE/pre-approval-summary.txt" | jq -se 'length==1 and .[0].mailsWaiting==0' >/dev/null
deploy plan-mail-switch outbox --stage production | tee "$HERE/pre-approval-outbox.plan"
printf '%s\n' "Old $OLD $(get OLD_SHA256), target $FINAL $(get H), data volume $(jq -er .volume "$STATE"); one snapshot and 120 h retention."
finish
