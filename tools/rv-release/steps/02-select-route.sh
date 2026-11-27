#!/usr/bin/env bash
# Bind the Windows ceiling answer (the board ask named in the release descriptor) to this candidate and its scope (DEL-83: the cost guard stays).
# The ceiling answer is Daniel's spend limit; the Windows walk itself runs only under the go (go_text windows).
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after 00-bind-full; after 01-select-parts
node "$S/select-route.mjs" select "$(dget .route)" "$FINAL" "$(dget .windowsScope)"
finish
