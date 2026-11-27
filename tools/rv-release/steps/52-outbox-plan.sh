#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after 51-hosted-before
require_smtp_window
load_keys START
test "$START" = paused
run signup status | grep -Fx 'production: sign-up paused'
run deploy plan-mail-switch outbox --stage production | tee "$HERE/outbox.plan"
printf "%s\n" "Read outbox.plan. Type its fingerprint in step 53."
finish
