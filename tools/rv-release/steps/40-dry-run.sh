#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after 13-packet; inputs; check_cut
save SIGNUP_AT_REVIEW "$(signup_http_state)"
test "$(ref prod)" = "$BASE"
git diff --exit-code "$BASE" "$FINAL" -- cloudflare/ wrangler.toml
test ! -e "$SOUT/rollback.json"
if [[ -e $SITEWT ]]; then clean_tree; fi
node "$SITE" "${siteargs[@]}" | tee "$HERE/dry.log"
clean_tree
jq -e --arg c "$FINAL" --arg r "$R" --arg m "$M" --arg w "$OLD_WORKER" --arg name "$C" '.candidate==$name and .candidateCut and .commit==$c and .receipt==$r and .manifestSha256==$m and .before.version==$w' "$SOUT/dry-run.json" >/dev/null
"$WR" versions view "$OLD_WORKER" --json --config "$CFG" > "$HERE/worker-before.json"
jq -e --arg w "$OLD_WORKER" '.id==$w' "$HERE/worker-before.json" >/dev/null
{
  node --input-type=module - "$S/worker-history.mjs" "$L" "$WR" "$CFG" "$OLD_WORKER" "$FINAL" <<'JS'
import {pathToFileURL} from 'node:url';
const [module,release,wr,cfg,prior,sha]=process.argv.slice(2);
const {history,checkPredecessor,verifyOldHome}=await import(pathToFileURL(module));
checkPredecessor(release,history(wr,cfg),sha);
verifyOldHome(prior);
JS
}
cp "$SOUT/dry-run.json" "$HERE/reviewed-dry.json"
if node "$S/check-privacy.mjs" "$KEPT/dist/web/privacy.html"; then save PRIVACY ready; else save PRIVACY hold; fi
test -s "$KEPT/dist/web/install.html"
gh api repos/1vecera/richview/releases/latest --jq .tag_name > "$HERE/public-latest-at-review.txt"; grep -Fx "$PREV_TAG" "$HERE/public-latest-at-review.txt"
for installer in install.sh install.ps1; do grep -Fq "https://github.com/1vecera/richview/releases/latest/download/$installer" "$KEPT/dist/web/install.html"; done
node "$S/audit-claims.mjs" source "$FINAL" "$OLD" > "$HERE/source-claims.tsv"
node "$S/audit-claims.mjs" kept "$KEPT/dist/web" > "$HERE/site-claims.tsv"
node "$S/check-homepage-a.mjs" "$SITEWT/dist/web/index.html" "$HERE/full-build-manifest.json"
printf 'Review the homepage and the table of claims that differ from the live $PREV_TAG site against staged bytes. Type the candidate SHA to accept the coverage.\n'
read -r reviewed; test "$reviewed" = "$FINAL"
save BEFORE_WORKER "$OLD_WORKER"; save DRY_SHA "$(hashfile "$HERE/reviewed-dry.json")"
printf 'Read the packet, reviewed-dry.json, source-claims.tsv and site-claims.tsv before requesting the candidate-specific go.\n'
finish
