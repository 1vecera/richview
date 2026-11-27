#!/bin/bash
set -euo pipefail
out=$1 checkout=$2 manifest=$3 label=$4
source "$D_TOOLS/guard.sh"
check_release
dir="$out/$label"
case ${5:-initial} in initial) log="$dir/pack.log";; resume) log="$dir/pack-resume.log";; *) exit 2;; esac
commit=$(git -C "$checkout" rev-parse HEAD)
jq -e --arg commit "$commit" '.schema==1 and .mode=="full" and .inputs.commit==$commit and .inputs.dirty==false' "$manifest" >/dev/null
test -z "$(git -C "$checkout" status --porcelain)"
name=richview-x86_64-pc-windows-msvc.tar.gz
archive="$dir/archive/$commit/$name"
check_pack_slot
test ! -e "$dir/archive"
bash "$D_TOOLS/checkout-deps.sh" "$checkout" "$commit" "$manifest"
check_release
check_pack_slot
(cd "$checkout" && unset AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY AWS_SESSION_TOKEN && source "$HOME/tmp/richview-rewrite/env.sh" && cargo run --release --locked -p richview-build -- --install-archive "$manifest" "$dir/native" "$dir/archive" --target x86_64-pc-windows-msvc) > "$log" 2>&1
test "$(tar -xOzf "$archive" ./BUILD)" = "$(printf 'commit=%s\nplatform=x86_64-pc-windows-msvc' "$commit")"
(cd "$(dirname "$archive")" && shasum -a 256 "$name" > "$name.sha256")
cat "$dir/native/SHA256SUMS" "$out/warehouse.sha256" > "$dir/build.sha256"
cat "$archive.sha256"
