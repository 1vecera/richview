#!/usr/bin/env bash
set -euo pipefail
[[ $# == 3 ]] || exit 2
seed=$(cd "$1" && pwd -P)
kit=$(cd "$2" && pwd -P)
server_log=$3
public=$(sed -n 's/^public=//p' "$seed/manifest")
runtime=$(cat "$seed/wasm/RUNTIME")
release=$seed/release
platform=$(uname -s)
case "$platform" in
  Darwin) checksums=(shasum -a 256 -c) ;;
  Linux) checksums=(sha256sum -c) ;;
  *) exit 2 ;;
esac
(cd "$release" && "${checksums[@]}" ./*.tar.gz.sha256)

uv run --offline --no-project python -u -m http.server --bind 127.0.0.1 --directory "$release" 0 > "$server_log" 2>&1 &
server=$!
trap 'kill "$server" 2>/dev/null || true; wait "$server" 2>/dev/null || true' EXIT
port=''
for attempt in {1..20}; do
  port=$(sed -nE 's/^Serving HTTP on 127\.0\.0\.1 port ([0-9]+) .*/\1/p' "$server_log")
  [[ -z "$port" ]] || break
  kill -0 "$server"
  sleep 0.25
done
[[ "$port" =~ ^[0-9]+$ ]]
url=http://127.0.0.1:$port
output=$(sh "$kit/verify-installer.sh" "$seed" --base-url "$url")
printf '%s\n' "$output"
grep -Fx "richview 3.0.0 (commit ${public:0:10}, runtime $runtime)" <<< "$output"
# The production verifier accepts loopback and checks both installer files and all archive bytes.
if [[ "$platform" == Darwin ]]; then
  assembler="$seed/checkout/scripts/assemble-release.ts"
  [[ -f "$assembler" ]] || assembler="$seed/checkout/scripts/assemble-release.mjs"
  node "$assembler" --verify-published --commit "$public" \
    --installer "$release/install.sh" --windows-installer "$release/install.ps1" --release-url "$url"
fi
