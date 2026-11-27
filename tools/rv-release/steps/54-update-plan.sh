#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after 53-outbox
require_smtp_window
archive_args
run deploy plan-update "$FINAL" "${archive[@]}" "${common[@]}" | tee "$HERE/update.plan"
printf "%s\n" "Read update.plan, including old/new archives and the independent data snapshot."
finish
