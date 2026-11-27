#!/bin/sh
set -eu
launch=$1
seed=$2
out=$3
main_fonts=$4
tested=$(sed -n 's/^tested=//p' "$seed/manifest")
public=$(sed -n 's/^public=//p' "$seed/manifest")
gl=$launch/restage/dl-gitleaks/gitleaks
"$launch/cutover-kit/check-seed-history.sh" "$seed"
"$gl" git --redact=100 --report-format json --report-path "$out-gitleaks-git.json" "$seed/checkout" > "$out-gitleaks-git.log" 2>&1
"$gl" dir --redact=100 --report-format json --report-path "$out-gitleaks-dir.json" "$seed/checkout" > "$out-gitleaks-dir.log" 2>&1
node -e 'for(const path of process.argv.slice(1)){if(require(path).length!==0) process.exit(1)}' "$out-gitleaks-git.json" "$out-gitleaks-dir.json"
printf 'PASS Gitleaks history and directory, zero findings\n'
node "$launch/cutover-kit/check-redaction.mjs" "$tested" > "$out-redaction.log" 2>&1
cat "$out-redaction.log"
node "$launch/d15/repo-move/private-scan.cjs" "$tested" "$public" "$seed/checkout" "$out-private-scan.json" > "$out-private-scan.log" 2>&1
cat "$out-private-scan.log"
(cd "$seed/checkout" && FONT_TMP="$out-font-inspect.bin" FONT_OUT="$out-fonts.json" node "$launch/restage/font-inventory.cjs") > "$out-fonts.log" 2>&1
cmp "$main_fonts" "$out-fonts.json"
node -e 'const fonts = require(process.argv[1]); if(fonts.some(x=>x.exit!==0 || !x.family || /poppins/i.test(x.family))) process.exit(1); console.log("PASS fonts: "+fonts.length+" entries, no Poppins or unknowns, inventory byte-identical to public main")' "$out-fonts.json"
