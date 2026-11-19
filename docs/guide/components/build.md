# Build: builder, benchmark, packages, CI and agent docs

This page describes how RichView is built, measured and checked, and the agent-facing files that sit beside the product, as they stood on `main` at `b4db233`: the build orchestrator `richview-build` (C-RICHVIEW-BUILD, card ARC-46) and its Wasm boundary and profile (D-WASM-BOUNDARY, ARC-04), the benchmark crate (C-RICHVIEW-BENCH, ARC-47), the generated npm packages `@richview/core` and `@richview/charts` (C-PKG-CORE and C-PKG-CHARTS, ARC-48 and ARC-49), the CI workflow (C-CI, ARC-50), the verification scripts (C-VERIFY-HARNESS, ARC-63), the comparison labs (C-BENCH-LABS, ARC-64), the font pipeline (C-FONT-PIPELINE, ARC-62), the examples (C-EXAMPLES, ARC-54), the agent skill and `llms.txt` (C-SKILL and C-LLMS-TXT, ARC-52 and ARC-53) and the agent instructions (C-AGENT-HARNESS, ARC-51). Two parts are not on `main`: the lockdown tooling (C-LOCKDOWN-TOOLING, ARC-65) is untracked in the lockdown worktree, and PR #68's development harness (C-DEV-HARNESS, ARC-61) exists only on its branch. The ARC- cards are in [Part 03](http://127.0.0.1:4790/#/part/03-architecture), the feature cards FEA-SF-47 to FEA-SF-49 in [Part 04c](http://127.0.0.1:4790/#/part/04c-features-surfaces), the gates that use the build in [Part 02](http://127.0.0.1:4790/#/part/02-delivery) and the build and benchmark budgets in [Part 07](http://127.0.0.1:4790/#/part/07-performance); none had a recorded answer on 22 October 2026. Daniel has since answered INT-71, DEL-40 and DEL-71 (26 October), DEL-37 and DEL-81 (27 October), among others, and the Proposed columns below follow them; the Today columns stay as of `b4db233` and 22 October.

