#!/usr/bin/env bash
# Build the release on the glibc 2.35 baseline without publishing anything. Any arguments go to
# richview-build, such as `--wasm-from tmp/wasm` to bind the raw Wasm the macOS build made, so both
# archives carry the same runtime; that folder must lie inside the checkout, which the container sees.
set -euo pipefail
root=$(git rev-parse --show-toplevel)
common=$(git rev-parse --path-format=absolute --git-common-dir)
cargo_home=${CARGO_HOME:-$HOME/.cargo}
rustup_home=${RUSTUP_HOME:-$HOME/.rustup}
node=$(command -v node)
node_dir=$(dirname "$(dirname "$(readlink -f "$node")")")
[[ $(node --version) == "v$(cat "$root/.node-version")" ]] || { echo 'Use the pinned Node version' >&2; exit 1; }
[[ $(wasm-bindgen --version) == 'wasm-bindgen 0.2.129' ]] || { echo 'Install wasm-bindgen-cli 0.2.129' >&2; exit 1; }
[[ $(uname -m) == x86_64 && $(uname -s) == Linux ]] || { echo 'Requires Linux x64' >&2; exit 1; }
[[ -z $(git -C "$root" status --porcelain --untracked-files=normal) ]] || { echo 'Build requires a clean committed checkout' >&2; exit 1; }
[[ -s "$root/scripts/third-party-notices-manifest.json" ]] || { echo 'Third-party notices manifest is missing' >&2; exit 1; }
mkdir -p "$cargo_home/registry" "$cargo_home/git"
docker build -t richview-linux-release "$root/scripts/linux-release"
docker run --rm --user "$(id -u):$(id -g)" \
  --volume "$root:$root" --volume "$common:$common:ro" \
  --volume "$cargo_home/bin:/host-cargo-bin:ro" --volume "$cargo_home/registry:/cargo/registry" \
  --volume "$cargo_home/git:/cargo/git" --volume "$rustup_home:/rustup:ro" \
  --volume "$node_dir:/node:ro" --workdir "$root" \
  --env HOME=/tmp --env CARGO_HOME=/cargo --env RUSTUP_HOME=/rustup \
  --env CARGO_TARGET_DIR="$root/target/linux-release-ubuntu-22.04" \
  --env PATH=/host-cargo-bin:/node/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
  richview-linux-release bash -euc 'npm ci --ignore-scripts; cargo run --release --locked -p richview-build -- --repository-public "$@"' build "$@"
