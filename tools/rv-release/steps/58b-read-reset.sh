#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after 58a-send-reset
load_keys RESET_SINCE
since=$RESET_SINCE
run env -u AWS_ACCESS_KEY_ID -u AWS_SECRET_ACCESS_KEY -u AWS_SESSION_TOKEN uv run "$OPS/read-test-mail.py" 'Reset your RichView password' "$since" > "$HERE/reset-headers.txt"

grep -Eq "^found 1$" "$HERE/reset-headers.txt"
grep -Eiq "^from:.*accounts@richview[.]ai" "$HERE/reset-headers.txt"
for mech in spf dkim dmarc; do grep -Eiq "(^|[^[:alnum:]_])${mech}=pass([^[:alnum:]_]|$)" "$HERE/reset-headers.txt"; done
printf "%s\n" "Read the delivered message headers for the same mail and SPF, DKIM, DMARC."
finish
