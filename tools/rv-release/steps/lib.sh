#!/usr/bin/env bash
# shellcheck disable=SC2034 # These variables are shared by the child steps.
# The release kit's common library (DEL-83). Every pin of a release (the candidate, what it replaces, the Windows ceiling ask, the notes)
# comes from the release descriptor RV_RELEASE_DESCRIPTOR (release.json, see tools/rv-release/lib/descriptor.mjs); nothing is hard-coded here.
# Steps belong to parts (tools/rv-release/parts.json). A failure latches its own part (FAILED.<part>), never the others.
set -euo pipefail
set +x +v
export -n RICHVIEW_SMTP_PASSWORD
umask 077
die() { printf '%s\n' "$*" >&2; exit 1; }
S=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P); KITROOT=$(dirname "$S")
D=${RV_RELEASE_DESCRIPTOR:?Set RV_RELEASE_DESCRIPTOR to the release.json of this release}
[[ -f $D ]] || die "No release descriptor at $D"
L=${RV_RELEASE_DIR:-$(dirname "$D")} # the release's working folder: journal, builds, evidence (not in git)
dget() { jq -er "$1" "$D"; }
ROOT=${RV_RELEASE_REPO:-$(git -C "$L" rev-parse --show-toplevel)}; cd "$ROOT"
export PATH="$HOME/tmp/richview-rewrite/node-v26.5.0/bin:$PATH"
[[ $(node --version) = v26.5.0 ]] || die 'Use the release Node 26.5.0 installation.'
[[ ${RV_GATE_HOME:-$HOME/.local/share/richview-gate} = "$HOME/.local/share/richview-gate" ]] || die 'Select the Claw Mini evidence store.'
FINAL=${RV_RELEASE_FINAL:?Export the reviewed 40-character commit first}
[[ $FINAL = $(dget .final) ]] || die "This release descriptor is for $(dget .final), not $FINAL."
[[ $FINAL =~ ^[0-9a-f]{40}$ && $(git rev-parse "$FINAL^{commit}") = "$FINAL" ]] || die 'Unknown candidate source.'
# The commit the release replaces: what private prod, the website and the hosted archive run before it.
OLD=$(dget .old); BASE=$OLD; OLD_CANDIDATE=$(dget .oldCandidate)
RELNAME=$(dget .name); RELNUM=${RELNAME#release-}
PREV="$L/../$(dget .previous.release)" # the previous release's folder
OLD_JOURNAL="$PREV/$(dget .previous.journal)"; OLD_RETAINED="$PREV/$(dget .previous.retainedFull)"
TREE=$(git rev-parse "$FINAL^{tree}"); GATE="$ROOT/tools/rv-gate/rv-gate.mjs"
E="$HOME/.local/share/richview-gate/evidence"; KEPT="$HOME/tmp/richview-rewrite/bin-release/${FINAL:0:10}"
HERE="$L/go-record-${FINAL:0:8}"; REC="$HERE/steps.tsv"; STEP=$(basename "$0" .sh)
C=; if [[ -f $REC ]]; then C=$(awk -F '\t' '$1=="CANDIDATE"{v=$2} END{print v}' "$REC"); fi
[[ -z $C || $C =~ ^candidate/[1-9][0-9]*$ ]] || die 'Malformed journaled candidate name.'
set -E
failed_prereq() {
  local file=$1 line=$2 raw
  raw=$(sed -n "${line}p" "$file" 2>/dev/null) || raw=
  case "$raw" in
    *PASSWORD*|*SECRET*|*TOKEN*|*password*|*credential*) raw='protected credential prerequisite';;
    *'test '*|*'[[ '*|*'jq -e'*|*'grep -'*|*'cmp '*) raw=${raw:0:240};;
    *) raw='command or prerequisite at this source line';;
  esac
  printf 'Step %s failed at %s:%s — %s. Evidence %s\n' "$STEP" "$file" "$line" "$raw" "$HERE" >&2
}
trap 'failed_prereq "${BASH_SOURCE[0]}" "$LINENO"' ERR
# The site deployer is versioned in this kit (deploy-kept-reviewed.mjs); it takes its repository and output folder from the environment.
SITE="$S/deploy-kept-reviewed.mjs"; SOUT="$L/site-deploy-${C#candidate/}"; export RV_REPO="$ROOT" RV_SITE_OUT="$SOUT"
SITEWT="$ROOT/.claude/worktrees/site-deploy-${C#candidate/}"; CFG="$SITEWT/wrangler.toml"; WR="$SITEWT/node_modules/.bin/wrangler"
OLD_WORKER=$(dget .oldWorker) # the Worker version the previous candidate deployed
RFULL=$(dget .full)
OPS=${RV_RELEASE_OPS:-"$L/../ops"}
hashfile() { shasum -a 256 "$1" | awk '{print $1}'; }
get() { awk -F '\t' -v k="$1" '$1==k{v=$2} END{if(!length(v))exit 1;print v}' "$REC"; }
save() { [[ -f $REC && -n $2 && $2 != *$'\t'* && $2 != *$'\n'* ]] || die 'Invalid go record.'; printf '%s\t%s\n' "$1" "$2" >> "$REC"; }
after() { [[ $(get FINAL) = "$FINAL" && $(get "ok-$1") = yes ]] || die "Complete $1 first."; }
finish() { save "ok-$STEP" yes; printf 'STEP %s OK\n' "$STEP"; }
# The part a step belongs to (parts.json: its prepare, checks, deploy, closing or rollback lists). Steps outside every part are shared.
part_of() { jq -r --arg s "$1" --arg a "${2:-}" '[.parts[] | select(any((.prepare, .checks, .deploy, .closing, .rollback.steps)[]?; .step == $s and (.args | join(" ")) == $a)) | .id] as $exact | if ($exact | length) > 0 then $exact[0] else ([.parts[] | select(any((.prepare, .checks, .deploy, .closing, .rollback.steps)[]?; .step == $s)) | .id] | first // "shared") end' "$KITROOT/parts.json"; }
PART=$(part_of "$STEP" "${1:-}")
guard() { if [[ -f "$HERE/selected-route.json" ]]; then cmp -s "$HERE/selected-route.json" "$L/selected-route.json" || die 'A different route is selected. Preserve this journal and its attempts.'; fi
  [[ ! -e "$HERE/FAILED.$PART" && ! -e "$HERE/FAILED.shared" && ! -e "$L/STOP" && ( ! -e "$L/FINAL" || $(cat "$L/FINAL") = "$FINAL" || $STEP = c-* || ${RV_HOTFIX_ROUTE:-} = yes ) ]] || die "Release part $PART held by STOP or a latched failure."; }
