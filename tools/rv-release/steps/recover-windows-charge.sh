#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
lane=${1:?aws-build or private/walk or private/walk-2 or public/walk or bonus-build}
case "$lane" in aws-build|bonus-build|private/walk|private/walk-2|public/walk) ;; *) die 'Unknown EC2 lane.';; esac
require_aws
test "$(awsr sts get-caller-identity --query Account --output text)" = "$ACCOUNT"
dir="$WIN/$lane"
if [[ -d $dir && ! -e "$dir/launch-token" && ! -e "$dir/instance-id" ]]; then
  bash "$S/windows/prelaunch-sweep.sh" "$dir"; finish; exit 0
fi
token=$(cat "$dir/launch-token"); [[ $token =~ ^[A-Fa-f0-9-]{36}$ ]]
awsr ec2 describe-instances --filters "Name=client-token,Values=$token" "Name=tag:richview,Values=windows-build" --query 'Reservations[].Instances[].{id:InstanceId,state:State.Name,token:ClientToken,start:LaunchTime}' --output json > "$HERE/launch-reconcile.json"
# Reserve the entire elapsed interval to inspection, including termination and a full extra minute.
jq -e --arg token "$token" 'length==1 and .[0].state=="terminated" and .[0].token==$token' "$HERE/launch-reconcile.json" >/dev/null || die 'No unique terminated instance for the token. Hold further launches and inspect EC2 history.'
bash "$S/windows/sweep.sh" > "$HERE/reconcile-sweep.log"
node --input-type=module - "$HERE/launch-reconcile.json" "$dir/reconciled-charge.json" <<'JS'
import {readFileSync,writeFileSync} from 'node:fs';
const [input,out]=process.argv.slice(2), [row]=JSON.parse(readFileSync(input));
const minutes=Math.ceil((Date.now()-Date.parse(row.start))/60000)+1;
if(!Number.isFinite(minutes)||minutes<1)throw Error('Invalid EC2 launch time.');
writeFileSync(out,JSON.stringify({...row,upperUsd:Math.ceil(minutes/60*(0.7754+0.005+80*0.0952/730)*10000)/10000})+'\n',{flag:'wx'});
JS
finish
