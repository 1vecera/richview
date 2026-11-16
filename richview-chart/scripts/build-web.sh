#!/usr/bin/env bash
# Build all shared native/browser targets and stage the chart showcase at dist/web/charts/.
set -euo pipefail
crate="$(cd "$(dirname "$0")/.." && pwd)"
repo="$(cd "$crate/.." && pwd)"
cd "$repo"
cargo run --release -p richview-build
