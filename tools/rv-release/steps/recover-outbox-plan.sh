#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
require_smtp_window; require_aws
deploy plan-mail-switch outbox --stage production | tee "$HERE/recover-outbox.plan"
printf "%s\n" "Read the current recovery plan and verify the running revision and transport."
finish
