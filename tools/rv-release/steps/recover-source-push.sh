#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after recover-source-plan; words recovery-go.txt >/dev/null; load_keys SEED SOURCE_REVERT
wt="$HERE/source-recovery"; test "$(git -C "$wt" rev-list --parents -1 HEAD)" = "$SOURCE_REVERT $SEED"
test "$(git -C "$wt" rev-parse 'HEAD^{tree}')" = "$(git -C "$wt" rev-parse "$BEFORE^{tree}")"
attempt source-revert; "${pub[@]}" -C "$wt" -c core.hooksPath="$KIT/public-hooks" push --no-follow-tags --no-recurse-submodules --force-with-lease="refs/heads/main:$SEED" "$PUBLIC" HEAD:refs/heads/main
test "$("${pub[@]}" ls-remote "$PUBLIC" refs/heads/main | cut -f1)" = "$SOURCE_REVERT"
finish