Related pages: [Delivery](../delivery.md#the-gate-commands) for the gate; [Performance](../performance.md#build-and-verification-time) for the budgets; [Web](web.md) for the site the build stages; [Native app](app.md) and [Editor](editor.md) for the binaries it builds; [Architecture](../architecture.md#build-packages-ci-and-tooling) for the component map; the [guide index](../README.md) for labels, IDs and links.

Statements marked re-run were checked on 22 October 2026: `target/release/richview-bench` with 20 iterations over the existing fixtures, the hashes and sizes of the fixtures and of the build outputs, file and line counts with `git ls-files`, the hosted CI runs, their annotations, the prerelease and the repository's visibility with `gh`, and the npm registry. The full build was not re-run for this page, so that the shared `dist/web` stayed untouched while other pages were captured from it; its timings come from the checks evidence of 2026-10-21 (`checks-performance.md` in docs/lockdown/evidence), which ran it on Claw Mini, as [Method and sources](#method-and-sources) details.

Terms: *golden inputs* are the examples INT-55 (D-EXAMPLES-CARRY) would carry into the rewrite, each of which must render in every accepted style without error diagnostics ([Delivery](../delivery.md#the-gate-commands)). The *fast gate* is `rv-gate fast`, the unit tests a change can affect, which an agent runs before every push to `dev`; the *release run* is `rv-gate release`, which the release agent runs per batch of up to 20 merges, merging `dev` into `pre-prod` and running the long suites for what the batch affects; the *full run* is `rv-gate full` on `pre-prod` before a candidate; and the *prod-candidate gate* is `rv-gate prod candidate/<n>`, which re-runs the full set on each frozen candidate for `prod` (DEL-08, DEL-33, DEL-71, DEL-81; `AGENTS.md` lists the commands).

## At a glance

| Question | Today | Proposed | Cards |
| --- | --- | --- | --- |
| How is RichView built? | One command, `cargo run --release --locked -p richview-build`: native binaries, one Wasm module bound three ways, two npm facades, the VS Code payload and the 81 MB site in one run; 153.9 s cold, 105.9 s after a one-file engine edit, 2.1 s with nothing changed | One entry point with separate `product` and `site` targets and a manifest of inputs, outputs and SHA-256 hashes that gates read; on Claw Mini every stream builds in one shared Cargo target, one build at a time, and release runs build in `target-release` | ARC-46, PRF-50, DEL-13, DEL-22 |
| Which Wasm boundary and profile? | wasm-bindgen 0.2.129 plus raw `rv_*` C exports nothing calls; `opt-level = "s"` forced for Wasm, `3` for native | One pinned wasm-bindgen, one Wasm profile, the raw exports removed | ARC-04 |
| How is speed measured? | `richview-bench`: four fixtures generated into ignored `tmp/`, warm render times and no memory figure on macOS (re-run); it checks no limit, and the documented limits are checked only by hand | Committed, hashed fixtures and a machine-readable receipt that Part 07's budgets read | ARC-47, PRF-07, PRF-10 |
| Are there packages or releases? | `@richview/core` and `@richview/charts` 3.0.0-alpha.0 generated but never published (npm 404, re-run); one GitHub prerelease with six assets, visible to anyone with access to the private repository | No npm or crates.io packages; the prerelease deleted, its tag kept as a history marker; installation from an agent prompt or a script, builds from `prod`, a source build once the repository is public, and the extension on the VS Code Marketplace and Open VSX, each publication on Daniel's go | ARC-48, ARC-49, FEA-SF-49, INT-41, INT-71, DEL-41, OPN-28 |
| What checks a change? | At the 22 October baseline, `ci.yml` had 44 `run` steps on pushes and PRs to `main`; no hosted job had started since 2026-10-13 (re-run); five checks ran in no CI step | `rv-gate fast` before every push to `dev`; `rv-gate release` per batch on `pre-prod`; `rv-gate full` and `rv-gate prod` on each candidate. The 29 October ARC-50 ruling leaves one manual-only `ci.yml` calling `rv-gate pre-pr`; hosted checks are advisory, with no automatic trigger | ARC-50, ARC-63, DEL-08, DEL-22, DEL-39, DEL-81 |
| What do agents read? | A skill, `llms.txt` and a quickstart: all three teach six chart types where nine exist, the skill and `llms.txt` three styles where five ship, and the skill two features that no longer work; none is served (re-run: 404) | Facts generated from core, the prose checked by the fast gate where the check is cheap and by the release run otherwise; the skill and `llms.txt` served at richview.ai; off `dev` until then | ARC-52, ARC-53, FEA-SF-48, OPN-31, INT-66 |

## What the build is for

The build turns one checkout into everything RichView ships and everything its checks test: the CLI with the MCP server, the report server, the editor service, the browser runtime, the VS Code payload and the static site. DEL-13 (G-BUILD), stated, has the release run build once per batch from `pre-prod`'s combined state, in the release build folder `target-release`, and test exactly those artifacts in every later step. INT-41 (D-RELEASE) and Daniel's INT-71 (D-DISTRIBUTION) answer set what leaves the machine: no npm or crates.io packages; people install from an agent prompt or a shell script, take builds from `prod`, or build from source once the repository is public, and the VS Code extension is published on the Marketplace and on Open VSX. The build command is therefore one install path and the producer of the builds from `prod`.

| Component | Path | Lines or size | Proposed |
| --- | --- | --- | --- |
| Build orchestrator | `crates/richview-build/` (`main.rs`, `documentation.rs`) | 1,047 lines of Rust | ARC-46: keep, change |
| Benchmark | `crates/richview-bench/` | 173 lines of Rust | ARC-47: keep, change |
| npm facades | `packages/core/`, `packages/charts/` (manifests tracked, `dist/` generated) | 9,912,179 B and 1,881 B generated | ARC-48, ARC-49: drop |
| CI workflow | `.github/workflows/ci.yml` | 87 lines, 44 `run` steps | ARC-50: keep, change |
| Verification scripts | `scripts/` | 75 files, 13,795 lines, 41 `verify-*` | ARC-63: keep, change |
| Comparison labs | `benchmarks/` (`chart-craft`, `chart-regression`, `highcharts`) | 50 chart-craft reports, 8 Highcharts reports and the chart-review report | ARC-64: keep, change |
| Font pipeline | `scripts/fonts/`, `crates/richview-core/assets/fonts-*.css` and manifest | staged catalog 432,785 B | ARC-62: keep, change |
| Examples | `examples/`, `assets/landing/anchor/` | flagship folder 8.7 MB, 6.6 MB of it research | ARC-54: keep, change |
| Agent skill, `llms.txt`, quickstart | `skill/`, `llms.txt`, `docs/AGENT-QUICKSTART.md` | `SKILL.md` 20 lines plus 14 in `reference/anti-slop.md`; `llms.txt` 31; quickstart 23 | ARC-52, ARC-53: keep, generated; OPN-31: off `dev` until then |
| Agent instructions | `CLAUDE.md`, `.claude/agents/` (nine definitions), `.claude/rules/` | — | ARC-51: one `AGENTS.md` imported by `CLAUDE.md` |
| Lockdown tooling | docs/lockdown, docs/guide and tools/lockdown-app, untracked in the lockdown worktree and not on `main` | the app's `server.mjs` 913 lines and `public/app.js` 2,621 lines | ARC-65: keep, never shipped |

## How it works

### One command

`crates/richview-build/src/main.rs` runs these steps in order:

- `cargo build --release --locked` of `richview-app`, `richview-editor` and `richview-site`, giving `richview`, `richview-server`, `richview-editor` and `richview-reel`, with the release profile from `Cargo.toml` (opt-level 3, LTO, one codegen unit, `panic = "abort"`, stripped).
- It copies `richview-editor` into the extension's `bin/` folder, regenerates the tracked `packages/vscode/snippets/rmd.json` (`main.rs:314-331`), and copies the font licences. With `--editor-only` it stops here.
- `cargo build` of `richview-site` for `wasm32-unknown-unknown`, with `CARGO_PROFILE_RELEASE_OPT_LEVEL=s` forced for that target only (`main.rs:301-305`).
- wasm-bindgen three times over the same module: `no-modules` into `dist/web` (`richview_site.js`, `richview_site_bg.wasm`), `nodejs` into the core package's `dist/` (renamed `rust.cjs`), and `web` into its `dist/browser/` (`runtime.js`).
- It gzips the module at maximum compression to `richview_site_bg.wasm.gz`, which offline exports embed, and writes the font catalog `richview-fonts.css`, which Wasm hosts must fetch and hand to core because the Wasm build compiles no fonts in.
- It writes the npm facades (`index.js`, `index.browser.js`, subpath stubs, the charts package's `dist/index.js`, declarations from `assets/node/`), `bootstrap.js` and `worker.js`, and copies the browser facade to `/bindings/core` and `/bindings/charts`.
- It stages the site (next section), copies the runtime into the extension's `web/` folder, writes a `report-folder.json` beside every staged `.rmd`, and prints two lines: "Rust native binaries: …" and "Browser + offline previews: …".

`--assets-only` skips both Cargo builds and redoes the rest from an existing target. In a target several checkouts share, it refuses when the RichView `RUSTC_WRAPPER`'s record `.rv-fast/last-wasm32-unknown-unknown--wasm-site` or `--wasm-release` names another checkout as the last to compile that Wasm, because it would bind that checkout's runtime under this one's pages; build the Wasm here first, run the full build, or pass `--wasm-from`. A target without the records binds as before. The run never empties `dist/web`: it recreates only the examples and the editor's runtime copy and deletes a few generated files before rewriting them (`main.rs:232`, `:376-377`, `:447`, `:488`, `:560-565`), so a file removed from `assets/` stays in a reused tree, and a deploy uploads whatever the tree holds (code reading; [Cloud and hosts](cloud.md#deploys)). Re-run sizes of the outputs of the local `b4db233` build, none of which Git tracks except the snippets file:

| Output | Size | Tracked |
| --- | --- | --- |
| `dist/web` | 81,073,727 B (81 MB) in 354 files | no |
| `richview_site_bg.wasm`, `.wasm.gz`, `richview-fonts.css` | 4,366,342 B, 1,512,537 B, 432,785 B | no |
| The core and charts packages' `dist/` | 9,912,179 B in 36 files and 1,881 B in 2 | no |
| The extension's `bin/richview-editor` and `web/` runtime | 6,443,200 B and 5,998,909 B | no |
| `packages/vscode/snippets/rmd.json` | rewritten on every build; byte-identical on 2026-10-21 | yes |
| `target/release`: `richview`, `richview-server`, `richview-editor`, `richview-reel`, `richview-build`, `richview-bench` | 8,976,352, 6,638,992, 6,443,200, 402,256, 5,580,384 and 4,669,264 B | no |

The checks evidence timed it on Claw Mini with 4 Cargo jobs at load 2.6 to 7.3: 153.9 s cold (builder 69 s, native 57 s, Wasm 25 s, staging about 3 s) with a 1.6 GB memory peak, 105.9 s after touching one file in `richview-chart`, 2.1 s with nothing changed (PRF-50); the last green hosted run, on 2026-10-12, needed 283 s for the same step.

### Site staging

`stage_assets` (`main.rs:368-436`) copies all of `assets/` to `/assets/`, which is why `/assets/_headers` is public, then the reel to `/reel/`, the legal pages and `_headers` to the root, `assets/styles` and `assets/fonts` to `/styles/` and `/fonts/`, and `assets/landing` to `/landing/`. It injects the demo folder's `report.rmd` and `report.rstyle.yaml` into the home page, highlights its code samples, and writes it as `/index.html` and `/landing/index.html`; it writes the playground four times (`playground.html`, `app.html`, `playground/index.html`, `app/index.html`); it copies the examples without `research/`, `tmp/` and caches, refusing symlinks; and it copies `_redirects`, which sends the retired `/charts/` to `/docs/charts`. `stage_reports` (`main.rs:438-555`) renders the flagship once and exports it with its sources into `/showcase/openai-vs-anthropic/`, copies the showcase shell, `themes.json`, `themes.css`, `fonts.css` and `og.png`, renders the home demo in every builtin style into `/landing/generated/` with `check.json`, and renders the chart-review reports into `/showcase/chart-review/`. `documentation::stage` renders the six public guides into `/docs/`, copies the first-report tutorial there and exports it five times, as written and in four builtin styles (`documentation.rs:128-157`), and copies `docs/RMD-SPEC.md` to `/docs/reference/`. What each page then does is on [Web](web.md#how-it-works); which routes should exist is FEA-SF-46 there.

### Benchmark

`richview-bench` renders four fixtures from `tmp/performance/fixtures` (ignored by Git) in the scientific style: `small` (`examples/sales-report`), `six-types` (one chart of six of the nine types), `dense` (2,033,557 B of inline YAML, 35,040 points in two groups) and `flagship`. `--write-fixtures` regenerates them, `--fixtures-only` stops after that, and `RICHVIEW_BENCH_ITERATIONS` (default 100) sets the timed renders after five warm-ups. It resolves styles and media first, then times `richview_core::render` alone and fails on any error diagnostic, but on no time: it has no limits of its own. It prints one JSON document with median, p95 and minimum per fixture; the memory peak is read from Linux's `/proc/self/status` and is `null` on macOS, and the `profile` field is a fixed string in the code, not read from the build. The documented limits (`docs/PERFORMANCE.md:7`) are checked only when someone runs `scripts/check-performance.py` by hand over saved evidence; no CI step runs it. The dense fixture's hash is now `d4a657d6…` where the 2026-10-12 capture recorded `8089599d…`, so the two captures did not measure the same bytes (ARC-47, re-run). Budgets and every other measurement are on [Performance](../performance.md).

### Packages and releases

The two npm facades are generated as `3.0.0-alpha.0` from the Wasm build: `@richview/core` exports `.`, `./parser`, `./renderer`, `./schemas` and `./runtime`, and its typed API still names only the sleek, scientific and brutalist style sources; `@richview/charts` re-exports the chart functions from `@richview/core/runtime`. Neither was ever published (re-run: the npm registry answers 404 for both). They are not unused: `verify-compat.mjs` and the CI type check of `verify-types.ts` import them, 22 scripts load the browser copy from `/bindings/`, which richview.ai serves, and two engine diagnostic hints tell agents to use `@richview/charts` (ARC-48, ARC-49). The root `package.json` is private and lists `packages/core`, `packages/charts` and `packages/vscode` as workspaces with Playwright 1.65.0, TypeScript 5.6.3 and Wrangler 4.151.0. `packages/cli/` holds only a licence and `packages/server/` a licence and the Fly configuration, leftovers of the TypeScript layout. One GitHub prerelease exists (re-run), visible only to people with access to the private repository: "Chart refinements — 12 October 2026", tag `chart-refinements-2026-10-12` on `5d47fbd`, with a chart-story zip (13.6 MB), the two package tarballs, a Linux x64 CLI archive (57.4 MB), a Linux x64 VSIX `3.0.0-alpha.1` (6.5 MB) and `SHA256SUMS.txt`; what produced it is not recorded.

### CI and verification scripts

`.github/workflows/ci.yml` runs on pushes and pull requests to `main`: one Ubuntu job with a 60-minute timeout installs Rust 1.99.0, runs `cargo fmt --check`, clippy for native and Wasm with warnings as errors and `cargo test --workspace`, installs wasm-bindgen 0.2.129 and runs the build; it then sets up Node 22, installs the npm workspace with `npm ci --ignore-scripts`, writes the benchmark fixtures, runs the editor, native, compatibility and type checks, installs Playwright's browsers, and runs 27 Chromium suites, a loop of 23 suites in Firefox and WebKit, the docs suite and the chart-review matrix, and uploads two artifacts. It has no deploy or release job. It last changed on 2026-10-15 (`407e62c`) and has never run in that form: the last runs that executed steps, on 2026-10-13, failed in `verify-browser.mjs` waiting for `#chart-style option`, and no run since has executed a step (re-run: the run for `7b23a22`, the commit richview.ai serves, ended after three seconds with no steps and GitHub's annotation "The job was not started because recent account payments have failed or your spending limit needs to be increased"). The last green run was on 2026-10-12 at `5d47fbd`. The repository moved to a personal account on GitHub Free on 2026-10-21 and no run has happened since, so whether runs would start now is unknown (checks evidence). Since then `ci.yml` was reduced to `workflow_dispatch` only (checked on `dev` on 2026-10-27; DEL-39, ARC-50).

`scripts/` holds 75 tracked files and 13,795 lines (re-run): 41 `verify-*` checks, the shared `browser-harness.mjs`, `design-review/`, benchmark drivers and the font pipeline. Five checks run in no CI step and are called by nothing else: `verify-assets.mjs` (needs Garage), `verify-authoring.mjs` (failing since `39b301f` on 2026-10-15), `verify-exports.mjs` (needs qpdf and Poppler), `verify-packaging.py` (needs a VSIX the build does not make) and the VS Code check (`verify-vscode.sh` with `.cjs`, needs an isolated Linux X11 session). `verify-homepage.mjs` and `verify-interest.mjs` run only inside other suites. Node is pinned three ways: `.node-version` says 20.11.0, CI uses 22 and the gate host runs 26.5.0 (DEL-49). One full CI-equivalent pass, run in sequence on Claw Mini without Firefox, took about 19.9 min on 2026-10-21.

### Agent knowledge: skill and llms.txt

`richview learn` and the MCP tool `richview_learn` serve five hand-written sections from `crates/richview-core/src/knowledge.rs` (FEA-SF-47, on [Native app](app.md#commands-and-terminal-output)). Beside them sit three files that agents read first. `skill/SKILL.md` (20 lines, plus 14 in `skill/reference/anti-slop.md`) tells agents to bind charts to "named frontmatter data", which the parser rejects, mentions `--allow-network`, which does nothing, lists six chart types where nine exist, three styles where five ship, and asks for a description that publishing only warns about. `llms.txt` (31 lines) repeats the six types and three styles and says "Publication validates title, description and version". `docs/AGENT-QUICKSTART.md` (23 lines) says "All six chart types" and names no styles. All three list seven MCP tools, which is right today and would become five under FEA-SF-09. None is staged by the build: richview.ai answers 404 for `/llms.txt` and `/skill/SKILL.md` (re-run), and neither the home page nor the docs index links the skill or the quickstart (OPN-31).

### Examples, fonts and labs

`examples/` holds the flagship (`openai-vs-anthropic`, 18 charts, 643 dated sources, a snapshot of 2 October 2026) and eight teaching examples, staged under `/examples/`; the home page's live example edits the hero's report in `assets/landing/anchor/`. The flagship's research folder holds 59 tracked files (re-run): 18 Python scripts run with uv, 32 CSV files, five Markdown notes, one Node script, `pyproject.toml`, `uv.lock` and a `.gitignore`. The font pipeline (`scripts/fonts/build.py`, Python with a pinned fontTools) downloads pinned upstream faces, checks their SHA-256 and writes the committed catalogs, manifest and licence texts; native builds compile the catalogs into core, Wasm hosts fetch the staged copy (ARC-62). `benchmarks/` holds the chart-craft lab (50 synthetic reports with Node, Python, R and Figma harnesses), a local Highcharts 13.1.1 comparison (8 reports) and the standard chart-review report; the comparison libraries are installed only into local evidence directories (ARC-64).

## How it looks

The build has no screen of its own; this is what its tools print. The benchmark, re-run on 22 October 2026 on Claw Mini (M4, load average 2.6) with 20 iterations; the four `results` entries are summarised in the table:

```text
$ RICHVIEW_BENCH_ITERATIONS=20 target/release/richview-bench
{
  "architecture": "aarch64",
  "engine": "Rust",
  "os": "macos",
  "peak_rss": null,
  "profile": "release opt-level=3 lto codegen-units=1",
  "results": [ … ]
}
```

| Workload | Source | HTML | Charts | Median | p95 | Documented limit (`docs/PERFORMANCE.md:7`), kept by PRF-10 |
| --- | --- | --- | --- | --- | --- | --- |
| small | 2,453 B | 233,789 B | 1 | 0.28 ms | 0.29 ms | 5 ms |
| six-types | 1,140 B | 267,430 B | 6 | 0.54 ms | 0.58 ms | 10 ms |
| dense | 2,033,557 B | 2,204,090 B | 1 | 89.6 ms | 97.8 ms | 250 ms |
| flagship | 124,588 B | 2,214,104 B | 18 | 11.5 ms | 11.7 ms | 35 ms |

Twenty iterations make the p95 the second-slowest run; PRF-10's figures use 100 iterations on the reference host, and on 2026-10-21 gave small 0.63, six-types 0.68, dense 113.1 and flagship 12.28 ms. The benchmark compares nothing with the limits; only a manual run of `scripts/check-performance.py` does.

Hosted CI and distribution, re-run the same day:

```text
$ gh run list -R 1vecera/richview --limit 3
completed  failure  S27-654: RichView flagship and chart polish (in progress)  Rust product checks  task/s27-654-chart-polish  pull_request  39182846372  4s  2026-10-19T00:54:09Z
completed  failure  Restore landing motion, simplify nav, add docs chart gallery  Rust product checks  task/s27-653-site  pull_request  39182252700  4s  2026-10-19T00:46:03Z
completed  failure  S27-654: RichView flagship and chart polish (in progress)  Rust product checks  task/s27-654-chart-polish  pull_request  39179751822  4s  2026-10-19T00:12:47Z
$ curl -s -o /dev/null -w "%{http_code}\n" https://registry.npmjs.org/@richview%2fcore
404
$ curl -s -o /dev/null -w "%{http_code}\n" https://richview.ai/llms.txt
404
```

`gh` prints tab-separated columns when piped; the tabs are shown as two spaces. Each failed run lasted four seconds.

## Features

The first column is the card, with the entity it defines or decides and, unless the card is observed, its confidence: *inferred*, the proposer's reading of several sources, or *stated*, written in a ticket, document or note but not verified. "Today" is `main` at `b4db233`; "Proposed" condenses the card. Evidence entities with no card of their own are mapped under [Method and sources](#evidence-entities-without-their-own-card).

### Build orchestrator and Wasm

| Card | Today | Proposed |
| --- | --- | --- |
| ARC-46 (C-RICHVIEW-BUILD) | One run builds and stages everything; provenance is implicit; the tracked snippets file is rewritten; a shared Cargo target once ran a builder from another worktree | Keep, change: separate `product` (binaries with the embedded report runtime, editor, VS Code package) and `site` (site runtime, pages, docs, gallery) targets; one Wasm profile; a manifest per build of inputs (commit, toolchain, wasm-bindgen version), outputs and SHA-256 hashes; tracked generated files checked for drift, not rewritten; a fast development profile for inner loops. On Claw Mini every stream builds in one shared Cargo target, one build at a time; the build stamps the sources it compiles and refuses to stage another checkout's artifacts, so the shared target never builds or stages the wrong checkout (DEL-22, DEL-81) |
| ARC-04 (D-WASM-BOUNDARY) | wasm-bindgen plus `rv_alloc`, `rv_frame` and other raw exports with no caller; the engine built as `rlib` and `cdylib` for them; PR #70 builds the runtime at opt-level 3 | wasm-bindgen only, pinned in `Cargo.lock` and checked against the installed CLI; raw exports and `cdylib` removed; every Wasm build `opt-level = "s"`, LTO, one codegen unit; native stays at 3; a profile change needs before-and-after numbers |
| PRF-50 (P-BUILD-TIME) | 153.9 s cold, 105.9 s after a one-file edit, 2.1 s no-op | ≤ 180 s cold, ≤ 60 s after a one-file edit, ≤ 5 s no-op, with 4 Cargo jobs on the reference host, checked in the full run; the release run reports its build time without judging it; gates always measure the production artifacts |
| DEL-13 (G-BUILD), stated | Builds run in any checkout and target | The release run builds once per batch from `pre-prod`'s combined state in `target-release`, so a long release build never holds the lock of the shared development target; later steps test those artifacts by manifest hash; fixtures verified, not regenerated |
| DEL-42 (G-WORKTREE-ISOLATION) and DEL-49 (G-TOOLCHAIN-PINS), inferred | A shared Cargo target once ran another worktree's builder; three Node pins disagree | Every stream and subagent builds in one shared Cargo target (`~/tmp/richview-rewrite/target`, set by `env.sh`), one build at a time under its lock, with a stamping wrapper and a 10 GiB disk floor (Daniel, 2026-10-29; it was 40) (DEL-22, `AGENTS.md`); the gate refuses to run unless Rust 1.99.0, wasm-bindgen 0.2.129, Playwright 1.65.0, one Node version and the gate host's macOS build match |

### Benchmark and fixtures

| Card | Today | Proposed |
| --- | --- | --- |
| ARC-47 (C-RICHVIEW-BENCH) | Fixtures generated into ignored `tmp/`; the dense fixture's bytes changed between captures | Keep, change: fixtures committed with recorded hashes; output a machine-readable receipt Part 07 reads; depends on core and host services only |
| PRF-07 (P-FIXTURES), inferred | small, six-types (six of nine types, no tables or blocks), dense, flagship; all in scientific; no phone profile | small, all-types (every chart and block type), flagship, dense, grid (35,040 rows × 4 series from CSV, a docs gallery example) and the docs gallery page |
| PRF-10 (P-NATIVE-RENDER) | Warm p95 on 2026-10-21: small 0.63, six-types 0.68, flagship 12.28 and dense 113.1 ms; the limits checked only by hand | p95 over 100 iterations: small ≤ 5 ms, all-types ≤ 10 ms, flagship ≤ 35 ms, dense ≤ 250 ms, the documented limits unchanged |
| PRF-17 (P-NODE-BOUNDARY) | `scripts/benchmark-boundary.mjs` takes 49 s and gates nothing; dense Node peak 571 MiB | Deleted with the packages; budgets only if a package returns |

### Packages, releases and installation

| Card | Today | Proposed |
| --- | --- | --- |
| ARC-48 (C-PKG-CORE), ARC-49 (C-PKG-CHARTS), FEA-SF-49 (F-PKG-CORE-API) | Generated facades, never published, used by checks and served at `/bindings/` | Dropped with their build steps and `/bindings/`; the checks move to the runtime APIs (ARC-36); the engine hints reworded; a JavaScript package returns only through a new feature card, generated from a runtime API and published only with Daniel's authorization |
| INT-41 (D-RELEASE), stated; DEL-41 (G-NO-RELEASES), inferred | One prerelease; no release process | The pipeline publishes only builds from `prod` and the VS Code extension on the Marketplace and Open VSX (INT-71), each on Daniel's go; no npm or crates.io packages; a deploy is identified by its `prod` commit and receipt; tags are history markers. Open: how builds and listings are versioned, whether builds from `prod` are GitHub releases, whether the `prod` go also publishes, and the publisher identity |
| OPN-28 (D-PRERELEASE) | The prerelease and its six assets are on GitHub, visible to anyone with access to the private repository | The prerelease and its assets deleted; the tag `chart-refinements-2026-10-12` kept as a history marker |
| INT-71 (D-DISTRIBUTION), Daniel's answer of 2026-10-26 | Build from source in a private repository | Three paths: an install page that leads with one prompt for the agent plus a shell script, both installing the CLI with its MCP server and, where wanted, the servers; builds from `prod`; a source build once the source is public. The VS Code extension is published on the Marketplace and on Open VSX; agent plugins may come later |

### CI, gate and scripts

| Card | Today | Proposed |
| --- | --- | --- |
| ARC-50 (C-CI); DEL-39 (G-HOSTED-CI), lead ruling of 29 October | At the 22 October baseline, 44 inline `run` steps duplicated the gate list, hosted CI was billing-blocked and `docs/CHART-REVIEW.md` claimed automatic PR checks | One `ci.yml` on `workflow_dispatch` only, with `contents: read`: prepare the pinned prerequisites, then run `node tools/rv-gate/rv-gate.mjs pre-pr`. `publish.yml` is folded into the gate registry. No automatic push or PR trigger, secrets, deploy or release job; hosted checks remain advisory. This file change runs no workflow and spends nothing |
| DEL-08 (G-DEV-GATE) and DEL-22 (G-GATE-HOST), inferred | Nine checks in `CLAUDE.md`, 27 Chromium suites and more in CI | `rv-gate` with `fast`, `release`, `full`, `prod` and `merge-prod` (`AGENTS.md`); the release and candidate runs happen on Claw Mini under Daniel's normal user, and agents on any host merge into `dev` after the fast gate (DEL-22, DEL-37, DEL-81); its registry is the single list of steps and says where each runs; the VS Code check runs at the candidate tier in Lenovo's offscreen Linux environment |
| ARC-63 (C-VERIFY-HARNESS) | 75 files; checks reach the product through `/bindings/` facades as well as binaries | Development tooling, never shipped; reaches the product only through binaries and the runtime APIs; every script gates or is deleted |
| PRF-51 (P-TEST-TIERS) | Fast tier 36.6–61.3 s after an edit; the steps that exist today, DEL-08's former per-merge gate, take about 12.5–15 min, against 19.9 min for today's full CI-equivalent pass | Fast gate ≤ 5 s target, ≤ 30 s maximum; all Rust tests ≤ 60 s after an edit; release run ≤ 45 min as an upper bound, set by the first release receipt; prod-candidate gate ≤ 45 min |
| ARC-67 (D-PLATFORMS), inferred | The extension ships a native `richview-editor` per platform, PDF export runs only on macOS and Linux, the gate runs only on Claw Mini (macOS arm64), the container is Debian Linux, and the only prerelease shipped a Linux x64 CLI; no card named the supported platforms | macOS on arm64, Linux on x64 and Windows 10 and 11 on x64 with the same features; the gate proves macOS on Claw Mini, Linux x64 is proven by the container build and by hosted CI when it runs, and Windows by the recorded walk on a fresh AWS Windows Server 2025 machine for each candidate (`tools/windows-e2e`), which `merge-prod` requires (Windows 10 and 11 are expected to behave the same, untested); other targets build from source without a promise; a platform is added only with a gate that runs on it |
| ARC-68 (D-DEPENDENCIES), observed | No gate checks dependency licences or advisories; core depends on `ammonia`, `latex2mathml`, `pulldown-cmark` and the deprecated `serde_yaml`; `LICENSING.md` still describes a TypeScript-era split | A dependency enters with its purpose stated in the PR and a licence on an allowlist compatible with MIT (MIT, Apache-2.0, BSD, ISC, Zlib, Unicode-3.0, OFL for fonts); a dependency gate, such as `cargo deny` plus an npm licence check, covers licences, duplicate versions and advisories for every Rust crate and the npm packages of adapters and tooling; core and engine take only pure dependencies (ARC-02) |
| DEL-18, DEL-53, DEL-59 and DEL-60, all inferred | No docs, surface, site or golden-input gate | The gate derives vocabularies from code and fails on drift (DEL-18), snapshots every public surface (DEL-53), checks the site's routes and layout (DEL-59) and renders every golden input in every style (DEL-60) |

### Agent-facing files

| Card | Today | Proposed |
| --- | --- | --- |
| FEA-SF-48 (F-SKILL-AGENT-GUIDE), ARC-52 (C-SKILL), ARC-53 (C-LLMS-TXT) | Stale facts; not served | The skill stays a short guide to judgement and workflow that sends agents to `richview learn` for facts, served at `https://richview.ai/skill/SKILL.md`; `llms.txt` generated at build time from core and the accepted lockdown and served at the site root; the quickstart goes; the docs gate fails a merge on any tag, type, style, flag or tool the code lacks |
| OPN-31 (D-GUIDE-AGENT-DOCS) | On `main` | The first `dev` commit removes `skill/`, `llms.txt` and `docs/AGENT-QUICKSTART.md` until the generated versions exist; `prod`, today's `main` renamed, keeps them until the rewrite merges into it (done on `dev`: none of the three exists there, checked on 2026-10-27) |
| INT-66 (G-AGENT-DOCS-CURRENT) | Hand-written and drifting | `richview learn`, the skill, `llms.txt`, `CLAUDE.md` and MCP descriptions generated from the code or checked against it by the fast gate where the check is cheap and by the release run otherwise (DEL-81) |
| ARC-51 (C-AGENT-HARNESS), OPN-30 (D-GUIDE-CLAUDE-MD) | `CLAUDE.md` names three canonical styles and nine checks; nine agent definitions prescribe task branches from the default branch; no `AGENTS.md` | `AGENTS.md` holds the one instruction text, with the branch model, the gate and tier commands and what agents may do alone, and `CLAUDE.md` only `@AGENTS.md`; the agent definitions leave and do not return (DEL-46, DEL-67). Done on `dev` on 2026-10-27: `AGENTS.md` exists, `CLAUDE.md` imports it, and `.claude/agents/` and the `agent` setting are gone |

### Examples, fonts, labs and tooling

| Card | Today | Proposed |
| --- | --- | --- |
| ARC-54 (C-EXAMPLES), INT-55 (D-EXAMPLES-CARRY) | Examples staged publicly; the flagship's research pipeline in the repository | Every example renders in the gate as a golden input in every accepted style; the research pipeline frozen content that product builds never re-run; the chart-refinement cases, chart-craft and Highcharts reports kept as fixtures and labs |
| ARC-62 (C-FONT-PIPELINE) | Pinned, hash-checked faces; catalogs reach core two ways | Development tooling with committed outputs; `build.py --check` and `optical.py --check` in the gate; catalogs compiled in natively and provided once by the site runtime; the report runtime carries none |
| ARC-64 (C-BENCH-LABS) | The build publishes the chart-review reports under `/showcase/chart-review/` | Fixtures of the visual gate, built into review output, no longer published; reference libraries never enter product builds |
| ARC-65 (C-LOCKDOWN-TOOLING) | This guide, the lockdown files and the review app, uncommitted in the lockdown worktree | Kept as content and tooling, the first change merged into `dev`, never deployed |

## Checks that cover it

| Check | What it covers | Result on 2026-10-21 | Gap |
| --- | --- | --- | --- |
| `cargo fmt`, native and Wasm clippy | Formatting; lint with warnings as errors | Pass: 0.5 s; 12.4 s and 19.0 s first run, 0.3 s warm | — |
| `cargo test --workspace --locked` | 438 tests in 63 executions | Pass, 42.9–44.4 s warm, 104.8 s cold | No tiers: no ignored tests, features or nextest configuration |
| The build | Everything above | Pass; tracked files unchanged | Nothing records what it built (ARC-46) |
| Benchmark fixtures | `--write-fixtures --fixtures-only` | Pass, 27.3 s, because selecting `-p richview-bench` recompiled 8 crates | Fixtures regenerated, not verified (DEL-13) |
| `verify-editor.cjs`, `verify-native.mjs`, `verify-compat.mjs`, `verify-types.ts` | Editor process, native CLI and MCP, the Node facades and their declarations | Pass: 2.1, 4.0, 0.1 and 1.2 s | The last two exist for packages ARC-48 and ARC-49 drop |
| Chromium, WebKit, docs and chart-review suites | Browser behaviour and the 60-page chart review | Pass: Chromium 374.8 s, WebKit 356.5 s; Firefox not run (not installed) | — |
| `verify-authoring.mjs`, `verify-exports.mjs`, `verify-assets.mjs`, `verify-packaging.py`, VS Code check | Theme switching, PDF exports, asset uploads, VSIX packaging, the extension in VS Code | Fails (`'auto' !== '8px'`); the others not run for missing qpdf, Garage, a VSIX and an X11 session | In no CI step; DEL-17 and DEL-22 decide their place |
| `scripts/benchmark-boundary.mjs` | Node package boundary | 49 s, disclosure only | PRF-17 deletes it |
| `scripts/check-performance.py` | The 25 documented budgets and 7 integrity checks over saved benchmark evidence; missing evidence fails | Run by hand in the capture, 0.05 s; it cannot pass on macOS, where the Linux memory budget fails closed | In no CI step; DEL-51, inferred, would run budgets in the gate's performance step, with a checker changed for macOS |

Sources: `checks-performance.md` in docs/lockdown/evidence; one full CI-equivalent pass took about 19.9 min on Claw Mini. The rules for which checks gate a merge are on [Delivery](../delivery.md#the-gate-commands).

## Known defects

Each defect is decided in the card named. In the Seen column, *re-run* means checked for this page; *code reading* means read in the code at `b4db233` but not run; *card* means reported by the card named and not repeated here.

| Defect | Seen | Decided in |
| --- | --- | --- |
| At the 22 October baseline, hosted CI had started no job since 2026-10-13 and the then-current workflow had never run | Re-run, 22 October | DEL-39, ARC-50 |
| At the 22 October baseline, `docs/CHART-REVIEW.md` claimed automatic PR checks; the 29 October ARC-50 change removes that claim | Card, historical baseline | ARC-50 |
| Five checks run in no CI step; `verify-authoring.mjs` has failed unnoticed since 2026-10-15 | Re-run (references), card | DEL-17, ARC-63 |
| The build never empties `dist/web`, so files removed from `assets/` survive in a reused tree and a plain deploy uploads them | Code reading (inferred effect) | ARC-46, DEL-13 |
| The build rewrites the tracked `packages/vscode/snippets/rmd.json` | Code reading | ARC-46 |
| The Wasm module exports raw `rv_*` functions nothing calls | Card | ARC-04 |
| Benchmark fixtures are not committed; the dense fixture's bytes changed between captures | Re-run (hash) | ARC-47, PRF-07 |
| The benchmark reports no memory on macOS, checks no limit, and its `profile` label is a fixed string | Re-run, code reading | ARC-47, PRF-10 |
| `six-types` covers six of nine chart types and no blocks | Card | PRF-07 |
| The npm facades' typed API names three style sources where five styles ship | Code reading | ARC-48 |
| Engine hints tell agents to use `@richview/charts`, which is not published | Card | ARC-49 |
| A GitHub prerelease exists; OPN-28 proposes deleting it and keeping its tag | Re-run | OPN-28 |
| Node is pinned three ways (20.11.0, 22 and 26.5.0) | Code reading, card | DEL-49 |
| The skill, `llms.txt` and the quickstart teach wrong facts and are not served | Re-run (404), card | OPN-31, FEA-SF-48 |
| At `b4db233` `CLAUDE.md` names three canonical styles and nine checks, the agent definitions predate the dev/prod model and no `AGENTS.md` exists; `dev` now has `AGENTS.md` and a `CLAUDE.md` that imports it | Card | ARC-51, OPN-30 |

## Branch-only additions

| PR | Build or tooling work on the branch | State observed | Decided in |
| --- | --- | --- | --- |
| #68 development harness (`feat/s27-652-development-harness` at `bba45ab`) | A Python runner and `journey.mjs`; a no-JavaScript drill that must fail with `HARNESS_RUNTIME_MISSING` before the normal journey passes; build-ownership checks for a shared Cargo target; evidence outside worktrees; a worker-reply hold replacing a 1,200 ms timer (`41946e3`); [docs/architecture-current.md](https://github.com/1vecera/richview/blob/bba45ab/docs/architecture-current.md) | 4 commits, 7 files, +946/−8; its own integration receipt stayed FAILED on a Cloudflare clean-up `PermissionError`; the worker-reply patch is duplicated in #70 (`3e10460`) | OPN-12 (close unmerged, fold into Part 02), ARC-61, FEA-BR-20, FEA-BR-47 |
| #70 Publish (`3f5c35f`) | A `publish.yml` workflow for publication pull requests; runtime builds at opt-level 3, recorded as producing different bytes | Owned by the RichView News session on PR #70 | ARC-04, DEL-39, [Cloud and hosts](cloud.md#branch-only-additions) |

> **FEA-BR-20 (F-DEVHARNESS-RUNTIME-DRILL), proposed.** Of #68, only these carry: the no-JavaScript completeness check grows into DEL-57's check of every golden input; the runtime-missing self-test becomes DEL-57's drill; the first-report journey stays `verify-docs.mjs` (DEL-15); build provenance becomes DEL-13, DEL-22 (the one shared target stamps sources before building, so it never builds the wrong checkout) and DEL-42; evidence outside worktrees becomes DEL-20; the worker-reply hold becomes FEA-BR-47. Nothing else from #68 is carried.

> **FEA-BR-47 (G-WORKER-REPLY-HOLD), proposed, stated.** Browser checks that test ordering between the page and the Wasm worker hold a real worker reply and release it once the state under test is reached; they never wait on a fixed timer, replace worker data or weaken the product assertion. Not re-run for the lockdown: whether it removes the load failures DEL-15 records is untested.

## Method and sources

Re-run on 22 October 2026: `RICHVIEW_BENCH_ITERATIONS=20 target/release/richview-bench` from the local release build of `b4db233` (2.7 s wall time); `shasum -a 256` and sizes of `tmp/performance/fixtures/*/report.rmd`; byte and file counts of `dist/web`, the packages' generated `dist/` folders, the extension's `bin/` and `web/` folders and `target/release`; `git ls-files` counts of `scripts/`, the build crates and the flagship's research folder; `git check-ignore` of the outputs; `wc -l` of the lockdown app; `git log` of `ci.yml`; the CI references of every `verify-*` script and of `check-performance.py`; `gh run list`, `gh run view` and the check-run annotations for the run of `7b23a22`; `gh release view chart-refinements-2026-10-12` and `gh repo view` for the repository's visibility; the npm registry for both packages; and richview.ai for `/llms.txt` and `/skill/SKILL.md`. Code read at `crates/richview-build/src/main.rs`, `documentation.rs`, `crates/richview-bench/src/main.rs`, `Cargo.toml`, `rust-toolchain.toml`, `.node-version`, `package.json`, `packages/*/package.json`, `.github/workflows/ci.yml`, `docs/PERFORMANCE.md`, `scripts/check-performance.py`, `skill/SKILL.md`, `llms.txt` and `docs/AGENT-QUICKSTART.md`.

Not re-run here: the full build and its timings, the test, lint and suite results, the hosted-versus-local comparison and the billing state after the transfer, all from the checks evidence of 2026-10-21; the PR #68 and #70 branch details from Part 04d and Part 08. The product evidence's `F-BENCH-WORKLOADS` row says the benchmark reports VmHWM peak memory; on macOS, the reference host, it reports `null`.

Repository files at `b4db233` (private repository; the links need access): [build orchestrator](https://github.com/1vecera/richview/blob/b4db233/crates/richview-build/src/main.rs), [docs build](https://github.com/1vecera/richview/blob/b4db233/crates/richview-build/src/documentation.rs), [benchmark](https://github.com/1vecera/richview/blob/b4db233/crates/richview-bench/src/main.rs), [CI workflow](https://github.com/1vecera/richview/blob/b4db233/.github/workflows/ci.yml), [core package manifest](https://github.com/1vecera/richview/blob/b4db233/packages/core/package.json), [skill](https://github.com/1vecera/richview/blob/b4db233/skill/SKILL.md), [llms.txt](https://github.com/1vecera/richview/blob/b4db233/llms.txt). Cards: ARC- in Part 03 (03-architecture.json in the worktree's docs/lockdown/parts), DEL- in Part 02, FEA-SF- in Part 04c, FEA-BR- in Part 04d, PRF- in Part 07, INT- in Part 01 and OPN- in Part 08.

### Evidence entities without their own card

| Entity in the product evidence | Where it is decided |
| --- | --- |
| F-BUILD-NATIVE, F-BUILD-WASM | ARC-46 (C-RICHVIEW-BUILD), ARC-04 (D-WASM-BOUNDARY) |
| F-BUILD-NPM, F-PKG-CHARTS-API | ARC-48, ARC-49, FEA-SF-49 |
| F-BUILD-SITE, F-BUILD-REPORTS | ARC-46, FEA-SF-46 and FEA-SF-40 on [Web](web.md), ARC-64 |
| F-BENCH-WORKLOADS | ARC-47 (C-RICHVIEW-BENCH), PRF-07, PRF-10 |
| F-CI-GATES | ARC-50 (C-CI), DEL-39 (G-HOSTED-CI) |
| F-CI-PRERELEASE | OPN-28 (D-PRERELEASE) |
| F-LLMS-TXT-SUMMARY | ARC-53 (C-LLMS-TXT), FEA-SF-48 |
| F-DEVHARNESS-JOURNEY, F-DEVHARNESS-BUILD-OWNERSHIP, F-DEVHARNESS-ARCHITECTURE-MAP | ARC-61 (C-DEV-HARNESS), FEA-BR-20, FEA-BR-47, OPN-12 (branch-only, above) |
