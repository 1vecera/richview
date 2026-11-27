#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
R=${1:?Pass the reviewed full receipt ID}
[[ $R = "$RFULL" ]] || die 'Different full receipt from the release descriptor.'
[[ $R =~ ^full-[0-9]{8}T[0-9]{6}Z-[a-f0-9]{10}$ && ${R##*-} = "${FINAL:0:10}" ]]
jq -e --arg c "$FINAL" --arg t "$TREE" --arg r "$R" '.kind=="full" and .result=="pass" and .cancelled!=true and .commit==$c and .tree==$t and .id==$r' "$E/receipts/$R.json" >/dev/null
git merge-base --is-ancestor "$BASE" "$FINAL"
mkdir -p "$HERE"; [[ -O $HERE && ! -L $HERE && $(stat -f %Lp "$HERE") = 700 ]]
if [[ ! -e "$REC" ]]; then (set -C; printf 'FINAL\t%s\n' "$FINAL" > "$REC"); fi
test "$(get FINAL)" = "$FINAL"
previous=$(get R || true); [[ -z $previous || $previous = "$R" ]] || die 'This SHA journal already belongs to a different receipt.'
bash "$S/retain-full" "$R"
src="$L/retained-full/$R/build-manifest.json"; dst="$HERE/full-build-manifest.json"
if [[ ! -e $dst ]]; then cp "$src" "$dst"; fi
cmp "$src" "$dst"; shasum -a 256 "$src" "$KEPT/build-manifest.json" > "$HERE/manifest-hashes.txt"
cmp "$dst" "$KEPT/build-manifest.json"
jq -e --arg c "$FINAL" '.schema==1 and .mode=="full" and .inputs.commit==$c and .inputs.dirty==false' "$dst" >/dev/null
node "$S/check-homepage-a.mjs" "$KEPT/dist/web/index.html" "$dst"
if [[ -e "$L/FINAL" ]]; then test "$(cat "$L/FINAL")" = "$FINAL"; else (set -C; echo "$FINAL" > "$L/FINAL"); fi
save R "$R"; save M "$(hashfile "$dst")"; inputs
printf 'Keep %s and manifest-hashes.txt beside the packet. Record directory %s\n' "$dst" "$HERE"
finish
