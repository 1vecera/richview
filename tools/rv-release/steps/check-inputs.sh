#!/usr/bin/env bash
# Read-only preflight for a release (the candidate named by RV_RELEASE_DESCRIPTOR). It reads refs, receipts, kept builds, the previous release's evidence, the public repository,
# the hosted controller's local state and Lenovo's helper files; it changes nothing, writes no journal, needs no AWS credentials and
# starts no build, browser or Windows machine. Run it with plain bash (not through `run`: a failing check must not latch FAILED).
#   bash steps/check-inputs.sh [--no-lenovo] [--no-network]
# Exit 0 when every check passes. Checks that a later step makes true (the Windows ceiling answer, the dry run, the deploy worktree)
# are reported as WAIT, not FAIL.
set -uo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
set +e; trap - ERR
CN=$(dget .candidate); CNUM=${CN#candidate/}; OLD8=${OLD:0:8}; BEFORE8=${BEFORE:0:8}; NEXTN=$((CNUM + 1)); CEIL=$(dget .windows.ceilingUsd); ASK=$(dget .windows.ask); YES=$(dget .windows.yes)
lenovo=yes; network=yes
for arg in "$@"; do case $arg in --no-lenovo) lenovo=no;; --no-network) network=no;; *) die 'Use --no-lenovo or --no-network.';; esac; done
fail=0
ok() { printf 'PASS  %s\n' "$1"; }
bad() { printf 'FAIL  %s\n' "$1"; fail=1; }
wait_for() { printf 'WAIT  %s\n' "$1"; }
check() { local label=$1; shift; if "$@" >/dev/null 2>&1; then ok "$label"; else bad "$label"; fi; }
section() { printf '\n== %s\n' "$1"; }
node=$(command -v node)

section 'Source, refs and receipts'
check "FINAL $FINAL is a commit (tree $TREE)" git cat-file -e "$FINAL^{commit}"
if [[ $network = yes ]]; then
  check "origin $CN is FINAL" test "$(ref "$CN")" = "$FINAL"
  check 'origin pre-prod is FINAL' test "$(ref pre-prod)" = "$FINAL"
  check "origin prod is $OLD_CANDIDATE ($OLD8)" test "$(ref prod)" = "$OLD"
  check "no candidate/$NEXTN exists" test -z "$(ref "candidate/$NEXTN")"
else wait_for 'refs not read (--no-network)'; fi
check "prod ($OLD8) is an ancestor of FINAL" git merge-base --is-ancestor "$OLD" "$FINAL"
check "full receipt $RFULL passed on FINAL and this tree" jq -e --arg c "$FINAL" --arg t "$TREE" --arg r "$RFULL" '.kind=="full" and .result=="pass" and .cancelled!=true and .commit==$c and .tree==$t and .id==$r and .prod=="'"$OLD"'"' "$E/receipts/$RFULL.json"
check "promotion receipt names FINAL, this tree and base $OLD8" jq -e --arg c "$FINAL" --arg t "$TREE" --arg r "$RFULL" '.receipt==$r and .commit==$c and .tree==$t and .base=="'"$OLD"'"' "$E/receipts/$RFULL.promotion.json"
check 'the gate would select this receipt for FINAL' node --input-type=module -e "
import {listReceipts} from './tools/rv-gate/lib/store.mjs';
const r=listReceipts().filter(r=>r.commit==='$FINAL'&&['full','affected'].includes(r.kind)).at(-1);
if(r?.id!=='$RFULL') throw Error('gate would pick '+r?.id);"
check "no journal, STOP, FINAL pointer or latch exists yet in $RELNAME" bash -c "test ! -e '$HERE/steps.tsv' && ! ls '$HERE'/FAILED* >/dev/null 2>&1 && test ! -e '$L/STOP' && test ! -e '$L/FINAL'"

