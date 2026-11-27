#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after 32-installers; require_aws; load_keys SEED
read -r tag seed ts < "$WIN/go3-confirmation"; test "$tag" = "$TAG"; test "$seed" = "$SEED"
test "$(hashfile "$NEXT/release/richview-x86_64-pc-windows-msvc.tar.gz")" = "$(awk -F '\t' '$3=="richview-x86_64-pc-windows-msvc.tar.gz"{print $1}' "$HERE/published.tsv")"; test "$(gh api repos/1vecera/richview/releases/latest --jq .tag_name)" = "$TAG"
test "$(git -C "$NEXT/checkout" rev-parse HEAD)" = "$SEED"
export D_ROOT="$L" D_TOOLS="$S/windows" D_FINAL="$FINAL" D_BIND="$HERE/bound-sha" D_SEED="$NEXT" D_TAG="$TAG" D_PREV_TAG="$PREV_TAG" AWS_REGION="$REGION"
node "$D_TOOLS/cost.mjs" "$WIN" public
test "$(awsr sts get-caller-identity --query Account --output text)" = "$ACCOUNT"
attempt public-walk; trap 'bash "$D_TOOLS/sweep.sh" > "$WIN/after-go3-exit-sweep.log"' EXIT
bash "$D_TOOLS/run-walk.sh" "$WIN" "$NEXT/checkout" public
bash "$D_TOOLS/sweep.sh" > "$WIN/after-go3-exit-sweep.log"; trap - EXIT
finish
