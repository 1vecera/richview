#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after recover-smtp-plan
require_smtp_window; require_aws; GO=$(words recovery-go.txt)
fp=$(reviewfp "$HERE/recover-smtp.plan"); save F_RESTORE_SMTP "$fp"
smtp mail-switch smtp --stage production --expect-plan "$fp" --go "$GO"
jq -e --arg t smtp '.phase=="provisioned" and .mailTransport==$t' "$STATE" >/dev/null; bash "$S/provider-check.sh"; verify_signup paused "$HERE/recovery-acceptance.txt"
finish
