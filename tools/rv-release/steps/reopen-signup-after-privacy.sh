#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after 50-mail-window; require_aws
after 55-update; after 57-smtp; after 59-hold
test "$(get SIGNUP_ORIGINAL)" = open # release 31 restores the state it found: open
# Verify stable release bytes and their public tag again at the registration decision.
if part public; then after 31-downloads; bash "$S/31-downloads.sh"
else # No new release in this run: the published latest must still be the previous one, with its eight assets answering.
  gh api repos/1vecera/richview/releases/latest > "$HERE/release-at-reopen.json"
  jq -e --arg tag "$PREV_TAG" '.tag_name==$tag and .draft==false and .prerelease==false and (.assets|length)==8' "$HERE/release-at-reopen.json" >/dev/null
  for url in $(jq -r '.assets[].browser_download_url' "$HERE/release-at-reopen.json"); do test "$(curl -sSIL --max-time 60 -o /dev/null -w '%{http_code}' "$url")" = 200; done
fi
H=$(get H); [[ $H =~ ^[0-9a-f]{64}$ ]]
jq -e --arg c "$FINAL" --arg h "$H" '.phase=="provisioned" and .candidate==$c and .archiveSha256==$h and .mailTransport=="smtp" and .update.stage=="complete"' "$STATE" >/dev/null
export RV_REQUIRE_HOST_ARCHIVE="$H"
go_text hosted >/dev/null; check_prod
curl --proto '=https' -fsS --max-time 30 https://richview.ai/privacy > "$HERE/reopen-privacy.html"
cmp "$HERE/reopen-privacy.html" "$KEPT/dist/web/privacy.html"
node "$S/check-privacy.mjs" "$HERE/reopen-privacy.html"
test "$(get PRIVACY_REOPEN)" = pending; bash "$S/provider-check.sh"; verify_signup paused "$HERE/pre-open.txt"
attempt reopen; run signup open; verify_signup open "$HERE/signup-open.txt"; save PRIVACY_REOPEN complete
finish
