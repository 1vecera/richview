# Agent-use measurement

Development tooling for FEA-SF-65 (measuring how well agents use RichView) and the harness half of DEL-78. Node ESM, no dependencies, not a `richview` command. A fresh agent gets a real analyst brief, an empty sandbox and only the installed `richview` CLI and MCP server; the run is scored and appended to a history, so a change to `richview learn`, an MCP description or a diagnostic shows up as a change in the score.

## Run a task

```sh
node tools/agent-use/run.mjs --product claude --task complaint-themes --richview /path/to/richview [--model sonnet]
```

- `--product claude|codex`, `--task <id>` (a folder in `tasks/`), `--richview <binary>` (never built by this tool), optional `--model`, `--timeout-min`, `--build <sha>`, `--no-history`.
- The sandbox is `$TMPDIR/rv-agent-use/<run-id>/sandbox`, outside every checkout, holding the task's data in `report/data/` and the binary as `bin/richview`. The run folder keeps `prompt.txt`, `mcp.json`, `transcript.jsonl`, `stderr.txt`, `final-check.txt`, `score.json` and the report folder.
- Claude Code starts through `clauth start <profile> --`, under the clauth profile `RV_AGENT_PROFILE` names (`work` when unset), in a runtime of its own; the run record and the history name the profile. Its environment holds `HOME`, `USER` and `LOGNAME` for clauth's Keychain sign-in and no other secret. It runs with `--strict-mcp-config`, tools limited to Read, Write, Edit and `Bash(richview:*)` plus the richview MCP tools, publishing and the server denied, no skills, and the brief on stdin. Codex runs `exec --json` in a workspace-write sandbox. The Codex path is wired from `codex exec --help` and has not been run live.
- Each run uses subscription capacity. Run one at a time; agents are not deterministic, so DEL-78's pass rate needs three runs per product.

Re-score a kept run with `node tools/agent-use/score.mjs <run-folder> [--write]`. Tasks: `tasks/<id>/task.json` (brief, `checks`) plus `data/`. Figures are written as computations over the data files; the scorer evaluates them, so the expected number is never typed by hand.

## Read the history

`~/.local/share/richview-gate/evidence/agent-use/history.jsonl` (override with `RV_AGENT_USE_HISTORY`), one line per run: product, model, build (binary version and commit), task, score, pass, attempts, tool calls, diagnostic codes met, misread and unresolved codes, failed facts, paths.

```sh
node tools/agent-use/report.mjs [--task <id>] [--product claude|codex] [--drop 15]
```

One table per task and product, oldest first. A run is compared with the mean of up to three earlier runs; a fall of `--drop` points or more is flagged `DROP`, and a changed build commit is marked `new build`. A flag asks you to open the transcript, not a verdict.

## The gate step and the weekly run

```sh
node tools/agent-use/gate.mjs --richview <binary> --out <folder> [--products claude,codex] [--runs 3] [--claude-model sonnet] [--codex-model <m>] [--unrun <product>:<reason>]
```

`rv-gate`'s candidate run (`prod`) runs this as `fresh-agent` (DEL-78); a full reports it unrun, "DEL-78: candidate and weekly pre-prod only". Since 1 Nov 2026 (Daniel: Codex is no longer used) the candidate and the weekly run pass `--products claude --unrun codex:<reason>`, and the appendix says Codex did not run. The run takes `--runs` runs per product, one at a time, rotating through `tasks/` from where the history stopped, so consecutive runs and candidates differ in task. It writes `fresh-agent.json` (per product: pass rate, runs, scores, attempts, tool calls, codes met, misread and unresolved codes, issue candidates), `appendix.md` (the packet appendix: pass rates, trend from the history through `report.mjs`, confusing diagnostics with transcript paths, issue candidates) and the kept `transcripts/` and `excerpts/`. The step fails only when a run could not execute (it timed out, the CLI is missing or exited before any tool call, or fewer runs finished than asked). Low pass rates and drops are findings in the appendix, because neither INT-69 nor FEA-SF-65 sets a threshold; tighten `exitDecision` in `lib/gate.mjs` once Daniel names one. The tool never files issues: the appendix lists issue candidates (code, message, excerpt path) and the release agent files them.

`weekly.sh` runs it on the `pre-prod` head in a worktree under `~/.local/share/richview-gate/agent-use-weekly`, with the binary the latest release receipt built, and writes the result to `evidence/agent-use/weekly/<date>-<sha>/`. `launchd/uk.richview.agent-use-weekly.plist` is the template, with `__HOME__` for the host's home; the release agent installs it: copy `weekly.sh` to `~/.local/share/richview-gate/bin/agent-use-weekly.sh`, write the plist with `sed "s#__HOME__#$HOME#g"` into `~/Library/LaunchAgents/`, create `~/.local/share/richview-gate/logs` and `launchctl load` it.

## What the score means

Out of 100: 40 for a final `richview check` (run by the harness on the kept report) with no warnings or errors, 20 or 10 if the agent's own first clean check came on attempt 1 or 2, and 40 times the share of the brief's checkable facts met. Facts are read from the report's chart, table and big-number data: required chart types, tables, a `Source:` note under every chart, and figures that match the dataset. `pass` is INT-69's bar (clean by attempt 2, three chart types, a table, no RichView source read) with every fact met.

Also recorded: attempts and tool calls, each diagnostic code met, codes that came back after an edit (`misread`) or without one (`ignored`), and codes still open at the end (`unresolved`). `invisibleDiagnostics` counts checks where the agent saw only a count, as the MCP tool's one-line summary does, so its codes could not be judged.

## What it does not mean

It judges nothing about the story (INT-77), and length is neither rewarded nor penalized (INT-13). It does not measure RichView's rendering quality. One run is noise; trends over several runs per build are the signal. The sandbox cannot hide the machine's user-level Claude configuration, so a score is comparable only with scores from the same machine and CLI version. The history is internal; a public claim about agent success needs a demonstration (INT-23).
