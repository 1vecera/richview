#!/usr/bin/env bash
# Release 31 only: sign-up is open on app.richview.ai (reopened 24 Nov), release 30 started from paused. The hosted update runs with sign-up
# paused, so the first prompt of the mail window pauses it. The original state is journaled; reopen-signup-after-privacy restores it.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after 49b-preflight
require_smtp_window
go_text hosted >/dev/null
signup status | grep -Fx 'production: sign-up open'
[[ $(curl -sS --max-time 30 -o /dev/null -w '%{http_code}' https://app.richview.ai/sign-up) = 200 ]]
save SIGNUP_ORIGINAL open
attempt pause-signup; guard
run signup pause
verify_signup paused "$HERE/signup-paused-for-update.txt"
printf '%s\n' "Sign-up is paused for the update window. Original state: open. Do not leave the window without reopening or recording why."
finish
