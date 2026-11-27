#!/bin/zsh -f
# One AWS Windows machine for a working session, under the kit's safety net (build-on-aws.md):
# the read-only check that nothing tagged richview=windows-build is left, launch, the dry-run terminate
# on the real id, the 45-minute first-boot shutdown, and on any exit the trap that terminates, sweeps volumes and
# snapshots, deletes the security group and prints the cost.
# Usage (under one pw run): session.sh RUN [MINUTES]
#   It installs the build tools (setup-tools.ps1), records the Windows images AWS offers, then holds the
#   machine until RUN/stop exists, MINUTES (default 150) have passed since launch, or the machine is
#   gone. Work happens over SSH with RUN/key to the address in RUN/host; the AWS keys stay in this process.
set -u
export COPYFILE_DISABLE=1
RUN=$1 MINUTES=${2:-150}
KIT=${RV_WINDOWS_KIT:?Run through the release command}
E2E=$KIT/e2e
export AWS_REGION=eu-central-1 AWS_PAGER=''
TYPE=${TYPE:-c7i.2xlarge}
IMAGE=${IMAGE:-resolve:ssm:/aws/service/ami-windows-latest/Windows_Server-2025-English-Full-Base}
SETUP=${SETUP:-tools}
mkdir -p $RUN
say() { echo "== $(date +%H:%M:%S) $*"; }
tell() { say "$*"; }
ID= SG= TERMINATING=
terminate() {
  [ -n "$TERMINATING" ] && return 0
  TERMINATING=1
  trap - EXIT INT TERM
  say "terminate"
  if [ -n "$ID" ]; then
    aws ec2 terminate-instances --instance-ids $ID --query 'TerminatingInstances[0].CurrentState.Name' --output text
    if aws ec2 wait instance-terminated --instance-ids $ID; then
      date -u +%FT%TZ > $RUN/terminated-at
    else
      tell "WILL NOT TERMINATE: instance $ID did not reach terminated; its own task shuts it down 45 minutes after first boot ($(cat $RUN/started-at 2>/dev/null))."
    fi
    aws ec2 describe-instances --instance-ids $ID --query 'Reservations[0].Instances[0].State.Name' --output text | tee $RUN/state-after.txt
    aws ec2 describe-instances --filters Name=tag:richview,Values=windows-build --query 'Reservations[].Instances[].[InstanceId,State.Name,LaunchTime]' --output text | tee $RUN/instances-by-tag.txt
    aws ec2 describe-volumes --filters Name=tag:richview,Values=windows-build --query 'Volumes[].VolumeId' --output text | tee $RUN/volumes-by-tag.txt
    { [ -s $RUN/volume-id ] && aws ec2 describe-snapshots --owner-ids self --filters Name=volume-id,Values=$(cat $RUN/volume-id) --query 'Snapshots[].SnapshotId' --output text
      aws ec2 describe-snapshots --owner-ids self --filters Name=tag:richview,Values=windows-build --query 'Snapshots[].SnapshotId' --output text; } | tr '\t' '\n' | grep -E '^snap-' | sort -u > $RUN/snapshots.txt
    for s in $(cat $RUN/snapshots.txt); do aws ec2 delete-snapshot --snapshot-id $s && echo "snapshot $s deleted"; done | tee $RUN/snapshots-deleted.txt
  fi
  if [ -n "$SG" ]; then
    for i in 1 2 3 4 5 6; do aws ec2 delete-security-group --group-id $SG 2>/dev/null && { echo "security group $SG deleted"; break; }; sleep 20; done
  fi
  [ -f $RUN/started-at ] && [ -f $RUN/terminated-at ] && node -e 'const [a,b]=process.argv.slice(1).map(f=>new Date(require("fs").readFileSync(f,"utf8").trim()));const h=(b-a)/3.6e6;console.log(`ran ${(h*60).toFixed(1)} min (instance 0.7754/h, IPv4 0.005/h, 80 GiB gp3 at 0.0952 per GiB-month), ${(h*(0.7754+0.005+80*0.0952/730)).toFixed(4)} USD`)' $RUN/started-at $RUN/terminated-at | tee $RUN/cost.txt
  tell "terminated: state $(cat $RUN/state-after.txt 2>/dev/null); instances with the tag: $(tr '\t\n' ' ;' < $RUN/instances-by-tag.txt 2>/dev/null); volumes with the tag: [$(cat $RUN/volumes-by-tag.txt 2>/dev/null)]; snapshots found and deleted: [$(tr '\n' ' ' < $RUN/snapshots.txt 2>/dev/null)]; $(cat $RUN/cost.txt 2>/dev/null)"
  rm -f $RUN/key $RUN/key.pub
  exit ${1:-0}
}
trap 'terminate 1' EXIT INT TERM
fail() { tell "stopped: $*"; terminate 1; }

