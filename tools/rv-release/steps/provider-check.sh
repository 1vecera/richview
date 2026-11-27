#!/usr/bin/env bash
# Inspect the existing provider selection. The SSM request contains read-only commands.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
require_aws
iid=$(jq -er .instance "$STATE"); [[ $iid =~ ^i-[0-9a-f]+$ ]]
# Release 29's journal held HOST_ON and is gone; the line survives in release 30's transcript (run-public-gos.log:3022) and is kept in previous/host-on.tsv.
last="$L/previous/host-on.tsv"; on=$(awk -F '\t' '$1=="HOST_ON"{v=$2} END{print v}' "$last")
[[ $on =~ ^[0-9a-f]{64}[[:space:]]github,microsoft,google$ ]]
test "$(awsr sts get-caller-identity --query Account --output text)" = "$ACCOUNT"
meta=$(awsr ssm describe-parameters --parameter-filters "Key=Name,Option=BeginsWith,Values=$PARAM/" --query 'Parameters[].{n:Name,t:Type,v:Version}' --output json | jq -ceS --arg p "$PARAM/" 'select(([.[].n]|sort)==[($p+"github"),($p+"google"),($p+"microsoft")] and all(.[];.t=="SecureString" and .v>=1))|sort_by(.n)')
if [[ -f "$HERE/provider-versions.json" ]]; then cmp "$HERE/provider-versions.json" <(echo "$meta"); else (set -C; echo "$meta" > "$HERE/provider-versions.json"); fi
awsr iam get-role-policy --role-name "$ROLE" --policy-name ReadProductionSignInInputs --query PolicyDocument --output json | jq -e --arg prefix "arn:aws:ssm:$REGION:$ACCOUNT:parameter$PARAM/" '(.Statement|length)==1 and (.Statement[0] | .Effect=="Allow" and .Action=="ssm:GetParameter" and (.Resource|sort)==([($prefix+"github"),($prefix+"microsoft"),($prefix+"google")]|sort))' >/dev/null
hostcheck=$(cat <<'HOST'
set -eu
test "$(cat /proc/sys/kernel/core_pattern)" = core
test "$(systemctl is-enabled apport.service || true)" = masked
test "$(findmnt -n -o FSTYPE -T /run)" = tmpfs
test "$(awk 'END{print NR-1}' /proc/swaps)" = 0
systemctl is-active --quiet richview.service
cd /var/lib/richview-release-29; echo 'c8d432a4ef43c27af8840bb5cfde9ed7ba0964ffadbd7b2e3bd97c67151bf600  sign-in-switch-host.sh' | sha256sum -c --status
/bin/bash ./sign-in-switch-host.sh status
HOST
)
if [[ -n ${RV_REQUIRE_HOST_ARCHIVE:-} ]]; then
  [[ $RV_REQUIRE_HOST_ARCHIVE =~ ^[a-f0-9]{64}$ ]]
  hostcheck="test \"\$(cat /opt/richview/current/.archive-sha256)\" = '$RV_REQUIRE_HOST_ARCHIVE' || exit 1
$hostcheck"
fi
request=$(mktemp "$HERE/host-check.XXXXXXXX"); trap 'rm -f "$request"' EXIT
jq -n --arg c "$hostcheck" '{commands:[$c]}' > "$request"
id=$(awsr ssm send-command --instance-ids "$iid" --document-name AWS-RunShellScript --parameters "file://$request" --query Command.CommandId --output text)
awsr ssm wait command-executed --command-id "$id" --instance-id "$iid"
awsr ssm get-command-invocation --command-id "$id" --instance-id "$iid" --output json | jq -e --arg on "$on" '.Status=="Success" and .ResponseCode==0 and (.StandardOutputContent|rtrimstr("\n"))==$on' >/dev/null
health; buttons on
node --input-type=module <<'JS'
const app='https://app.richview.ai', starts={github:'https://github.com/login/oauth/authorize',microsoft:'https://login.microsoftonline.com/common/oauth2/v2.0/authorize',google:'https://accounts.google.com/o/oauth2/v2/auth'};
for(const [p,endpoint] of Object.entries(starts)) {
  const r=await fetch(`${app}/sign-in/${p}`,{redirect:'manual'}); if(r.status!==303) throw Error(`Start failed for ${p}`);
  const u=new URL(r.headers.get('location')||app), q=u.searchParams, cookies=r.headers.getSetCookie();
  if(u.origin+u.pathname!==endpoint || q.get('redirect_uri')!==`${app}/sign-in/${p}/callback` || !q.get('state') || !q.get('code_challenge') || q.get('code_challenge_method')!=='S256') throw Error(`Redirect check failed for ${p}`);
  if(!cookies.some(c=>c.startsWith('rv_oauth=') && ['HttpOnly','Secure','SameSite=Lax','Path=/sign-in','Max-Age=600'].every(a=>c.split(';').map(s=>s.trim()).includes(a)))) throw Error(`Flow cookie check failed for ${p}`);
  console.log(`${p}: anonymous start passed`);
}
JS

printf 'Existing provider selection, role, parameter versions and crash policy verified.\n'
