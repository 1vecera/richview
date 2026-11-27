#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after 58b-read-reset; require_smtp_window; bash "$S/provider-check.sh"
verify_signup paused "$HERE/hosted-accepted.txt"
sed -n '/^{/,/^}/p' "$HERE/hosted-accepted.txt" | jq -se 'length==1 and .[0].mailsWaiting==0' >/dev/null
save PRIVACY_REOPEN pending
finish
