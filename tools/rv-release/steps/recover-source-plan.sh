#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after 21-push; load_keys SEED; publicmain=$("${pub[@]}" ls-remote "$PUBLIC" refs/heads/main | cut -f1); test "$publicmain" = "$SEED"
wt="$HERE/source-recovery"; test ! -e "$wt"; git -C "$NEXT/checkout" worktree add --detach "$wt" "$SEED"
git -C "$wt" -c commit.gpgSign=false revert --no-edit "$SEED"; tip=$(git -C "$wt" rev-parse HEAD)
test "$(git -C "$wt" rev-parse 'HEAD^{tree}')" = "$(git -C "$wt" rev-parse "$BEFORE^{tree}")"
git -C "$wt" diff --stat "$SEED" "$tip"; save SOURCE_REVERT "$tip"
finish
