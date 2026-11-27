#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
require_smtp_window; require_aws
smtp plan-mail-switch smtp --stage production | tee "$HERE/recover-smtp.plan"
printf "%s\n" "Read the current recovery plan and verify the running revision and transport."
finish
