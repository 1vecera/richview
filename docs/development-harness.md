# Development harness

Use one source snapshot, the shared build target and one retained result to verify a change. This runner composes the repository's existing builder and verifiers through the host's Cargo shim. It does not add a scheduler, release service or second build cache.

## Start and claim ownership

Fetch the remote default branch before creating `.claude/worktrees/<slug>` with a new task branch. Do not use the stale primary checkout as the implementation base. Lock the new worktree with `git worktree lock --reason '<issue>: preserve until evidence retained' <path>`; preserve existing locks. Read current contributor guidance and the task coordination root. Record exact files, branch/base and owners before editing a shared module.

Install the repository's pinned Rust toolchain/target and matching `wasm-bindgen-cli` as described in [CLAUDE.md](../CLAUDE.md). Use `npm ci --ignore-scripts` for the pinned Node development dependencies. Reuse installed Playwright browsers; install a missing browser only when needed. No renderer service, account credential or public deployment is required for the tutorial journey.

## Repeatable commands

On Claw Mini, source `~/tmp/richview-rewrite/env.sh` in the task worktree before starting the runner. It selects the shared target and Cargo shim described in [AGENTS.md](../AGENTS.md), including its profile locks and 15 GiB disk floor. Python 3.13 or newer is required (declared for `uv`). The runner records the Cargo executable it uses and refuses a different target or Cargo executable on this host. The former `run-shared-build.py` remains for a historical recovery caller; it names the old target and 5 GiB floor and must not be used for the rewrite.

Run from the task worktree. Evidence must be a **new absolute directory outside the repository, worktrees and shared target**:

```sh
source ~/tmp/richview-rewrite/env.sh
uv run --no-project scripts/dev-harness/run.py \
  --evidence ~/tmp/richview-rewrite/<issue>/smoke-01 \
  --suite smoke --drill
```

```sh
source ~/tmp/richview-rewrite/env.sh
uv run --no-project scripts/dev-harness/run.py \
  --evidence ~/tmp/richview-rewrite/<issue>/integration-01 \
  --suite integration
```

`--browser chromium|firefox|webkit` selects the headless engine for the journey and verifiers that use the shared browser harness. Some existing verifiers, including the motion runtime script, select Chromium themselves; this flag does not make every suite a complete cross-engine run. `--check verify-example.mjs` adds an existing task-specific verifier; repeat it as needed. Use peers' acceptance scripts on the source they cover instead of copying them into another overlapping suite. `--suite site` runs the browser, site and docs checks. A smoke or site pass is not a full integration pass.

| Suite | Boundary |
| --- | --- |
| `smoke` | Build; actual first-report source/CSV/style → native export → offline interactive browser at 320/390/1440 px under both motion preferences; existing browser API and motion lifecycle verifiers. |
| `site` | Build; browser, site and documentation verifiers, including the homepage and showcase. |
| `integration` | Formatting, native/Wasm clippy and workspace tests; build and benchmark-fixture generation; tutorial journey and the native, editor, browser, report-runtime and site verifiers. The gate registry defines the further chart/browser-engine suites; run relevant ones for changed product behavior and report the distinction. Hosted CI is manual-only and advisory. |

`--drill` disables JavaScript only in an isolated browser context. The authored static report must still exist, but the interactive-initializer assertion must fail with `HARNESS_RUNTIME_MISSING`. The normal journey immediately follows and must pass. No source or built product file is intentionally damaged. A different error, a missing runtime in the recovery run, or an unexpected zero exit fails the drill.

The broad browser verifier holds an actual Rust worker reply until both subsequent edits have entered the pending slot, then releases it explicitly. It deliberately waits longer than the former 1200 ms timer and still requires edit B to be superseded by C. The error-recovery case uses an actual Rust options-decoding error. This controls test ordering without replacing worker data or weakening the product assertions; it does not certify an untested connector branch.

## Evidence and failure handling

