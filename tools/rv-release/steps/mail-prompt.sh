#!/usr/bin/env bash
set -euo pipefail; set +x +v
export -n RICHVIEW_SMTP_PASSWORD
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
[[ ${RICHVIEW_SMTP_PASSWORD:-} =~ ^[A-Za-z0-9_-]{20,}$ ]] || die 'SMTP credential unavailable'
export RV_SMTP_WINDOW=$$
exec </dev/tty
for n in 50a-pause-signup 51-hosted-before 52-outbox-plan 53-outbox 54-update-plan 55-update 56-smtp-plan 57-smtp 58a-send-reset 58b-read-reset 59-hold; do
  read -r -p "Read the preceding output. Type $n to run it: " answer; test "$answer" = "$n"
  case "$n" in 56-*|57-*) export RICHVIEW_SMTP_PASSWORD;; esac
  bash "$S/$n.sh"
  export -n RICHVIEW_SMTP_PASSWORD
done
