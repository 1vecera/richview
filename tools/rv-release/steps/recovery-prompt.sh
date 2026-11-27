#!/usr/bin/env bash
set -euo pipefail; set +x +v; export -n RICHVIEW_SMTP_PASSWORD
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"; export RV_SMTP_WINDOW=$$
while read -r -p 'Recovery step or done: ' n; do
case "$n" in done) exit 0;; recover-status|recover-resume|recover-outbox-plan|recover-outbox|recover-roll-plan|recover-roll) ;; recover-smtp-plan|recover-smtp) export RICHVIEW_SMTP_PASSWORD;; *) exit 1;; esac
bash "$S/$n.sh"; export -n RICHVIEW_SMTP_PASSWORD
done </dev/tty
