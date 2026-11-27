#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
require_aws; words recovery-go.txt >/dev/null; test -e "$HERE/attempt-reopen"
signup status | grep -Fx 'production: sign-up open'; attempt recover-pause; signup pause; verify_signup paused "$HERE/recovery-signup-paused.txt"
finish
