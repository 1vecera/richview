#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after 14-private-prod; test -e "$HERE/attempt-deploy"; from=${1:?use the version proved by recover-worker-history}
# DEL-48: a rollback to the previous deployment when the checks after a deploy fail. The go for the website part is the authority, the basis is named in the text.
go="$(go_text site) [rollback of $FINAL's Worker $from to $OLD_WORKER: DEL-48, its checks after the deploy failed]"
node "$S/worker-change.mjs" rollback "$ROOT" "" "$SITEWT" "" "$FINAL" "$(get R)" "$(get M)" "$OLD_WORKER" "$HERE" "$C" "$go" "$from"
finish
