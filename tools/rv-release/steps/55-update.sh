#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after 54-update-plan
require_smtp_window
archive_args; GO=$(go_text hosted)
F_UPDATE=$(reviewfp "$HERE/update.plan")
save F_UPDATE "$F_UPDATE"; mark_attempt update
run deploy update "$FINAL" "${archive[@]}" "${common[@]}" --expect-plan "$F_UPDATE" --go "$GO" > "$HERE/update.apply" 2>&1
jq -e --arg c "$FINAL" --arg h "$H" --arg old "$OLD" '.phase=="provisioned" and .candidate==$c and .archiveSha256==$h and .mailTransport=="outbox" and .update.stage=="complete" and .update.from.candidate==$old' "$STATE" >/dev/null
test "$(jq -er .update.from.sha256 "$STATE")" = "$(get OLD_SHA256)"
S_UPDATE=$(jq -er '.update.snapshot' "$STATE"); [[ $S_UPDATE =~ ^snap-[0-9a-f]+$ ]]
save S_UPDATE "$S_UPDATE"
awsr ec2 describe-snapshots --snapshot-ids "$S_UPDATE" --query 'Snapshots[].{ID:SnapshotId,Volume:VolumeId,Start:StartTime,State:State}' --output json > "$HERE/update-snapshot.json"
jq -e --arg v "$(jq -er .volume "$STATE")" 'length==1 and .[0].State=="completed" and .[0].Volume==$v' "$HERE/update-snapshot.json" >/dev/null
save UPDATE_VOLUME "$(jq -er .volume "$STATE")"
node -e 'const [s]=JSON.parse(require("fs").readFileSync(process.argv[1]));console.log(s.ID,"delete before",new Date(Date.parse(s.Start)+5*86400000).toISOString())' "$HERE/update-snapshot.json" | tee "$HERE/update-deadline.txt"
bash "$S/provider-check.sh"; signup status | grep -Fx "production: sign-up paused"
run deploy plan-rollback "$OLD" "${common[@]}" | tee "$HERE/rollback-inspection.plan"
printf "%s\n" "Inspect update.apply and rollback-inspection.plan for FINAL running and $OLD_CANDIDATE ($OLD) retained."
finish
