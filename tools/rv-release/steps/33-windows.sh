#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after 32w-public-walk
load_keys SEED
jq -e --arg c "$SEED" --arg h "$(hashfile "$NEXT/release/richview-x86_64-pc-windows-msvc.tar.gz")" '.commit==$c and .archiveSha256==$h and .result=="E2E PASS" and .terminated!=null' "$WIN/public/walk/manifest.json" >/dev/null
grep -Fx '51 checks passed, 0 failed' "$WIN/public/e2e.txt"; grep -Fx terminated "$WIN/public/walk/state-after.txt"
grep -F 'zero live tagged instances, volumes, security groups, snapshots or keys' "$WIN/after-go3-exit-sweep.log"; node "$S/windows/cost.mjs" "$WIN"
finish
