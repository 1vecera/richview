#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
require_aws; GO=$(words recovery-go.txt)
F_RESUME=$(reviewfp "$HERE/resume.plan"); save F_RESUME "$F_RESUME"
deploy resume pending --stage production --expect-plan "$F_RESUME" --go "$GO"
jq -e '.phase=="provisioned"' "$STATE" >/dev/null; health
finish
