check_pack_slot() {
  local hold="$HOME/tmp/richview-rewrite/target/.rv-gate-release"
  [[ ! -e $hold && ! -L $hold ]] || { echo "Shared release target held at $hold. Wait for the full to finish." >&2; return 1; }
}
check_release() {
  local available
  local journal
  journal=$(dirname "$D_BIND")
  [[ ! -e "$D_ROOT/STOP" && $(cat "$D_BIND") = "$D_FINAL" ]] || { echo 'Release stopped or candidate changed.' >&2; return 1; }
  if [[ -e "$journal/FAILED.windows" || -e "$journal/FAILED.shared" ]]; then
    local marker="$journal/attempt-windows-site-resume" proof="$journal/rearm-windows-pack.json"
    if [[ ${D_WINDOWS_RESUME_LANE:-} = private/walk-2 ]]; then
      marker="$journal/attempt-windows-walk-2"; proof="$journal/rearm-windows-walk-2.json"
    fi
    # Only the running recovery may continue while the original failure holds the other steps.
    [[ ${D_WINDOWS_REARM_PID:-} =~ ^[0-9]+$ && $(cat "$marker") = "$D_WINDOWS_REARM_PID" && $(cat "$journal/FAILED.windows") = 11-windows ]] &&
      kill -0 "$D_WINDOWS_REARM_PID" && cmp -s "$journal/selected-route.json" "$D_ROOT/selected-route.json" &&
      jq -e --arg sha "$D_FINAL" --arg pid "$D_WINDOWS_REARM_PID" '.sha==$sha and .pid==$pid' "$proof" >/dev/null || { echo 'Windows failure is held.' >&2; return 1; }
  fi
  available=$(df -k "$D_ROOT" | awk 'NR==2 {print $4}')
  [ "$available" -ge 16777216 ] || { echo "Claw Mini has under 16 GiB free." >&2; return 1; }
}
