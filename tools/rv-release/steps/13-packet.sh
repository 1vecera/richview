#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after 12-record-walk; after 10-cut; inputs; check_cut; selected_full
test "$(ref prod)" = "$BASE"
export SITE_FULL_ID="$R"
node --input-type=module - "$FINAL" "$TREE" "$C" <<'JS'
import {execFileSync} from 'node:child_process';import {receiptNotes} from './tools/rv-gate/lib/store.mjs';import {shipsWindows,readWindowsRun} from './tools/rv-gate/lib/windows.mjs';
const [head,tree,candidate]=process.argv.slice(2), id=process.env.SITE_FULL_ID;
if(shipsWindows(execFileSync('git',['show',head+':assets/install/release.json'],{encoding:'utf8'}))){
 const note=receiptNotes(id).find(n=>n.kind?.startsWith('windows-e2e')&&n.result==='pass'&&n.commit===head&&n.tree===tree&&n.candidate===candidate&&n.receipt===id);
 if(!note)throw Error('Need the private Windows walk note on this candidate/full.');
 const walk=readWindowsRun(note.run,head);if(!walk.ok||walk.summary.archiveSha256!==note.archiveSha256||!walk.summary.terminated)throw Error('Private Windows evidence mismatch: '+walk.problems.join(', '));console.log('Private Windows walk:',note.run,walk.summary.archiveSha256);
}
JS
node "$GATE" report --from origin/prod --to "$C" | tee "$HERE/report.log"
packet=$(sed -n 's/^Report: //p' "$HERE/report.log"); test -s "$packet"; grep -Fq "$R" "$packet"
site_packet "$packet"; packet=$(get PACKET); printf 'Review the visuals, coverage, Windows evidence and unrun steps in %s.\n' "$packet"
finish