ref() { git ls-remote --heads origin "refs/heads/$1" | cut -f1; }
check_cut() { test "$(ref "$C")" = "$FINAL"; }
check_prod() { check_cut; test "$(ref prod)" = "$FINAL"; }
# Daniel's own words for a recovery action (DEL-48 rollbacks other than the Worker's, and clean-up): one dated file for each scope.
words() { local w; [[ -f "$HERE/${1:?name the approval file}" && ! -L "$HERE/${1:?name the approval file}" ]] || die "Missing exact approval in ${1:?name the approval file}."
  w=$(cat "$HERE/${1:?name the approval file}"); [[ ${#w} -ge 8 && $w != *$'\n'* && $w != *$'\r'* && $w != *'<'* && $w != *'>'* && $w != *'«'* && $w != *'»'* ]] || die 'Invalid approval.'
  if grep -Eiq 'placeholder|TODO|TBD|\{\{|^ *$' "$HERE/${1:?name the approval file}"; then die 'Replace placeholder with the actual go.'; fi
  printf '%s' "$w"; }
# The go for a part: Daniel's words from the one go that covers it, and the evidence it is bound to (tools/rv-release/lib/go.mjs). Refuses when no go covers the part
# or the evidence changed after the go was first used. `go_text <part> [prepare]`.
go_text() { node "$KITROOT/release.mjs" go-text --release "$D" --part "$1" ${2:+--stage "$2"}; }
attempt() { (set -C; date -u +%FT%TZ > "$HERE/attempt-$1") || die "Inspect the recorded $1 attempt before any retry."; }
clean_kit() { test -z "$(git -C "$KITROOT" status --porcelain --untracked-files=no -- "$KITROOT")" || die 'The release kit differs from git. The kit that runs a release is the committed one.'; }
inputs() {
  R=$(get R); M=$(get M)
  [[ $R = "$RFULL" ]] || die 'Use the reviewed full receipt of this release descriptor.'
  [[ $R =~ ^full-[0-9]{8}T[0-9]{6}Z-[a-f0-9]{10}$ && ${R##*-} = "${FINAL:0:10}" && $M =~ ^[a-f0-9]{64}$ ]]
  jq -e --arg c "$FINAL" --arg t "$TREE" --arg r "$R" '.kind=="full" and .result=="pass" and .cancelled!=true and .commit==$c and .tree==$t and .id==$r' "$E/receipts/$R.json" >/dev/null
  test "$(hashfile "$HERE/full-build-manifest.json")" = "$M"; cmp "$HERE/full-build-manifest.json" "$KEPT/build-manifest.json"
  # The deployer is the one in git (deploy-kept-reviewed.mjs); a changed or uncommitted kit is refused.
  clean_kit
  siteargs=(--candidate "$C" --receipt "$R" --manifest-sha256 "$M"); }
worker() { "$WR" deployments list --json --config "$CFG" | jq -er 'max_by(.created_on).versions | select(length==1 and .[0].percentage==100) | .[0].version_id'; }
clean_tree() { test "$(git -C "$SITEWT" rev-parse HEAD)" = "$FINAL"; test -f "$(git -C "$SITEWT" rev-parse --absolute-git-dir)/locked"; test -z "$(git -C "$SITEWT" status --porcelain --untracked-files=normal)"; }
selected_full() {
  node --input-type=module - "$FINAL" "$C" "$R" <<'JS'
import {listReceipts} from './tools/rv-gate/lib/store.mjs';
const [head,candidate,id]=process.argv.slice(2), r=listReceipts().filter(r=>r.commit===head&&(['full','affected'].includes(r.kind)||(['prod','hotfix'].includes(r.kind)&&r.candidate===candidate))).at(-1);
if(r?.id!==id) throw Error('The gate would select a different receipt.');
JS
}
STEPS="$S"; TAG=$(dget .tag); PREV_TAG=$(dget .previousTag)
# The CLI prints the workspace version of its build; a tag and the CLI can differ when the workspace version is not bumped.
BINVER=$(dget .binaryVersion); [[ $(git show "$FINAL:Cargo.toml" | awk -F '"' '/^version = /{print $2; exit}') = "$BINVER" ]] || die 'The CLI version expected by this release differs from the workspace version of FINAL.'
BEFORE=$(dget .publicBefore) # public main before this release: the parent of the seed
NOTES="$L/$(dget .notes)"; ASSET_LIST="$L/$(dget .assets)"; COMPAT="$L/compat-$RELNUM.md"
NEXT="$L/next-${FINAL:0:8}"; WIN="$L/windows-$FINAL"
# What a release still reads from folders outside git (the cutover kit, the hosted provider, the Windows source preparation): override by environment.
KIT=${RV_CUTOVER_KIT:-"$ROOT/tmp/team/launch/cutover-kit"}
export RV_HOSTED_DEPLOY=${RV_HOSTED_DEPLOY:-"$ROOT/tmp/team/launch/hosted-deploy"} RV_WINDOWS_KIT=${RV_WINDOWS_KIT:-"$ROOT/tmp/team/launch/d16/windows"} # outside git: the hosted provider and the Windows tools
P=${RV_HOSTED_PROVIDER:-"$RV_HOSTED_DEPLOY/production-candidate-provider"}; STATE="$P/../production-candidate/aws-production-state.json"
PREPARE_SOURCE=${RV_PREPARE_SOURCE:-"$L/../release-27/windows-tools/prepare-source.sh"}
# The build host (Lenovo) keeps its checkout in the same place below its own home folder, which the descriptor names (never written in git).
LENOVO_HOME=$(dget .hosts.lenovoHome); LENOVO_REPO="$LENOVO_HOME/${ROOT#"$HOME"/}"; LENOVO_REL="$LENOVO_REPO/${L#"$ROOT"/}" # the same folder on Lenovo
export LENOVO_HOME LENOVO_REPO
A="$L/go5-private/richview-x86_64-unknown-linux-gnu.tar.gz"
PUBLIC=git@github.com:1vecera/richview.git
pub=(env -u GIT_CONFIG_COUNT -u GIT_CONFIG_PARAMETERS GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null GIT_CONFIG_NOSYSTEM=1 git)
REGION=eu-central-1; ACCOUNT=762052017888; STACK=$(dget .hosted.stack); ROLE="$STACK-host"; PARAM=/richview/cowork/production/sign-in
ulimit -c 0
unset BASH_ENV ENV NODE_OPTIONS NODE_DEBUG NODE_DEBUG_NATIVE LD_PRELOAD LD_AUDIT DYLD_INSERT_LIBRARIES PYTHONINSPECT PYTHONSTARTUP SSLKEYLOGFILE AWS_PROFILE AWS_DEFAULT_PROFILE
export AWS_CONFIG_FILE="$P/sign-in-aws-config" AWS_SHARED_CREDENTIALS_FILE=/dev/null AWS_PAGER='' AWS_CLI_AUTO_PROMPT=off
cmp -s "$AWS_CONFIG_FILE" <(printf '[default]\ncli_history = disabled\n') || die 'AWS configuration mismatch'
require_aws() { [[ -n ${AWS_ACCESS_KEY_ID:-} && -n ${AWS_SECRET_ACCESS_KEY:-} && -z ${HISTFILE:-} ]] || die 'Use the single AWS operator shell'; }
run() { guard; "$@"; }
mark_attempt() { attempt "$1"; }
load_keys() { local key value; for key in "$@"; do value=$(get "$key"); printf -v "$key" '%s' "$value"; done; }
awsr() { aws --region "$REGION" --no-cli-pager "$@"; }
deploy() { node "$P/deploy.mjs" "$@"; }
smtp() { RICHVIEW_SMTP_PASSWORD="${RICHVIEW_SMTP_PASSWORD:?Missing SMTP password}" RICHVIEW_MAIL_FROM=accounts@richview.ai RICHVIEW_SMTP_HOST=smtp.mx.cloudflare.net RICHVIEW_SMTP_PORT=465 RICHVIEW_SMTP_USER=api_token node "$P/deploy.mjs" "$@"; }
forward() { RV_FORWARD_DIR="$HERE" bash "$STEPS/with-forward.sh" "$@"; }
signup() { forward node "$P/../signups.mjs" --stage production "$1"; }
require_smtp_window() { [[ ${RV_SMTP_WINDOW:-} = "$PPID" ]] || die 'Use the numbered Go 5 prompt; its parent holds the SMTP credential.'; }
ok200() { test "$(curl -sS --max-time 30 -o /dev/null -w '%{http_code}' "$1")" = 200; }
reviewfp() { local found entered; found=$(sed -nE 's/^Reviewed (mail switch|update|resume) SHA-256: ([0-9a-f]{64})$/\2/p' "$1"); [[ $found =~ ^[0-9a-f]{64}$ ]] || return 1; read -r -p 'Type the fingerprint of the plan you reviewed: ' entered; test "$entered" = "$found" || return 1; printf '%s\n' "$found"; }
public_refs() { "${pub[@]}" ls-remote "$PUBLIC" refs/heads/main "refs/tags/$TAG" "refs/tags/$TAG^{}"; }
summary() { forward node "$OPS/summary.mjs"; }
health() { ok200 https://app.richview.ai/ && ok200 https://app.richview.ai/health && ok200 https://reports.richview.ai/health; }
buttons() { node --input-type=module - "$1" <<'JS'
const resp=await fetch('https://app.richview.ai/',{redirect:'manual'}), html=await resp.text();
if(resp.status!==200 || !html.includes('action="/ui/sign-in"') || !html.includes('type="password"')) throw Error('Missing email/password page');
for(const p of ['github','microsoft','google']) if(new RegExp(`href="/sign-in/${p}(?:"|\\?)`).test(html)!==(process.argv[2]==='on')) throw Error(`Unexpected button for ${p}`);
JS
}

# The parts of this run (01-select-parts records them): windows, prod, site, hosted, public.
part() { [[ ",$(get PARTS)," = *",$1,"* ]]; }
# A bounded retry of one unchanged check, for results that need the edge to settle (Daniel, 25 Nov 15:11 UTC, ask deploy-check-propagation-wait: wait up to 3 minutes, then the same full comparison).
settle() { local label=$1 start=$SECONDS; shift; until "$@"; do (( SECONDS - start < 180 )) || { echo "Still failing after $((SECONDS - start)) s: $label" >&2; return 1; }; echo "Waiting for edge propagation: $label ($((SECONDS - start)) s)" >&2; sleep 15; done; }
archive_args() { load_keys H; TEXT=$(cat "$HERE/compatibility.txt"); test "$(hashfile "$HERE/compatibility.txt")" = "$(get COMPAT_SHA)"; common=(--stage production --store-format hosted-v2 --compatibility "$TEXT"); archive=(--archive "$A" --archive-sha256 "$H"); }
verify_signup() {
  local mode=$1 log=$2 http paused
  case "$mode" in open) http=200; paused=false;; paused) http=503; paused=true;; *) die 'Invalid signup mode.';; esac
  signup status | grep -Fx "production: sign-up $mode"
  test "$(curl -sS --max-time 30 -o /dev/null -w '%{http_code}' https://app.richview.ai/sign-up)" = "$http"
  summary | tee "$log"
  sed -n '/^{/,/^}/p' "$log" | jq -se --argjson paused "$paused" 'length==1 and .[0].signupsPaused==$paused' >/dev/null
}
source "$S/site-authority.sh"
case "$STEP" in record-word|90-rollback|c-rollback|recovery-prompt|provider-check|recover-*|cleanup-snapshots) ;; *) guard;; esac
