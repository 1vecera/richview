#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after 55-update
require_smtp_window
run smtp plan-mail-switch smtp --stage production | tee "$HERE/smtp.plan"
printf "%s\n" "Read smtp.plan. Check sender, endpoint, running archive and transport."
finish