The runner uses the Cargo executable selected by the sourced environment. On macOS it scopes the toolchain library fallback to the subprocess tree. It creates a task-local `target` symlink only when absent and rejects a conflicting cache; existing verifiers use that path. Keep that operational symlink out of commits (Git's directory-only ignore rule may show it as untracked). It clears RichView endpoint overrides before starting local verification.

The runner builds bootstrap native binaries and both Wasm runtimes, stages a local preview, builds the native hosts with their embedded runtime, then restages packages with those hosts. The explicit `--assets-only --local-preview` builder mode omits install routes and text regardless of the launch switch. Its manifest is marked `local-preview`; it is not a release build and cannot satisfy full-build provenance. The normal full build and enabled `--assets-only` guard are unchanged. Cargo commands have a one-hour timeout; individual verifiers have fifteen minutes. Timeouts are failure evidence, not permission to declare the task complete.

The shim serializes each Cargo build and keeps its release lock while the builder stages assets. Other streams may build between commands or during verifiers: the runner checks source and artifact identity around each verifier and fails if the files changed. It touches only the current workspace target entrypoints discovered through Cargo metadata, changing mtimes but no source bytes. It checks native and Wasm dep-info for foreign source paths and verifies that the CLI names the staged runtime hash and source commit. Receipts identify the source tree (including uncommitted source), head, tool versions, Cargo executable, manifest mode and hashes of the native/distribution files used. Do not edit or commit in the task worktree while a run is active.

`run.json` contains every command, duration, exit/status and log path. Compact top-level verifier JSON/PNG/SVG captures are retained externally; copied distributions, stores and complete offline bundles stay in ignored task scratch. Screenshots support review but do not certify aesthetic quality. Inspect the actual narrow and wide output and record any remaining defects. Do not copy raw sessions, credentials, customer data or private audit extracts into a commit, PR or deployed directory.

Build failure prevents verification against leftover artifacts. Check failures remain red while independent gates finish and preserve evidence. A command timeout or SIGTERM handled by the runner terminates its process group. An external force-kill of the runner is outside that guarantee; inspect and stop only the task’s remaining child processes before allowing another build. Keep the failed result; diagnose its specific signal and use a new evidence directory after the relevant correction. Do not turn a check green by repeatedly retrying an unexplained failure.

Cleanup distinguishes live processes from exited zombies, which macOS may refuse to signal. It waits for live helpers after TERM and KILL; a surviving process or a genuine signal-permission failure stays red. This distinction has a real-process regression test. The first integration trial completed its product assertions but failed during final cleanup; retain that failure separately from the focused recovery check.

If disk headroom drops below the floor, record the gate and let the resource owner resolve it. No `cargo clean`, cache deletion, worktree removal or package removal is part of this harness. A blocked build slot is not a failed product test. Concurrent unrelated CPU work makes performance comparisons unsuitable; ordinary command durations are not speedup measurements.

## Review, release and recovery

Before handoff, review the complete diff, inspect the captured report/site, verify the exact source/artifact identity, preserve failures/limits and run the required gates. Commit focused changes, push the task branch and open a draft PR with the issue closing reference. Check CI's actual outcome; a billing-blocked runner provides no test evidence and does not authorize spending. Keep partial work in progress.

For the 18 October batch, the deployment owner alone owns the authorized `richview.ai` publication. Harness/core PRs remain draft. A green local receipt does not authorize a merge, release, reviewer request or public upload. The deployment owner records the exact combined commit, target/bindings, prior version, deployment version and post-deploy reader checks. Use the previous deployed version for an authorized rollback; do not reconstruct production from whichever shared binary happens to be present.

Rollback of this tooling change is a focused revert; existing standalone verifiers remain usable. Retain the receipt and decision outside the worktree before unpinning through normal maintenance. Rebuild the intended source if provenance is uncertain; the workspace-only timestamp invalidation is deliberate and costs a local-code rebuild on each invocation. Do not add unique global compiler flags, which would rebuild expensive third-party dependencies. See [Cargo’s dep-info contract](https://doc.rust-lang.org/cargo/reference/build-cache.html#dep-info-files). Do not reuse a stale executable merely because its name or timestamp looks right.

Maintenance is the runner, one tutorial journey and focused harness tests. Add a suite entry only for a distinct contract; prefer adding a relevant `--check` at the call site over growing a universal gate. Update command lists when contributor guidance changes. The first ownership-recovery trial spent 647 seconds compiling the builder on a heavily loaded Claw host; its nested build then reached a misclassified fifteen-minute limit. The final command layout separates compilation from staging. No controlled speed comparison was made, and no recurring job or measured quality/speed gain is claimed.
