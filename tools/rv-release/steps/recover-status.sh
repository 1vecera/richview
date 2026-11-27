#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
jq '{phase,updateStage:.update.stage,mailStage:.mailSwitch.stage}' "$STATE"
require_aws
phase=$(jq -er .phase "$STATE")
case "$phase" in
  update-pending) deploy status pending --stage production | tee "$HERE/resume.plan";;
  mail-switch-pending) id=$(jq -er .mailSwitch.commandId "$STATE"); awsr ssm get-command-invocation --command-id "$id" --instance-id "$(jq -er .instance "$STATE")" --query '[Status,ResponseCode]' --output text;;
  provisioned) jq '{candidate,mailTransport,updateStage:.update.stage}' "$STATE";;
  *) die 'Inspect the controller state before repair.';;
esac
printf "%s\n" "Read the stage and command status. Only a terminal archive reconciliation plan accepts resume."
finish
