#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
test -e "$HERE/attempt-deploy"; wt=$SITEWT
node "$S/worker-change.mjs" inspect "$ROOT" "" "$wt" "" "$FINAL" "$R" "$(get M)" "$OLD_WORKER" "$HERE" "$C"
finish
