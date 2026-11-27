#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after recover-roll-plan
archive_args; require_aws; GO=$(words recovery-go.txt)
F_ROLL=$(reviewfp "$HERE/rollback-current.plan"); save F_ROLL "$F_ROLL"
deploy rollback "$OLD" "${common[@]}" --expect-plan "$F_ROLL" --go "$GO"
jq -e --arg c "$OLD" '.phase=="provisioned" and .candidate==$c and .mailTransport=="outbox"' "$STATE" >/dev/null
s=$(jq -er .update.snapshot "$STATE"); [[ $s =~ ^snap-[0-9a-f]+$ ]]; save S_ROLL "$s"
awsr ec2 describe-snapshots --snapshot-ids "$s" --query 'Snapshots[].{ID:SnapshotId,Volume:VolumeId,Start:StartTime,State:State}' --output json | tee "$HERE/rollback-snapshot.json"
save ROLL_VOLUME "$(jq -er .volume "$STATE")"
node -e 'const [s]=JSON.parse(require("fs").readFileSync(process.argv[1]));console.log(s.ID,"delete before",new Date(Date.parse(s.Start)+5*86400000).toISOString())' "$HERE/rollback-snapshot.json" | tee "$HERE/rollback-deadline.txt"
health; buttons on
printf "%s\n" "Assign this rollback snapshot its own 120-hour deletion deadline."
finish
