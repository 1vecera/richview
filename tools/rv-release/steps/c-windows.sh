private_walk_dir() {
  local lane
  if awk -F '\t' '$1=="WINDOWS_PRIVATE_WALK"{found=1} END{exit !found}' "$REC"; then
    lane=$(get WINDOWS_PRIVATE_WALK) || return 1
    [[ $lane = private/walk-2 && -s "$HERE/rearm-windows-walk-2.json" && $(hashfile "$HERE/rearm-windows-walk-2.json") = "$(get WINDOWS_WALK_RECONCILIATION)" ]] || return 1
  else
    lane=private/walk
  fi
  printf '%s/%s\n' "$WIN" "$lane"
}
# Check the private archive, walk and receipt for the bound candidate.
check_private_windows() {
  after 11-windows
  local phase=${2:?site or combined}
  [[ $phase = site || $phase = combined ]] && [[ $(get "WINDOWS_$phase") = yes ]] || die 'Missing successful private Windows journal.'
  check_private_walk_evidence "$1" "$phase"
}
check_private_walk_evidence() {
  local phase=$2
  local dir="$WIN/private" n=richview-x86_64-pc-windows-msvc.tar.gz digest
  local walk=${3:-}
  if [[ -z $walk ]]; then walk=$(private_walk_dir); fi
  [[ $walk = "$dir/walk" || $walk = "$dir/walk-2" ]] || return 1
  digest=$(hashfile "$dir/archive/$FINAL/$n")
  (cd "$dir/archive/$FINAL" && shasum -a 256 -c "$n.sha256")
  cmp "$walk/build.sha256" <(cat "$dir/native/SHA256SUMS" "$WIN/warehouse.sha256")
  grep -Fx "$(cat "$dir/version")" "$dir/e2e.txt"; grep -Fx 'E2E PASS' "$dir/e2e.txt"
  jq -e --arg id "$(cat "$walk/instance-id")" '.instance==$id and .terminated!=null and .result=="E2E PASS"' "$walk/manifest.json" >/dev/null
  grep -Fx '51 checks passed, 0 failed' "$dir/e2e.txt"; grep -Fx terminated "$walk/state-after.txt"
  grep -Fq 'zero live tagged instances, volumes, security groups, snapshots or keys' "$dir/sweep.log"
  grep -Fq 'zero live tagged instances, volumes, security groups, snapshots or keys' "$WIN/build-$phase-exit-sweep.log"
  node --input-type=module - "$R" "$C" "$FINAL" "$TREE" "$walk" "$digest" "$1" <<'JS'
import {listReceipts,receiptNotes} from './tools/rv-gate/lib/store.mjs';
import {readWindowsRun} from './tools/rv-gate/lib/windows.mjs';
const [id,candidate,commit,tree,dir,digest,mode]=process.argv.slice(2);
const selected=listReceipts().filter(r=>r.commit===commit&&(['full','affected'].includes(r.kind)||(['prod','hotfix'].includes(r.kind)&&r.candidate===candidate))).at(-1);
if(selected?.id!==id)throw Error('Gate would select another receipt.');
const walk=readWindowsRun(dir,commit);
if(!walk.ok||!walk.summary.terminated||walk.summary.archiveSha256!==digest)throw Error('Packaged archive and Windows walk disagree: '+walk.problems.join(', '));
const notes=receiptNotes(id).filter(n=>n.kind?.startsWith('windows-e2e'));
const valid=n=>n.result==='pass'&&n.receipt===id&&n.commit===commit&&n.tree===tree&&n.candidate===candidate&&n.run===dir&&n.archiveSha256===digest;
if(notes.length>1||notes.some(n=>!valid(n))||(!notes.length&&mode==='required'))throw Error('The selected receipt needs the matching journaled Windows walk note.');
console.log(notes.length?'recorded':'unrecorded');
JS
}
