#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
mode=${1:?site, combined or bonus}
case "$mode" in site|combined|bonus) ;; *) die 'Unknown Windows scope';; esac
after 10-cut; kept=$KEPT
if [[ $mode = bonus ]]; then test "$(get WINDOWS_site)" = yes; fi
if [[ $mode != site ]]; then after 16-macos; fi
require_aws
# The spend needs the go (the windows part, or the public part for the public-only build) and the board's ceiling answer, which the cost guard below re-reads.
if [[ $mode = bonus ]]; then go_text public prepare >/dev/null; else go_text windows prepare >/dev/null; fi
export D_WINDOWS_MODE="$mode" D_ROOT="$L" D_TOOLS="$S/windows" D_FINAL="$FINAL" D_BIND="$HERE/bound-sha" D_SEED="$NEXT" D_TAG="$TAG" D_PREV_TAG="$PREV_TAG" AWS_REGION="$REGION" COPYFILE_DISABLE=1
echo "$FINAL" > "$D_BIND"; source "$D_TOOLS/guard.sh"; check_release
check_pack_slot
node "$D_TOOLS/cost.mjs" "$WIN" "$mode" | tee "$HERE/budget-$mode.json"
test "$(awsr sts get-caller-identity --query Account --output text)" = "$ACCOUNT"
bash "$D_TOOLS/sweep.sh" > "$HERE/pre-$mode-sweep.log"
attempt "windows-$mode"
private="$ROOT/.claude/worktrees/windows${RELNUM}-${FINAL:0:10}"; if [[ ! -d "$private" ]]; then git worktree add --detach "$private" "$FINAL"; git worktree lock --reason 'active stream' "$private"; fi
test "$(git -C "$private" rev-parse HEAD)" = "$FINAL"; test -z "$(git -C "$private" status --porcelain)"
mkdir -p "$WIN/bin" "$WIN/private/manifest" "$WIN/public"
cp "$HERE/full-build-manifest.json" "$WIN/private/manifest/manifest.json"; cmp "$kept/build-manifest.json" "$WIN/private/manifest/manifest.json"
prep="$PREPARE_SOURCE"
if [[ "$mode" != bonus ]]; then bash "$prep" "$ROOT" "$FINAL" "$WIN/private" > "$WIN/private/source.sha256"; fi
if [[ "$mode" != site ]]; then
 seed=$(git -C "$NEXT/checkout" rev-parse HEAD); test "$(git -C "$NEXT/checkout" rev-parse 'HEAD^{tree}')" = "$TREE"
 bash "$prep" "$NEXT/checkout" "$seed" "$WIN/public" > "$WIN/public/source.sha256"
fi
web="$kept/dist/web"; runtime=$(cat "$web/richview_runtime.js" "$web/richview_runtime_bg.wasm.gz" | shasum -a 256 | cut -c1-12)
version=$("$kept/richview" --version); release=$(awk '{print $2}' <<< "$version")
test "$version" = "richview $release (commit ${FINAL:0:10}, runtime $runtime)"
test "$release" = "$BINVER"
mkdir -p "$WIN/kit/wasm"; echo "$runtime" > "$WIN/kit/wasm/RUNTIME"
if [[ "$mode" != site ]]; then test "$(cat "$NEXT/wasm/RUNTIME")" = "$runtime"; fi
assets=(richview_runtime.js richview_runtime_bg.wasm.gz richview_docs.js richview_docs_bg.wasm bootstrap.js)
if [[ "$mode" != site ]]; then
 for n in "${assets[@]}"; do cmp "$web/$n" "$NEXT/checkout/dist/web/$n"; test "$(hashfile "$web/$n")" = "$(jq -er --arg n "checkout:dist/web/$n" '.outputs[$n].sha256' "$NEXT/macos/manifest.json")"; done
fi
for n in "${assets[@]}"; do test "$(hashfile "$web/$n")" = "$(jq -er --arg n "checkout:dist/web/$n" '.outputs[$n].sha256' "$kept/build-manifest.json")"; done
tar -czf "$WIN/runtime.tgz" -C "$web" "${assets[@]}"
oldwin="$PREV/windows-$OLD" # the previous release's warehouse fixture
git diff --exit-code "$OLD" "$FINAL" -- examples/sql-report/data examples/sql-report/queries | cat > "$HERE/warehouse-input-delta.txt" || die 'Windows owner must verify/rebuild the SQL fixture for changed inputs before use'
lock_duckdb() { git show "$1:Cargo.lock" | awk 'BEGIN{RS="";ORS="\n\n"} /name = "(libduckdb-sys|duckdb)"/{print}'; }
lock_duckdb "$OLD" > "$HERE/duckdb-before.txt"; test -s "$HERE/duckdb-before.txt"; lock_duckdb "$FINAL" | cmp - "$HERE/duckdb-before.txt"
(cd "$oldwin" && shasum -a 256 -c warehouse.sha256)
cp "$oldwin/warehouse.duckdb" "$WIN/warehouse.duckdb"; (cd "$WIN" && shasum -a 256 warehouse.duckdb > warehouse.sha256)
# Each archive is packed in its own candidate checkout by the shared Cargo shim.
if [[ $mode = site ]]; then seed_dir="$WIN/kit"; args=(); else seed_dir="$NEXT"; args=("$mode"); fi
if [[ $mode = bonus ]]; then export RV_WIN_SESSION="$WIN/bonus-build"; args=(public-only); fi
trap 'bash "$D_TOOLS/sweep.sh" > "$HERE/windows-exit-sweep.log"' EXIT
# Confirm that a changed candidate orchestration script still matches the reviewed adapter.
git diff --exit-code "$BASE" "$FINAL" -- tools/windows-e2e/walk.sh tools/windows-e2e/first-boot.ps1.template
bash "$D_TOOLS/build.sh" "$WIN" "$seed_dir" "$private" "$release" ${args[@]+"${args[@]}"} > "$WIN/build-$mode.log" 2>&1
if [[ $mode != bonus ]]; then
 bash "$D_TOOLS/pack.sh" "$WIN" "$private" "$WIN/private/manifest/manifest.json" private
 bash "$D_TOOLS/run-walk.sh" "$WIN" "$private" private
 bash "$D_TOOLS/sweep.sh" > "$WIN/private/sweep.log"
fi
if [[ $mode != site ]]; then
 bash "$D_TOOLS/pack.sh" "$WIN" "$NEXT/checkout" "$NEXT/macos/manifest.json" public
 mkdir -p "$NEXT/windows"; n=richview-x86_64-pc-windows-msvc.tar.gz; cp "$WIN/public/archive/$seed/$n" "$WIN/public/archive/$seed/$n.sha256" "$NEXT/windows/"
 (cd "$NEXT/windows" && shasum -a 256 -c "$n.sha256")
fi
bash "$D_TOOLS/sweep.sh" > "$WIN/build-$mode-exit-sweep.log"; trap - EXIT; save "WINDOWS_$mode" yes
finish
