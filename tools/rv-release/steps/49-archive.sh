#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after 17-linux; require_aws
load_keys R
# A check of the staged archive, run before any deploy (DEL-83): the prod merge has not happened yet, so what only the merge makes true
# (private prod and the candidate at FINAL, the prod-merge receipt) is checked by 49b-preflight, right before the host is touched.
A="$L/go5-private/richview-x86_64-unknown-linux-gnu.tar.gz"
read -r -p 'Private archive SHA-256 from the Lenovo transfer and SHA256SUMS: ' H
[[ $A = "$L/"* && $A != "$NEXT/release/"* && $H =~ ^[0-9a-f]{64}$ ]]
test "$(hashfile "$A")" = "$H"; AD=$(dirname "$A")
(cd "$AD" && shasum -a 256 -c SHA256SUMS)
test "$(tar -xOzf "$A" ./BUILD)" = "$(printf 'commit=%s\nplatform=x86_64-unknown-linux-gnu' "$FINAL")"
runtime=$(cat "$NEXT/wasm/RUNTIME"); [[ $runtime =~ ^[0-9a-f]{12}$ ]]
grep -Fx "richview $BINVER (commit ${FINAL:0:10}, runtime $runtime)" "$AD/summary.txt"
jq -e --arg c "$FINAL" '.inputs.commit==$c and .inputs.dirty==false' "$AD/manifest.json" >/dev/null
for w in docs.wasm richview_runtime.wasm richview_site.wasm; do
  expected=$(jq -er --arg w "$w" '.inputs.wasm_from[$w].sha256' "$AD/manifest.json")
  [[ $expected =~ ^[0-9a-f]{64}$ ]]
  actual=$(hashfile "$NEXT/wasm/$w"); test "$actual" = "$expected"
done
git diff "$OLD" "$FINAL" -- crates/richview-app > "$HERE/hosted-archive.diff"
git diff --name-status "$OLD" "$FINAL" > "$HERE/all-paths.tsv"
cat "$COMPAT"; printf 'Review the FINAL diff including persisted format, providers, tokens, cookies, archive, volume and downgrade. Enter the reviewed one-line compatibility finding naming BOTH full SHAs:\n'
IFS= read -r text; [[ $text = *"$FINAL"* && $text = *"$OLD"* && ${#text} -ge 100 ]]
printf '%s\n' "$text" > "$HERE/compatibility.txt"; save COMPAT_SHA "$(hashfile "$HERE/compatibility.txt")"
(cd "$P" && shasum -a 256 -c "$L/$(dget .pins.controllerHashes)")
jq -e --arg b "$OLD" '.candidate==$b and .phase=="provisioned" and .mailTransport=="smtp"' "$STATE" >/dev/null
# H of the previous release's journal is the digest of the archive that runs now, the one a rollback restores.
oldDigest=$(awk -F '\t' '$1=="H"{v=$2} END{print v}' "$OLD_JOURNAL/steps.tsv")
test "$oldDigest" = "$(jq -er .archiveSha256 "$STATE")"; save OLD_SHA256 "$oldDigest"
save H "$H"
save HOSTED_VOLUME "$(jq -er .volume "$STATE")"
printf "%s\n" "Check the private archive verification and the compatibility text."
finish
