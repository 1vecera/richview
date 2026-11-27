site_packet() {
  local gate=$1 cover="$HERE/website-review.md"
  local digest retained
  retained=$(node "$S/packet-directory.mjs" retain "$gate" "$HERE") || return 1
  digest=$(hashfile "$retained")
  {
    printf '# Website review for %s at %s\n\n' "$C" "$FINAL"
    cat "$L/visitor-table.md"
    printf '\n\nGate visuals, Windows walk and verification: [%s](<%s>)\n\nSHA-256 %s\n' "$(get R)" "$retained" "$digest"
    printf '\nOriginal gate output: %s\n' "$gate"
  } > "$cover"
  save GATE_PACKET "$retained"; save GATE_PACKET_SHA "$digest"; save PACKET "$cover"; save PACKET_SHA "$(hashfile "$cover")"
}
# Sourced after the release journal helpers. (The exact sentences of the old per-part words files are gone: one go covers the parts it names, DEL-83.)

# Sign-up was paused through the whole of release 30's site work. It is open now (since 24 Nov 17:52 UTC+1), so the site steps
# hold the state they find at review instead: 40-dry-run records it, 41-deploy refuses to continue if it changed, and 42-verify
# refuses to leave sign-up open unless the live privacy page passes.
signup_http_state() {
  local status
  status=$(curl --proto '=https' -sS --max-time 30 -o /dev/null -w '%{http_code}' https://app.richview.ai/sign-up) || die 'Cannot verify anonymous sign-up.'
  case $status in 200) echo open;; 503) echo paused;; *) die "Unexpected anonymous sign-up status HTTP $status.";; esac
}
require_signup_unchanged() {
  local now; now=$(signup_http_state)
  [[ $now = "$(get SIGNUP_AT_REVIEW)" ]] || die "Sign-up is $now; it was $(get SIGNUP_AT_REVIEW) at the dry run. Inspect before the Worker changes."
}
