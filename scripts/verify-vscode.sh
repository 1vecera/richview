#!/usr/bin/env bash
# Host reason (ARC-03): compatibility entry point for isolated native-host VS Code checks.
set -euo pipefail
exec node scripts/verify-vscode.mjs "$@"
