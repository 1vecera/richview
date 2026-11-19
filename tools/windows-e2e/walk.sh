#!/usr/bin/env bash
# The Windows end-to-end walk of one candidate (ARC-67) on a fresh AWS Windows machine, recorded.
#
# Usage: tools/windows-e2e/walk.sh RUN COMMIT ARCHIVE EDITOR DATABASE SUMS
#   RUN       a new folder for the record
#   COMMIT    the candidate's full commit
#   ARCHIVE   richview-x86_64-pc-windows-msvc.tar.gz built from COMMIT
#   EDITOR    richview-editor.exe built from COMMIT
#   DATABASE  the SQL example's warehouse.duckdb, made by the seed_duckdb example (it is gitignored)
#   SUMS      the build's sha256 list (`<sha256>  <name>` lines), naming richview.exe,
#             richview-sql-worker.exe, richview-editor.exe and warehouse.duckdb; the walk refuses an
#             ARCHIVE whose programs, or an EDITOR or DATABASE, that differ from it
#
# It installs the archive with the PowerShell one-liner and walks e2e.ps1: the previous release and the
# upgrade from it, init, check, generate, the preview, the editor, PDF, SQL, MCP, publishing to a
# server on the machine, and uninstalling. RUN then holds manifest.json (commit, instance, image,
# start and end, cost), e2e.log, the screenshots and the AWS trail; `rv-gate windows <candidate>
# --run RUN` records a passing walk on the candidate's receipt.
#
# AWS credentials come from the environment, as the AWS CLI reads them; nothing here reads a secret
# store. Settings: AWS_REGION (eu-central-1), WINDOWS_E2E_TYPE (c7i.2xlarge), WINDOWS_E2E_IMAGE
# (Windows Server 2025 through SSM), WINDOWS_E2E_HOURLY_USD (0.7754, the type's Windows on-demand
# price in eu-central-1, for the cost line). WINDOWS_E2E_SCRIPT walks a changed e2e.ps1 instead of
# the commit's own, to try a walk on an existing build; manifest.json then says so, and
# `rv-gate windows` does not accept the run as a record.
#
# Safety net, for every run:
# - one machine at a time: anything tagged richview=windows-build stops the run before launch;
# - the machine shuts itself down three hours after first boot, and shutting down terminates it;
# - a dry-run terminate on the real id proves the account may end it;
# - on any exit a trap terminates it, waits until it is terminated, deletes tagged volumes, snapshots
#   of its volume and the security group, and writes the cost.
set -euo pipefail
# The run's files include the machine's private host key (user-data.txt) and the login key.
umask 077
export COPYFILE_DISABLE=1 AWS_PAGER=''
export AWS_REGION=${AWS_REGION:-eu-central-1}
[ $# -eq 6 ] || { sed -n '2,12p' "$0" >&2; exit 2; }
RUN=$1 COMMIT=$2 ARCHIVE=$3 EDITOR=$4 DATABASE=$5 SUMS=$6
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../.." && pwd)
TYPE=${WINDOWS_E2E_TYPE:-c7i.2xlarge}
IMAGE=${WINDOWS_E2E_IMAGE:-resolve:ssm:/aws/service/ami-windows-latest/Windows_Server-2025-English-Full-Base}
HOURLY=${WINDOWS_E2E_HOURLY_USD:-0.7754}
TAG=richview
TAG_VALUE=windows-build

[[ $COMMIT =~ ^[0-9a-f]{40}$ ]] || { echo "COMMIT must be a full commit" >&2; exit 2; }
for file in "$ARCHIVE" "$EDITOR" "$DATABASE" "$SUMS"; do [ -f "$file" ] || { echo "missing $file" >&2; exit 2; }; done
sha256() { shasum -a 256 "$1" | cut -d' ' -f1; }
# The build's sha256 of NAME, from SUMS.
listed() { awk -v name="$1" '$2 == name || $2 == "*" name { print tolower($1) }' "$SUMS" | head -1; }
EDITOR_SHA=$(sha256 "$EDITOR") DATABASE_SHA=$(sha256 "$DATABASE")
# The archive's programs, the editor and the database are all the ones SUMS lists, so the editor is
# the same build as the richview.exe whose --version the walk checks against COMMIT.
packed() { tar -xzOf "$ARCHIVE" "./bin/$1" | shasum -a 256 | cut -d' ' -f1; }
RICHVIEW_SHA=$(packed richview.exe) WORKER_SHA=$(packed richview-sql-worker.exe)
for pair in "richview.exe $RICHVIEW_SHA" "richview-sql-worker.exe $WORKER_SHA" "richview-editor.exe $EDITOR_SHA" "warehouse.duckdb $DATABASE_SHA"; do
	read -r name have <<< "$pair"
	want=$(listed "$name")
	[ -n "$want" ] && [ "$have" = "$want" ] || { echo "the $name given has sha256 $have; the build's is ${want:-not listed in $SUMS}" >&2; exit 2; }
done
ARCHIVE_SHA=$(sha256 "$ARCHIVE")
git -C "$REPO" cat-file -e "$COMMIT^{commit}" || { echo "$COMMIT is not in this repository" >&2; exit 2; }
[ ! -e "$RUN" ] || { echo "$RUN exists; each run gets a new folder" >&2; exit 2; }
SCRIPT=${WINDOWS_E2E_SCRIPT:-}
[ -z "$SCRIPT" ] || [ -f "$SCRIPT" ] || { echo "missing $SCRIPT" >&2; exit 2; }
mkdir -p "$RUN"
RUN=$(cd "$RUN" && pwd)
cp "$SUMS" "$RUN/build.sha256"
now() { date -u +%Y-%m-%dT%H:%M:%SZ; }
say() { echo "== $(now) $*" | tee -a "$RUN/walk.log"; }

ID='' SG='' ENDING=''
manifest() {
	node - "$RUN" "$COMMIT" "$HOURLY" "$ARCHIVE_SHA" <<'EOF'
const [run, commit, hourly, archiveSha256] = process.argv.slice(2);
const fs = require("node:fs");
const read = (name) => (fs.existsSync(`${run}/${name}`) ? fs.readFileSync(`${run}/${name}`, "utf8").trim() : null);
const started = read("started-at");
const terminated = read("terminated-at");
// The instance at its hourly price, its public IPv4 address and an 80 GiB gp3 volume.
const hours = started && terminated ? (new Date(terminated) - new Date(started)) / 3.6e6 : null;
const costUsd = hours === null ? null : Number((hours * (Number(hourly) + 0.005 + (80 * 0.0952) / 730)).toFixed(4));
const log = read("e2e.log")?.replace(/\r/g, "") ?? "";
const result = log.match(/^E2E (PASS|FAIL)\b.*$/gm)?.at(-1) ?? null;
const walk = read("walk") ?? "commit";
const sums = Object.fromEntries((read("sent.sha256") ?? "").split("\n").filter(Boolean).map((line) => line.split(/\s+/).reverse()));
const build = Object.fromEntries((read("build.sha256") ?? "").split("\n").filter(Boolean).map((line) => line.split(/\s+/).reverse()));
const manifest = { commit, walk, instance: read("instance-id"), image: read("image"), type: read("type"), region: read("region"), started, terminated, costUsd, result, editorSha256: sums["richview-editor.exe"] ?? null, databaseSha256: sums["warehouse.duckdb"] ?? null, archiveSha256, richviewSha256: build["richview.exe"] ?? null, workerSha256: build["richview-sql-worker.exe"] ?? null };
fs.writeFileSync(`${run}/manifest.json`, `${JSON.stringify(manifest, null, 2)}\n`);
console.log(`instance ${manifest.instance}, ${result ?? "no result"}, ${hours === null ? "?" : (hours * 60).toFixed(1)} min, ${costUsd ?? "?"} USD`);
EOF
}
finish() {
	local code=$?
	[ -n "$ENDING" ] && return
	ENDING=1
	trap - EXIT INT TERM
	set +e
	if [ -n "$ID" ]; then
		say "terminate $ID"
		aws ec2 terminate-instances --instance-ids "$ID" --query 'TerminatingInstances[0].CurrentState.Name' --output text
		if aws ec2 wait instance-terminated --instance-ids "$ID"; then
			now > "$RUN/terminated-at"
		else
			say "WILL NOT TERMINATE: $ID did not reach terminated; it shuts itself down three hours after first boot"
		fi
		aws ec2 describe-instances --instance-ids "$ID" --query 'Reservations[0].Instances[0].State.Name' --output text > "$RUN/state-after.txt"
		aws ec2 describe-volumes --filters "Name=tag:$TAG,Values=$TAG_VALUE" --query 'Volumes[].VolumeId' --output text > "$RUN/volumes-after.txt"
		{
			[ -s "$RUN/volume-id" ] && aws ec2 describe-snapshots --owner-ids self --filters "Name=volume-id,Values=$(cat "$RUN/volume-id")" --query 'Snapshots[].SnapshotId' --output text
			aws ec2 describe-snapshots --owner-ids self --filters "Name=tag:$TAG,Values=$TAG_VALUE" --query 'Snapshots[].SnapshotId' --output text
		} | tr '\t' '\n' | grep -E '^snap-' | sort -u > "$RUN/snapshots.txt"
		while read -r snapshot; do aws ec2 delete-snapshot --snapshot-id "$snapshot" && echo "deleted $snapshot"; done < "$RUN/snapshots.txt" > "$RUN/snapshots-deleted.txt"
		say "state $(cat "$RUN/state-after.txt"); tagged volumes [$(cat "$RUN/volumes-after.txt")]; snapshots deleted [$(tr '\n' ' ' < "$RUN/snapshots-deleted.txt")]"
	fi
	if [ -n "$SG" ]; then
		for _ in 1 2 3 4 5 6; do aws ec2 delete-security-group --group-id "$SG" 2>/dev/null && { say "security group $SG deleted"; break; }; sleep 20; done
	fi
	# The keys and the user data that carried the host key were for this machine alone.
	rm -f "$RUN/key" "$RUN/key.pub" "$RUN/host-key" "$RUN/user-data.txt"
	say "$(manifest)"
	exit "$code"
}
trap finish EXIT INT TERM

say "nothing tagged $TAG=$TAG_VALUE is left from another run (read only)"
left=$(
	aws ec2 describe-instances --filters "Name=tag:$TAG,Values=$TAG_VALUE" Name=instance-state-name,Values=pending,running,shutting-down,stopping,stopped --query 'Reservations[].Instances[].InstanceId' --output text
	aws ec2 describe-volumes --filters "Name=tag:$TAG,Values=$TAG_VALUE" --query 'Volumes[].VolumeId' --output text
	aws ec2 describe-security-groups --filters "Name=tag:$TAG,Values=$TAG_VALUE" --query 'SecurityGroups[].GroupId' --output text
)
[ -z "${left//[[:space:]]/}" ] || { say "tagged resources exist, nothing launched: ${left//$'\n'/ }"; exit 1; }

say "launch one $TYPE in $AWS_REGION"
ssh-keygen -q -t ed25519 -N '' -C richview-windows-e2e -f "$RUN/key"
# The machine's host key, made fresh for this run and sent in its user data over the AWS API: SSH
# then checks the machine against it instead of trusting the first key it is shown. User data is
# readable through the instance metadata and DescribeInstanceAttribute, which is fine for a key
# that lives as long as this one terminated machine and is never reused.
ssh-keygen -q -t ed25519 -N '' -C richview-windows-e2e-host -f "$RUN/host-key"
node -e '
const fs = require("node:fs");
const [template, key, host, hostPublic, out] = process.argv.slice(1);
const text = (file) => fs.readFileSync(file, "utf8").trim();
fs.writeFileSync(out, fs.readFileSync(template, "utf8").replace("@PUBLIC_KEY@", () => text(key)).replace("@HOST_KEY@", () => text(host)).replace("@HOST_PUBLIC_KEY@", () => text(hostPublic)));
' "$HERE/first-boot.ps1.template" "$RUN/key.pub" "$RUN/host-key" "$RUN/host-key.pub" "$RUN/user-data.txt"
address=$(curl -fsS https://checkip.amazonaws.com)
SG=$(aws ec2 create-security-group --group-name "richview-windows-e2e-$(basename "$RUN")" --description 'RichView Windows walk: SSH from one address' \
	--tag-specifications "ResourceType=security-group,Tags=[{Key=$TAG,Value=$TAG_VALUE}]" --query GroupId --output text)
aws ec2 authorize-security-group-ingress --group-id "$SG" --protocol tcp --port 22 --cidr "$address/32" --query Return --output text > /dev/null
ID=$(aws ec2 run-instances --image-id "$IMAGE" --instance-type "$TYPE" --count 1 --security-group-ids "$SG" \
	--instance-initiated-shutdown-behavior terminate --metadata-options HttpTokens=required \
	--block-device-mappings 'DeviceName=/dev/sda1,Ebs={VolumeSize=80,VolumeType=gp3,DeleteOnTermination=true}' \
	--user-data "file://$RUN/user-data.txt" \
	--tag-specifications "ResourceType=instance,Tags=[{Key=Name,Value=richview-windows-e2e},{Key=$TAG,Value=$TAG_VALUE}]" "ResourceType=volume,Tags=[{Key=$TAG,Value=$TAG_VALUE}]" \
	--query 'Instances[0].InstanceId' --output text)
echo "$ID" > "$RUN/instance-id"
now > "$RUN/started-at"
echo "$TYPE" > "$RUN/type"
echo "$AWS_REGION" > "$RUN/region"
dry=$(aws ec2 terminate-instances --dry-run --instance-ids "$ID" 2>&1 | tail -1 || true)
echo "$dry" > "$RUN/terminate-dry-run.txt"
[[ $dry == *DryRunOperation* ]] || { say "terminate is not proven for $ID: $dry"; exit 1; }
aws ec2 wait instance-running --instance-ids "$ID"
read -r host ami < <(aws ec2 describe-instances --instance-ids "$ID" --query 'Reservations[0].Instances[0].[PublicIpAddress,ImageId]' --output text)
aws ec2 describe-images --image-ids "$ami" --query 'Images[0].[ImageId,Name]' --output text | tr '\t' ' ' > "$RUN/image"
aws ec2 describe-instances --instance-ids "$ID" --query 'Reservations[0].Instances[0].BlockDeviceMappings[0].Ebs.VolumeId' --output text > "$RUN/volume-id"
say "$ID runs $(cat "$RUN/image"); waiting for SSH"
echo "$host $(cut -d' ' -f1,2 "$RUN/host-key.pub")" > "$RUN/known_hosts"
options=(-i "$RUN/key" -o BatchMode=yes -o StrictHostKeyChecking=yes -o HostKeyAlgorithms=ssh-ed25519 -o "UserKnownHostsFile=$RUN/known_hosts" -o ConnectTimeout=10 -o ServerAliveInterval=60 -o ServerAliveCountMax=10)
remote() { ssh "${options[@]}" "Administrator@$host" "$@"; }
deadline=$(($(date +%s) + 1800))
until remote 'type C:\richview-build-limit.txt' > "$RUN/limit.txt" 2>/dev/null; do
	[ "$(date +%s)" -lt "$deadline" ] || { say "SSH did not answer within 30 minutes"; exit 1; }
	sleep 30
done

say "send the candidate: the archive, install.ps1 stamped for the machine's loopback, the examples, the editor"
mkdir -p "$RUN/send/release" "$RUN/send/editor"
cp "$ARCHIVE" "$RUN/send/release/richview-x86_64-pc-windows-msvc.tar.gz"
git -C "$REPO" show "$COMMIT:scripts/install.ps1" > "$RUN/install-template.ps1"
node "$HERE/stamp.mjs" "$RUN/install-template.ps1" "$RUN/send/release/richview-x86_64-pc-windows-msvc.tar.gz" "$COMMIT" http://127.0.0.1:8765 "$RUN/send/release/install.ps1" | tee "$RUN/archive.sha256"
cp "$EDITOR" "$RUN/send/editor/richview-editor.exe"
git -C "$REPO" archive --format=tar -o "$RUN/send/examples.tar" "$COMMIT" examples/sql-report examples/openai-vs-anthropic
if [ -n "$SCRIPT" ]; then
	cp "$SCRIPT" "$RUN/send/e2e.ps1"
	echo "changed script, sha256 $(shasum -a 256 "$SCRIPT" | cut -d' ' -f1)" > "$RUN/walk"
else
	git -C "$REPO" show "$COMMIT:tools/windows-e2e/e2e.ps1" > "$RUN/send/e2e.ps1"
fi
remote 'New-Item -ItemType Directory -Force -Path C:\rv\e2e\examples\sql-report\data | Out-Null'
scp -q "${options[@]}" -r "$RUN/send/release" "$RUN/send/editor" "$RUN/send/e2e.ps1" "$RUN/send/examples.tar" "Administrator@$host:C:/rv/e2e/"
remote 'tar -xf C:\rv\e2e\examples.tar -C C:\rv\e2e'
scp -q "${options[@]}" "$DATABASE" "Administrator@$host:C:/rv/e2e/examples/sql-report/data/warehouse.duckdb"
# What arrived is what the build made.
remote '"{0}  richview-editor.exe" -f (Get-FileHash -Algorithm SHA256 C:\rv\e2e\editor\richview-editor.exe).Hash.ToLowerInvariant(); "{0}  warehouse.duckdb" -f (Get-FileHash -Algorithm SHA256 C:\rv\e2e\examples\sql-report\data\warehouse.duckdb).Hash.ToLowerInvariant()' | tr -d '\r' > "$RUN/sent.sha256"
printf '%s  richview-editor.exe\n%s  warehouse.duckdb\n' "$EDITOR_SHA" "$DATABASE_SHA" | cmp -s - "$RUN/sent.sha256" || { say "the files on the machine differ from the build's: $(tr '\n' ' ' < "$RUN/sent.sha256")"; exit 1; }

say "walk"
remote "powershell -NoProfile -ExecutionPolicy Bypass -File C:\\rv\\e2e\\e2e.ps1 -Commit $COMMIT" > "$RUN/e2e-console.log" 2>&1 || true
scp -q "${options[@]}" -r "Administrator@$host:C:/rv/e2e/out" "$RUN/out" || true
[ -f "$RUN/out/e2e.log" ] && cp "$RUN/out/e2e.log" "$RUN/e2e.log"
result=$(tr -d '\r' < "$RUN/e2e.log" 2>/dev/null | grep -E '^E2E (PASS|FAIL)' | tail -1 || true)
say "${result:-the walk left no result}"
[ "$result" = "E2E PASS" ]