section 'Kept build and retained manifest'
check 'retained manifest equals the kept build manifest' cmp "$L/retained-full/$RFULL/build-manifest.json" "$KEPT/build-manifest.json"
check 'manifest is the clean full build of FINAL' jq -e --arg c "$FINAL" '.schema==1 and .mode=="full" and .inputs.commit==$c and .inputs.dirty==false' "$KEPT/build-manifest.json"
runtime=$(cat "$KEPT/dist/web/richview_runtime.js" "$KEPT/dist/web/richview_runtime_bg.wasm.gz" | shasum -a 256 | cut -c1-12)
check "kept CLI prints richview $BINVER (commit ${FINAL:0:10}, runtime $runtime)" test "$("$KEPT/richview" --version)" = "richview $BINVER (commit ${FINAL:0:10}, runtime $runtime)"
printf 'INFO  manifest sha256 %s\n' "$(hashfile "$KEPT/build-manifest.json")"
check 'homepage is the reviewed variant A and matches the manifest' "$node" "$S/check-homepage-a.mjs" "$KEPT/dist/web/index.html" "$KEPT/build-manifest.json"
check 'privacy page: approved provider section and one-hour cookie sentence' "$node" "$S/check-privacy.mjs" "$KEPT/dist/web/privacy.html"
for installer in install.sh install.ps1; do check "install page links releases/latest/download/$installer" grep -Fq "https://github.com/1vecera/richview/releases/latest/download/$installer" "$KEPT/dist/web/install.html"; done
for f in index privacy install; do
  a=$(jq -r --arg k "checkout:dist/web/$f.html" '.outputs[$k].sha256' "$OLD_RETAINED/build-manifest.json"); b=$(jq -r --arg k "checkout:dist/web/$f.html" '.outputs[$k].sha256' "$KEPT/build-manifest.json")
  if [[ $a = "$b" ]]; then ok "$f.html is byte-identical to $OLD_CANDIDATE's"; else printf 'INFO  %s.html differs from the previous candidate (%s -> %s)\n' "$f" "${a:0:12}" "${b:0:12}"; fi
done
check 'release notes: first line, no slides heading' bash -c "test \"\$(head -1 '$NOTES')\" = '# RichView ${TAG#v}' && ! grep -Eiq '^(#+[[:space:]]+slides([[:space:].]|\$)|\\*\\*slides[. :]?\\*\\*)' '$NOTES'"

section 'Pins of this release'
check 'the release kit is the committed one (clean_kit)' clean_kit
check 'controller files equal the reviewed hashes' bash -c "cd '$P' && shasum -a 256 -c '$L/$(dget .pins.controllerHashes)'"
check 'AWS config file is the reviewed two-line file' cmp -s "$AWS_CONFIG_FILE" <(printf '[default]\ncli_history = disabled\n')
check "cloudflare/ and wrangler.toml are unchanged from $OLD8" git diff --exit-code "$OLD" "$FINAL" -- cloudflare/ wrangler.toml
check 'the Windows walk driver and first-boot template are unchanged' git diff --exit-code "$OLD" "$FINAL" -- tools/windows-e2e/walk.sh tools/windows-e2e/first-boot.ps1.template

