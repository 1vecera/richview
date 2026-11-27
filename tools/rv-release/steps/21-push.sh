#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after 20-seed
load_keys SEED
test "$(ref prod)" = "$FINAL"; test "$(sed -n 's/^tested=//p' "$NEXT/manifest")" = "$FINAL"
sh "$KIT/check-seed-history.sh" "$NEXT"
test "$(git -C "$NEXT/checkout" rev-list --parents -1 HEAD)" = "$SEED $BEFORE"
test "$(git -C "$NEXT/checkout" rev-parse 'HEAD^{tree}')" = "$TREE"
refs=$(public_refs); test "$refs" = "$(printf '%s\trefs/heads/main' "$BEFORE")"
go_text public >/dev/null; mark_attempt public-push
run "${pub[@]}" -C "$NEXT/checkout" -c core.hooksPath="$KIT/public-hooks" push --force-with-lease="refs/heads/main:$BEFORE" --no-follow-tags --no-recurse-submodules "$PUBLIC" HEAD:refs/heads/main
refs=$(public_refs); test "$refs" = "$(printf '%s\trefs/heads/main' "$SEED")"
finish
