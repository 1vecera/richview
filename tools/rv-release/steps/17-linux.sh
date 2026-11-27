#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"
after 16-macos; after 10-cut; load_keys SEED
rr=$LENOVO_REL
remote="lenovo:$rr"; limit='systemd-run --user --scope -p CPUQuota=800% -p MemoryHigh=16G nice -n 19 ionice -c3'
ssh -o BatchMode=yes lenovo "$limit mkdir -p '$rr/incoming' '$rr/lenovo-bin'"
rsync --rsync-path="$limit rsync" -a "$S/kit/lenovo-build.sh" "$S/kit/verify-staged.sh" "$remote/"
# The Docker wrapper for the Linux build: the copy in the release folder, its digest pinned in the descriptor.
test "$(hashfile "$L/lenovo-bin/docker")" = "$(dget .pins.dockerSha256)"
rsync --rsync-path="$limit rsync" -a "$L/lenovo-bin/docker" "$remote/lenovo-bin/"
rsync --rsync-path="$limit rsync" -a "$NEXT/public.bundle" "$NEXT/manifest" "$remote/incoming/"
ssh -o BatchMode=yes lenovo "$limit bash -s -- '$FINAL' '$rr' '$C' '$RELNAME' '$LENOVO_REPO'" <<'REMOTE'
set -euo pipefail
cd "$5"; final=$1; rr=$2; p=.claude/worktrees/$4-private-$final
git fetch --no-tags origin "refs/heads/$3"; test "$(git rev-parse FETCH_HEAD)" = "$final"
if [[ ! -e "$p" ]]; then git worktree add --detach "$p" "$final"; git worktree lock --reason 'active stream' "$p"; fi
test "$(git -C "$p" rev-parse HEAD)" = "$final"; test -z "$(git -C "$p" status --porcelain)"
mkdir -p "$HOME/tmp/richview-rewrite/locks"
bash tmp/team/launch/cutover-kit/import-bundle.sh "$rr/incoming/public.bundle" "$rr/incoming/manifest" "$rr/next-${final:0:8}"
REMOTE
rsync --rsync-path="$limit rsync" -a "$NEXT/wasm/" "$remote/next-${FINAL:0:8}/wasm/"
for kind in public private; do guard; ssh -o BatchMode=yes lenovo "$limit bash '$rr/lenovo-build.sh' '$FINAL' '$kind'"; done
mkdir -p "$NEXT/linux" "$L/go5-private"
rsync --rsync-path="$limit rsync" -a "$remote/next-${FINAL:0:8}/linux/" "$NEXT/linux/"
rsync --rsync-path="$limit rsync" -a --exclude='check/' "$remote/go5-${FINAL:0:8}/" "$L/go5-private/"
rsync --rsync-path="$limit rsync" -a "$remote/linux-${FINAL:0:8}.log" "$L/"
(cd "$NEXT/linux" && shasum -a 256 -c SHA256SUMS); (cd "$L/go5-private" && shasum -a 256 -c SHA256SUMS)
date -u > "$L/linux-${FINAL:0:8}.done"
finish
