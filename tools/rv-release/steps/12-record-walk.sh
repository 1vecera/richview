#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after 11-windows; after 10-cut; inputs; check_cut
phase=site; if [[ $(get WINDOWS_combined || true) = yes ]]; then phase=combined; fi
source "$S/c-windows.sh"
check_private_windows optional "$phase" > "$HERE/windows-note-check.log"
if [[ $(tail -1 "$HERE/windows-note-check.log") = unrecorded ]]; then
  node "$GATE" windows "$C" --run "$(private_walk_dir)" | tee "$HERE/windows-record.log"
fi
check_private_windows required "$phase"
finish
