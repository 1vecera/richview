#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after 49-archive
require_smtp_window
jq -e --arg c "$OLD" '.phase=="provisioned" and .stack=="richview-cowork-production" and .region=="eu-central-1" and .candidate==$c and .mailTransport=="smtp" and .update.stage=="complete"' "$STATE" >/dev/null
test "$(get OLD_SHA256)" = "$(jq -er .archiveSha256 "$STATE")"; bash "$S/provider-check.sh"
run summary | tee "$HERE/summary-before.txt"; grep -Eq '"mailsWaiting"[[:space:]]*:[[:space:]]*0([,[:space:]]|$)' "$HERE/summary-before.txt"
run signup status | tee "$HERE/signup-before.txt"
START=$(sed -nE 's/^production: sign-up (open|paused)$/\1/p' "$HERE/signup-before.txt"); test "$START" = paused; test "$(get SIGNUP_ORIGINAL)" = open
if [[ ! -e "$HERE/state-before-update.json" ]]; then cp "$STATE" "$HERE/state-before-update.json"; fi; cmp "$STATE" "$HERE/state-before-update.json"
if awk -F '\t' '$1=="START"{found=1}END{exit !found}' "$REC"; then
  test "$START" = "$(get START)" || die 'Signup differs from the recorded original. Use the original record for recovery.'
else save START "$START"; fi
printf "%s\n" "Read summary-before.txt, signup-before.txt and the starting SMTP/archive state."
finish
