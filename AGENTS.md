# Agent instructions

The one instruction text for coding agents in this repository; `CLAUDE.md` imports it (DEL-46). RichView is a presentation layer for your agent: a coding agent writes Rich Markdown (`.rmd`), and RichView renders it into a designed, self-contained report (INT-01, INT-04).

Contributing from outside the maintainer's team, or working for someone who is? [CONTRIBUTING.md](CONTRIBUTING.md) has the setup and checks, and the lockdown, build commands, source map and contracts below apply to you. The sections on branches and authority, work packets and the shared build host describe the maintainer's own agent team; in a fork, open a pull request against `main` instead.

## Source of truth

- The lockdown in [`docs/lockdown/`](docs/lockdown/README.md) is the only set of rules. Daniel's answers in `docs/lockdown/answers/` bind; read any card with `node tools/lockdown-app/cli.mjs show <ID>`. [GOALS](docs/lockdown/GOALS.md), [RULES](docs/lockdown/RULES.md), [BOUNDARIES](docs/lockdown/BOUNDARIES.md), [DELIVERY](docs/lockdown/DELIVERY.md) and [EVIDENCE](docs/lockdown/EVIDENCE.md) summarize it, and [`docs/guide/`](docs/guide/README.md) is the design documentation.
- During the rewrite, an unanswered card is built as its current proposal and recommended option. A PR that relies on one names it under `assumed:`, so Daniel can change it later.
- Any feature the lockdown does not decide and track is dropped, even when today's code has it (INT-49). When Daniel asks for something new, propose the lockdown change too (DEL-06).
- Only the lockdown app writes `docs/lockdown/answers/` (DEL-45, DEL-66).

## Branches and authority

Daniel's delivery model of 2026-10-27 (DEL-81) supersedes the per-change gate.

- **Branches.**
  - `dev` is the default branch. Development agents merge into it freely.
  - `pre-prod` follows `dev`. The release agent fast-forwards it independently of check or review verdicts. Checks diagnose the combined state and drive fixes.
  - `prod` moves only by `rv-gate merge-prod candidate/<n> --go "<Daniel's words, date>"`. Since 4 Nov 2026 there is no second test round: a candidate is cut from a `pre-prod` that a full passed, Daniel checks the visuals in the release packet (`rv-gate report`) and gives his go for that candidate, and `merge-prod` accepts that full's receipt (DEL-04, DEL-71 as amended, S27-883).
  - The pre-push hook refuses `prod`, `pre-prod`, `main` and `candidate/*` from agent checkouts (DEL-37 as amended by DEL-81).
- **Development agents.** Several run in parallel, one per independent stream, each in its own worktree under `.claude/worktrees/<slug>` from fresh `origin/dev`, on a branch named `<kind>/s27-<n>-<slug>` for one Linear issue. To land work, run the fast gate, rebase onto `origin/dev` and push to `dev` (or merge your PR into `dev`). One pass at a stack's tip records the whole stack, and the pre-push hook allows a push when its tip is a tested tip, so run fast at the commit you push; a commit amended after the run, or a prefix of the stack, needs a new run. Browser, visual and end-to-end runs are at the agent's discretion.
- **Models** (DEL-81, Daniel 2026-10-29). Non-design development and its reviewer run on Codex (`gpt-6.1-sol`, reasoning xhigh). Claude design work is paused. Codex may make the smallest fix for an explicit mechanical design rule, with no taste decisions; mark it `design review pending` and leave it in the design owner's mailbox. Taste-only work waits for the design agents to resume.
- **The release agent.**
  - Daniel's live ruling of 2026-10-29 removes release batches and batch limits: keep `pre-prod` following `dev`, ready or not, and check the combined state.
  - It fixes forward until RichView runs correctly. Checks and reviews report defects; they do not gate the move to `pre-prod`. Safety, data truth and the `prod` gate remain binding.
  - The release receipt is the evidence (DEL-21).
