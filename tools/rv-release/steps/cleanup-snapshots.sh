#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
require_aws; words cleanup-go.txt >/dev/null; test "$(get FINAL)" = "$FINAL"
[[ ${1:-} = delete-reviewed-snapshots ]] || die 'Read the deadlines and confirm the recorded IDs before cleanup.'
test "$(awsr sts get-caller-identity --query Account --output text)" = "$ACCOUNT"
check_snap() { awsr ec2 describe-snapshots --snapshot-ids "$1" --owner-ids self --output json | jq -e --arg s "$1" --arg v "$2" --arg a "$ACCOUNT" '.Snapshots|length==1 and .[0].SnapshotId==$s and .[0].VolumeId==$v and .[0].OwnerId==$a' >/dev/null; }
seen=0
for pair in S_UPDATE:UPDATE_VOLUME S_ROLL:ROLL_VOLUME; do
  key=${pair%:*}; volume=${pair#*:}
  if ! awk -F '\t' -v k="$key" '$1==k{found=1} END{exit !found}' "$REC"; then continue; fi
  s=$(get "$key"); v=$(get "$volume"); [[ $v =~ ^vol-[a-f0-9]+$ ]]; seen=1
  [[ $s =~ ^snap-[a-f0-9]+$ ]] || die 'Invalid snapshot ID'
  rows=$(awsr ec2 describe-snapshots --owner-ids self --filters "Name=snapshot-id,Values=$s" --output json)
  if [[ $(jq -er '.Snapshots|length' <<<"$rows") -gt 0 ]]; then
    check_snap "$s" "$v"
    awsr ec2 delete-snapshot --snapshot-id "$s"
  fi
  awsr ec2 describe-snapshots --owner-ids self --filters "Name=snapshot-id,Values=$s" --output json | jq -e '.Snapshots|type=="array" and length==0' >/dev/null
  awsr ec2 list-snapshots-in-recycle-bin --snapshot-ids "$s" --output json | jq -e '.Snapshots|type=="array" and length==0' >/dev/null
  save "deleted-$s" "$(date -u +%FT%TZ)"
done
test "$seen" = 1
finish
