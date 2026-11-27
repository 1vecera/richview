#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after 57-smtp
require_smtp_window
read -rs -p 'Daniel’s approved reset-test address: ' addr; printf '\n'; test -n "$addr"
since=$(date -u +%FT%TZ); guard
save RESET_SINCE "$since"; mark_attempt reset
test "$(printf %s "$addr" | curl -sS -o /dev/null -w '%{http_code}' -H 'Origin: https://app.richview.ai' --data-urlencode 'email@-' https://app.richview.ai/ui/forgot)" = 200; unset addr
finish