section 'Inputs from the previous release'
check "warehouse fixture of $OLD8 verifies" bash -c "cd '$PREV/windows-$OLD' && shasum -a 256 -c warehouse.sha256"
check "SQL fixture inputs unchanged between $OLD8 and FINAL" git diff --exit-code "$OLD" "$FINAL" -- examples/sql-report/data examples/sql-report/queries
lock_duckdb() { git show "$1:Cargo.lock" | awk 'BEGIN{RS="";ORS="\n\n"} /name = "(libduckdb-sys|duckdb)"/{print}'; }
if [[ -n $(lock_duckdb "$OLD") && $(lock_duckdb "$OLD" | shasum -a 256) = "$(lock_duckdb "$FINAL" | shasum -a 256)" ]]; then ok 'duckdb entries of Cargo.lock are identical in both commits'; else bad 'duckdb entries of Cargo.lock differ'; fi
check 'old archive digest H of the previous release equals the running archive (state file)' test "$(awk -F '\t' '$1=="H"{v=$2} END{print v}' "$OLD_JOURNAL/steps.tsv")" = "$(jq -er .archiveSha256 "$STATE")"
check 'HOST_ON line is well formed' bash -c "awk -F '\t' '\$1==\"HOST_ON\"{v=\$2} END{print v}' '$L/previous/host-on.tsv' | grep -Eq '^[0-9a-f]{64} github,microsoft,google\$'"
check "public-main font inventory and private scan describe $BEFORE8" bash -c "jq -e '.current.commit==\"$BEFORE\"' '$PREV/scan-$BEFORE8-private-scan.json' && test -s '$PREV/scan-$BEFORE8-fonts.json'"
check 'prepare-source.sh exists' test -s "$PREPARE_SOURCE"
check 'Docker wrapper for Lenovo has its pinned digest' test "$(hashfile "$L/lenovo-bin/docker")" = "$(dget .pins.dockerSha256)"
check 'worker history baseline files of the previous candidate exist' bash -c "test -s '$OLD_JOURNAL/worker-attempt.json' && test -s '$OLD_JOURNAL/worker-history-observed.json' && test -s '$ROOT/$(dget .previous.siteDeploy)'"
check 'previous candidate retained manifest digest equals the journaled M' test "$(hashfile "$OLD_RETAINED/build-manifest.json")" = "$(dget .worker.manifestSha256)"

section 'Hosted controller state (local file, no AWS call)'
check "state: provisioned, SMTP, update complete, candidate = $OLD8, volume $(dget .hosted.volume)" jq -e --arg c "$OLD" --arg s "$STACK" --arg v "$(dget .hosted.volume)" '.phase=="provisioned" and .stack==$s and .region=="eu-central-1" and .candidate==$c and .mailTransport=="smtp" and .update.stage=="complete" and .volume==$v' "$STATE"
if jq -e '.previous.updateSnapshot' "$D" >/dev/null 2>&1; then check 'the previous update snapshot is the one in the state file' jq -e --arg s "$(dget .previous.updateSnapshot)" '.update.snapshot==$s' "$STATE"; fi

section 'Public repository and GitHub'
if [[ $network = yes ]]; then
  refs=$(public_refs); old_refs=$("${pub[@]}" ls-remote "$PUBLIC" refs/heads/main "refs/tags/$PREV_TAG" "refs/tags/$PREV_TAG^{}")
  if [[ $(awk '$2=="refs/heads/main"{print $1}' <<<"$old_refs") = "$BEFORE" && $(awk -v t="refs/tags/$PREV_TAG" '$2==t{print $1}' <<<"$old_refs") = "$BEFORE" ]]; then ok "public main is the seed of $PREV_TAG ($BEFORE8) and $PREV_TAG points at it"; else bad "public main or the $PREV_TAG tag is not at $BEFORE8"; fi
  if [[ $refs = "$(printf '%s\trefs/heads/main' "$BEFORE")" ]]; then ok "public refs are exactly main at $BEFORE and no $TAG tag (what 20-seed, 21-push and 30-release require)"; else bad "public refs differ from main at $BEFORE with no $TAG tag"; fi
  check "latest GitHub release is $PREV_TAG with eight assets" bash -c "gh api repos/1vecera/richview/releases/latest | jq -e --arg t '$PREV_TAG' '.tag_name==\$t and .draft==false and .prerelease==false and (.assets|length)==8'"
  check "no GitHub release $TAG exists" bash -c "! gh api repos/1vecera/richview/releases/tags/$TAG >/dev/null 2>&1"
  check 'sign-up is open now (anonymous /sign-up answers 200)' test "$(signup_http_state)" = open
  check "live richview.ai homepage is $OLD_CANDIDATE bytes (manifest digest)" "$node" --input-type=module -e "
import {verifyOldHome} from '$S/worker-history.mjs'; verifyOldHome('$OLD_WORKER');"
  check 'app.richview.ai, its health and reports.richview.ai health answer 200' health
else wait_for 'public refs, GitHub, live site not read (--no-network)'; fi

