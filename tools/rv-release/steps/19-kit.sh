#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after 18-stage; after 00-bind-full
SEED=$(sed -n 's/^public=//p' "$NEXT/manifest")
test "$(git -C "$NEXT/checkout" rev-list --parents -1 HEAD)" = "$SEED $BEFORE"
test "$(git -C "$NEXT/checkout" rev-parse 'HEAD^{tree}')" = "$TREE"
test "$(cat "$L/FINAL")" = "$FINAL"
test "$(head -1 "$NOTES")" = "# RichView ${TAG#v}"
if grep -Eiq '^(#+[[:space:]]+slides([[:space:].]|$)|\*\*slides[. :]?\*\*)' "$NOTES"; then die 'Release notes still feature slides. Return to notes owner.'; fi
test -s "$L/stage-${FINAL:0:8}.done"
for os in macos linux; do grep -Fq "richview $BINVER (commit ${SEED:0:10}, runtime $(cat "$NEXT/wasm/RUNTIME"))" "$L/verify-$os-${FINAL:0:8}.log"; grep -Fq 'PASS installer, CLI, worker protocol and uninstall' "$L/verify-$os-${FINAL:0:8}.log"; done
( cd "$NEXT/release"; test "$(ls -1A | wc -l | tr -d ' ')" = 8; for os in aarch64-apple-darwin x86_64-unknown-linux-gnu x86_64-pc-windows-msvc; do n=richview-$os.tar.gz; shasum -a 256 -c "$n.sha256"; test "$(tar -xOzf "$n" ./BUILD)" = "$(printf 'commit=%s\nplatform=%s' "$SEED" "$os")"; done
for i in install.sh install.ps1; do grep -Fq "release_commit='$SEED'" "$i"; grep -Fq "release_base_url='https://github.com/1vecera/richview/releases/download/$TAG'" "$i"; done
for i in *; do printf '%s\t%s\t%s\n' "$(hashfile "$i")" "$(stat -f %z "$i")" "$i"; done ) > "$HERE/kit-verification.log"
tail -8 "$HERE/kit-verification.log" | LC_ALL=C sort -k3,3 > "$HERE/stage.tsv"
LC_ALL=C sort -k3,3 "$ASSET_LIST" | cmp - "$HERE/stage.tsv"
cat "$HERE/stage.tsv" "$NOTES"
read -r -p 'After reviewing final release notes, site claims and eight assets, type the public seed: ' confirmation
test "$confirmation" = "$SEED"; save ASSETS_HASH "$(hashfile "$HERE/stage.tsv")"; save NOTES_HASH "$(hashfile "$NOTES")"
finish
