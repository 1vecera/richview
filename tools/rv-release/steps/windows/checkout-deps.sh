#!/usr/bin/env bash
# Install the dependencies selected by this checkout's recorded lockfile.
set -euo pipefail
cd "${1:?checkout}"
commit=${2:?commit}; manifest=${3:?manifest}
test "$(git rev-parse HEAD)" = "$commit"
verify_lock() {
  git show "$commit:package-lock.json" | cmp - package-lock.json
  local digest
  digest=$(shasum -a 256 package-lock.json | awk '{print $1}')
  jq -e --arg c "$commit" --arg d "$digest" '.inputs.commit==$c and .inputs.config["package-lock.json"]==$d' "$manifest" >/dev/null
}
verify_lock
for dir in node_modules packages packages/vscode packages/vscode/node_modules; do
  [[ ! -L $dir ]] || { echo "Linked $dir in $PWD. Use a private dependency directory before installing." >&2; exit 1; }
done
env -u AWS_ACCESS_KEY_ID -u AWS_SECRET_ACCESS_KEY -u AWS_SESSION_TOKEN -u CLOUDFLARE_API_TOKEN npm ci --ignore-scripts
verify_lock
test -d node_modules && test ! -L node_modules
test -x node_modules/esbuild/bin/esbuild
