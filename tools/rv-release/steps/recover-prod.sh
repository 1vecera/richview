#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
test "$(words private-recovery-go.txt)" = "Restore private origin/prod from $FINAL ($C) to $BASE, receipt $(get R), $(get GO_DATE)."
[[ ${1:-} = restore-private-prod ]] || die 'Use restore-private-prod only under the DEL-48 recovery decision.'
test "$(ref prod)" = "$FINAL"
read -r -p 'Absolute path of the authorized recovery checkout: ' checkout
[[ $checkout = /* && -d $checkout ]]
test "$(git -C "$checkout" remote get-url --push origin)" = "$(git -C "$ROOT" remote get-url --push origin)"
git -C "$checkout" push --force-with-lease="refs/heads/prod:$FINAL" origin "$BASE:refs/heads/prod"
test "$(ref prod)" = "$BASE"
finish