- Agents may, without asking: create worktrees and branches; push them and merge into `dev`; revert `dev` changes; run local builds, checks, benchmarks and loopback previews; create and update RichView Linear issues; delete task branches; set up automations within these limits; and coordinate with other agents through Linear and local helpers (DEL-05).
- Everything else waits for Daniel's explicit go:
  - any change to `prod`;
  - deploys and rollbacks, except DEL-48's rollback and `dev`'s deploy to the hidden preview of richview.ai behind Cloudflare Access (DEL-06, ARC-22);
  - Cloudflare, D1, R2, Access, DNS, Fly and secrets;
  - origin settings;
  - users and tokens on the gate host;
  - spending;
  - publication (tags, releases, package uploads, posts);
  - weakening a release check;
  - changing an accepted answer.

  [DELIVERY.md](docs/lockdown/DELIVERY.md#outward-actions-waiting-for-daniels-go) lists the actions waiting for him. Hosted GitHub CI is billing-blocked: never spend to unblock it (DEL-39).
- In Claude Code, never pass `isolation: "worktree"` to the Agent tool; it branches from the session's start, not from `origin/dev`.

## Work packets and interfaces

The release organizer assigns one owner per path in `OWNERS`. A packet names its owned paths, consumed interfaces and verification, and normally takes three to six hours. Keep coupled producer/consumer contract changes with one owner. Ask the organizer for a scoped grant before changing another owner's files; preserve concurrent edits. Lock each new worktree immediately with `git worktree lock --reason 'active stream' <path>` and unlock only your own finished worktree before removal.

Read the shared board, your mailbox and your packet at natural seams: before starting a packet and before pushing. Reconcile landed changes only as far as your touched paths and consumed interfaces require. Write routine results to the mailbox and rewrite the stream handoff at packet completion. Inject a message only when your commit broke `dev`, you are blocking another stream now, or Daniel changed a ruling affecting work in flight. There is no fixed cap on streams; the organizer partitions their writes.

During development, use focused tests of the crate or behavior you change at meaningful milestones. Run fast at the coherent tip before pushing, including available interface contract tests. Public APIs and payload/resource boundaries have owner-maintained snapshots or contracts; interface changes update them explicitly. The existing affected-dependent lint policy remains binding until the snapshot-aware selector is implemented and verified. Release runs the long affected suites at check time; full and candidate checks remain unchanged.

Prioritize actual failures in the combined system. The organizer keeps `pre-prod` following `dev`, without batch or review gates, while the owned interface packets proceed. `prod` still requires Daniel's candidate go.

## Tiers

| Tier | Command | When |
| --- | --- | --- |
| fast | `node tools/rv-gate/rv-gate.mjs fast` | Before every push to `dev`. The unit tests of the changed crates and of the crates that `include_str!` a changed file, and every changed integration test file; `cargo clippy -D warnings` of them and their dependents, tests included, whose dependents' tests run at release (`--dependent-tests` runs them here), and the same lint for wasm32 of those that ship as Wasm; the node tests of the tools whose folder changed; rustfmt on the changed files. `--plan` shows the selection, and `--all` runs every crate |
| release | `node tools/rv-gate/rv-gate.mjs release` | Follow `dev` into `pre-prod`, then check the combined state and record the result; failures drive fixes |
| full | `node tools/rv-gate/rv-gate.mjs full` | On `dev`'s head by the full loop; a pass promotes it to `pre-prod`, and that receipt is the release evidence (no candidate re-run since 4 Nov) |

`node tools/rv-gate/rv-gate.mjs steps` lists every step, its tier and its cards. A step that is not built yet reports `unrun` with its reason. Only release and candidate receipts are evidence (DEL-21).

A commit or PR that lands in `dev` names its issue: `Refs S27-<n>` in the PR body, or in the commit message when pushing directly. For a lockdown change, add the fields of `.github/pull_request_template.md`:
- `decision: none | design | feature`: `design` is a capture, motion, token, typography or copy change; `feature` is scope added, changed or removed (DEL-32).
- `items`.
- `assumed`.

Tests may be deleted when they are obsolete; say which and why in the commit or PR (DEL-36).

## Build and verification

Use the pinned toolchain: Rust 1.99.0 (`rust-toolchain.toml`) with `wasm32-unknown-unknown`, `wasm-bindgen-cli` 0.2.129, Playwright 1.65.0 and the Node version in `.node-version` (DEL-49).

```sh
cargo install wasm-bindgen-cli --version 0.2.129 --locked
npm ci --ignore-scripts
cargo run --release --locked -p richview-build      # stages dist/web, packages and the editor
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo clippy -p richview-site --target wasm32-unknown-unknown --locked -- -D warnings
cargo test --workspace --locked
```

On Claw Mini every development stream and subagent builds in one shared Cargo target, `~/tmp/richview-rewrite/target` (Daniel, 2026-10-27). Start each shell with `source ~/tmp/richview-rewrite/env.sh`. It exports `CARGO_TARGET_DIR`, links the checkout's `target/` to it (the scripts read `target/release/...`) and puts a Cargo shim, `~/tmp/richview-rewrite/bin/cargo`, first on PATH; never call another `cargo`. Release, full and prod runs build there too and take its locks like every other build (Daniel, 2026-11-03). Never create another target folder.

- **One build per profile at a time.** Debug and release builds run in parallel, because Cargo keeps them in separate folders; within a profile, builds take turns in arrival order, so a gate run's release build and a stream's take turns. A build command holds its profile's lock for its whole run and nested Cargo runs inside it. `cargo run` holds it only while building, except `cargo run -p richview-build`, which holds the release lock while it runs, because it starts nested release builds. Stream builds use at most 4 jobs each, which keeps rustc memory, and with it swap and the disk floor, in check.
- **Your sources, not another checkout's.** Every checkout produces the same artifact names and Cargo judges freshness by file times, so a crate another checkout built last can look fresh. A `RUSTC_WRAPPER` records which checkout compiled last in each output folder; under the lock, the shim touches this checkout's files unless it compiled last in every folder of the profile it builds, and Cargo rebuilds from this checkout.
- **Disk floor: 15 GiB free** (Daniel, 2026-11-03: Claw Mini's disk is the lead's to manage, and at least 15 GB stay free for Daniel's other sessions; it was 10). Below it the shim first runs the janitor's urgent cleanup (idle target outputs, all rebuildable) and builds if that restores the floor. Only if it cannot do the shim, `rv-gate fast` and `rv-gate release` refuse to start; then pause and tell the lead rather than fill the disk. Keep your own scratch small, delete it when done, and remove each worktree as soon as its work lands.
- **Small target.** Debug builds keep line tables only, dependencies keep no debug info, and `CARGO_INCREMENTAL=0` keeps no incremental caches.
- **Janitor.** Every 10 minutes it deletes loose split-debuginfo objects older than 30 minutes, and RichView, gate and Playwright temp folders untouched for 2 hours. Below 35 GiB free it also deletes outputs idle for 30 minutes and incremental caches idle for 20, stalest target first, but not from a target a build or gate command is using, since a running build reads older rlibs. From such a busy target it deletes only idle test and bin executables and the workspace's own `librichview*` outputs, never third-party dependencies: below 22 GiB those idle for 2 hours, since a long `cargo test` run starts binaries it built up to about 35 minutes earlier, and below 18 GiB those idle for 30 minutes. While a gate run's own Cargo command runs, it sweeps that target only below 15 GiB. Below 15 GiB it alerts the lead, at most once an hour.
- **Gate hold.** While a gate run's Cargo command runs, `target/.rv-gate-cargo` names the run's process and `target/.rv-gate-cargo.profile` says `all`, so every other build waits (S27-738); a command still queued in a full's budget holds nothing. A fast run holds the same file with `debug` for its Cargo sequence, so only debug builds wait for it. A full or prod run also names itself in `target/.rv-gate-release` for its whole run, because its suites test its own release build: between its Cargo commands, release builds and `cargo clean` wait and debug builds go ahead (lead, 2026-11-03).
- **Perf hold.** Once per release run, the perf step names itself in `target/.rv-perf-quiet`. New builds wait ("waiting for the perf measurement") while it waits up to 2 minutes for in-flight builds and headless browsers to finish, measures for a few minutes, and removes the hold. If the work never drains, it measures anyway and records the load. When a timing budget fails while the 5-minute load is over 8, the hold stays up to 15 more minutes while perf waits for that load to fall under 6, and the capture after the wait decides.
- Gate runs delete their temp folders when they end, and the evidence store keeps run folders only for each open PR's latest run (S27-725).
- **Shared processes.** Stop only processes you started, by the PID you recorded or through your own session's task controls. Never `pkill` or `killall` by name: a broad kill on 2026-10-28 took down a running release's browsers. Subagent briefs repeat this rule.

## Source map

| Responsibility | Location |
| --- | --- |
| Chart engine: contracts, scales, geometry, interaction policy | `richview-chart/` |
| Format, schemas, style cascade, renderer, knowledge | `crates/richview-core/` |
| CLI, resolver, HTTP host, report store, MCP | `crates/richview-app/` |
| Browser runtime: charts, controls, playground, landing, showcase | `crates/richview-site/` |
| Editor service | `crates/richview-editor/` |
| Build orchestrator and benchmarks | `crates/richview-build/`, `crates/richview-bench/` |
| RichView Publish builder and Worker (PR #70, ported to Rust in the rewrite) | `crates/richview-publish/`, `cloudflare/publish/` |
| VS Code adapter | `packages/vscode/` |
| Site pages, styles and fonts | `assets/` |
| Gate, lockdown app | `tools/rv-gate/`, `tools/lockdown-app/` |

Core and engine stay pure: no filesystem, network, process, environment or clock access (ARC-02). Handwritten JavaScript is allowed only where ARC-03 lists it, and every such file names its host reason in its header.

## Contracts

- The schema registry (`crates/richview-core/src/registry.rs`) is the one definition of tags, fields, chart types and diagnostic codes (ARC-09). [RMD-SPEC.md](docs/RMD-SPEC.md)'s reference tables are generated from it: edit the registry, then run `RICHVIEW_BLESS=1 cargo test -p richview-core --test spec_docs`. Prose explains but never lists the vocabulary, and docs never state a count the build does not have; the `docs-match` gate step checks both (DEL-18).
- Diagnostics carry level, code, message, path, line and tag; every surface returns them unchanged (ARC-08, TYP-53).
- Authors customize only through validated vocabulary: no raw HTML, CSS, JavaScript, engine specs or SVG from any source or style file (INT-28, ARC-12, ARC-13). Safety is never traded (INT-79).
- Readers never get more precision than the author chose (FEA-CH-37).
- Design work follows part 06 of the lockdown and [docs/guide/styles.md](docs/guide/styles.md); a review agent judges visual and motion quality, and tests alone never establish it (INT-67, DEL-68).

License: MIT everywhere (INT-37).
