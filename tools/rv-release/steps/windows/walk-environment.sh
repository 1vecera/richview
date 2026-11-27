# Sourced once by the unchanged gate walk driver in its Bash process.
unset BASH_ENV
aws() {
  if [[ ${1:-} = ec2 && ${2:-} = run-instances ]]; then
    local i user_data= args=("$@")
    for ((i=0; i<${#args[@]}; i++)); do
      if [[ ${args[i]} = --user-data ]]; then user_data=${args[i+1]}; fi
    done
    [[ $user_data = "file://$RUN/user-data.txt" ]] || return 1
    # Preserve the gate's generated SSH keys. Only shorten the guest shutdown deadline.
    node --input-type=module - "$RUN/user-data.txt" <<'JS' || return 1
import fs from 'node:fs';
const path=process.argv[2], original=fs.readFileSync(path,'utf8');
const before='$at = (Get-Date).AddHours(3)';
if(original.split(before).length!==2)throw Error('Unrecognized gate guest shutdown timer');
fs.writeFileSync(path,original.replace(before,'$at = (Get-Date).AddMinutes(16)'));
JS
    source "$D_TOOLS/guard.sh"
    check_release || return 1
    local token
    token=$(uuidgen) || return 1
    (set -C; printf '%s\n' "$token" > "$RUN/launch-token") || return 1
    command aws "$@" --client-token "$token"
  else
    command aws "$@"
  fi
}
ssh() {
  local last=${!#}
  if [[ $RUN != */private/walk-2 || $last != "powershell -NoProfile -ExecutionPolicy Bypass -File C:\\rv\\e2e\\e2e.ps1 -Commit $COMMIT" ]]; then
    command ssh "$@"; return
  fi
  local connection=("${@:1:$#-1}") count=$(( $#-2 )) observer rc=0
  # Sample the listener from a separate PowerShell process. The tested invocation and its stdio remain the gate's.
  scp -q "${@:1:count}" "$D_TOOLS/observe-installer.ps1" "${connection[count]}:C:/rv/observe-installer.ps1" || return 1
  command ssh "${connection[@]}" 'powershell -NoProfile -ExecutionPolicy Bypass -File C:\rv\observe-installer.ps1' > "$RUN/installer-observation.log" 2>&1 &
  observer=$!
  command ssh "$@" || rc=$?
  wait "$observer" || echo 'The independent loopback observation was incomplete.' >> "$RUN/installer-observation.log"
  return "$rc"
}
