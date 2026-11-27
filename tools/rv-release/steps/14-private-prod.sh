#!/usr/bin/env bash
# Move private origin/prod to the candidate through the gate, on Daniel's go and the full's receipt (DEL-04). Nothing here touches the website.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after 13-packet; inputs; selected_full; check_cut
go=$(go_text prod); p=$(ref prod)
test -s "$(get PACKET)"; test "$(hashfile "$(get PACKET)")" = "$(get PACKET_SHA)"
if [[ $p = "$BASE" ]]; then
  attempt private-prod; guard; node "$GATE" merge-prod "$C" --go "$go"
else
  [[ $p = "$FINAL" && -f "$HERE/attempt-private-prod" ]] || die 'Private prod changed outside this attempt.'
fi
check_prod
jq -e --arg c "$FINAL" --arg go "$go" '.commit==$c and .go==$go' "$E/receipts/$R.go.json" >/dev/null
jq -e --arg c "$FINAL" --arg t "$TREE" '.commit==$c and .tree==$t' "$E/receipts/$R.prod-merge.json" >/dev/null
save GO_SHA "$(printf '%s' "$go" | shasum -a 256 | awk '{print $1}')"
finish
