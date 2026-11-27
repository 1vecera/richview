#!/usr/bin/env bash
set -euo pipefail
[[ ${1:-} =~ ^[0-9a-f]{40}$ ]] || exit 2
final=$1
short=${final:0:8}
release=$(cd "$(dirname "$0")" && pwd -P) # the release folder on Lenovo: this script is copied into it by 17-linux
repo=$(git -C "$release" rev-parse --show-toplevel)
kit=$repo/tmp/team/launch/cutover-kit
seed=$release/next-$short
private=$repo/.claude/worktrees/$(basename "$release")-private-$final
go5=$release/go5-$short
archive=richview-x86_64-unknown-linux-gnu.tar.gz
slot=$HOME/tmp/richview-rewrite/locks/lenovo-3
task=$2
[[ "$task" == public || "$task" == private ]] || exit 2
phase=preflight

record_status() {
  local code=$1
  printf '%s %s exit %s\n' "$(date '+%F %T %Z')" "$phase" "$code" > "$release/lenovo-$short-$task.status"
  return "$code"
}
trap 'record_status $?' EXIT
export PATH="$release/lenovo-bin:$HOME/.local/share/mise/installs/node/26.5.0/bin:$HOME/.cargo/bin:$PATH"
[[ $(node --version) == v26.5.0 ]]
[[ $(wasm-bindgen --version) == 'wasm-bindgen 0.2.129' ]]
[[ $(sed -n 's/^tested=//p' "$seed/manifest") == "$final" ]]
(cd "$seed/wasm" && sha256sum -c SHA256SUMS)
git -C "$repo" cat-file -e "$final^{commit}"
[[ $(git -C "$repo" rev-parse "$final^{tree}") == "$(sed -n 's/^tree=//p' "$seed/manifest")" ]]
[[ $(git -C "$private" rev-parse HEAD) == "$final" ]]
"$kit/check-seed-history.sh" "$seed"

phase=slot
mkdir "$slot"
release_slot() {
  rm -f "$slot/holder"
  rmdir "$slot"
}
trap 'code=$?; release_slot || code=1; record_status "$code"; exit "$code"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
printf 'release kit pid %s %s FINAL %s Linux public and private builds; -j 4\n' "$$" "$(date '+%F %T %Z')" "$short" > "$slot/holder"

if [[ "$task" == public ]]; then
phase=public
if [[ -d "$seed/linux" ]]; then mv "$seed/linux" "$seed/linux-prior-$(date +%s)"; fi
if [[ -d "$seed/checkout/tmp/wasm-from" ]]; then mv "$seed/checkout/tmp/wasm-from" "$seed/wasm-from-prior-$(date +%s)"; fi
date '+Linux public started %F %T %Z' > "$release/linux-$short.log"
bash "$kit/native-build.sh" linux "$seed" "$seed/wasm" >> "$release/linux-$short.log" 2>&1
grep -Fq "richview 3.0.0 (commit $(sed -n 's/^public=//p' "$seed/manifest" | cut -c1-10), runtime $(cat "$seed/wasm/RUNTIME"))" "$release/linux-$short.log"
(cd "$seed/linux" && sha256sum "$archive" manifest.json > SHA256SUMS)
date '+Linux public verified %F %T %Z' > "$release/linux-$short.done"

phase=complete
exit 0
fi

phase=private
if [[ -d "$go5" ]]; then mv "$go5" "$go5-prior-$(date +%s)"; fi
[[ $(git -C "$private" rev-parse HEAD) == "$final" ]]
[[ -z $(git -C "$private" status --porcelain --untracked-files=normal) ]]
mkdir -p "$go5" "$private/tmp/wasm-from"
for module in docs.wasm richview_runtime.wasm richview_site.wasm; do
  cp "$seed/wasm/$module" "$private/tmp/wasm-from/"
done
date '+Linux private started %F %T %Z' > "$go5/build.log"
(cd "$private" && scripts/linux-release/build.sh --wasm-from tmp/wasm-from) >> "$go5/build.log" 2>&1
target=$private/target/linux-release-ubuntu-22.04/release
cp "$target/install/$final/$archive" "$go5/"
cp "$target/richview-build-manifest.json" "$go5/manifest.json"

phase=private-checks
cd "$go5"
(
  date '+Linux private check %F %T %Z'
  sha256sum "$archive" manifest.json | tee SHA256SUMS
  stat -c 'size bytes: %s' "$archive"
  identity=$(tar -xOzf "$archive" ./BUILD)
  [[ "$identity" == "$(printf 'commit=%s\nplatform=x86_64-unknown-linux-gnu' "$final")" ]]
  printf '%s\n' "$identity"
  mkdir check
  tar -xzf "$archive" -C check ./bin/richview ./bin/richview-sql-worker
  version=$(check/bin/richview --version)
  printf '%s\n' "$version"
  [[ "$version" == "richview 3.0.0 (commit ${final:0:10}, runtime $(cat "$seed/wasm/RUNTIME"))" ]]
  worker=$(printf '{}' | check/bin/richview-sql-worker)
  printf 'worker: %s\n' "$worker"
  [[ "$worker" == '{"status":"failed","error":"input"}' ]]
  for binary in richview richview-sql-worker; do
    glibc=$(objdump -T "check/bin/$binary" | grep -o 'GLIBC_[0-9.]*' | sort -Vu | tail -1)
    printf '%s %s\n' "$binary" "$glibc"
    [[ $(printf '%s\n' "$glibc" GLIBC_2.35 | sort -V | tail -1) == GLIBC_2.35 ]]
  done
  node - "$final" "$seed/wasm" <<'NODE'
const fs = require('node:fs');
const crypto = require('node:crypto');
const path = require('node:path');
const [commit, wasm] = process.argv.slice(2);
const { inputs } = JSON.parse(fs.readFileSync('manifest.json', 'utf8'));
if (inputs?.commit !== commit || inputs.dirty !== false) throw Error('private build provenance differs');
for (const name of ['docs.wasm', 'richview_runtime.wasm', 'richview_site.wasm']) {
  const bytes = fs.readFileSync(path.join(wasm, name));
  const sha256 = crypto.createHash('sha256').update(bytes).digest('hex');
  if (inputs.wasm_from?.[name]?.sha256 !== sha256 || inputs.wasm_from?.[name]?.bytes !== bytes.length) {
    throw Error(name + ' does not match the macOS seed');
  }
  console.log(sha256, name);
}
NODE
  printf 'PASS private archive identity, CLI, worker, glibc and shared Wasm\n'
) > summary.txt 2>&1
phase=complete
