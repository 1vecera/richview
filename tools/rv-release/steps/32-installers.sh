#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after 31-downloads
load_keys SEED
run sh "$KIT/verify-installer.sh" "$NEXT" | tee "$HERE/mac-live-install.log"
run ssh -o BatchMode=yes lenovo "systemd-run --user --scope -p CPUQuota=800% -p MemoryHigh=16G nice -n 19 ionice -c3 sh ${KIT/#$ROOT/$LENOVO_REPO}/verify-installer.sh $LENOVO_REL/next-${FINAL:0:8}" | tee "$HERE/linux-live-install.log"

for log in mac linux; do grep -Fx "richview $BINVER (commit ${SEED:0:10}, runtime $(cat "$NEXT/wasm/RUNTIME"))" "$HERE/$log-live-install.log"; grep -Fq 'PASS installer, CLI, worker protocol and uninstall' "$HERE/$log-live-install.log"; done
printf '%s %s %s\n' "$TAG" "$SEED" "$(date -u +%FT%TZ)" > "$WIN/go3-confirmation"
printf "%s\n" "Read the Windows owner’s published installer result before the hosted update."
finish
