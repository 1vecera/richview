#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after 49b-preflight
require_aws; go_text hosted >/dev/null
# This read happens before signup, mail or the binary changes.
pw run richview-cloudflare-email --as RICHVIEW_SMTP_PASSWORD=credential -- bash "$STEPS/mail-prompt.sh"
after 59-hold

finish
