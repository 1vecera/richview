# Evidence: checks, CI and performance on origin/main

Evidence for lockdown part 02 (delivery model and gates, `G-`) and part 07 (performance, `P-`). It records what verifies RichView today, how long each check takes, which performance budgets hold, and which GitHub limits shape a `dev`/`prod` auto-merge model. Every `G-` and `P-` ID is a proposal until Daniel answers the parts.

**Scope and method.** Commit `b4db233` (`origin/main`, merge of PR #61, 2026-10-16) in `github.com/1vecera/richview`, checked out in this lockdown worktree. On 2026-10-21 (18:12–18:54 UTC) I built it from an empty `target/`, ran every check that `.github/workflows/ci.yml` runs except the Firefox loop, ran the performance harnesses and the budget checker, and read the GitHub Actions history, repository settings and billing usage read-only with `gh`. I changed no code, settings or remote state. Timing logs stay in the ignored `tmp/lockdown-checks/`; build outputs (`target/`, `dist/web`) stay in place for screenshot work. Paths are relative to the repository; `path:N` is line N at `b4db233`.

**Confidence tags.** **O** observed: verified in code, a 2026-10-21 run or a read-only GitHub API call. **S** stated: written in a doc, PR or ticket, not verified here. **I** inferred: my reading across sources.

## Run context

| Item | Value | Tag |
| --- | --- | --- |
| Host | Claw Mini (Mac mini, Apple M4, 10 cores, 16 GiB), macOS (Darwin 25.3.0) | O |
| Load | 1-minute load average 2.6–7.3 during runs (other agents active); not a quiet machine | O |
| Rust | `rustc 1.99.0 (b940084d7 2026-09-28)`, `cargo 1.99.0`, pinned by `rust-toolchain.toml:2`; `wasm-bindgen 0.2.129` | O |
| Cargo settings | `CARGO_BUILD_JOBS=4`, `nice -n 10`, worktree-local `target/` (empty at start), shared `~/.cargo/registry` (warm download cache, 335 MB) | O |
| Node and browsers | Node v26.5.0 locally; Playwright 1.63.0 with Chromium 153.0.8010.12 and WebKit 26.6. Firefox (Playwright revision 1543) is not installed on this host, so the Firefox loop was not run | O |
| Node pin conflict | `.node-version` says `20.11.0`; CI uses Node 22 (`ci.yml:26`); the host has v26.5.0 | O |
| Artifacts | Wasm sha256 `67270c8c…d8d5`, CLI sha256 `8d83286a…103f` | O |

## Key findings

- Every check CI runs passes on `b4db233` locally, except the Firefox loop, which was not run: 438 Rust tests, fmt, both clippy runs, the build, 4 host checks, 27 Chromium suites, docs, 23 WebKit suites and the 60-page chart review. **O**
- Hosted CI has not executed a single step since 2026-10-13. Every run since then ended within 25 s (nearly all in 2–8 s) without starting its job. All seven runs I sampled carry the annotation "The job was not started because recent account payments have failed or your spending limit needs to be increased". The current `ci.yml` (last changed 2026-10-15) has never run on GitHub, and `main` received PRs #60 and #61 with only billing-blocked runs. **O**
- The full Rust suite takes 43–44 s warm, which breaks Daniel's 30 s maximum for a fast tier. One test causes 26.6 s of that time (`crates/richview-core/tests/contracts.rs:231`), and a second takes 4.3 s (`crates/richview-app/src/asset_upload/upload.rs:1233`). Without those two tests, the remaining 436 tests run in 10.9–16.0 s warm. **O**
- After an edit, rebuild overhead dominates. On this Mac, each newly linked test executable costs 0.2–0.6 s on its first launch, and the workspace has 58 test executables. Switching between `-p <crate>` and `--workspace` also recompiles 1–41 crates, because Cargo resolves features differently for each selection. **O**
- With `docs/PERFORMANCE.md`'s own harness on this host, 24 of 25 budgets hold, along with all 7 integrity checks. The 25th, the Linux `VmHWM` memory gate, fails closed on macOS by design. The last recorded capture (`9b300f0`, Linux, 2026-10-12) failed 6 of 25. Neither capture ran on a quiet machine, and the budgets name no reference host. **O**
- The Wasm transfer budget is nearly used up. The browser received 1,520,124 gzip bytes against a 1,572,864-byte limit, leaving 3.4 % headroom; on 2026-10-12 it was 1,404,354 bytes. **O**
- No automatic visual regression exists. The chart-review matrix asserts geometric invariants and saves 1,202 PNGs, but it never compares pixels across builds. **O**
- GitHub cannot enforce `prod` on this repository today. Branch protection, rulesets and branch rules return HTTP 403 on this GitHub Free private repository, and `allow_auto_merge` is false. Even with Pro, push restrictions and bypass lists need an organization. Agents also act as Daniel's own account. **O** (plan, 403, settings) / **S** (docs) / **I** (consequence)

## Check inventory

### Rust

| Crate | Unit tests | Integration tests (files) | Doc tests | Warm time inside test binaries | Tag |
| --- | --- | --- | --- | --- | --- |
| `richview-chart` | 153 | 129 (30) | 1 | 2.3 s | O |
| `richview-core` | 14 | 77 (10) | 0 | 27.0 s (26.5 s in `tests/contracts.rs`) | O |
| `richview-app` | 6 | 23 (3) | 0 | 9.9 s | O |
| `richview-editor` | 3 | 14 (3) | 0 | 0.5 s | O |
| `richview-web` | 17 | 0 | 0 | 1.0 s | O |
| `richview-build` | 1 | 0 | 0 | 0 s | O |
| `richview-bench` | 0 | 0 | 0 | — | O |
| **Total** | 194 | 243 (46) | 1 | 40.8 s | O |

`cargo test --workspace --locked` runs 438 tests through 63 test executions (58 test executables and 5 doc-test runs), with 0 ignored and 0 failed. No tier markers exist: there are no `#[ignore]` tests, no `[features]` in any crate manifest and no nextest configuration. The runner also sees no wasm-bindgen tests. **O** PR #68 (unmerged) also reports "all 438 Rust test executions". **S**

The slowest tests are `repeated_embedded_assets_and_complete_report_overhead_have_a_bounded_output` at 26.6 s in a debug build (it renders reports up to the 64 MiB output limit, `crates/richview-core/src/lib.rs:56`), `uploads_resume_from_listed_parts_retry_verify_and_abort` at 4.3 s (its retry policy uses 1–4 ms delays, so the cause was not isolated), and the two `installed_assets` tests at 2.0 s and 1.1 s. All other tests take 0.3 s or less. **O**

### Format, lint and build

| Check | Command | Source | Result 2026-10-21 | Time on this host | Tag |
| --- | --- | --- | --- | --- | --- |
| Format | `cargo fmt --all --check` | `ci.yml:18`, `CLAUDE.md:28` | pass | 0.5 s | O |
| Native clippy | `cargo clippy --workspace --all-targets --locked -- -D warnings` | `ci.yml:19`, `CLAUDE.md:29` | pass, 0 warnings | 12.4 s first run after the test build (203 crates checked); 0.3 s warm; 2.9 s after touching `richview-chart/src/lib.rs` | O |
| Wasm clippy | `cargo clippy -p richview-web --target wasm32-unknown-unknown --locked -- -D warnings` | `ci.yml:20`, `CLAUDE.md:30` | pass, 0 warnings | 19.0 s first run; 0.3 s warm | O |
| Release build | `cargo run --release --locked -p richview-build` | `ci.yml:23`, `CLAUDE.md:27` | pass; tracked files unchanged | 153.9 s cold (builder 69 s, native 57 s, Wasm 25 s, staging about 3 s; peak RSS 1.6 GB); 105.9 s after touching `richview-chart/src/lib.rs`; 2.1 s no-op | O |
| Benchmark fixtures | `cargo run --release --locked -p richview-bench -- --write-fixtures --fixtures-only` | `ci.yml:28` | pass | 27.3 s, because selecting `-p richview-bench` recompiled 8 crates | O |
| wasm-bindgen install | `cargo install wasm-bindgen-cli --version 0.2.129 --locked` | `ci.yml:22` | preinstalled here | 100 s on the hosted runner (run 38290782212) | O |

The builder writes ignored outputs (`dist/web`, `packages/*/dist`, `packages/vscode/{bin,web,font-licenses}`, `target/`). It also regenerates the tracked `packages/vscode/snippets/rmd.json` (`crates/richview-build/src/main.rs:331`), which came out byte-identical here, so `git status` showed only untracked lockdown files. **O**

### Host and package checks

| Check | Covers | CI line | Result | Time | Tag |
| --- | --- | --- | --- | --- | --- |
| `node scripts/verify-editor.cjs` | VS Code API double driving the real native editor process: preview, refresh, diagnostics, formatting, completion | `ci.yml:29` | pass | 2.1 s | O |
| `node scripts/verify-native.mjs` | Release CLI generate/validate/check, MCP (7 tools, 2 resources), loopback server and publication store | `ci.yml:30` | pass | 4.0 s | O |
| `node scripts/verify-compat.mjs` | Node package facades (`render`, `parse`, schemas, `renderChart`) | `ci.yml:31` | pass | 0.1 s | O |
| `npx tsc --noEmit --strict … scripts/verify-types.ts` | Generated declaration types | `ci.yml:32` | pass | 1.2 s | O |

### Browser behaviour (Playwright)

Suites using `scripts/browser-harness.mjs` start the release `richview-server` on a random loopback port, with a fresh copy of `dist/web` per run (`scripts/browser-harness.mjs:60`). They choose the engine through `RICHVIEW_VERIFY_BROWSER` (default Chromium, `:42`) and run headless (`:77`). **O** Each run copies the 78 MB distribution, so one full pass left 4.3 GB in `tmp/verification/`; I deleted those copies afterwards. **O**

| Suite (`scripts/verify-*.mjs`) | Covers | Chromium in CI | Firefox/WebKit loop (`ci.yml:61-67`) | Chromium time | WebKit time |
| --- | --- | --- | --- | --- | --- |
| `browser` | Playground: live Rust render, all chart types, themes, offline HTML download, worker races, paged source editing, undo/redo, table sorting | `:34` | yes | 33.9 s | 46.2 s |
| `landing` (imports `verify-homepage.mjs`) | Homepage controls keep the prerendered first frame | `:35` | yes | 10.8 s | 12.5 s |
| `cloudflare` | Local `wrangler dev` with local D1: signup endpoint (`verify-interest.mjs`) and homepage, no credentials | `:36` | no | 15.5 s | — |
| `showcase` | Lazy mount and settled state of every chart in showcase reports | `:37` | yes | 18.7 s | 26.0 s |
| `flagship` | Research snapshot through native/browser render and offline export | `:38` | no | 14.5 s | — |
| `reel` | Rust/Wasm seeks, playback, reduced motion, native media output | `:39` | no | 5.8 s | — |
| `browser-api` | Generated ESM facades in a browser | `:40` | yes | 3.5 s | 5.6 s |
| `chart-api` | Wasm chart handles reject malformed replacements | `:41` | yes | 4.2 s | 4.7 s |
| `text-measurement` | SVG label geometry without paint halos | `:42` | yes | 4.3 s | 4.3 s |
| `figure-expand` | Figure controls, width changes, exports, lifecycle | `:43` | yes | 17.7 s | 17.8 s |
| `chart-keyboard` | Keyboard focus survives SVG replacement | `:44` | yes | 6.5 s | 7.2 s |
| `numeric-zoom` | Finite numeric zoom intervals | `:45` | yes | 28.7 s | 29.0 s |
| `numeric-safety` | Extreme values across 12 chart channels | `:46` | yes | 14.1 s | 14.4 s |
| `source-tables` | Complete source tables and CSV downloads | `:47` | yes | 7.6 s | 8.2 s |
| `temporal-fidelity` | Temporal values, ranges, original downloads | `:48` | yes | 5.8 s | 6.2 s |
| `temporal-decimals` | Exact decimal temporal readouts | `:49` | yes | 14.7 s | 15.3 s |
| `csv-decimals` | CSV decimals and integers beyond 2^53 | `:50` | yes | 8.2 s | 8.6 s |
| `histograms` | Histogram contracts in the browser and export | `:51` | yes | 22.2 s | 23.1 s |
| `boxplots` | Box plot contracts in the browser and export | `:52` | yes | 35.3 s | 36.4 s |
| `intervals` | Interval chart contracts | `:53` | yes | 36.0 s | 35.8 s |
| `chart-readouts` | Long readouts scroll without changing state | `:54` | yes | 5.7 s | 5.3 s |
| `step-curves` | Step curves through updates, zoom, export | `:55` | yes | 9.4 s | 10.1 s |
| `line-smoothing` | Smoothing contract through updates, zoom, export | `:56` | yes | 6.7 s | 7.2 s |
| `bar-paint` | Painted bar geometry and one entrance | `:57` | yes | 10.4 s | 10.2 s |
| `ink-alignment` | Painted pixel columns at 2x | `:58` | yes | 18.8 s | 16.3 s |
| `facet-export` | Whole-facet SVG export | `:59` | yes | 6.2 s | 6.2 s |
| `motion-runtime` (Chromium only) | Lazy mount, print, facets, reduced motion | `:60` | no | 1.6 s | — |
| `docs` | Published tutorial through native, MCP and browser boundaries | `:68` | no | 8.3 s | — |
| **Total** | 27 suites + docs (Chromium); 23 suites (WebKit) | | | **374.8 s** | **356.5 s** |

All 51 runs passed (exit 0). **O** FEATURES.md states that the full screenshot matrix, the timing budgets, PDF output and multi-touch run only in Chromium, while focused ink measurements run in all three engines (`docs/FEATURES.md:45`). **S** It also records a known WebKit limitation: preview navigation clears native undo history (`docs/FEATURES.md:47`). **S**

### Visual and screenshot checks

| Check | What it does | In CI | Result and time here | Tag |
| --- | --- | --- | --- | --- |
| `node scripts/verify-chart-review.mjs --out … --phase after` | Captures 3 fixed cases (standard 8 charts, flagship 18, bar study 1) in 5 styles × 4 widths (1440/768/390/320): 60 pages and 540 figures, each with a resting and a keyboard-tooltip screenshot. In the `after` phase it asserts no label collisions or clipping, no page overflow, no browser errors, one calendar row, no heatmap colour bars, caption, axis title and legend alignment within 1.5 px, tooltip containment, and a series toggle cycle at 390 px (`scripts/verify-chart-review.mjs:78-124`) | `ci.yml:69`; artifact `chart-review` kept 7 days (`:70-79`) | pass: 60 captures, 540 figures, 0 label findings, 0 errors; 1,202 PNGs, 227 MB; 136.3 s | O |
| Same, with `--baseline DIR` | Asserts that the baseline used the same fixture, themes and widths, and records the baseline commit (`:30-34`). **No pixel or perceptual comparison.** | no | — | O |
| Screenshots inside behaviour suites | `browser`, `docs`, `figure-expand`, `homepage`, `showcase`, `flagship`, `motion-runtime`, `reel` and the per-chart suites write PNGs to `tmp/verification/<suite>-*/`; CI uploads JSON plus text-measurement PNGs as `browser-contracts` (`ci.yml:80-87`) | partly | captured, never compared | O |
| `scripts/design-review/capture.mjs` | Full-page and chart crops, motion filmstrips, frame pacing, layout diagnostics and contact sheets; the optional baseline is only shown side by side. Its only pixel comparison is within one build: settled versus reduced-motion frames, at most 0.5 % differing pixels (`scripts/design-review/capture.mjs:146-147`, `sheets.mjs:78-80`) | no | not run | O |
| `scripts/design-review/figma-plates.mjs` | Editable before/after SVG plates for Figma (`docs/CHART-REVIEW.md:12,21`) | no | not run | S |
| `benchmarks/chart-craft/`, `benchmarks/highcharts/` | Side-by-side comparison against Highcharts 13.1.1, ECharts, Plotly and ggplot2, installed outside the repository (`benchmarks/chart-craft/README.md`, `benchmarks/highcharts/README.md`) | no | not run (needs reference installs and R) | S |

### Checks outside CI

| Check | Needs | Status on `b4db233` | Tag |
| --- | --- | --- | --- |
| `scripts/verify-authoring.mjs` | Harness only | **Fails:** `'auto' !== '8px'` at `scripts/verify-authoring.mjs:40` (expects bar `rx: 8px`; bar ends changed in `39b301f`, 2026-10-15) | O |
| `scripts/verify-exports.mjs` | Poppler and qpdf, a Chromium-family browser | not run: `qpdf` missing on this host | O |
| `scripts/verify-assets.mjs` | `GARAGE_BIN` (Garage ≥ 2.3) | not run: Garage missing | O |
| `scripts/verify-packaging.py` | A locally packaged VSIX | not run: no VSIX packaging step in the build | O |
| `scripts/verify-vscode.sh` / `.cjs` | An isolated X11 runner and a VSIX; Linux | not run; must not touch the live desktop | O |
| `scripts/verify-homepage.mjs`, `scripts/verify-interest.mjs` | — | run indirectly through `landing` and `cloudflare` | O |
| `scripts/dev-harness/run.py` (smoke, site and integration suites) | A shared build helper | exists only on PR #68's branch (unmerged); its `integration` suite is the CLAUDE.md list, not CI's | S |
| `benchmarks/chart-craft/verify.mjs`, `benchmarks/highcharts/verify.mjs` | Reference-engine installs outside the repository | not run | O |

Not checks: `scripts/chart-appearance.mjs`, `flagship-images.mjs` and `marker-ink.mjs` are helpers imported by the `flagship`, `showcase` and `ink-alignment` suites. `scripts/record-reel.py`, `stage-docs-flagship.py` and `scripts/fonts/build.py` produce assets. **O**

### Benchmarks and performance harness

| Tool | Measures | Feeds | Time here | Tag |
| --- | --- | --- | --- | --- |
| `crates/richview-bench` (via `scripts/benchmark-rust.mjs`) | Warm native render of 4 fixtures, 100 iterations; Linux `VmHWM` | `tmp/performance/rust.json` → `P-NATIVE-*`, `P-NATIVE-MEMORY` | 12.7 s | O |
| `scripts/benchmark-native.py` (`uv run`) | Process-cold CLI (100 runs × 6), editor preview RPC (100 × 3), HTTP with 1 and 4 clients | `native-surfaces.json` → `P-CLI-*`, `P-EDITOR-*`, `P-HTTP-*` | 75.9 s | O |
| `scripts/benchmark-browser.mjs` | Dense zoom, theme switch, source edits, cold mount, delivered Wasm | `browser.json` → `P-ZOOM-*`, `P-THEME-*`, `P-EDIT-*`, `P-COLD-MOUNT`, `P-WASM-GZIP` | 19.5 s | O |
| `scripts/benchmark-boundary.mjs` | Node↔Wasm boundary costs | `boundary.json`; disclosure, not a gate (`scripts/check-performance.py:305`) | 49.0 s | O |
| `scripts/check-performance.py` | Evaluates 25 budgets and 7 integrity checks; missing evidence fails | `gates.json` | 0.05 s | O |
| `cargo run --release -p richview-chart --example benchmark` | Chart kernel ingestion, layout and hover | Disclosure (`richview-chart/README.md:81`) | not run | S |
| `scripts/benchmark-baseline.mjs`, `scripts/benchmark-preview.py` | Before/after baseline and preview timing helpers | Not referenced by PERFORMANCE.md | not run | O |

## Hosted CI

**Workflow.** There is one workflow on `main`, "Rust product checks" (`.github/workflows/ci.yml`). It triggers only on `push` and `pull_request` to `main` (`:3-7`), has read-only contents permission (`:9-10`), and runs a single `ubuntu-latest` job with a 60-minute timeout (`:13-14`) through the steps listed under Format, lint and build, Host and package checks, Browser behaviour (Playwright) and Visual and screenshot checks. Two more workflows are registered on the repository: "Publication validation" (`publish.yml`, only on PR #70's branch) and a historical "richview-chart" workflow. **O**

**History** (`gh run list -R 1vecera/richview --limit 400`, 298 runs since 2026-05-21). **O**

| Fact | Evidence |
| --- | --- |
| "Rust product checks": 140 success, 124 failure; "Publication validation": 31 failure | run list |
| Last green run: 2026-10-12, `5d47fbd`, 28.9 min ([38290782212](https://github.com/1vecera/richview/actions/runs/38290782212)); green runs had a median of 18.1 min and a maximum of 48.7 min | run list |
| Last runs that executed steps: 2026-10-13 on `main`. They failed at `node scripts/verify-browser.mjs` when a Playwright wait for `#chart-style option` timed out after 30 s ([38417834734](https://github.com/1vecera/richview/actions/runs/38417834734), `cff6511`) | `gh run view --log-failed` |
| Since 2026-10-12 18:40 UTC, 140 of the 144 runs failed in ≤ 10 s. The other four are the three executed runs of 2026-10-13 and one 25 s run on 2026-10-18 that executed 0 steps. `gh run view 39182846372 --log-failed` returns "log not found" | run list, jobs API |
| Billing annotation: "The job was not started because recent account payments have failed or your spending limit needs to be increased. Please check the 'Billing & plans' section in your settings". It appears on all seven runs sampled across 2026-10-12…2026-10-19, including [39182846372](https://github.com/1vecera/richview/actions/runs/39182846372) (the most recent run) and [38922906380](https://github.com/1vecera/richview/actions/runs/38922906380) (the push of `b4db233` itself) | check-run annotations |
| `ci.yml` was last changed 2026-10-15 (`407e62c`), after the last executed run, so the current workflow has never run on GitHub | `git log -- .github/workflows/ci.yml` |
| Second annotation on recent runs: "The ubuntu-latest label will migrate to Ubuntu 26 beginning October 19, 2026" | annotations |

**Hosted vs local step times** (last green run against this host). The build took 283 s there and 154 s here; tests 93 s and 105 s cold; native clippy 47 s there as the first compile step, against 12 s here after the test build; `verify-browser` 169 s and 34 s; `landing` 64 s and 11 s; `cloudflare` 65 s and 16 s; chart review 193 s and 136 s. The hosted run's workflow was smaller than today's: it had no extra per-chart suites and no Firefox/WebKit loop. **O** One full CI-equivalent pass, run sequentially on this host without Firefox, takes about 19.9 min (1,193 s: build, fmt, clippy, cold tests, fixtures, host, Chromium plus docs, WebKit and chart review). **O** A hosted run of the current workflow would probably take 45–90 min, which risks the 60-minute timeout. **I**

**Billing after the transfer.** These runs were billed to the previous owner organization. The repository now belongs to the personal account `1vecera` on GitHub Free (`gh api user` → `plan.name: free`). That account's billing usage for October 2026 shows 490 Actions Linux minutes with a net cost of $0, and no RichView usage. **O** GitHub Free includes 2,000 Actions minutes a month for private repositories ([GitHub plans](https://docs.github.com/en/get-started/learning-about-github/githubs-plans), read 2026-10-21). **S** No run has happened since the transfer, so whether runs still fail is unknown. At 30–90 min per full run, the free quota covers roughly 22–67 full runs a month (17–50 for the rest of October), which is not enough for per-PR CI under frequent agent merges. **I**

## Measured test tiers

Daniel's rule: "a fast tier that covers most changes in the edit loop, under 10 s where possible and 30 s at most on Lenovo including startup, and slower marked tiers… Speed comes from fixing measured waste, never from dropped coverage or hidden skips" (DANIEL-PREFERENCES.md, Coding progress and verification). **S** All times below are from Claw Mini; Lenovo was not measured.

| Command | Tests | Warm, no rebuild | After `touch richview-chart/src/lib.rs` | First run after switching package selection | Tag |
| --- | --- | --- | --- | --- | --- |
| `cargo fmt --all --check` | — | 0.5 s | — | — | O |
| `cargo test -p richview-chart --lib` | 153 | 0.09–0.11 s | 1.9 s | 8.2 s (11 crates rebuilt) | O |
| `cargo test -p richview-chart -p richview-core --lib` | 167 | 0.14 s | — | 2.1–4.1 s | O |
| `cargo test -p richview-core -- --skip <slow test>` | 90 (1 filtered) | 0.8 s | — | 10.4 s | O |
| `cargo test -p richview-chart` | 283 | 3.7 s | 21.7 s | 21.4–21.9 s | O |
| `cargo test --workspace --lib` | 190 | 5.6–5.8 s | 10.7 s | 10.5 s | O |
| `cargo test --workspace -- --skip <2 slow tests>` | 436 (2 filtered) | 10.9–16.0 s | 54.4 s (core touch: 33.2 s) | 42.6–50.3 s | O |
| `cargo test --workspace --locked` (full) | 438 | 42.9–44.4 s | — | 104.8 s cold, including the debug build | O |
| `cargo clippy` native / Wasm | — | 0.3 s / 0.3 s | 2.9 s (native) | 12.4 s / 19.0 s | O |
| Release build | — | 2.1 s (no-op) | 105.9 s | 153.9 s cold | O |
| Host checks (4) | — | 7.5 s | — | — | O |
| Chromium suites + docs | 28 runs | 374.8 s | — | — | O |
| WebKit loop | 23 runs | 356.5 s | — | — | O |
| Chart review matrix | 60 pages | 136.3 s | — | — | O |
| Performance harness and checker | 25 budgets | 157.2 s | — | — | O |

**Where the time goes.** **O**
- Two tests take 30.9 s of the 40.8 s spent inside test binaries.
- When a rebuild relinks most test executables, wall time exceeds compile time plus in-binary time by 16–37 s; without a rebuild the excess is 1–3 s. For example, `--workspace -- --skip` after touching `richview-chart` spent 7.9 s compiling and 9.8 s in tests, yet took 54.4 s, and `-p richview-chart` (33 executables) carried 16.4 s of excess. Copying one test binary three times shows a first launch of 0.27, 0.61 and 0.21 s against 0.003 s for a second launch; `spctl --status` reports "assessments enabled". With 58 test executables this adds tens of seconds to every edit loop. Attributing it to macOS executable assessment is **I**.
- Cargo resolves features per selection, so `-p richview-core --lib` after a workspace run recompiled 41 crates, and returning to `--workspace` recompiled 7.
- The release build uses LTO with `codegen-units = 1` (`Cargo.toml:12-17`), so a one-file edit in `richview-chart` costs 106 s against 154 s cold.

**Tier reading.** No fast tier meets the rule for every change today. `cargo test -p richview-chart --lib` (153 tests, 1.9 s after a chart edit) fits under 10 s, and `cargo test --workspace --lib` nearly does (190 tests, 5.6 s warm, 10.7 s after a chart edit), but both skip all 243 integration tests. **O** A fast tier of all tests except the two slow ones fits the 30 s maximum only without a rebuild (10.9–16.0 s). **O** The rewrite could get there without dropping coverage. **I**
- Move the 64 MiB output-limit test to a marked slow tier, or make it cheaper.
- Find and remove the cause of the upload test's 4.3 s.
- Build one integration-test executable per crate instead of 46 files.
- Keep one package selection, or unify features, so switching does not rebuild.

All of these need code changes. On Lenovo the cost of a newly linked binary may differ. **I**

## Performance budgets (`docs/PERFORMANCE.md`)

The budgets are in `docs/PERFORMANCE.md:5-19`: 25 endpoints and 7 integrity checks (`:21`), enforced only when someone runs `scripts/check-performance.py`. CI does not run it. **O** The reproduction order is: build, generate fixtures, then on a quiet machine `RICHVIEW_BENCH_ITERATIONS=100 node scripts/benchmark-rust.mjs`, `uv run scripts/benchmark-native.py`, `node scripts/benchmark-boundary.mjs`, `node scripts/benchmark-browser.mjs` and `uv run scripts/check-performance.py --evidence-dir tmp/performance` (`:25-35`). **S** The rule "Do not widen a limit to fit a result" is at `:21`. **S**

The "recorded" column is the capture of `9b300f0` (2026-10-12) on an "Intel Core i7-12700H, Linux, shared host" with 4 pinned cores and `quiet_machine: false` (`docs/evidence/chart-review/verification.json`). **S** The "now" column is `b4db233` on 2026-10-21 on this Mac, at a load average of 3.7–4.7. **O** The dense fixture bytes differ between the two captures (sha256 `8089599d…` recorded, `d4a657d6…` now), and `crates/richview-bench/src/main.rs` changed after `9b300f0` (37 lines added, 9 removed), so the dense rows do not compare like for like. **O**

| Proposed ID | Endpoint (checker name) | Limit | Recorded 9b300f0, Linux | Now b4db233, Mac | Holds now | Command |
| --- | --- | --- | --- | --- | --- | --- |
| `P-NATIVE-SMALL` | Warm native report p95 (`native.small`) | ≤ 5 ms | 1.9 | 0.63 | yes | `benchmark-rust.mjs` |
| `P-NATIVE-SIX` | `native.six-types` | ≤ 10 ms | 2.4 | 0.68 | yes | same |
| `P-NATIVE-FLAGSHIP` | `native.flagship` | ≤ 35 ms | 25.0 | 12.28 | yes | same |
| `P-NATIVE-DENSE` | `native.dense` | ≤ 250 ms | **420.4 (fail)** | 113.08 | yes | same |
| `P-CLI-SMALL` | Process-cold complete offline CLI p95 (`cli.small`) | ≤ 150 ms | 54.9 | 23.63 | yes | `benchmark-native.py` |
| `P-CLI-FLAGSHIP` | `cli.flagship` | ≤ 250 ms | 121.9 | 62.05 | yes | same |
| `P-CLI-DENSE` | `cli.dense` | ≤ 400 ms | **652.0 (fail)** | 191.94 | yes | same |
| `P-EDITOR-SMALL` | Warm editor preview RPC p95 | ≤ 10 ms | 2.4 | 1.22 | yes | same |
| `P-EDITOR-FLAGSHIP` | | ≤ 50 ms | 46.8 | 19.5 | yes | same |
| `P-EDITOR-DENSE` | | ≤ 400 ms | 298.4 | 113.95 | yes | same |
| `P-HTTP1-SMALL` | Gzip loopback HTTP p95, 1 client | ≤ 200 ms | 112.9 | 50.25 | yes | same |
| `P-HTTP1-DENSE` | | ≤ 600 ms | 446.7 | 202.9 | yes | same |
| `P-HTTP4-SMALL` | Same, 4 clients | ≤ 500 ms | 165.4 | 58.76 | yes | same |
| `P-HTTP4-DENSE` | | ≤ 1,500 ms | 608.9 | 287.08 | yes | same |
| `P-HTTP-TPUT-SMALL` | 4-client throughput | ≥ 10 req/s | 28.4 | 73.09 | yes | same |
| `P-HTTP-TPUT-DENSE` | | ≥ 4 req/s | 6.8 | 16.45 | yes | same |
| `P-ZOOM-LAYOUT` | 100 dense zooms, Rust layout + DOM p95 | ≤ 16 ms | 8.7 | 4.5 | yes | `benchmark-browser.mjs` |
| `P-ZOOM-FRAMES` | Same, event to two animation frames p95 | ≤ 100 ms | 51.4 | 54.6 | yes | same |
| `P-THEME-FIRST` | Warm 18-chart theme switch, first frame of all charts p95 | ≤ 100 ms | **202.9 (fail)** | 44.4 | yes | same |
| `P-THEME-LATEST` | Same, latest frames after two animation frames | ≤ 100 ms | **202.9 (fail)** | 44.4 | yes | same |
| `P-EDIT-SMALL` | Small source edit to first live chart p95 | ≤ 300 ms | **416.5 (fail)** | 148.9 | yes | same |
| `P-EDIT-DENSE` | Dense worker source edit to first live chart (single event) | ≤ 1,000 ms | **2,011.5 (fail)** | 547.4 | yes | same |
| `P-COLD-MOUNT` | Cold three-example chart page, all mounts | ≤ 2,500 ms | 1,122.8 | 277.25 | yes | same |
| `P-NATIVE-MEMORY` | Native workload Linux `VmHWM` | ≤ 256 MiB | 155.1 MiB | **not measurable on macOS** | fails closed (`scripts/check-performance.py:209-223`) | `benchmark-rust.mjs` on Linux |
| `P-WASM-GZIP` | Delivered gzip Wasm bytes | ≤ 1,572,864 | 1,404,354 | 1,520,124 (96.6 %) | yes | `benchmark-browser.mjs` |
| `P-EVIDENCE-INTEGRITY` | 7 provenance and integrity checks (`:232-296`) | all pass | 6 of 7 | 7 of 7 | yes | checker |

The checker's verdict now is `failed`, 24 of 25, failing only on `native.peak-memory` with "expected string or bytes-like object, got 'NoneType'" (`tmp/performance/gates.json`). **O** PERFORMANCE.md predicts that macOS cannot produce the memory evidence (`:39`). **S**

**Conflicts.**
- The table holds or fails depending on the host. Every endpoint that failed on the shared Linux host passes on the M4 by a factor of 2–4. PERFORMANCE.md names no reference machine and asks for a quiet CPU, and neither capture used one. **O/I**
- `docs/evidence/chart-review/README.md` still describes the 9b300f0 capture as 19 of 25 with browser latency "a limitation, not a claimed improvement". **S**
- Native memory can only be gated on Linux.

Some measurements have no limit and are disclosures only:
- Node boundary: dense `rust_render_to_json_string` p95 149.8 ms; Node peak RSS 571 MiB (`tmp/performance/boundary.json`).
- Native RSS on macOS: CLI process peaks 13–31 MiB (small), 25–39 MiB (flagship) and 113–129 MiB (dense).
- Editor process current RSS after each 100-request preview series: 13 MiB (small), 160 MiB (flagship) and 1,520 MiB (dense) (`tmp/performance/native-surfaces.json`). **O**

No budget covers the editor's memory. The 256 MiB gate measures only the native report workload. **O** Whether the dense editor figure is retained allocator memory or growth was not investigated. **I**

## GitHub limits for a `dev`/`prod` model

| Fact | Evidence | Tag |
| --- | --- | --- |
| The repository is `1vecera/richview`, private, owned by a personal account (User) on GitHub Free; `default_branch: main`; `main` is unprotected | `gh api repos/richview-universe/richview-v3` (redirects); `gh api repos/1vecera/richview/branches/main` → `protected: false`; `gh api user` → `plan.name: free` (2026-10-21) | O |
| Branch protection, rulesets and branch rules are unavailable: HTTP 403 "Upgrade to GitHub Pro or make this repository public to enable this feature." | `GET …/branches/main/protection`, `GET …/rulesets`, `GET …/rules/branches/main` (2026-10-21) | O |
| Auto-merge is off (`allow_auto_merge: false`); merge commits, squash and rebase are all allowed; `delete_branch_on_merge: false` | `gh api repos/1vecera/richview` | O |
| Auto-merge is available in private repositories only on paid plans, and GitHub offers it only on PRs that cannot merge yet because required reviews or status checks are pending | [Managing auto-merge (GHES 3.13 text with plan availability)](https://docs.github.com/en/enterprise-server@3.13/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/managing-auto-merge-for-pull-requests-in-your-repository); [Automatically merging a PR](https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/incorporating-changes-from-a-pull-request/automatically-merging-a-pull-request) (read 2026-10-21) | S |
| GitHub Pro adds protected branches, required reviewers and code owners in private repositories, plus 3,000 Actions minutes (Free: 2,000) | [GitHub plans](https://docs.github.com/en/get-started/learning-about-github/githubs-plans) (read 2026-10-21) | S |
| "Restrict who can push to matching branches" works only in organization-owned repositories, and "Actors may only be added to bypass lists when the repository belongs to an organization." Admins can push to protected branches unless "Do not allow bypassing" is set | [About protected branches](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches/about-protected-branches) (read 2026-10-21) | S |
| Agents push and merge as the repository owner: `gh` on Claw Mini is authenticated as `1vecera`, and that account authored 74 of the 75 PRs and merged 60 of the 61 merged ones | `gh auth status`; `gh pr list --state all` (2026-10-21); [history-prs.md](history-prs.md) | O |
| Actions is enabled with all actions allowed. Default workflow token permission is `write`, and `can_approve_pull_request_reviews` is `true`. There are no environments, self-hosted runners or webhooks | `gh api …/actions/permissions{,/workflow}`, `…/environments`, `…/actions/runners`, `…/hooks` | O |
| No workflow deploys: `ci.yml` only verifies, so merging into any branch publishes nothing | `.github/workflows/ci.yml` | O |
| CI triggers only on `main`, so `dev`/`prod` need workflow changes | `ci.yml:3-7` | O |

**Consequences.** **I**
- On this plan GitHub can enforce neither "agents merge to `dev` only after the gates pass" nor "only Daniel merges to `prod`". An agent can merge or push to any branch.
- With Pro, `prod` could require a PR, passing status checks and an approving review, with "Do not allow bypassing". That only separates Daniel from agents if agents use a different GitHub identity (a bot account or GitHub App) whose PRs Daniel approves. Restricting pushes by identity would still need an organization.
- Without Pro, enforcement must be local: a merge tool that agents use runs the required gates on a pinned host, records a receipt (commit, artifact hashes, results, screenshots) and only then merges into `dev`. Agent permissions or hooks deny pushes to `prod`.
- Hosted CI can be an extra signal only if minutes and billing allow it. A self-hosted runner on a Daniel machine would avoid minutes but needs a decision. Daniel's standing rule is not to spend to unblock hosted CI.

## Proposed gates (`G-`)

| ID | Gate | Command or evidence | Proposed tier | Time on this host | State on `b4db233` |
| --- | --- | --- | --- | --- | --- |
| `G-FMT` | Formatting | `cargo fmt --all --check` | fast | 0.5 s | pass |
| `G-CLIPPY` | Native lint, warnings as errors | `cargo clippy --workspace --all-targets --locked -- -D warnings` | fast | 0.3–12.4 s | pass |
| `G-CLIPPY-WASM` | Wasm lint | `cargo clippy -p richview-web --target wasm32-unknown-unknown --locked -- -D warnings` | fast | 0.3–19.0 s | pass |
| `G-TEST-FAST` | Rust fast tier, under 10 s target and 30 s maximum | Proposed: the full suite minus marked slow tests | fast | 10.9–16.0 s warm; 33–54 s after edits | **does not exist yet** (needs slow-test markers; see Measured test tiers) |
| `G-TEST-FULL` | All Rust tests | `cargo test --workspace --locked` | slow (dev merge) | 43–44 s warm; 105 s cold | pass, 438/438 |
| `G-BUILD` | Release build and staging | `cargo run --release --locked -p richview-build` | dev merge | 2–154 s | pass |
| `G-FIXTURES` | Benchmark fixtures required by browser suites | `cargo run --release --locked -p richview-bench -- --write-fixtures --fixtures-only` | dev merge | 27 s | pass |
| `G-HOST` | Editor, native, compat, types | 4 checks under Host and package checks | dev merge | 7.5 s | pass |
| `G-BROWSER-CHROMIUM` | Behaviour suites + docs | 28 runs under Browser behaviour (Playwright) | dev merge | 375 s | pass |
| `G-VISUAL-MATRIX` | Fixed chart review, geometry invariants and screenshots | `node scripts/verify-chart-review.mjs --out <dir> --phase after` | dev merge | 136 s | pass, 0 findings |
| `G-VISUAL-DIFF` | Cross-build screenshot comparison against the `dev` head | Proposed new; `--baseline` today compares no pixels | dev merge | — | **missing** (needs code) |
| `G-SCREENSHOT-EVIDENCE` | Keep the screenshots and manifest of each merge for Daniel's batch review | CI keeps artifacts only 7 days (`ci.yml:74,84`) and CI does not run | dev merge | — | **missing** (needs a retention decision) |
| `G-BROWSER-WEBKIT` | Cross-engine loop | `RICHVIEW_VERIFY_BROWSER=webkit`, 23 suites (`ci.yml:61-67`) | slow (prod candidate) | 357 s | pass |
| `G-BROWSER-FIREFOX` | Cross-engine loop | Same with `firefox` | slow (prod candidate) | not run | **unverified** (browser not installed here) |
| `G-PERF-BUDGETS` | 25 budgets + 7 integrity checks | The harness under Performance budgets, on a named quiet Linux reference host | prod candidate | 157 s on this host | 24/25 here; Linux memory gate unmeasured; 19/25 on 2026-10-12 |
| `G-WASM-SIZE` | Delivered gzip Wasm ≤ 1.5 MiB | Read `dist/web/richview_web_bg.wasm.gz` after build (builder gzip 1,512,537 B; served 1,520,124 B) | dev merge (cheap) | < 1 s | pass, 3.4 % headroom |
| `G-NONCI` | PDF export, assets, packaging, VS Code, authoring | Checks outside CI | prod candidate, or retire | — | authoring **fails**; 4 not runnable here |
| `G-HOSTED-CI` | GitHub Actions run of `ci.yml` | `.github/workflows/ci.yml` | optional signal | 29 min last green | **blocked** (billing; no run since the transfer) |
| `G-PROD-MERGE` | Only Daniel merges `dev` → `prod` | Local enforcement, or Pro + separate agent identity (see GitHub limits for a `dev`/`prod` model) | prod | — | **not enforceable on GitHub today** |

## Contradictions

- **CI described as running.** `docs/CHART-REVIEW.md:17` says "CI runs this matrix on every PR and retains its screenshots and manifest… for seven days", and `docs/evidence/chart-review/README.md` repeats it. In fact no CI job has started since 2026-10-13, and the current workflow has never run (see Hosted CI). **O**
- **Required checks differ by source.** `CLAUDE.md:39-52` lists 9 Node checks plus `verify-types` and asks for "the complete required checks for an integration handoff". `ci.yml` runs 27 Chromium suites, docs, the chart review, Cloudflare, flagship and 46 cross-engine runs. PR #68's `integration` suite follows CLAUDE.md, not CI. The repository defines no single list of required checks. **O/S**
- **"Enforced" budgets.** `docs/PERFORMANCE.md:3` says the limits "are enforced by check-performance.py", but nothing runs the checker automatically. The last recorded capture failed 6 of 25, and those failures were disclosed rather than blocking (`docs/evidence/chart-review/README.md`). On the agents' host (macOS) the checker cannot pass by design. **O**
- **Tier rule versus suite shape.** Daniel's rule caps the fast tier at 30 s. The only defined Rust command, the full suite, takes 43–44 s warm, and the repository has no tier markers. **O/S**
- **Node version.** `.node-version` (20.11.0), CI (22) and this host (26.5.0) disagree. **O**
- **Visual regression.** The lockdown brief asks for visual-regression gates. The repository has screenshot capture and geometric assertions, but no comparison across builds. **O/S**
- **Checks outside CI rot.** `verify-authoring.mjs` fails on `main` without anyone noticing, because CI does not run it. **O**
- **Comparable inputs.** Performance captures claim exact input signatures, but the dense fixture bytes differ between the 2026-10-12 capture and today. **O**

## Gaps

- Firefox suites, `verify-exports.mjs`, `verify-assets.mjs`, `verify-packaging.py` and the VS Code extension check were not run (missing Firefox, qpdf, Garage, VSIX packaging and an isolated Linux X11 session).
- No timings from Lenovo. Daniel's tier rule names Lenovo; all numbers here are from Claw Mini under concurrent agent load (load average 2.6–7.3).
- Every performance number comes from a non-quiet machine. The Linux `VmHWM` gate was not measured.
- Whether hosted Actions runs succeed under the new owner is unknown, because nothing has run since the transfer. I could not read the account's spending limit or payment status through the API.
- The cause of the 4.3 s upload test was not isolated. Attributing the first-launch overhead to macOS executable assessment is inferred, not traced.
- The hosted duration of the current `ci.yml` is an estimate; it has never run.