say "nothing of an earlier run is left (read only)"
left=$( { aws ec2 describe-instances --filters Name=tag:richview,Values=windows-build Name=instance-state-name,Values=pending,running,shutting-down,stopping,stopped --query 'Reservations[].Instances[].InstanceId' --output text &&
  aws ec2 describe-volumes --filters Name=tag:richview,Values=windows-build --query 'Volumes[].VolumeId' --output text &&
  aws ec2 describe-security-groups --filters Name=tag:richview,Values=windows-build --query 'SecurityGroups[].GroupId' --output text &&
  aws ec2 describe-key-pairs --filters Name=tag:richview,Values=windows-build --query 'KeyPairs[].KeyPairId' --output text; } 2>&1 ) || fail "the read-only check failed: $(echo $left | cut -c1-300)"
echo "left from earlier runs: [${left//$'\n'/ }]" | tee $RUN/left-before.txt
[ -z "${left//[[:space:]]/}" ] || fail "resources with the tag exist before launch: ${left//$'\n'/ }; nothing launched"
# Read only: whether AWS offers a Windows 11 image this account can start (the lead, 18 Nov 10:25).
aws ec2 describe-images --owners amazon --filters 'Name=name,Values=*Windows_11*' --query 'Images[].[ImageId,Name,UsageOperation]' --output text > $RUN/windows11-images.txt 2>&1
echo "Windows 11 images from Amazon: $(wc -l < $RUN/windows11-images.txt | tr -d ' ') lines" ; head -5 $RUN/windows11-images.txt

