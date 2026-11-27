#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after 56-smtp-plan
require_smtp_window
GO=$(go_text hosted)
F_SMTP=$(reviewfp "$HERE/smtp.plan")
save F_SMTP "$F_SMTP"; mark_attempt smtp
run smtp mail-switch smtp --stage production --expect-plan "$F_SMTP" --go "$GO" > "$HERE/smtp.apply" 2>&1
jq -e --arg c "$FINAL" '.phase=="provisioned" and .candidate==$c and .mailTransport=="smtp"' "$STATE" >/dev/null
bash "$S/provider-check.sh"; signup status | grep -Fx "production: sign-up paused"
finish
