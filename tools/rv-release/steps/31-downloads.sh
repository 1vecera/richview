#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after 30-release
load_keys SEED
ASSETS=(install.sh install.ps1)
for platform in aarch64-apple-darwin x86_64-unknown-linux-gnu x86_64-pc-windows-msvc; do n="richview-$platform.tar.gz"; ASSETS+=("$n" "$n.sha256"); done
gh api repos/1vecera/richview/releases/latest > "$HERE/release.json"
jq -e --arg tag "$TAG" '.tag_name==$tag and .name==$tag and .draft==false and .prerelease==false and (.assets|length)==8' "$HERE/release.json" >/dev/null
refs=$(public_refs); printf '%s\n' "$refs" | awk -v ref="refs/tags/$TAG" -v sha="$SEED" '$2==ref {tag=$1} $2==ref"^{}" {peeled=$1} END{exit ((peeled?peeled:tag)!=sha)}'
jq -r '.assets[] | [(.digest|ltrimstr("sha256:")),.size,.name] | @tsv' "$HERE/release.json" | LC_ALL=C sort -k3,3 > "$HERE/published.tsv"; cmp "$HERE/stage.tsv" "$HERE/published.tsv"
assembler="$NEXT/checkout/scripts/assemble-release.ts"
[[ -f "$assembler" ]] || assembler="$NEXT/checkout/scripts/assemble-release.mjs"
verify=(node "$assembler" --verify-published --commit "$SEED" --installer "$NEXT/release/install.sh" --windows-installer "$NEXT/release/install.ps1")
run "${verify[@]}"
for n in "${ASSETS[@]}"; do run curl -fsSL --proto '=https' --proto-redir '=https' "https://github.com/1vecera/richview/releases/latest/download/$n" | cmp - "$NEXT/release/$n"; done
finish
