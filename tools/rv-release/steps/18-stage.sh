#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after 17-linux; test -s "$L/linux-${FINAL:0:8}.done"; test -f "$NEXT/windows/richview-x86_64-pc-windows-msvc.tar.gz.sha256"; test "$(get WINDOWS_combined 2>/dev/null || get WINDOWS_bonus)" = yes
bash "$S/kit/stage-check.sh" "$FINAL" | tee "$HERE/stage.log"
finish