section 'Windows ceiling, route and cost guard (logic only)'
if [[ $("$node" -e "import('$S/windows/decision.mjs').then(m=>console.log(m.latestAnswer()?'yes':'none'))") = none ]]; then wait_for "no $ASK answer in the board ledger yet: select-route and every Windows step hold, as designed"; else ok "a $ASK answer exists"; fi
check "decision logic: the exact $CEIL label authorizes site and combined for route B and this SHA; a wrong label, route or option does not" "$node" --input-type=module -e "
import {writeFileSync,mkdtempSync} from 'node:fs'; import {tmpdir} from 'node:os'; import {join} from 'node:path';
import {latestAnswer,authorization,permits,eligible} from '$S/windows/decision.mjs';
const dir=mkdtempSync(join(tmpdir(),'rv-ledger-')), file=join(dir,'answers.jsonl');
const good={at:'2026-01-02T03:04:05+00:00',ask:'$ASK',option:'$CEIL.00',label:$(jq -c '.windows.yes' "$D"),note:'test',via:'test'};
writeFileSync(file,JSON.stringify({...good,ask:'$ASK-earlier',option:'99.00',label:'Yes, 99 USD'})+'\n'+JSON.stringify(good)+'\n');
const a=latestAnswer(file); if(a.option!=='$CEIL.00') throw Error('latest');
for(const s of ['site','combined']){const r=authorization('B',s,'$FINAL',a); if(r.limit!==$CEIL||r.date!=='2026-01-02'||r.scope!==s) throw Error('auth '+s);}
let threw=0; for(const f of [()=>authorization('A','site','$FINAL',a),()=>authorization('B','bonus','$FINAL',a),()=>authorization('B','site','$FINAL',null)]){try{f()}catch{threw++}} if(threw!==3) throw Error('should refuse');
writeFileSync(file,JSON.stringify({...good,label:'Yes, $CEIL USD'})+'\n'); try{latestAnswer(file);throw Error('wrong label accepted')}catch(e){if(e.message==='wrong label accepted')throw e}
if(!permits('combined','bonus')||permits('site','bonus')||!permits('site','site')||!permits('combined','public')) throw Error('permits');"
check 'cost guard counts only this release folders: settled spend is 0 with no windows-<sha> folder' bash -c "node '$S/windows/cost.mjs' '$WIN' settled | jq -e '.actual==0 and .projected==0'"
check "e2e.ps1 of FINAL has the two lines the public walk rewrites (previous tag $PREV_TAG)" bash -c "t=\$(mktemp -d \"\${TMPDIR:-/tmp}/rv-check.XXXXXX\") && git show '$FINAL:tools/windows-e2e/e2e.ps1' > \$t/e2e.ps1 && node '$S/windows/public-test.mjs' \$t/e2e.ps1 \$t/out.ps1 '$PREV_TAG' && grep -Fq 'releases/download/$PREV_TAG/install.ps1' \$t/out.ps1 && grep -Fq 'irm https://github.com/1vecera/richview/releases/latest/download/install.ps1 | iex' \$t/out.ps1"

section 'Deploy state (made by 40-dry-run)'
wtsite="$ROOT/.claude/worktrees/site-deploy-$CNUM"
if [[ -e $wtsite ]]; then printf 'INFO  %s already exists\n' "$wtsite"; else wait_for "deploy worktree $wtsite does not exist; 40-dry-run creates it and runs npm ci there"; fi
check "no rollback.json for $CN yet (a second deploy must not overwrite a saved rollback)" test ! -e "$L/site-deploy-$CNUM/rollback.json"

