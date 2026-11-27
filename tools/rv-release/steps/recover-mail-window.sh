#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
require_aws; words recovery-go.txt >/dev/null
pw run richview-cloudflare-email --as RICHVIEW_SMTP_PASSWORD=credential -- bash "$S/recovery-prompt.sh"
finish
