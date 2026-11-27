#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after 41-deploy; inputs; check_prod; clean_tree
test "$(worker)" = "$(get NEW_WORKER)"
# The edge can serve the previous bytes for a short time after an upload. Wait up to 3 minutes for the three kept pages (Daniel, 25 Nov, ask
# deploy-check-propagation-wait), then run every original check below once, unchanged: nothing here is looser than release 30's step.
live_matches() { curl -fsS --max-time 30 "https://richview.ai$1" | cmp -s - "$2"; }
settle 'privacy page' live_matches /privacy "$KEPT/dist/web/privacy.html"
settle 'home page' live_matches / "$KEPT/dist/web/index.html"
settle 'install page' live_matches /install "$KEPT/dist/web/install.html"
curl -fsS --max-time 30 https://richview.ai/privacy > "$HERE/live-privacy.html"
cmp "$HERE/live-privacy.html" "$KEPT/dist/web/privacy.html"; if node "$S/check-privacy.mjs" "$HERE/live-privacy.html"; then save LIVE_PRIVACY ready; else save LIVE_PRIVACY hold; fi
# Sign-up is open in this release (it was paused throughout release 30). A privacy page that fails its check must not stay live beside open sign-up.
[[ $(get LIVE_PRIVACY) = ready || $(signup_http_state) != open ]] || die 'Live privacy fails its check while sign-up is open. Pause sign-up first (recover-pause, with recovery-go.txt), then decide.'
curl -fsS --max-time 30 https://richview.ai/ | cmp - "$KEPT/dist/web/index.html"
curl -fsS --max-time 30 https://richview.ai/install | cmp - "$KEPT/dist/web/install.html"
env -u RV_CHECK_HEADERS -u RV_CHECK_BREAK RV_CHECK_REPO="$SITEWT" node "$OPS/site-smoke.mjs" https://richview.ai "$HERE/site-smoke"
for browser in chromium firefox; do env -u RV_CHECK_HEADERS -u RV_CHECK_BREAK RV_CHECK_REPO="$SITEWT" node "$OPS/../chart-mount-check.mjs" https://richview.ai --engine "$browser" --out "$HERE/mount-$browser"; done
test "$(worker)" = "$(jq -er .version "$SOUT/deploy.json")"; test "$(ref prod)" = "$FINAL"; test "$(ref "$C")" = "$FINAL"
mkdir -p "$HERE/gate-browser"
{ printf '%s\n' 'import {createRequire} from "node:module"; import {join} from "node:path"; const BASE="https://richview.ai"; const insist=(ok,m)=>{if(!ok)throw Error(m)};'; { git show "$FINAL:tools/rv-gate/deploy-site.mts" 2>/dev/null || git show "$FINAL:tools/rv-gate/deploy-site.mjs"; } | sed -n '/^const SMOKE_PATHS = /,/^function loadHotfix(/{ /^function loadHotfix(/!p; }'; printf '%s\n' 'console.log(JSON.stringify(verifyBrowserSmoke(await browserSmoke({cwd:process.argv[2],out:process.argv[3]}))))'; } | node --input-type=module-typescript - "$SITEWT" "$HERE/gate-browser" > "$HERE/gate-browser.json"
jq -e '.status=="pass"' "$HERE/gate-browser.json" >/dev/null

read -r -p 'After reviewing live desktop/mobile captures, type the deployed Worker version: ' w; test "$w" = "$(get NEW_WORKER)"
finish