section 'Host tools and disk'
for tool in aws gtimeout ssh scp ssh-keygen uuidgen curl jq tar shasum gh pw rsync git; do check "tool: $tool" command -v "$tool"; done
check 'node is v26.5.0' test "$("$node" --version)" = v26.5.0
check 'at least 16 GiB free (the Windows guard needs it for the whole run)' test "$(df -k "$L" | awk 'NR==2{print $4}')" -ge 16777216
check 'no gate hold on the release target (.rv-gate-release, .rv-gate-cargo, .rv-perf-quiet)' bash -c "test ! -e '$HOME/tmp/richview-rewrite/target/.rv-gate-release' && test ! -e '$HOME/tmp/richview-rewrite/target/.rv-gate-cargo' && test ! -e '$HOME/tmp/richview-rewrite/target/.rv-perf-quiet' && test ! -e '$HOME/tmp/richview-rewrite/status/full-running'"
check 'the shared release slot heavy-2 (taken by 16-macos) is free' test ! -e "$HOME/tmp/richview-rewrite/locks/heavy-2"
free_gib=$(( $(df -k "$L" | awk 'NR==2{print $4}') / 1048576 ))
if [[ $free_gib -ge 30 ]]; then ok "disk: $free_gib GiB free on Claw Mini (30 or more is comfortable for pack, 16-macos and the deploy worktree)"; else printf 'WARN  disk: %s GiB free on Claw Mini. The Windows guard refuses under 16 GiB at every launch check, and the Windows pack, 16-macos and the site-deploy npm install need several GiB. Free room first (idle target outputs, worktree archives; never source, documents or other sessions).\n' "$free_gib"; fi
if [[ -n ${AWS_ACCESS_KEY_ID:-} ]]; then printf 'INFO  an AWS operator shell is active in this shell\n'; else printf 'INFO  no AWS credentials in this shell (expected for this check)\n'; fi

section 'Lenovo helpers (read-only ssh)'
if [[ $lenovo = yes ]]; then
  kit="$KIT"; lkit="$LENOVO_REPO/${KIT#"$ROOT"/}"
  local_sums=$(cd "$kit" && shasum -a 256 import-bundle.sh native-build.sh verify-installer.sh check-seed-history.sh docker-env/docker | awk '{print $1"  "$2}')
  remote_sums=$(ssh -o BatchMode=yes -o ConnectTimeout=10 lenovo "cd '$lkit' && sha256sum import-bundle.sh native-build.sh verify-installer.sh check-seed-history.sh docker-env/docker" 2>&1)
  check 'Lenovo cutover-kit helpers are byte-identical to Claw Mini (import-bundle, native-build, verify-installer, check-seed-history, docker-env)' test "$local_sums" = "$remote_sums"
  check 'Lenovo repository holds FINAL and its tree' ssh -o BatchMode=yes lenovo "cd $LENOVO_REPO && git cat-file -e $FINAL'^{commit}' && test \"\$(git rev-parse $FINAL'^{tree}')\" = $TREE"
  check 'Lenovo docker wrapper of the previous release equals the pinned digest' bash -c "test \"\$(ssh -o BatchMode=yes lenovo \"sha256sum '$(dirname "$LENOVO_REL")/$(dget .previous.release)/lenovo-bin/docker'\" | cut -d' ' -f1)\" = '$(dget .pins.dockerSha256)'"
  check "Lenovo has no $RELNAME folder or $RELNAME-private worktree yet" bash -c "! ssh -o BatchMode=yes lenovo 'ls $LENOVO_REL $LENOVO_REPO/.claude/worktrees/$RELNAME-private-$FINAL' >/dev/null 2>&1"
  free=$(ssh -o BatchMode=yes lenovo "df -k $LENOVO_HOME | awk 'NR==2{print \$4}'" 2>/dev/null); if [[ $free =~ ^[0-9]+$ && $free -ge 104857600 ]]; then ok 'Lenovo has 100 GiB free on /home'; else bad 'Lenovo has under 100 GiB free on /home'; fi
  check 'Lenovo toolchain: node 26.5.0, wasm-bindgen 0.2.129, docker present' ssh -o BatchMode=yes lenovo 'export PATH=$HOME/.local/share/mise/installs/node/26.5.0/bin:$HOME/.cargo/bin:$PATH; test "$(node --version)" = v26.5.0 && test "$(wasm-bindgen --version)" = "wasm-bindgen 0.2.129" && test -x /usr/bin/docker'
else wait_for 'Lenovo not read (--no-lenovo)'; fi

printf '\n'
if [[ $fail = 0 ]]; then echo 'ALL CHECKS PASSED (WAIT lines are made true by later steps)'; else echo 'SOME CHECKS FAILED'; fi
exit "$fail"
