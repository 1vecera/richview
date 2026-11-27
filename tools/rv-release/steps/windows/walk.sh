#!/bin/bash
# Start the candidate's own gate driver. Its shell gets the release launch guard and the optional retry probe.
set -euo pipefail
driver="$D_CHECKOUT/tools/windows-e2e/walk.sh"
git -C "$D_CHECKOUT" show "$2:tools/windows-e2e/walk.sh" | cmp - "$driver"
git -C "$D_CHECKOUT" show "$2:tools/windows-e2e/first-boot.ps1.template" | cmp - "$D_CHECKOUT/tools/windows-e2e/first-boot.ps1.template"
BASH_ENV="$D_TOOLS/walk-environment.sh" exec /bin/bash "$driver" "$@"
