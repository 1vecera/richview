#!/usr/bin/env bash
set -euo pipefail
r=${RV_RELEASE_DIR:?Run through steps/run, which sets RV_RELEASE_DIR}
D=${RV_RELEASE_DESCRIPTOR:?Run through steps/run, which sets RV_RELEASE_DESCRIPTOR}
steps=$(cd "$(dirname "$0")/.." && pwd -P)
repo=${RV_RELEASE_REPO:-$(git -C "$r" rev-parse --show-toplevel)}
tag=$(jq -er .tag "$D"); assets_name=$(jq -er .assets "$D")
cd "$repo"
source ~/tmp/richview-rewrite/env.sh
kit=${RV_CUTOVER_KIT:-$repo/tmp/team/launch/cutover-kit}
rr=${LENOVO_REPO:?Run through the release command}/${r#"$repo"/}
final=$1
short=${final:0:8}
seed=$r/next-$short
public=$(sed -n 's/^public=//p' "$seed/manifest")
mac=richview-aarch64-apple-darwin.tar.gz
linux=richview-x86_64-unknown-linux-gnu.tar.gz
win=richview-x86_64-pc-windows-msvc.tar.gz
until [[ -s "$r/linux-$short.done" && -s "$seed/windows/$win" && -s "$seed/windows/$win.sha256" ]]; do
  [[ ! -e $r/STOP && $(cat "$r/FINAL") = "$final" ]]; sleep 20
done
[[ ! -e $r/STOP && $(cat "$r/FINAL") = "$final" ]]
digest=$(awk 'NR==1{print $1}' "$seed/windows/$win.sha256")
[[ "$digest" =~ ^[0-9a-f]{64}$ ]]
[[ $(shasum -a 256 "$seed/windows/$win" | cut -d' ' -f1) == "$digest" ]]
if [[ ! -d "$seed/release" ]]; then
  bash "$kit/stage-release.sh" "$seed" "$tag" "$seed/macos/$mac" "$seed/linux/$linux" "$seed/windows/$win" > "$r/stage-$short.log" 2>&1
fi
(
cd "$seed/release"
[[ $(find . -mindepth 1 -maxdepth 1 | wc -l) -eq 8 ]]
for asset in install.ps1 install.sh "$mac" "$mac.sha256" "$linux" "$linux.sha256" "$win" "$win.sha256"; do
  test -f "$asset"
  printf '%s\t%s\t%s\n' "$(shasum -a 256 "$asset" | cut -d' ' -f1)" "$(stat -f %z "$asset")" "$asset"
done
for installer in install.sh install.ps1; do
  grep -Fq "release_commit='$public'" "$installer"
  grep -Fq "release_base_url='https://github.com/1vecera/richview/releases/download/$tag'" "$installer"
done
) > "$r/$assets_name"
[[ ! -e $r/STOP && $(cat "$r/FINAL") = "$final" ]]
bash "$steps/kit/verify-staged.sh" "$seed" "$kit" "$r/mac-server-$short.log" > "$r/verify-macos-$short.log" 2>&1
for attempt in {1..12}; do
  [[ ! -e $r/STOP && $(cat "$r/FINAL") = "$final" ]]
  if rsync --rsync-path="systemd-run --user --scope -p CPUQuota=800% -p MemoryHigh=16G nice -n 19 ionice -c3 rsync" -a --partial "$seed/release/" "lenovo:$rr/next-$short/release/" &&
     rsync --rsync-path="systemd-run --user --scope -p CPUQuota=800% -p MemoryHigh=16G nice -n 19 ionice -c3 rsync" -a --partial "$r/$assets_name" "lenovo:$rr/"; then break; fi
  if [[ $attempt == 12 ]]; then exit 1; fi
  sleep 15
done
[[ ! -e $r/STOP && $(cat "$r/FINAL") = "$final" ]]
ssh -o BatchMode=yes -o ConnectTimeout=10 lenovo "systemd-run --user --scope -p CPUQuota=800% -p MemoryHigh=16G nice -n 19 ionice -c3 bash -s -- '$rr' '$short' '$assets_name' '${kit/#$repo/$LENOVO_REPO}'" > "$r/verify-linux-$short.log" 2>&1 <<'REMOTE_VERIFY'
set -euo pipefail
rr=$1 short=$2 assets=$3 kit=$4
export PATH=$HOME/.local/share/mise/installs/node/26.5.0/bin:$HOME/.local/bin:$PATH
cd "$rr/next-$short/release"
awk -F "\\t" '{print $1 "  " $3}' "$rr/$assets" | sha256sum -c -
bash "$rr/verify-staged.sh" "$rr/next-$short" "$kit" "$rr/linux-server-$short.log"
REMOTE_VERIFY
rsync --rsync-path="systemd-run --user --scope -p CPUQuota=800% -p MemoryHigh=16G nice -n 19 ionice -c3 rsync" -a --partial "lenovo:$rr/linux-server-$short.log" "$r/"
[[ ! -e $r/STOP && $(cat "$r/FINAL") = "$final" ]]
date > "$r/stage-$short.done"
