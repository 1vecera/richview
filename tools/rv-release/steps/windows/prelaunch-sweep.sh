#!/usr/bin/env bash
set -euo pipefail
dir=${1:?Windows session directory}
[[ -d $dir && ! -e "$dir/launch-token" && ! -e "$dir/instance-id" ]] || { echo 'Use instance reconciliation for an attempted launch.' >&2; exit 1; }
tmp=$(mktemp "$dir/prelaunch-sweep.XXXXXXXX"); trap 'rm -f "$tmp"' EXIT
bash "$(dirname "$0")/sweep.sh" > "$tmp"
[[ ! -e "$dir/launch-token" && ! -e "$dir/instance-id" ]]
mv "$tmp" "$dir/prelaunch-sweep.log"
