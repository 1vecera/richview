#!/usr/bin/env bash
# Record which parts this run releases and the date of Daniel's go. The parts are windows, prod, site, hosted and public (tools/rv-release/parts.json);
# release.mjs has already checked that every part's needs are chosen or passed. This step only keeps the journal's list: it may grow in a later run
# (a superset), never lose a part, and the date is fixed by the first run because the recovery sentences carry it.
# Usage: bash steps/run 01-select-parts <comma list> <YYYY-MM-DD of Daniel's go>
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after 00-bind-full
parts=${1:?Pass the chosen parts, e.g. windows,prod,site}; day=${2:?Pass the date of Daniel\'s go, YYYY-MM-DD}
[[ $parts =~ ^[a-z]+(,[a-z]+)*$ && $day =~ ^20[0-9]{2}-(0[1-9]|1[0-2])-(0[1-9]|[12][0-9]|3[01])$ ]] || die 'Use a list like windows,prod,site and a date like 2026-11-25.'
if awk -F '\t' '$1=="PARTS"{f=1}END{exit !f}' "$REC"; then
  old=$(get PARTS); [[ $(get GO_DATE) = "$day" ]] || die 'The go date is fixed by the first run.'
  IFS=, read -ra kept <<< "$old"
  for p in "${kept[@]}"; do [[ ",$parts," = *",$p,"* ]] || die "A later list must contain every part already chosen: $p."; done
fi
save PARTS "$parts"; if ! awk -F '\t' '$1=="GO_DATE"{f=1}END{exit !f}' "$REC"; then save GO_DATE "$day"; fi
printf 'Parts %s, go date %s.\n' "$parts" "$(get GO_DATE)"
finish
