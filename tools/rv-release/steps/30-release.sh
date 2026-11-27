#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after 29-release-plan
load_keys SEED
cat "$NOTES"
ASSETS=(install.sh install.ps1)
for platform in aarch64-apple-darwin x86_64-unknown-linux-gnu x86_64-pc-windows-msvc; do n="richview-$platform.tar.gz"; ASSETS+=("$n" "$n.sha256"); done
files=(); for n in "${ASSETS[@]}"; do files+=("$NEXT/release/$n"); done
inventory() { local n; for n in "${ASSETS[@]}"; do test -f "$NEXT/release/$n" && test ! -L "$NEXT/release/$n" || return 1; printf '%s\t%s\t%s\n' "$(hashfile "$NEXT/release/$n")" "$(stat -f %z "$NEXT/release/$n")" "$n"; done | LC_ALL=C sort -k3,3; }
test "$(hashfile "$HERE/stage.tsv")" = "$(get ASSETS_HASH)"; inventory > "$HERE/stage-again.tsv"; cmp "$HERE/stage-again.tsv" "$HERE/stage.tsv"; LC_ALL=C sort -k3,3 "$ASSET_LIST" > "$HERE/kit.tsv"; cmp "$HERE/stage.tsv" "$HERE/kit.tsv"
refs=$(public_refs); test "$refs" = "$(printf '%s\trefs/heads/main' "$SEED")"
test "$(gh api repos/1vecera/richview/releases/latest --jq .tag_name)" = "$PREV_TAG"
go_text public >/dev/null; test "$(hashfile "$NOTES")" = "$(get NOTES_HASH)"; mark_attempt release
run gh release create "$TAG" -R 1vecera/richview --target "$SEED" --latest --title "$TAG" --notes-file "$NOTES" "${files[@]}"
finish
