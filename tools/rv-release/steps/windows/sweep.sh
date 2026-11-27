#!/bin/bash
set -eu
export AWS_REGION=eu-central-1 AWS_PAGER=''
tag=Name=tag:richview,Values=windows-build
check() {
  local ids
  ids=$(aws ec2 "$@") || return 1
  [ -z "$(printf %s "$ids" | tr -d '[:space:]')" ] || { printf 'Tagged resources remain: %s\n' "$ids"; return 1; }
}
check describe-instances --filters "$tag" Name=instance-state-name,Values=pending,running,shutting-down,stopping,stopped --query 'Reservations[].Instances[].InstanceId' --output text
check describe-volumes --filters "$tag" --query 'Volumes[].VolumeId' --output text
check describe-security-groups --filters "$tag" --query 'SecurityGroups[].GroupId' --output text
check describe-snapshots --owner-ids self --filters "$tag" --query 'Snapshots[].SnapshotId' --output text
check describe-key-pairs --filters "$tag" --query 'KeyPairs[].KeyPairId' --output text
date -u '+%FT%TZ zero live tagged instances, volumes, security groups, snapshots or keys'
