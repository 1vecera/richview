#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after recover-outbox-plan
require_smtp_window; require_aws; GO=$(words recovery-go.txt)
fp=$(reviewfp "$HERE/recover-outbox.plan"); save F_RESTORE_OUT "$fp"
deploy mail-switch outbox --stage production --expect-plan "$fp" --go "$GO"
jq -e --arg t outbox '.phase=="provisioned" and .mailTransport==$t' "$STATE" >/dev/null; health; buttons on
finish
