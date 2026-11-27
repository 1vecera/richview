#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after 15-seed; load_keys SEED
source "$HOME/tmp/richview-rewrite/env.sh"; test "$(git -C "$NEXT/checkout" rev-parse HEAD)" = "$SEED"
slot="$HOME/tmp/richview-rewrite/locks/heavy-2"; mkdir "$slot"; trap 'rm -f "$slot/holder"; rmdir "$slot"' EXIT
printf '%s macOS release kit %s\n' "$$" "$FINAL" > "$slot/holder"
read -r -p 'Type SLOT after the release owner has reserved the shared release artifacts through copy: ' slot_go; test "$slot_go" = SLOT
RV_RELEASE_BUILD_PAUSED=1 bash "$KIT/native-build.sh" macos "$NEXT" > "$L/macos-${FINAL:0:8}.log" 2>&1
(cd "$NEXT/wasm" && shasum -a 256 -c SHA256SUMS)
grep -Fx "richview $BINVER (commit ${SEED:0:10}, runtime $(cat "$NEXT/wasm/RUNTIME"))" "$L/macos-${FINAL:0:8}.log"
grep -Fq 'PASS aarch64-apple-darwin archive' "$L/macos-${FINAL:0:8}.log"
save RUNTIME "$(cat "$NEXT/wasm/RUNTIME")"
finish
