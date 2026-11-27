#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
[[ ${1:-} = restore-previous-latest ]] || die 'Confirm the authorized latest-pointer recovery.'
test "$(words latest-recovery-go.txt)" = "Restore latest in 1vecera/richview from $TAG at $(get SEED) to $PREV_TAG, $(get GO_DATE)."
test "$(gh api repos/1vecera/richview/releases/latest --jq .tag_name)" = "$TAG"
gh release edit "$PREV_TAG" -R 1vecera/richview --latest
test "$(gh api repos/1vecera/richview/releases/latest --jq .tag_name)" = "$PREV_TAG"
finish
