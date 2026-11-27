#!/bin/bash
set -euo pipefail
out=$1 checkout=$2 label=$3
export AWS_REGION=eu-central-1 AWS_PAGER='' D_CHECKOUT="$checkout" COPYFILE_DISABLE=1
unset WINDOWS_E2E_SCRIPT WINDOWS_E2E_TYPE WINDOWS_E2E_HOURLY_USD WINDOWS_E2E_IMAGE
source "$D_TOOLS/guard.sh"
check_release
commit=$(git -C "$checkout" rev-parse HEAD)
dir="$out/$label"
lane=${4:-walk}
case "$label/$lane" in private/walk|public/walk|private/walk-2) ;; *) exit 2;; esac
walk="$dir/$lane"
name=richview-x86_64-pc-windows-msvc.tar.gz
archive="$dir/archive/$commit/$name"
test -z "$(git -C "$checkout" status --porcelain)"
[[ ! -e "$walk" && ! -L "$walk" ]] || { echo "Existing walk attempt at $dir/walk; inspect it before continuing." >&2; exit 1; }
for file in scripts/install.ps1 tools/windows-e2e/stamp.mts tools/windows-e2e/e2e.ps1; do
  git -C "$checkout" show "$commit:$file" | cmp - "$checkout/$file"
done
for tool in aws gtimeout ssh scp ssh-keygen uuidgen curl node jq tar shasum; do command -v "$tool" >/dev/null; done
test -s "$checkout/tools/windows-e2e/first-boot.ps1.template"
test -s "$dir/version"
(cd "$dir/native" && shasum -a 256 -c SHA256SUMS)
(cd "$out" && shasum -a 256 -c warehouse.sha256)
cmp "$dir/build.sha256" <(cat "$dir/native/SHA256SUMS" "$out/warehouse.sha256")
(cd "$(dirname "$archive")" && shasum -a 256 -c "$name.sha256")
if [ "$label" = public ]; then
  test -s "$out/go3-confirmation"
  node "$D_TOOLS/public-test.mjs" "$checkout/tools/windows-e2e/e2e.ps1" "$dir/published-walk.ps1" "${D_PREV_TAG:?previous release tag}"
  export WINDOWS_E2E_SCRIPT="$dir/published-walk.ps1"
  base=https://github.com/1vecera/richview/releases/download/${D_TAG:?release tag}
  curl -fsSL "$base/install.ps1" -o "$dir/published-install.ps1"
  cmp "$dir/published-install.ps1" "$D_SEED/release/install.ps1"
  curl -fsSL https://github.com/1vecera/richview/releases/latest/download/install.ps1 -o "$dir/latest-install.ps1"
  cmp "$dir/latest-install.ps1" "$dir/published-install.ps1"
  curl -fsSL "$base/$name" | shasum -a 256 > "$dir/published-archive.sha256"
  test "$(awk '{print $1}' "$dir/published-archive.sha256")" = "$(awk '{print $1}' "$archive.sha256")"
fi
phase=private-walk; if [[ $label = public ]]; then phase=public; fi
node "$D_TOOLS/cost.mjs" "$out" "$phase"
if gtimeout --signal=TERM 16m /bin/bash "$D_TOOLS/walk.sh" "$walk" "$commit" "$archive" "$dir/native/richview-editor.exe" "$out/warehouse.duckdb" "$dir/build.sha256" > "$dir/$lane-console.log" 2>&1; then :
else
  rc=$?
  if [[ ! -e "$walk/launch-token" && ! -e "$walk/instance-id" ]]; then
    mkdir -p "$walk"; bash "$D_TOOLS/prelaunch-sweep.sh" "$walk"
  fi
  exit "$rc"
fi
grep -qx terminated "$walk/state-after.txt"
tr -d '\r' < "$walk/e2e.log" > "$dir/e2e.txt"
grep -Fx "$(cat "$dir/version")" "$dir/e2e.txt"
grep -Fx '51 checks passed, 0 failed' "$dir/e2e.txt"
grep -Fx 'E2E PASS' "$dir/e2e.txt"
jq -e --arg commit "$commit" --arg sum "$(awk '{print $1}' "$archive.sha256")" '.commit==$commit and .result=="E2E PASS" and .archiveSha256==$sum and .terminated!=null and (.costUsd|type)=="number"' "$walk/manifest.json" >/dev/null
