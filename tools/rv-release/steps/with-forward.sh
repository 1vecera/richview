#!/bin/bash
# Launch restage, 16 Nov: open the README's operator port forward to production (instance 8083 -> 127.0.0.1:18083),
# run one command under the AWS credentials, then close the forward: the local listener this script opened and its
# SSM session. Usage: with-forward.sh COMMAND [ARG...]. Prints no secret; forward.out holds only the session id.
set -u
[ $# -ge 1 ] || { echo 'usage: with-forward.sh COMMAND [ARG...]' >&2; exit 2; }
here=${RV_FORWARD_DIR:?Missing private release record}
log=$(mktemp "$here/forward.XXXXXXXX") || exit 1
root=${RV_HOSTED_DEPLOY:?Run through the release command}
instance=$(node -e 'console.log(JSON.parse(require("fs").readFileSync(process.argv[1],"utf8")).instance)' "$root/production-candidate/aws-production-state.json")
[[ $instance =~ ^i-[a-f0-9]+$ ]] || { echo 'no instance id' >&2; exit 2; }
if nc -z 127.0.0.1 18083 2>/dev/null; then echo 'port 18083 is already in use' >&2; exit 2; fi
export AWS_PAGER=''
[[ -n ${AWS_ACCESS_KEY_ID:-} && -n ${AWS_SECRET_ACCESS_KEY:-} ]] || exit 2
creds=(env)
"${creds[@]}" aws ssm start-session --target "$instance" --document-name AWS-StartPortForwardingSession --parameters '{"portNumber":["8083"],"localPortNumber":["18083"]}' --region eu-central-1 > "$log" 2>&1 &
fwd=$!
for i in $(seq 1 40); do nc -z 127.0.0.1 18083 2>/dev/null && break; sleep 0.5; done
if nc -z 127.0.0.1 18083 2>/dev/null; then "${creds[@]}" "$@"; code=$?
else echo 'forward did not open' >&2; code=3; fi
# Stop only this process and its observed children. Do not signal another process using the same port.
kids=$(pgrep -P "$fwd" 2>/dev/null); grandkids=""; for k in $kids; do grandkids="$grandkids $(pgrep -P "$k" 2>/dev/null)"; done
kill $grandkids $kids "$fwd" 2>/dev/null; wait "$fwd" 2>/dev/null
session=$(grep -o -E 'SessionId: [A-Za-z0-9_-]+' "$log" | head -1 | cut -d' ' -f2)
[ -n "$session" ] || exit 4
"${creds[@]}" aws ssm terminate-session --region eu-central-1 --session-id "$session" --output text > /dev/null || exit 4
sleep 1; nc -z 127.0.0.1 18083 2>/dev/null && { echo 'port 18083 still open' >&2; exit 4; }
echo "forward closed (session ${session:-unknown}), command exit $code at $(date -u +%FT%TZ)"
exit $code
