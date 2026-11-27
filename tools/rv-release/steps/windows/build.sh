#!/bin/bash
# One credential shell, one build machine followed by a fresh walk machine for each archive.
set -euo pipefail
umask 077
export AWS_REGION=eu-central-1 AWS_PAGER='' COPYFILE_DISABLE=1
unset WINDOWS_E2E_SCRIPT WINDOWS_E2E_TYPE WINDOWS_E2E_IMAGE WINDOWS_E2E_HOURLY_USD TYPE IMAGE SETUP
out=$1 seed=$2 private=$3 release_version=$4
shift 4
tools=$(cd "$(dirname "$0")" && pwd)
kit=${RV_WINDOWS_KIT:?Run through the release command}
session=${RV_WIN_SESSION:-$out/aws-build}
runtime=$(cat "$seed/wasm/RUNTIME")
tag=Name=tag:richview,Values=windows-build
session_pid=
have_started=no
note() { printf '== %s %s\n' "$(date -u +%FT%TZ)" "$*"; }
cleanup() {
  local status=$?
  trap - EXIT
  if [ -n "$session_pid" ]; then
    kill -TERM "$session_pid" 2>/dev/null || true
    wait "$session_pid" || true
  fi
  if [ "$have_started" = yes ]; then
    if [[ ! -e "$session/launch-token" && ! -e "$session/instance-id" ]]; then
      bash "$tools/prelaunch-sweep.sh" "$session" || status=1
    elif ! verify_empty; then status=1; fi
  fi
  exit "$status"
}
verify_empty() {
  local remaining
  remaining=$(
    aws ec2 describe-instances --filters "$tag" Name=instance-state-name,Values=pending,running,shutting-down,stopping,stopped --query 'Reservations[].Instances[].InstanceId' --output text &&
    aws ec2 describe-volumes --filters "$tag" --query 'Volumes[].VolumeId' --output text &&
    aws ec2 describe-security-groups --filters "$tag" --query 'SecurityGroups[].GroupId' --output text &&
    aws ec2 describe-snapshots --owner-ids self --filters "$tag" --query 'Snapshots[].SnapshotId' --output text &&
    aws ec2 describe-key-pairs --filters "$tag" --query 'KeyPairs[].KeyPairId' --output text
  ) || return 1
  if [ -n "${remaining//[[:space:]]/}" ]; then
    note "Tagged AWS resources remain: $remaining"
    return 1
  fi
  note "Verified zero live tagged instances, volumes, security groups, snapshots and key pairs."
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
test ! -e "$session"
test -s "$out/runtime.tgz"
test -s "$out/warehouse.duckdb"
[[ ${1:-} = public-only ]] || test -s "$out/private/source.tgz"
[ "$#" -eq 0 ] || test -s "$out/public/source.tgz"
source "$tools/guard.sh"
check_release
mkdir "$session"
have_started=yes
verify_empty
note "Start the Windows Server 2025 build machine."
env -u HC_TERMINAL_ID gtimeout --signal=TERM 44m /bin/zsh -f "$tools/session.sh" "$session" 42 > "$session/session.log" 2>&1 &
session_pid=$!
while [ ! -s "$session/tools-ready" ]; do
  kill -0 "$session_pid" || { tail -15 "$session/session.log"; exit 1; }
  sleep 5
done
grep -q DryRunOperation "$session/terminate-dry-run.txt"
host=$(cat "$session/host")
opts=(-i "$session/key" -o BatchMode=yes -o StrictHostKeyChecking=yes -o "UserKnownHostsFile=$session/known_hosts" -o ConnectTimeout=10 -o ServerAliveInterval=60 -o ServerAliveCountMax=3)
remote() { ssh "${opts[@]}" "Administrator@$host" "$@"; }
put() { scp -q "${opts[@]}" "$1" "Administrator@$host:$2"; }
get() { scp -q "${opts[@]}" "Administrator@$host:$1" "$2"; }
put "$out/runtime.tgz" 'C:/rv/runtime.tgz'
put "$tools/release-build.ps1" 'C:/rv/release-build.ps1'
build() {
  local label=$1 checkout=$2 commit expected dir version
  commit=$(git -C "$checkout" rev-parse HEAD)
  expected="richview $release_version (commit ${commit:0:10}, runtime $runtime)"
  dir="$out/$label"
  mkdir "$dir/native"
  note "Build $label $commit."
  put "$dir/source.tgz" "C:/rv/source-$label.tgz"
  remote "New-Item -ItemType Directory -Force C:\\rv\\src-$label\\dist\\web | Out-Null; tar -xzf C:\\rv\\source-$label.tgz -C C:\\rv\\src-$label; if (\$LASTEXITCODE) { exit \$LASTEXITCODE }; tar -xzf C:\\rv\\runtime.tgz -C C:\\rv\\src-$label\\dist\\web; exit \$LASTEXITCODE"
  remote "powershell -NoProfile -ExecutionPolicy Bypass -File C:\\rv\\release-build.ps1 -Commit $commit -Dir C:\\rv\\src-$label -Out C:\\rv\\out-$label" > "$dir/build-console.log" 2>&1
  tr -d '\r' < "$dir/build-console.log" > "$dir/build.log"
  version=$(sed -n 's/^version: //p' "$dir/build.log")
  test "$version" = "$expected" || { note "Wanted [$expected], got [$version]"; return 1; }
  test "$(grep -cE '^dependents of [^:]+: .*\.dll' "$dir/build.log")" = 3
  grep -qx 'Visual C++ runtime DLLs among them: 0' "$dir/build.log"
  for file in richview.exe richview-sql-worker.exe richview-editor.exe SHA256SUMS; do
    get "C:/rv/out-$label/$file" "$dir/native/$file"
  done
  (cd "$dir/native" && shasum -a 256 -c SHA256SUMS)
  printf '%s\n' "$version" > "$dir/version"
  note "$label $version. Static C runtime confirmed by dumpbin."
}
check_release
if [[ ${1:-} != public-only ]]; then build private "$private"; fi
check_release
if [ "$#" -gt 0 ]; then build public "$seed/checkout"; fi
touch "$session/stop"
wait "$session_pid"
session_pid=
grep -qx terminated "$session/state-after.txt"
test -s "$session/cost.txt"
verify_empty
cat "$session/cost.txt"
