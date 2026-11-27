#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after 52-outbox-plan
require_smtp_window
GO=$(go_text hosted)
F_OUT=$(reviewfp "$HERE/outbox.plan")
save F_OUT "$F_OUT"; mark_attempt outbox
run deploy mail-switch outbox --stage production --expect-plan "$F_OUT" --go "$GO" > "$HERE/outbox.apply" 2>&1
jq -e --arg c "$OLD" '.phase=="provisioned" and .candidate==$c and .mailTransport=="outbox"' "$STATE" >/dev/null
bash "$S/provider-check.sh"; signup status | grep -Fx "production: sign-up paused"
finish