say "launch, then prove terminate on the real id"
ssh-keygen -q -t ed25519 -N '' -C richview-windows-build -f $RUN/key
sed "s|@PUBLIC_KEY@|$(cat $RUN/key.pub)|" "$D_TOOLS/build-boot.template" > $RUN/user-data.txt
MYIP=$(curl -s https://checkip.amazonaws.com); echo "my address $MYIP"
SG=$(aws ec2 create-security-group --group-name richview-windows-build-$(basename $RUN) --description 'RichView Windows build: SSH from one address' --tag-specifications 'ResourceType=security-group,Tags=[{Key=richview,Value=windows-build}]' --query GroupId --output text) || fail "no security group"
echo $SG > $RUN/security-group
aws ec2 authorize-security-group-ingress --group-id $SG --protocol tcp --port 22 --cidr $MYIP/32 --query Return --output text || fail "no SSH rule"
source "$D_TOOLS/guard.sh"
check_release || fail "STOP, changed FINAL or low disk before launch"
token=$(uuidgen) || exit 1
(set -C; printf '%s\n' "$token" > "$RUN/launch-token") || { echo "Existing launch intent at $RUN; reconcile EC2 first." >&2; exit 1; }
ID=$(aws ec2 run-instances --client-token "$token" --image-id $IMAGE --instance-type $TYPE --count 1 --security-group-ids $SG \
  --instance-initiated-shutdown-behavior terminate --metadata-options HttpTokens=required \
  --block-device-mappings 'DeviceName=/dev/sda1,Ebs={VolumeSize=80,VolumeType=gp3,DeleteOnTermination=true}' \
  --user-data file://$RUN/user-data.txt \
  --tag-specifications 'ResourceType=instance,Tags=[{Key=Name,Value=richview-windows-build},{Key=richview,Value=windows-build}]' 'ResourceType=volume,Tags=[{Key=richview,Value=windows-build}]' \
  --query 'Instances[0].InstanceId' --output text) || fail "run-instances failed"
echo $ID > $RUN/instance-id; date -u +%FT%TZ > $RUN/started-at
answer=$(aws ec2 terminate-instances --dry-run --instance-ids $ID 2>&1 | tail -1); echo "$answer" | tee $RUN/terminate-dry-run.txt
case $answer in *DryRunOperation*) ;; *) fail "TERMINATE NOT PROVEN for $ID: $answer; it shuts itself down 45 minutes after first boot";; esac
aws ec2 wait instance-running --instance-ids $ID || fail "instance $ID did not reach running"
HOST=$(aws ec2 describe-instances --instance-ids $ID --query 'Reservations[0].Instances[0].PublicIpAddress' --output text); echo $HOST > $RUN/host
aws ec2 describe-instances --instance-ids $ID --query 'Reservations[0].Instances[0].BlockDeviceMappings[0].Ebs.VolumeId' --output text > $RUN/volume-id
tell "instance $ID runs ($TYPE, $(basename $IMAGE), $HOST, started $(cat $RUN/started-at)); waiting for SSH"
OPTS=(-i $RUN/key -o BatchMode=yes -o StrictHostKeyChecking=accept-new -o UserKnownHostsFile=$RUN/known_hosts -o ConnectTimeout=10 -o ServerAliveInterval=60 -o ServerAliveCountMax=10)
SSH=(ssh $OPTS Administrator@$HOST)
SCP=(scp $OPTS)
deadline=$(( $(date +%s) + 1800 ))
until $SSH 'type C:\richview-build-limit.txt' 2>/dev/null; do
  [ $(date +%s) -gt $deadline ] && fail "SSH did not answer within 30 minutes"
  sleep 30
done
echo ready > $RUN/ssh-ready

if [ "$SETUP" = tools ]; then
  say "build tools"
  url=$(gh api repos/git-for-windows/git/releases/latest --jq '.assets[] | select(.name|test("^MinGit-[0-9.]+-64-bit\\.zip$")) | .browser_download_url')
  curl -fsSL -o $RUN/MinGit.zip "$url" || fail "MinGit download"
  echo "$url" > $RUN/mingit-url.txt; shasum -a 256 $RUN/MinGit.zip | tee $RUN/downloads.sha256
  $SCP $RUN/MinGit.zip $E2E/setup-tools.ps1 Administrator@$HOST:C:/rv/ || fail "upload"
  $SSH 'powershell -NoProfile -ExecutionPolicy Bypass -File C:\rv\setup-tools.ps1' > $RUN/setup-console.log 2>&1 || fail "setup failed: $(tail -3 $RUN/setup-console.log | tr '\n' ' ')"
  $SCP Administrator@$HOST:C:/rv/downloads.sha256 $RUN/downloads-instance.sha256 && cat $RUN/downloads-instance.sha256 >> $RUN/downloads.sha256
  echo ready > $RUN/tools-ready
  tell "tools installed ($(grep -E '^== windows|msedge' $RUN/setup-console.log | tr -d '\r' | tr '\n' ' ' | cut -c1-200))"
fi

say "held until $RUN/stop, $MINUTES minutes after launch, or the machine is gone"
until=$(( $(date -u -j -f %Y-%m-%dT%H:%M:%SZ "$(cat $RUN/started-at)" +%s) + MINUTES * 60 ))
while [ ! -e $RUN/stop ] && [ $(date +%s) -lt $until ]; do
  state=$(aws ec2 describe-instances --instance-ids $ID --query 'Reservations[0].Instances[0].State.Name' --output text 2>/dev/null)
  [ "$state" = running ] || { say "instance state $state"; break; }
  sleep 60
done
terminate 0
