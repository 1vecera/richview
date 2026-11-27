#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
archive_args; require_aws
deploy plan-rollback "$OLD" "${common[@]}" | tee "$HERE/rollback-current.plan"
printf "%s\n" "Read the retained $OLD_CANDIDATE ($OLD) archive and the store compatibility note."
finish
