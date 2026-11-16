#!/usr/bin/env node
// rv-gate: RichView's local gate (DEL-08, DEL-24, DEL-33, DEL-71).
//
//   rv-gate fast [--plan] [--all] [--dependent-tests] [--base <ref>]   the changed crates' unit
//                                tests and a check of their dependents (before every push to dev)
//   rv-gate pre-pr               the registry's fast tier plus slow tests, whole workspace
//   rv-gate release [--plan] [--no-push] [--no-reuse] [--at <ref>]   follow dev, then check the combined state
//                                pre-prod moves only after a complete passing full;
//                                diagnostics cover the combined state and reuse byte-identical passes
//   rv-gate note <receipt> <kind> <file>   record a review or verdict on a receipt
//   rv-gate dev <pr> [--dry-run] integration tier on the PR rebased onto origin/dev; merges on pass
//   rv-gate full [<ref>] [--parallel n]  full tier on a ref; n independent post-build suites at once
//   rv-gate affected [<ref>] [--parallel n]  prelaunch changes since the last promoted green ancestor
//   rv-gate candidate            cut candidate/<n> from origin/pre-prod (DEL-63, DEL-81)
//   rv-gate hotfix <dev-commit>  check one dev commit on top of prod and cut a hotfix candidate
//   rv-gate prod <candidate>     full tier on a frozen candidate; writes the go card and report
//   rv-gate merge-prod <candidate> --go "<Daniel's words and time>"   merge into prod on his go
//   rv-gate report [--from <ref>] [--to <ref>]   the before-and-after report (DEL-69)
//   rv-gate audit                every dev commit since prod has a receipt (DEL-38)
//   rv-gate value [--since YYYY-MM-DD]  write the advisory execution/value ledger (DEL-82)
//   rv-gate surface --write [--bin <dir>]  regenerate docs/surface from the release build or <dir> (DEL-53)
//   rv-gate install-hooks        install the pre-push hook for every checkout (DEL-37)
//
// Options: --only a,b  run only these steps; --skip a,b  report these as unrun (local tiers only).
// phase.json freezes prelaunch reruns and approved full rotation; launch restores strict checks.
//
// Edit-loop results are not evidence (DEL-42): only `dev`, `full` and `prod` write receipts.

import { spawnSync } from "node:child_process";
import { existsSync, lstatSync, mkdirSync, readFileSync, readdirSync, readlinkSync, renameSync, rmSync, symlinkSync, unlinkSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import * as checks from "./lib/checks.mjs";
import { cleanCheckoutTmp, failedRunsKept, removeTree, runActive, runTemp, runsToPrune, sweepRunTemps, tmpSnapshot, trimFailedRun } from "./lib/cleanup.mjs";
import { parsePrBody, waitForMergeable } from "./lib/pr.mjs";
import { npmCi, npmStale } from "./lib/npm.mjs";
import { gateEnv, ignoredTests, notRunParts, requireDiskFloor, runLogged, sh, unrunLines } from "./lib/proc.mjs";
import { missingEnv } from "./lib/connectors.mjs";
import { STEPS, isRegistry, mergeRegistry, stepsForTier } from "./lib/steps.mjs";
import { claimSources, heldOnTurn, holdCargo, holdRelease, keepReleaseBuild, linkTarget as linkSharedTarget, recordSourceOwner, releaseBinDir, pruneWorkspaceOutputs, runningCargo, waitForCargo, waitUntilIdle, workspaceBuiltAt } from "./lib/target.mjs";
import { captureBuildSnapshot } from "./lib/build-snapshot.mjs";
import { claim, includes } from "./lib/fast.mjs";
import { includeInputs, withDependencies, workspaceGraph } from "./lib/impact.mjs";
import { unrecordedCommits } from "./lib/fastpass.mjs";
import { batchCommits, batchFields, gateRefFor, releasePlan, reusable, reuseEnvironment, reuseKey, runPool } from "./lib/release.mjs";
import { readDev, sourceChanges } from "./lib/pre-prod.mjs";
import { fullStepGroups, runFullSteps } from "./lib/full.mjs";
import {withRunCancellation, alertUnresolvedWorkers} from "./lib/cancellation.mjs";
import { NO_WEIGHTS, commandOrder, readRecentWeights } from "./lib/weights.mjs";
import { specialChecks } from "./lib/step-functions.mjs";
import {readPhase, prelaunchException, browserGap, CHROMIUM_ONLY_REASON} from "./lib/phase.mjs";
import {retryCommands} from "./lib/affected-retry.mjs";
import {executeCaseCommand} from "./lib/case-executor.mjs";
import {forbidInheritedCases} from "./lib/cases.mjs";
import {withGateSourceEnvironment} from "./lib/source-env.mjs";
import {cohortApplicable, runCohortStep} from "./lib/cohort-run.mjs";
import {commandId, failureCounts} from "./lib/selection.mjs";
import {affectedContext} from "./lib/affected-context.mjs";
import {affectedReceipts, fullReceipts} from "./lib/affected-history.mjs";
import {policyFingerprint, lastGreenBase, outcomesForSelection} from "./lib/affected-coverage.mjs";
import {rotationFromHistory, rotationReceipts, writeValue} from "./lib/value.mjs";
import {withRerun, archiveAttempt, archiveBuildSnapshot} from "./lib/rerun.mjs";
import {freshCheck, waitForWorkerGroups} from "./lib/check-process.mjs";
import {executionMeter} from "./lib/step-time.mjs";
import { addLateOutputs, lateOutputs } from "./lib/late-outputs.mjs";
import { runScheduledCommand } from "./lib/command.mjs";
import { withBrowserEvents } from "./lib/browser-receipt.mjs";
import { previewReloadLine } from "./lib/preview-reload.mjs";
import {
	acquireLock,
	appendChange,
	ensureStore,
	evidenceDir,
	gateHome,
	listReceipts,
	newReceiptId,
	readBaseline,
	readReceipt,
	receiptNotes,
	recordReusePass,
	reusePass,
	runDir as makeRunDir,
	writeBaseline,
	writeReceipt,
	writeReceiptNote,
} from "./lib/store.mjs";
import { loadNote, loadRetryLine } from "./lib/perf.mjs";
import { BUILD_MANIFEST, buildManifestProblems } from "./lib/build-manifest.mjs";
import { diffSurface, generateSurface, readCommitted, writeSurface } from "./lib/surface.mjs";
import { gateRepository } from "./lib/repository.mjs";

const here = dirname(fileURLToPath(import.meta.url));
// One Rust target for the gate and every checkout on the host, named by CARGO_TARGET_DIR (S27-725).
const SHARED_TARGET = resolve(process.env.CARGO_TARGET_DIR || join(homedir(), "tmp/richview-rewrite/target"));
/** Where passing release builds are kept for captures, reviews and the weekly fresh-agent run. */
const RELEASE_BIN_DIR = resolve(process.env.RV_RELEASE_BIN_DIR || join(homedir(), "tmp/richview-rewrite/bin-release"));
/** Tiers whose results are receipts: they hold the shared target while a Cargo command runs. */
const GATED_MODES = new Set(["dev", "release", "full", "prod"]);
/** Modes whose Cargo commands wait at the disk floor instead of exiting; fast and dev runs still exit at once. */
const WAIT_FLOOR_MODES = new Set(["release", "full", "prod"]);

function parseArgs(argv) {
	const args = { _: [] };
	for (let i = 0; i < argv.length; i++) {
		const arg = argv[i];
		if (arg.startsWith("--")) {
			const key = arg.slice(2);
			const next = argv[i + 1];
			if (next !== undefined && !next.startsWith("--")) {
				args[key] = next;
				i++;
			} else args[key] = true;
		} else args._.push(arg);
	}
	return args;
}

const list = (value) => (typeof value === "string" ? value.split(",").filter(Boolean) : []);

/** The parts that passing and failing steps did not run, under the receipt line: never silently absent (DEL-21). */
function printNotRun(results) {
	const parts = results.flatMap((result) => notRunParts(result).map((part) => `${result.id}: ${part.what} (${part.reason})`));
	if (parts.length) console.log(`Not run inside steps: ${parts.join("; ")}`);
}

function line(result) {
	const mark = result.flaky && result.status === "pass" ? "FLAKY" : { pass: "PASS ", fail: "FAIL ", unrun: "UNRUN" }[result.status];
	const time = result.durationMs !== undefined ? `${(result.durationMs / 1000).toFixed(1)}s` : "";
	const reason = result.reason ? `  ${result.reason.split("\n")[0].slice(0, 220)}` : "";
	// The files that can loosen a check are in the printed report too, not only in the receipt (DEL-36).
	const loosening = result.details?.thresholdChanges?.length ? `\n      threshold files changed, for the batch review: ${result.details.thresholdChanges.join(", ")}` : "";
	const ignored = result.details?.ignored ? `\n      ignored Rust tests: ${result.details.ignored.length}` : "";
	const loaded = result.details?.measuredUnderLoad ? `\n      perf measured under load: load ${result.details.loadBefore} before, ${result.details.loadAfter} after; ${result.details.stillRunning?.length ? `still running: ${result.details.stillRunning.join("; ")}` : "nothing of ours still running"}` : "";
	const retried = result.details?.loadRetry ? `\n      perf ${loadRetryLine(result.details.loadRetry)}` : "";
	const parts = notRunParts(result);
	const skipped = parts.length ? `\n      not run: ${parts.map((p) => `${p.what} (${p.reason})`).join("; ")}` : "";
	return `${mark} ${result.id.padEnd(15)} ${time.padStart(7)}${reason}${loosening}${ignored}${loaded}${retried}${skipped}`;
}

/**
 * Every checkout shares one Cargo target, and Cargo names a workspace crate's artifacts by its
 * path relative to the workspace root, so two worktrees build to the same files. Cargo then
 * trusts an artifact another worktree built whenever this checkout's sources are older than it.
 * Stamp only when a fingerprint changed outside this checkout's Cargo hold. Commands claim
 * again under the hold, then record their own generation, so unchanged workspace crates stay warm.
 */
function freshenSources(cwd) {
	return claimSources(SHARED_TARGET, cwd);
}

/** Make sure a checkout sees the shared Cargo target and the npm dev dependencies. */
function prepareCheckout(cwd, { npm = true } = {}) {
	linkTarget(cwd);
	freshenSources(cwd);
	if (npm && npmStale(cwd)) {
		npmCi(cwd);
	}
}

/** Link the checkout's `target/` to this run's Cargo target: the scripts read `target/release/...`. */
function linkTarget(cwd) {
	linkSharedTarget(cwd, SHARED_TARGET);
}

/**
 * The checkout's own registry, read in a child process: it is the code under test, so it never
 * runs inside the gate (DEL-36). Undefined when the checkout has none or it does not load.
 */
function checkoutRegistry(cwd) {
	const file = join(cwd, "tools/rv-gate/lib/steps.mjs");
	if (!existsSync(file)) return undefined;
	const read = "const { STEPS } = await import(process.argv[1]); process.stdout.write(JSON.stringify(STEPS));";
	const out = spawnSync(process.execPath, ["--input-type=module", "-e", read, pathToFileURL(file).href], { cwd, encoding: "utf8", timeout: 60_000, maxBuffer: 16 << 20 });
	if (out.status !== 0) return undefined;
	try {
		const registry = JSON.parse(out.stdout);
		return isRegistry(registry) ? registry : undefined;
	} catch {
		return undefined;
	}
}

/**
 * `rv-gate dev` merges a PR's registry additions into the gate's (DEL-36). Full and prod runs
 * test dev itself or a candidate cut from it, so they run the gate's registry unchanged, and a
 * missing script fails there.
 */
function registryFor(ctx) {
	if (ctx.mode !== "dev") return { steps: STEPS, retired: [], added: [] };
	return mergeRegistry(STEPS, checkoutRegistry(ctx.cwd), (path) => existsSync(join(ctx.cwd, path)));
}

async function runSteps(ctx, tier, options = {}) {
	return withGateSourceEnvironment(ctx.mode, () => runStepsWithSourceEnvironment(ctx, tier, options));
}

async function runStepsWithSourceEnvironment(ctx, tier, { only = [], skip = [], parallel = 1 } = {}) {
	const results = [];
	const registry = registryFor(ctx);
	ctx.steps = registry.steps;
	ctx.registry = { retired: registry.retired, added: registry.added };
	ctx.completed = new Map();
	if (registry.retired.length) console.log(`  retired suites, checked by retired-tests: ${registry.retired.join(", ")}`);
	if (registry.added.length) console.log(`  suites added by the checkout: ${registry.added.join(", ")}`);
	const steps = stepsForTier(tier, ctx.mode, registry.steps);
	ctx.phasePolicy ??= readPhase();
	const history = tier === "full" ? rotationReceipts(ctx.phasePolicy, listReceipts) : [];
	ctx.rotation = rotationFromHistory(ctx.phasePolicy, {mode: ctx.mode, steps: registry.steps, receipts: history});
	let buildStatus;
	// A full or prod run's Cargo waits at the disk floor rather than exiting, as a release run's does.
	const env = gateEnv({ CARGO_TARGET_DIR: SHARED_TARGET, ...(ctx.tempDir && { TMPDIR: ctx.tempDir }), ...(WAIT_FLOOR_MODES.has(ctx.mode) && { RV_GATE_WAIT_FLOOR: "1" }) });
	// The checkout's sources were just stamped; a workspace build recorded after this was not ours.
	ctx.cargoMark ??= Date.now();
	const one = async (step, scheduleCommand) => {
		const meter = executionMeter();
		const partial = ctx.mode === "full" && ctx.phasePolicy.phase === "prelaunch" && ctx.phasePolicy.prelaunchFull?.partialRetry;
		const selected = ctx.affectedSelection?.filter(item => item.status === "run" && item.step === step.id);
		const cohort = partial && cohortApplicable(step);
		const commandRetry = partial && (cohort || step.run?.length > 1 || parallel > 1 && step.run?.some(command => command.shard));
		const result = await withRerun(commandRetry ? {...ctx.phasePolicy, phase: "launch"} : ctx.phasePolicy, async attempt => {
		const started = Date.now();
		// The build and its snapshot hold the shared target together, so no other checkout's build
		// lands between them.
		const hold = step.id === "build" && ctx.tempDir && GATED_MODES.has(ctx.mode) ? await holdCargo(SHARED_TARGET, { signal: ctx.signal }) : () => {};
		let result;
		try {
			const exception = prelaunchException(ctx.phasePolicy, {mode: ctx.mode, id: step.id});
			if (selected && !selected.length) result = {status: "unrun", reason: "Not affected or already passed at a covering head."};
			else if (only.length && !only.includes(step.id)) result = { status: "unrun", reason: "Not selected (--only)." };
			else if (skip.includes(step.id)) result = { status: "unrun", reason: "Skipped (--skip); local tiers only." };
			else if (exception) result = {status: "unrun", reason: exception.reason, exception};
			else if (browserGap(ctx.phasePolicy, {mode: ctx.mode, step})) result = {status: "unrun", reason: CHROMIUM_ONLY_REASON};
			else if (ctx.rotation.skipped.includes(step.id)) result = {status: "unrun", reason: "prelaunch rotation", rotation: {ordinal: ctx.rotation.ordinal, cycle: 3}};
			else if (step.unrun) result = { status: "unrun", reason: step.unrun };
			else if (step.unrunIn?.[ctx.mode]) result = { status: "unrun", reason: step.unrunIn[ctx.mode] };
			else if (step.needs === "build" && buildStatus !== "pass")
				result = { status: "unrun", reason: buildStatus ? "The build step failed." : "Needs the build step." };
			else if (buildStatus === "pass" && !ctx.snapshot && workspaceBuiltAt(SHARED_TARGET, { release: true }) > ctx.cargoMark)
				result = {
					status: "fail",
					reason: "Another checkout rebuilt workspace crates in the shared Cargo target after this run's build, so the release binaries may not be this checkout's. Rerun when no other release build is running.",
				};
			else if (step.fn) {
				// Post-build functions await their artifact dependencies before reading the snapshot.
				if (ctx.snapshot) addLateOutputs(ctx, SHARED_TARGET);
				try {
					result = await meter.measure(() => ctx.cancel ? freshCheck(step, ctx, SHARED_TARGET, attempt) : attempt === 2 ? freshCheck(step, ctx, SHARED_TARGET) : step.fn in special ? special[step.fn](ctx) : checks[step.fn](ctx));
				} catch (error) {
					result = { status: "fail", reason: error.message };
				}
			} else if (cohort) {
				const shards = selected?.filter(item => item.caseKind === "rust-cohort");
				result = await runCohortStep(step, ctx, {target: SHARED_TARGET, env, parallel, schedule: meter.schedule(scheduleCommand ?? (run => run())), ...(shards?.length && {selected: shards.map(item => item.caseKey)})});
				if (shards?.length) result.details.units = shards.map(item => ({id:item.id, status:result.details.commands.find(command => command.id === item.caseKey)?.status ?? "unrun"}));
			} else {
				result = await runCommands(step, ctx, env, meter.schedule(scheduleCommand ?? (run => run())), parallel, attempt);
			}
			result = { id: step.id, title: step.title, cards: step.cards, durationMs: Date.now() - started, ...result };
			(ctx.ended ??= new Map()).set(step.id, Date.now());
			// The binaries decide whether later steps can run; a rewritten tracked file fails the build
			// step (and so the run) without stopping them, so one stale generated file still lets every
			// suite report.
			// The later steps test a private copy of this build, so another checkout's release build cannot swap it.
			if (step.id === "build" && result.status === "pass" && ctx.tempDir && !ctx.signal?.aborted) {
				result = await meter.measure(() => captureBuildSnapshot({ ctx, target: SHARED_TARGET, result, notAfter: ctx.ended.get("build") }));
			}
			if (step.id === "build") buildStatus = result.status;
		} finally { hold(); }
		if (step.id === "build" && !ctx.signal?.aborted && (ctx.mode === "full" || ctx.mode === "prod")) result = checks.buildTreeResult(result, ctx.cwd);
		return result;
		}, {signal: ctx.signal, beforeRerun: first => meter.measure(() => {
			const saved = archiveAttempt(step, ctx, first);
			if (step.id === "build") { saved.buildSnapshotArchive = archiveBuildSnapshot(ctx); buildStatus = undefined; linkTarget(ctx.cwd); }
			return saved;
		})});
		if (selected?.length && !selected.some(item => item.id === step.id)) {
			const observed = new Map((result.details?.units ?? []).map(unit => [unit.id, unit.status]));
			result.units = selected.map(item => ({id: item.id, status: observed.get(item.id) ?? (result.status === "fail" ? "fail" : "unrun")}));
		}
		if (commandRetry && result.details?.commands?.some(command => command.flaky || command.shards?.some(shard => shard.flaky))) result.flaky = result.status === "pass";
		Object.assign(result, meter.finish(result.durationMs));
		ctx.completed.set(step.id, result.status);
		console.log(line(result));
		return result;
	};
	if (parallel > 1) ctx.weights = ctx.mode === "full" || ctx.mode === "prod" ? readRecentWeights(join(evidenceDir(), "receipts")) : NO_WEIGHTS;
	results.push(...await runFullSteps(steps, one, parallel, ctx.weights, {signal: ctx.signal}));
	return results;
}

async function runCommands(step, ctx, env, scheduleCommand = run => run(), parallel = 1, attempt = 1) {
	// npm's Publish Wasm build is also a Cargo writer, so every command of a step that runs Cargo
	// holds the shared target while it runs, then remembers its generation before releasing it, so
	// our own work stays warm.
	const ownsSources = ctx.mode !== "release" && GATED_MODES.has(ctx.mode) && step.run.some(command => command.cmd === "cargo");
	return runCommandList(step, ctx, env, scheduleCommand, ownsSources, parallel, attempt);
}

async function runCommandList(step, ctx, env, scheduleCommand, ownsSources, parallel, attempt = 1) {
	const logs = [];
	const failures = [];
	const ignored = [];
	const unrun = [];
	const selected = ctx.affectedSelection?.filter(item => item.status === "run" && item.step === step.id);
	const allCommands = step.run.map((command, index) => ({ command, index }));
	const commands = !selected || selected.some(item => item.id === step.id) ? allCommands : allCommands.filter(item => selected.some(entry => entry.index === item.index));
	// A step's commands start longest first too; outcomes go back to the command order, which the receipt keeps.
	const started = parallel > 1 ? commandOrder(step, commands, ctx.weights) : commands;
	const unrunInputs = [];
	const partial = ctx.mode === "full" && ctx.phasePolicy?.phase === "prelaunch" && ctx.phasePolicy.prelaunchFull?.partialRetry;
	const firstLogs = new Map();
	const one = async ({ command, index }, commandAttempt = 1) => {
		if (ctx.signal?.aborted) return {status: "unrun", reason: "Gate cancelled before command start."};
		const gap = browserGap(ctx.phasePolicy, {mode: ctx.mode, step, command});
		if (gap) {
			unrunInputs.push({what: `${command.cmd} ${command.args.join(" ")} (${command.env?.RICHVIEW_VERIFY_BROWSER})`, reason: gap});
			return {status: "unrun", reason: gap};
		}
		if (partial && commandAttempt === 2) {
			const prior = firstLogs.get(index);
			const archived = archiveAttempt({...step, run: [command]}, ctx, {logs: [prior]});
			const position = logs.indexOf(prior);
			if (position >= 0) logs[position] = archived.logs[0];
		}
		const logPath = join(ctx.runDir, "logs", `${step.id}-${index}${attempt === 2 || commandAttempt === 2 ? "-attempt-2" : ""}.log`);
		if (partial && commandAttempt === 1) firstLogs.set(index, logPath);
		logs.push(logPath);
		// A command may add arguments while some environment variables are missing (rust-slow skips the MetricFlow test).
		const commandEnv = Object.fromEntries(Object.entries(command.env ?? {}).map(([name, value]) => [name, String(value).replaceAll("{run}", ctx.runDir)]));
		const absent = command.whenEnvMissing && missingEnv(command.whenEnvMissing.vars, { ...env, ...commandEnv }).length > 0;
		if (absent) unrun.push(...command.whenEnvMissing.unrun);
		const args = [...command.args, ...(absent ? command.whenEnvMissing.args : [])].map((arg) => arg.replaceAll("{run}", ctx.runDir));
		const cargo = command.cmd === "cargo";
		// A gate tier holds the shared target while each of its Cargo commands runs, and while each
		// command of a step that runs Cargo does; anything else waits for it.
		const held = ownsSources || cargo && GATED_MODES.has(ctx.mode);
		const waiting = (pid) => console.log(`  waiting for gate process ${pid} to finish its Cargo command before ${step.id}`);
		if (cargo && !held) await waitForCargo(SHARED_TARGET, { waiting });
		// The hold starts when the command's turn in the run's budget comes, so a Cargo step queued
		// behind a full's browser suites keeps no other build waiting.
		const turn = held ? heldOnTurn(scheduleCommand, () => holdCargo(SHARED_TARGET, { signal: ctx.signal, waiting })) : scheduleCommand;
		let busy = false;
		const schedule = (run, permits) => turn(async () => {
			// A Cargo started before the hold may still be writing the target; let it finish first.
			if (held && !(await waitUntilIdle(() => runningCargo(SHARED_TARGET), { signal: ctx.signal, waiting: (pids) => console.log(`  waiting for running Cargo processes (${pids.join(", ")}) before ${step.id}`) }))) {
				busy = true;
				return { status: "fail" };
			}
			ctx.signal?.throwIfAborted();
			// A release run claims its crates up front and runs its own Cargo commands side by side,
			// so a newer build there is its own.
			if ((ownsSources || cargo && ctx.mode !== "release" && workspaceBuiltAt(SHARED_TARGET) > ctx.cargoMark) && freshenSources(ctx.cwd)) {
				// Cargo would trust that build over this checkout's older sources.
				console.log(`  another checkout built workspace crates in the shared target; stamping this checkout's sources before ${step.id}`);
			}
			const outcome = await run();
			if (ownsSources) recordSourceOwner(SHARED_TARGET, ctx.cwd);
			return outcome;
		}, permits);
		const executionEnv = { ...env, ...commandEnv };
		// Never let a parent gate's phase leak into nested test harnesses or a launch run.
		delete executionEnv.RV_GATE_PARTIAL_RETRY;
		delete executionEnv.RV_GATE_PRELAUNCH_CHROMIUM_ONLY;
		if (step.id === "tool-tests") delete executionEnv.RICHVIEW_VERIFY_BROWSER;
		else {
			if (partial) Object.assign(executionEnv, {RV_GATE_PARTIAL_RETRY: "1", RICHVIEW_VERIFY_BROWSER: "chromium"});
			executionEnv.RV_GATE_PRELAUNCH_CHROMIUM_ONLY = partial ? "1" : "0";
		}
		const execute = (commandArgs, commandEnvironment) => runScheduledCommand({ ...command, ...(command.shard && { shard: { ...command.shard, out: command.shard.out.replaceAll("{run}", ctx.runDir) } }) }, commandArgs, {
			cwd: ctx.cwd, env: commandEnvironment, logPath, signal: ctx.signal, parallel, scheduleCommand: schedule, sourceHold: ownsSources, mode: ctx.mode,
		});
		const cases = selected?.filter(item => item.index === index && item.caseKind);
		let outcome;
		try { outcome = cases?.length ? await executeCaseCommand({command, args, selection:cases, cwd:ctx.cwd, runDir:ctx.runDir, env:executionEnv, logPath, signal:ctx.signal, parallel, schedule, runCommand:execute}) : await execute(args, executionEnv); }
		catch (error) { if (!ctx.signal?.aborted) throw error; outcome = {status: "unrun", reason: "Gate cancelled during command."}; }
		if (ctx.signal?.aborted) outcome = {...outcome, status: "unrun", reason: "Gate cancelled during command.", ...(outcome.caseUnits && {caseUnits: outcome.caseUnits.map(unit => ({...unit, status: "unrun"}))})};
		if (busy) {
			return { status: "fail", reason: `Other Cargo processes kept running for 30 minutes, so ${step.id} could not have the shared target to itself` };
		}
		if (cargo) ctx.cargoMark = Date.now();
		let log = "";
		try {
			log = readFileSync(logPath, "utf8");
		} catch {
			// No log: nothing ignored or unrun to report; the command's own status already says why.
		}
		if (step.reportsIgnored) ignored.push(...ignoredTests(log));
		unrunInputs.push(...unrunLines(log));
		if (!partial && outcome.status !== "pass") failures.push(`${[command.cmd, ...args].join(" ")} (exit ${outcome.exitCode}, log ${logPath})`);
		return outcome;
	};
	mkdirSync(join(ctx.runDir, "logs"), { recursive: true });
	// A thrown command must drain its peers and leave every remaining selected command run.
	let ran;
	const shouldRetry = (result, item) => !item.command.shard || !result.shards?.length;
	if (partial && step.run.length > 1 && !(step.parallel > 1)) {
		ran = [];
		let blocked = false;
		for (const item of started) {
			if (blocked) {
				const reason = "An earlier command in this step failed.";
				ran.push({status: "unrun", reason});
				unrunInputs.push({what: `${item.command.cmd} ${item.command.args.join(" ")}`, reason});
				continue;
			}
			const [result] = await retryCommands([item], one, {signal: ctx.signal, shouldRetry});
			ran.push(result);
			// Publish's later commands consume the Wasm and parity artifacts from earlier ones.
			// Other serial commands are independent and continue so one run reports every failure.
			if (result.status === "fail" && step.id === "publish-tests") blocked = true;
		}
	} else if (partial && step.run.length > 1) {
		ran = await retryCommands(started, one, {limit: step.parallel, signal: ctx.signal, shouldRetry});
	} else {
		ran = await runPool(started.map(command => () => one(command)), step.parallel || 1, new AbortController(), { keepGoing: true });
	}
	const outcomes = commands.map(command => ran[started.indexOf(command)]);
	for (const outcome of outcomes) if (outcome.status === "fail") failures.push(outcome.reason ?? `Command failed (exit ${outcome.exitCode}).`);
	// Skipped and ignored Rust tests are visible in the receipt, never silently absent (lead, 28 Oct).
	if (step.id === "build" && !failures.length && !ctx.signal?.aborted) {
		failures.push(...buildManifestProblems({ cwd: ctx.cwd, target: SHARED_TARGET, head: ctx.head ?? gitIn(ctx.cwd, "rev-parse", "HEAD"), rust: checks.PINS.rust, wasmBindgen: checks.PINS.wasmBindgen }));
		if (!failures.length) writeFileSync(join(ctx.runDir, "build-manifest.json"), readFileSync(join(SHARED_TARGET, BUILD_MANIFEST)));
	}
	let details = step.reportsIgnored ? { ignored, ...(unrun.length ? { unrunTests: unrun } : {}) } : undefined;
	details = {...details, commands: outcomes, ...(selected && !selected.some(item => item.id === step.id) && {units: commands.flatMap((item, index) => outcomes[index].caseUnits ?? [{id: commandId(step, item.command), status: outcomes[index].status}])})};
	const browserRelaunches = outcomes.flatMap(outcome => outcome.browserRelaunches ?? []);
	if (browserRelaunches.length) details = { ...details, browserRelaunches };
	// Each Firefox preview reload is listed with its case and screenshot (S27-844).
	const previewReloads = outcomes.flatMap(outcome => outcome.previewReloads ?? []);
	if (previewReloads.length) details = { ...details, previewReloads };
	if (ctx.signal?.aborted) return {status: "unrun", reason: "Gate cancelled during step.", logs, details};
	// A perf capture taken while the host was busy says so in the receipt, not only in perf.json.
	let note;
	if (step.id === "perf") {
		try {
			const perf = JSON.parse(readFileSync(join(ctx.runDir, "perf/perf.json"), "utf8"));
			note = loadNote(perf);
			// Charts judged on no baseline are a gap the receipt names, never a silent pass (review 1l should-fix 5).
			if (perf.gaps?.length) details = { ...details, gaps: perf.gaps };
			// The host gzip that measured the sizes, so a change to it shows in the receipt (S27-855).
			if (perf.compressor) details = { ...details, compressor: perf.compressor };
		} catch {
			// No perf.json: the command's own status already says why.
		}
		if (note) details = { ...details, ...note.details };
	}
	// An input a script left unrun (a connector example without its runtime) is listed, not passed silently.
	if (unrunInputs.length) details = { ...details, unrunInputs };
	if (outcomes.length && outcomes.every(outcome => outcome.status === "unrun" && outcome.reason === CHROMIUM_ONLY_REASON)) return {status: "unrun", reason: CHROMIUM_ONLY_REASON, logs, details};
	if (failures.length) return { status: "fail", reason: [...failures, note?.reason].filter(Boolean).join("; "), logs, ...(details && { details }) };
	const passNote = note?.reason ?? "";
	return { status: "pass", ...(passNote && { reason: passNote }), logs, ...(details && { details }) };
}

// ---------------------------------------------------------------------------------------------
// Checks that need the run's artifacts.

const special = specialChecks(SHARED_TARGET);

// ---------------------------------------------------------------------------------------------
// Local tiers: fast and pre-PR, in the current checkout.

async function localTier(tier, args) {
	const cwd = resolve(args.cwd ?? process.cwd());
	prepareCheckout(cwd);
	sh("git", ["fetch", "-q", "origin", "dev"], { cwd });
	const base = sh("git", ["merge-base", "HEAD", "origin/dev"], { cwd });
	// Local runs leave nothing behind (S27-725): their logs and temp files live in one temp
	// folder, a failing step's log is shown before it goes, and only what the run added to the
	// checkout's tmp/ is removed.
	sweepRunTemps();
	const before = tmpSnapshot(cwd);
	const runDir = runTemp();
	try {
		console.log(`rv-gate ${tier}: ${cwd} (base ${base.slice(0, 10)})`);
		const results = await runSteps({ cwd, base, head: undefined, mode: tier, runDir, tempDir: runDir }, tier, { only: list(args.only), skip: list(args.skip) });
		const failed = results.filter((r) => r.status === "fail");
		for (const result of failed) for (const log of result.logs ?? []) showTail(log);
		console.log(failed.length ? `\n${failed.length} step(s) failed.` : "\nAll run steps passed.");
		return failed.length ? 1 : 0;
	} finally {
		removeTree(runDir);
		cleanCheckoutTmp(cwd, before);
	}
}

/** Print the end of a failing command's log, which goes with the run's temp folder. */
function showTail(log, lines = 120) {
	let text;
	try {
		text = readFileSync(log, "utf8");
	} catch {
		return;
	}
	const tail = text.trimEnd().split("\n").slice(-lines).join("\n");
	if (tail) console.log(`\n--- last ${lines} lines of ${log.split("/").slice(-1)[0]}; the log goes with the run's temp folder ---\n${tail}`);
}

/**
 * After a dev, full or prod run, pass or fail: delete its temp folder and what it wrote under the
 * gate checkout's tmp/, then prune the evidence runs to the latest per open PR (S27-725).
 */
function finishRun({ cwd, tempDir, current }) {
	// Housekeeping never changes a run's outcome: a failure here is reported and left.
	try {
		// The checkout's `target` link points into the run's temp folder; point it back first.
		if (cwd) linkTarget(cwd);
		if (tempDir) removeTree(tempDir);
		if (cwd) cleanCheckoutTmp(cwd);
		sweepRunTemps();
		const listed = sh("gh", ["pr", "list", "-R", gateRepository({ cwd: here }), "--state", "open", "--json", "number", "--limit", "1000"], { allowFail: true });
		const dir = join(evidenceDir(), "runs");
		const all = existsSync(dir) ? readdirSync(dir) : [];
		const receipts = new Map(listReceipts().map((receipt) => [receipt.id, receipt]));
		// Without the open PRs, dev runs stay; release, full and candidate runs never depend on them.
		const known = listed.status === 0;
		if (!known) console.log("  evidence: open PRs unknown (gh failed), so dev run folders stay");
		const openPrs = new Set(known ? JSON.parse(listed.stdout).map((pr) => pr.number) : []);
		// A run without a receipt may be in progress: it stays whether or not gh answered.
		const runs = known ? all : all.filter((id) => receipts.get(id)?.kind !== "dev");
		const doomed = runsToPrune({ runs, receipts, openPrs, current, active: (id) => runActive(join(dir, id)) });
		for (const id of doomed) removeTree(join(dir, id));
		// A failed run that a newer one of its kind has replaced keeps its logs and failed steps only.
		const newest = new Map([...all].filter((id) => receipts.has(id)).sort().map((id) => [receipts.get(id).kind, id]));
		const trimmed = [...failedRunsKept({ runs: all.filter((id) => !doomed.includes(id)), receipts })].filter((id) => id !== newest.get(receipts.get(id).kind))
			.reduce((count, id) => count + trimFailedRun(join(dir, id), receipts.get(id)), 0);
		if (trimmed) console.log(`  evidence: trimmed ${trimmed} artifact folder(s) of older failed runs; their logs stay`);
		if (doomed.length) console.log(`  evidence: deleted ${doomed.length} run folder(s) of merged, closed or superseded runs; receipts stay`);
	} catch (error) {
		console.log(`  housekeeping failed, run outcome unchanged: ${error.message}`);
	}
}

// ---------------------------------------------------------------------------------------------
// The dev gate: rebase the PR onto origin/dev, run the integration tier, write the receipt and
// squash-merge only when every step passes and dev has not moved (DEL-03, DEL-08, DEL-43).

function gitIn(cwd, ...args) {
	return sh("git", args, { cwd });
}

function workCheckout(name) {
	const repo = sh("git", ["rev-parse", "--path-format=absolute", "--git-common-dir"], { cwd: here });
	const dir = join(gateHome(), name);
	if (!existsSync(join(dir, ".git"))) {
		mkdirSync(dirname(dir), { recursive: true });
		sh("git", ["--git-dir", repo, "worktree", "add", "--detach", dir, "origin/dev"]);
	}
	return dir;
}

async function devGate(prNumber, args) {
	if (!prNumber) throw new Error("Usage: rv-gate dev <pr-number>");
	const repo = gateRepository({ cwd: here });
	ensureStore();
	const release = acquireLock("merge", { wait: true });
	const run = {};
	try {
		const pr = JSON.parse(sh("gh", ["pr", "view", String(prNumber), "-R", repo, "--json", "number,title,body,headRefName,headRefOid,baseRefName,isDraft,url,state"]));
		if (pr.state !== "OPEN") throw new Error(`PR #${prNumber} is ${pr.state}.`);
		if (pr.baseRefName !== "dev") throw new Error(`PR #${prNumber} targets ${pr.baseRefName}; the gate merges only into dev.`);
		const cwd = workCheckout("work");
		prepareCheckout(cwd, { npm: false });
		gitIn(cwd, "fetch", "-q", "origin", "dev", pr.headRefName);
		const base = gitIn(cwd, "rev-parse", "origin/dev");
		gitIn(cwd, "checkout", "-q", "--detach", pr.headRefOid);
		const rebase = sh("git", ["rebase", "-q", base], { cwd, allowFail: true });
		if (rebase.status !== 0) {
			sh("git", ["rebase", "--abort"], { cwd, allowFail: true });
			throw new Error(`PR #${prNumber} does not rebase cleanly onto origin/dev; resolve and re-queue (DEL-43).`);
		}
		const head = gitIn(cwd, "rev-parse", "HEAD");
		const tree = gitIn(cwd, "rev-parse", "HEAD^{tree}");
		prepareCheckout(cwd);
		const id = newReceiptId("dev", head);
		const runDir = makeRunDir(id);
		// A run that was killed left its output; the gate owns this checkout, so all of it goes.
		cleanCheckoutTmp(cwd);
		Object.assign(run, { cwd, tempDir: runTemp(), current: id });
		const fields = parsePrBody(pr.body);
		const ctx = { cwd, base, head, mode: "dev", pr: fields, runDir, tempDir: run.tempDir };
		console.log(`rv-gate dev: PR #${prNumber} ${pr.title}\n  rebased ${head.slice(0, 10)} onto dev ${base.slice(0, 10)}; receipt ${id}`);
		const gateCode = sh("git", ["rev-parse", "HEAD"], { cwd: here, allowFail: true }).stdout?.trim();
		const results = await runSteps(ctx, "integration");
		const failed = results.filter((r) => r.status === "fail");
		const receipt = {
			id,
			kind: "dev",
			phase: ctx.phasePolicy.phase,
			phasePolicy: ctx.phasePolicy,
			pr: pr.number,
			title: pr.title,
			url: pr.url,
			commit: head,
			tree,
			base,
			fields,
			gateCode: { path: here, commit: gateCode, bootstrap: Boolean(args.bootstrap) },
			toolchain: results.find((r) => r.id === "pins")?.details,
			steps: results,
			registry: ctx.registry,
			captures: ctx.captures,
			snapshot: ctx.snapshotFiles,
			captureDiffs: ctx.captureDiffs?.filter((d) => d.status !== "same"),
			surfaceChanges: ctx.surfaceChanges,
			result: failed.length ? "fail" : "pass",
			at: new Date().toISOString(),
		};
		writeReceipt(receipt);
		console.log(`\nReceipt ${id}: ${receipt.result} (${failed.length} failed, ${results.filter((r) => r.status === "unrun").length} unrun)`);
		printNotRun(results);
		if (failed.length) return 1;
		if (args["dry-run"]) {
			console.log("Dry run: not merging.");
			return 0;
		}
		// Await inside the try: the merge lock must cover the whole merge, including the wait for
		// GitHub's mergeability (DEL-43).
		return await mergeIntoDev({ pr, receipt, cwd, ctx, repo });
	} finally {
		// Housekeeping runs under the lock, so no other run is using the gate checkout; a failure
		// in it never keeps the lock.
		try {
			finishRun(run);
		} finally {
			release();
		}
	}
}

async function mergeIntoDev({ pr, receipt, cwd, ctx, repo }) {
	const remoteDev = sh("git", ["ls-remote", "origin", "refs/heads/dev"], { cwd }).split(/\s/)[0];
	if (remoteDev !== receipt.base) throw new Error(`dev moved from ${receipt.base.slice(0, 10)} to ${remoteDev.slice(0, 10)}; re-queue (DEL-43).`);
	// Put the gated commit on the PR branch so GitHub squashes exactly that tree.
	sh("git", ["push", "-q", `--force-with-lease=refs/heads/${pr.headRefName}:${pr.headRefOid}`, "origin", `${receipt.commit}:refs/heads/${pr.headRefName}`], { cwd });
	if (pr.isDraft) sh("gh", ["pr", "ready", String(pr.number), "-R", repo]);
	await waitForMergeable(() => JSON.parse(sh("gh", ["pr", "view", String(pr.number), "-R", repo, "--json", "headRefOid,mergeable"])), receipt.commit);
	const f = receipt.fields;
	const body = [
		`Refs ${f.issue}`,
		`decision: ${f.decision}`,
		f.cause ? `cause: ${f.cause.kind}${f.cause.detail ? ` ${f.cause.detail}` : ""}` : "",
		f.items.length ? `items: ${f.items.join(", ")}` : "",
		f.assumed.length ? `assumed: ${f.assumed.join(", ")}` : "",
		`receipt: ${receipt.id}`,
	]
		.filter(Boolean)
		.join("\n");
	sh("gh", ["pr", "merge", String(pr.number), "-R", repo, "--squash", "--delete-branch", "--match-head-commit", receipt.commit, "--subject", `${pr.title} (#${pr.number})`, "--body", body]);
	sh("git", ["fetch", "-q", "origin", "dev"], { cwd });
	const merged = sh("git", ["rev-parse", "origin/dev"], { cwd });
	const mergedTree = sh("git", ["rev-parse", "origin/dev^{tree}"], { cwd });
	if (mergedTree !== receipt.tree) throw new Error(`Merged tree ${mergedTree} differs from the gated tree ${receipt.tree}; audit this merge.`);
	writeReceiptNote(receipt.id, "merge", { commit: merged, tree: mergedTree });
	if (ctx.captures) reportBaseline("dev", writeBaseline("dev", ctx.captures));
	const entry = {
		at: new Date().toISOString(),
		commit: merged,
		pr: pr.number,
		title: pr.title,
		receipt: receipt.id,
		class: f.decision,
		cause: f.cause,
		issue: f.issue,
		items: f.items,
		assumed: f.assumed,
		changedCaptures: receipt.captureDiffs?.map((d) => ({ name: d.name, status: d.status, before: d.before, after: d.after, changedPixels: d.changedPixels })),
		changedSurfaces: receipt.surfaceChanges,
		deletedTests: f.retired,
		revert: `git revert ${merged}`,
	};
	appendChange(entry);
	await updateLinear(f.issue, f.decision === "none" ? "Done ✅" : "To Review");
	console.log(`Merged PR #${pr.number} into dev as ${merged.slice(0, 10)}; change log updated.`);
	return 0;
}

async function updateLinear(issue, stateName) {
	const token = process.env.LINEAR_TOKEN || process.env.DAS_ITEM_LINEAR_NEW__API_KEY;
	if (!token || !issue) {
		console.log(`Linear: set ${issue} to "${stateName}" by hand (no API key in this environment).`);
		return;
	}
	const call = async (query, variables) => {
		const response = await fetch("https://api.linear.app/graphql", {
			method: "POST",
			headers: { "content-type": "application/json", authorization: token },
			body: JSON.stringify({ query, variables }),
		});
		return (await response.json()).data;
	};
	const found = await call(`query($id:String!){issue(id:$id){id team{states{nodes{id name}}}}}`, { id: issue });
	const state = found?.issue?.team?.states?.nodes?.find((s) => s.name === stateName);
	if (!state) return console.log(`Linear: state "${stateName}" not found for ${issue}.`);
	await call(`mutation($id:String!,$s:String!){issueUpdate(id:$id,input:{stateId:$s}){success}}`, { id: found.issue.id, s: state.id });
	console.log(`Linear: ${issue} → ${stateName}.`);
}

// ---------------------------------------------------------------------------------------------
// The release run (DEL-81): the long suites for what a batch of dev affects, then pre-prod.

/** Run one registry step: a built-in check or its commands. Never throws. */
async function runStep(step, ctx, env, finalize = result => result) {
	const meter = executionMeter();
	let result = await withRerun(ctx.phasePolicy, async attempt => {
		let result;
		try {
			if (step.fn) result = await meter.measure(() => attempt === 2 ? freshCheck(step, ctx, SHARED_TARGET) : step.fn in special ? special[step.fn](ctx) : checks[step.fn](ctx));
			else result = await runCommands(step, ctx, env, meter.schedule(run => run()), 1, attempt);
		} catch (error) { result = {status: "fail", reason: error.message}; }
		return await meter.measure(() => finalize(result));
	}, {signal: ctx.signal, beforeRerun: first => meter.measure(() => {
		const saved = archiveAttempt(step, ctx, first);
		if (step.id === "build") { saved.buildSnapshotArchive = archiveBuildSnapshot(ctx); linkTarget(ctx.cwd); }
		return saved;
	})});
	if (result.status === "fail") {
		// A command stopped because another step failed first is not a failure of its own.
		if (ctx.firstFailure && ctx.signal?.aborted) result = { status: "unrun", reason: `Stopped: ${ctx.firstFailure} failed first.` };
		else ctx.firstFailure = step.id;
	}
	const saved = {id: step.id, title: step.title, cards: step.cards, ...result, ...meter.finish(result.durationMs)};
	console.log(line(saved));
	return saved;
}

/** A Cargo step limited to the batch's crates instead of the whole workspace. */
function reportBaseline(name, outcome) {
	if (!outcome.written) console.log(`  the ${name} captures baseline was kept: ${outcome.reason}. Check the run's captures.`);
}

function scopedToCrates(step, crates, all) {
	if (!step.run || crates.length >= all) return step;
	return {
		...step,
		run: step.run.map((command) => {
			if (command.cmd !== "cargo" || !command.args.includes("--workspace")) return command;
			const at = command.args.indexOf("--workspace");
			return { ...command, args: [...command.args.slice(0, at), ...crates.flatMap((name) => ["-p", name]), ...command.args.slice(at + 1)] };
		}),
	};
}

async function releaseRun(args) {
	return withGateSourceEnvironment("release", () => releaseRunWithSourceEnvironment(args));
}

async function releaseRunWithSourceEnvironment(args) {
	ensureStore();
	// Release diagnostics do not promote: only a complete passing full receipt can do that.
	const tracking = readDev(workCheckout("gate"), { at: args.at ?? "origin/dev" });
	const { base, head } = tracking;
	const unlock = acquireLock("merge", { wait: true });
	const run = {};
	try {
		const cwd = workCheckout("work");
		gitIn(cwd, "fetch", "-q", "origin", "--prune");
		const commits = batchCommits(sh("git", ["log", "--first-parent", "--format=%H%x1f%B%x1e", `${base}..${head}`], { cwd }));
		// The work checkout is the gate's own: a file an earlier build rewrote is discarded, not merged.
		gitIn(cwd, "checkout", "-q", "-f", "--detach", head);
		linkTarget(cwd);
		const env = gateEnv({ CARGO_TARGET_DIR: SHARED_TARGET });
		const metadata = JSON.parse(sh("cargo", ["metadata", "--format-version", "1", "--no-deps", "--offline"], { cwd, env }));
		const graph = workspaceGraph(metadata, cwd);
		const changed = sh("git", ["diff", "--name-only", "--no-renames", base, head], { cwd }).split("\n").filter(Boolean);
		const plan = releasePlan(STEPS, changed, graph, includeInputs(includes(cwd), graph), { combined: true });
		console.log(`rv-gate release: combined state ${head.slice(0, 10)}, ${commits.length} new commit(s), ${changed.length} changed file(s)`);
		for (const commit of commits) console.log(`  ${commit.commit.slice(0, 10)} ${commit.subject.slice(0, 100)}`);
		// Report only (history is already on dev): a commit with no fast-pass record reached dev without
		// the pre-push hook, for example a PR merged on the forge.
		// A commit recorded only below a run's tip was tested with the commits above it, not alone.
		const { missing: unrecorded, belowTip } = unrecordedCommits(cwd, commits.map((commit) => commit.commit));
		const subjects = new Map(commits.map((commit) => [commit.commit, commit.subject]));
		for (const { commit, reason } of unrecorded) console.log(`  no fast-pass record (${reason}): ${commit.slice(0, 10)} ${(subjects.get(commit) ?? "").slice(0, 100)}`);
		for (const { commit } of belowTip) console.log(`  tested only at a stack's tip, not built alone: ${commit.slice(0, 10)} ${(subjects.get(commit) ?? "").slice(0, 100)}`);
		console.log(`  crates: ${plan.crates.length === graph.length ? "all" : plan.crates.join(", ") || "none"}`);
		console.log(`  steps: ${[...plan.cheap, ...plan.parallel, ...plan.afterBuild, ...plan.alone].map((step) => step.id).join(", ")}`);
		for (const skip of plan.skipped) console.log(`  skipped ${skip.id}: ${skip.reason}`);
		if (args.plan) return 0;
		requireDiskFloor();

		const id = newReceiptId("release", head);
		const runDir = makeRunDir(id);
		cleanCheckoutTmp(cwd);
		Object.assign(run, { cwd, tempDir: runTemp(), current: id });
		if (npmStale(cwd)) npmCi(cwd);
		// The adversarial reviewer reads the batch's whole diff while the suites run (DEL-35).
		writeFileSync(join(runDir, "batch.diff"), sh("git", ["diff", base, head], { cwd }));
		console.log(`  batch diff for the review: ${join(runDir, "batch.diff")}; record it with rv-gate note ${id} adversarial-review <file>`);
		// Stamp only crates another checkout built since this one did: no whole-workspace rebuild.
		const closure = withDependencies(graph.map((crate) => crate.name), graph);
		const stamped = claim(cwd, SHARED_TARGET, closure, graph, ["debug", "release"]);
		if (stamped.length) console.log(`  rebuilt from this checkout: ${stamped.join(", ")}`);

		const controller = new AbortController();
		const fields = batchFields(commits);
		// One line per commit: near-miss IDs by name, other words only counted.
		for (const m of fields.malformed) console.log(`note: ${m.commit}: items or assumed words that are no lockdown item, not sent to coverage: ${[m.near.length ? `near-miss IDs ${m.near.join(", ")}` : "", m.words ? `${m.words} other word(s)` : ""].filter(Boolean).join("; ")}`);
		const ctx = {
			cwd,
			base,
			head,
			mode: "release",
			phasePolicy: readPhase(),
			baseline: "pre-prod",
			pr: { items: fields.items, retired: fields.retired },
			runDir,
			tempDir: run.tempDir,
			signal: controller.signal,
			registry: { retired: [], added: [] },
			cargoMark: Date.now(),
		};
		// The release run's Cargo waits up to 15 minutes at the disk floor instead of exiting (the shim
		// reads RV_GATE_WAIT_FLOOR), so a dip while the streams build does not void the run (lead, 28 Oct).
		const stepEnv = gateEnv({ CARGO_TARGET_DIR: SHARED_TARGET, TMPDIR: run.tempDir, RV_GATE_WAIT_FLOOR: "1" });
		const results = [];
		// `--keep-going` runs every step even after a failure, so one rerun shows every problem; the
		// receipt still fails, and a failed run never moves pre-prod.
		for (const step of plan.cheap) {
			const result = await runStep(step, ctx, stepEnv);
			results.push(result);
		}
		const limit = Number(args.parallel ?? 3);
		// A rerun takes a step's earlier pass when the step's commands and every file it reads are
		// byte-identical; each reuse names its receipt. `--no-reuse` runs everything.
		const keys = new Map();
		if (!args["no-reuse"]) {
			const treeLines = sh("git", ["ls-tree", "-r", head], { cwd }).split("\n").filter(Boolean);
			const browsers = process.env.PLAYWRIGHT_BROWSERS_PATH || join(homedir(), process.platform === "darwin" ? "Library/Caches/ms-playwright" : ".cache/ms-playwright");
			const versions = [
				sh("rustc", ["-vV"], { cwd, env: stepEnv }),
				sh("cargo", ["-V"], { cwd, env: stepEnv }),
				process.version,
				`browsers: ${existsSync(browsers) ? readdirSync(browsers).sort().join(" ") : "none"}`,
			];
			const environment = reuseEnvironment(stepEnv, versions);
			for (const step of [...plan.parallel, ...plan.afterBuild]) {
				const key = reuseKey(scopedToCrates(step, plan.crates, graph.length), treeLines, environment);
				if (key) keys.set(step.id, key);
			}
		}
		const reused = reusable(plan, keys, reusePass);
		if (reused.size) console.log(`  reusing ${reused.size} pass(es) on identical inputs: ${[...reused.keys()].join(", ")} (--no-reuse runs them)`);
		const runOrReuse = async (step, finalize = result => result) => {
			if (reused.has(step.id)) {
				const result = await finalize({ id: step.id, title: step.title, cards: step.cards, status: "pass", durationMs: 0, runMs: 0, queuedMs: 0, reused: reused.get(step.id), reason: `reused from ${reused.get(step.id)}` });
				console.log(line(result));
				return result;
			}
			const result = await runStep(step, ctx, stepEnv, finalize);
			if (result.status === "pass" && !result.flaky && !result.attempts?.some(attempt => attempt.status === "fail") && keys.has(step.id)) {
				result.reuseKey = keys.get(step.id);
				recordReusePass(step.id, keys.get(step.id), id);
			}
			return result;
		};
		let buildPassed = false;
		let snapshot;
		let snapshotFiles = [];
		// When each pool step ended: a file the snapshot copies must not be newer than its build.
		const ended = new Map();
		{
			const tasks = plan.parallel.map((step) => async () => {
				// The build, the kept copy and the snapshot hold the shared target together, so no other
				// checkout's build lands between the build and the copy the later steps test.
				const hold = step.id === "build" && !reused.has("build") ? await holdCargo(SHARED_TARGET, { signal: ctx.signal, waiting: pid => console.log(`  waiting for gate process ${pid} before the build`) }) : () => {};
				try {
					return await runOrReuse(scopedToCrates(step, plan.crates, graph.length), async result => {
					ended.set(step.id, Date.now());
					// The binaries decide whether later steps run; a rewritten tracked file fails the build
					// step (and so the batch) without stopping them.
					if (step.id === "build") buildPassed = result.status === "pass";
					if (step.id === "build" && !reused.has("build")) {
						const checked = checks.buildTreeResult(result, cwd);
						if (checked !== result) console.log(line(checked));
						result = checked;
					}
					if (step.id === "build" && buildPassed && !reused.has("build")) {
						// The steps after the build test a private copy, so another checkout's release build
						// in the shared target cannot swap the binaries under them.
						// A tracked-file failure above does not invalidate binaries; only the copy's
						// own outcome decides whether the dependent suites have usable artifacts.
						const copied = captureBuildSnapshot({ ctx, target: SHARED_TARGET, result: { ...result, status: "pass" }, notAfter: ended.get("build") });
						snapshot = ctx.snapshot;
						snapshotFiles = ctx.snapshotFiles;
						buildPassed = copied.status === "pass";
						if (!buildPassed) result = { ...copied, reason: [result.status === "fail" && result.reason, copied.reason].filter(Boolean).join("; ") };
						if (buildPassed) {
							try {
								const kept = keepReleaseBuild({ target: SHARED_TARGET, cwd, commit: head, dir: RELEASE_BIN_DIR });
								console.log(`  release build kept in ${kept}`);
							} catch (error) {
								buildPassed = false;
								result = { ...result, status: "fail", reason: [result.status === "fail" && result.reason, `Cannot retain the release build: ${error.message}`].filter(Boolean).join("; ") };
							}
						}
						if (!buildPassed) console.log(line(result));
					}
					return result;
					});
				} finally {
					hold();
				}
			});
			results.push(...(await runPool(tasks, limit, controller, { keepGoing: true, steps: plan.parallel })).filter(Boolean));
		}
		if (buildPassed) {
			// The steps after the build read its release binaries. Without a private copy (a reused build,
			// or a checkout whose target is its own folder), a release build another checkout wrote into
			// the shared target since means they are not this batch's (S27-725).
			const builtAt = Date.now();
			// The publish-tests step may have been building the Wasm during the copy; the pool is done.
			if (snapshot) snapshotFiles.push(...lateOutputs({ target: SHARED_TARGET, dir: snapshot, notAfter: ended.get("publish-tests") ?? builtAt }));
			const tasks = plan.afterBuild.map((step) => async () =>
				!snapshot && !reused.has(step.id) && workspaceBuiltAt(SHARED_TARGET, { release: true }) > builtAt + 1000
					? { id: step.id, title: step.title, cards: step.cards, status: "fail", reason: "Another build wrote release workspace crates into this target after the build step; rerun the release." }
					: runOrReuse(step),
			);
			results.push(...(await runPool(tasks, limit, controller, { keepGoing: true, steps: plan.afterBuild })).filter(Boolean));
		}
		// Steps that need the host to themselves (the perf step) run last, one at a time, on a batch that
		// passed everything else: nothing of ours is building or testing while they measure (PRF-02).
		if (buildPassed && plan.alone.length) {
			for (const step of plan.alone) {
				const result = await runStep(step, ctx, stepEnv);
				results.push(result);
			}
		}
		const ran = new Set(results.map((result) => result.id));
		for (const step of [...plan.cheap, ...plan.parallel, ...plan.afterBuild, ...plan.alone]) {
			if (!ran.has(step.id)) results.push({ id: step.id, title: step.title, cards: step.cards, status: "unrun", reason: "Stopped at the first failure." });
		}
		for (const skip of plan.skipped) results.push({ ...skip, status: "unrun", reason: `Not affected: ${skip.reason}` });
		const failed = results.filter((result) => result.status === "fail");
		const receipt = {
			id,
			kind: "release",
			phase: ctx.phasePolicy.phase,
			phasePolicy: ctx.phasePolicy,
			commit: head,
			tree: gitIn(cwd, "rev-parse", `${head}^{tree}`),
			base,
			tracking,
			batch: commits.map((commit) => ({ commit: commit.commit, subject: commit.subject, issue: commit.fields.issue, decision: commit.fields.decision })),
			unrecorded: unrecorded.map(({ commit, reason }) => ({ commit, subject: subjects.get(commit), reason })),
			belowTip: belowTip.map(({ commit }) => ({ commit, subject: subjects.get(commit) })),
			changed: changed.length,
			crates: plan.crates,
			steps: results,
			captures: ctx.captures,
			snapshot: ctx.snapshotFiles,
			captureDiffs: ctx.captureDiffs?.filter((diff) => diff.status !== "same"),
			result: failed.length ? "fail" : "pass",
			at: new Date().toISOString(),
		};
		writeReceipt(receipt);
		console.log(`\nReceipt ${id}: ${receipt.result}${failed.length ? ` (${failed.map((r) => r.id).join(", ")} failed)` : ""}`);
		printNotRun(results);
		// The release target keeps only dependencies between runs; the workspace rebuilds per batch.
		if (basename(SHARED_TARGET) === "target-release") {
			const freed = pruneWorkspaceOutputs(SHARED_TARGET, metadata);
			console.log(`  ${SHARED_TARGET}: deleted ${(freed / 2 ** 30).toFixed(1)} GiB of workspace outputs`);
		}
		if (failed.length) return 1;
		if (args["no-push"]) {
			console.log("--no-push: pre-prod not moved.");
			return 0;
		}
		writeReceiptNote(id, "diagnostic", { commit: head, tree: receipt.tree, commits: commits.map((commit) => commit.commit) });
		console.log(`Combined-state checks passed at ${head.slice(0, 10)}. Pre-prod and its capture baseline require a passing full receipt.`);
		return 0;
	} finally {
		try {
			finishRun(run);
		} finally {
			unlock();
		}
	}
}

// ---------------------------------------------------------------------------------------------
// Full tier, candidates and the prod step.

async function fullTier(ref, args, extra = {}) {
	return withRunCancellation((signal, received, cancel) => fullTierCancellable(ref, args, extra, signal, received, cancel));
}

async function fullTierCancellable(ref, args, extra, signal, received, cancel) {
	const affected = Boolean(extra.affected);
	if (affected && (readPhase().phase !== "prelaunch" || args.only)) throw new Error("Affected checks require prelaunch and cannot use --only");
	const parallel = extra.candidate ? 1 : Number(args.parallel ?? 1);
	fullStepGroups([], parallel); // Refuse invalid limits before acquiring the diagnostic lock.
	ensureStore();
	const release = acquireLock("merge", { wait: true });
	const run = {};
	let ctx, releaseHold = () => {};
	try {
		const cwd = workCheckout("work");
		gitIn(cwd, "fetch", "-q", "origin");
		const commit = gitIn(cwd, "rev-parse", ref);
		gitIn(cwd, "checkout", "-q", "-f", "--detach", commit);
		prepareCheckout(cwd);
		const kind = extra.candidate ? "prod" : affected ? "affected" : "full";
		const id = newReceiptId(kind, commit);
		const runDir = makeRunDir(id);
		cleanCheckoutTmp(cwd);
		Object.assign(run, { cwd, tempDir: runTemp(), current: id });
		const base = gitIn(cwd, "rev-parse", "origin/prod");
		const sourceTree = gitIn(cwd, "rev-parse", `${commit}^{tree}`);
		const rangeChanges = sourceChanges(cwd, { base, head: commit, receipt: id });
		const fields = batchFields(rangeChanges.map(entry => ({ commit: entry.commit, fields: { items: entry.items ?? [], assumed: entry.assumed ?? [], retired: entry.deletedTests ?? [] } })));
		ctx = { cwd, base, head: commit, mode: kind === "prod" ? "prod" : "full", runDir, tempDir: run.tempDir, pr: fields, rangeChanges, candidate: extra.candidate, signal, cancel };
		let affectedPlan, affectedBase, affectedGreen, affectedHash, affectedPreProd;
		if (affected) {
			ctx.phasePolicy = readPhase();
			affectedPreProd = readDev(cwd, {at: commit}).base;
			function* anchors() { yield* fullReceipts(); yield* affectedReceipts(); }
			const anchor = lastGreenBase({head: commit, receipts: anchors(), notes: receiptNotes,
				contains: (later, earlier) => {
					const result = spawnSync("git", ["merge-base", "--is-ancestor", earlier, later], {cwd, encoding: "utf8"});
					if (![0, 1].includes(result.status)) throw new Error("Cannot verify green ancestor");
					return result.status === 0;
				}, treeOf: source => gitIn(cwd, "rev-parse", `${source}^{tree}`)});
			affectedBase = anchor.base;
			affectedGreen = anchor.receipt;
			const steps = stepsForTier("full", "full"), areas = JSON.parse(readFileSync(join(here, "areas.json"), "utf8"));
			const counts = failureCounts(fullReceipts(), steps);
			affectedHash = policyFingerprint({policy: ctx.phasePolicy, steps, areas, counts});
			affectedPlan = affectedContext({cwd, base: affectedBase, head: commit, steps, areas, policy: ctx.phasePolicy, counts, history: [...affectedReceipts()], policyHash: affectedHash});
			ctx.affectedSelection = affectedPlan.selection;
			console.log(`  green coverage base ${affectedBase.slice(0, 10)} (${affectedGreen}); pre-prod ${affectedPreProd.slice(0, 10)}; ${affectedPlan.commits.length} commits; ${affectedPlan.pending.length} outstanding units`);
			for (const item of affectedPlan.selection) console.log(`  ${item.status.toUpperCase()} ${item.id}: ${item.reason}`);
		}
		console.log(`rv-gate ${kind}: ${ref} at ${commit.slice(0, 10)}; receipt ${id}; parallel ${parallel}`);
		// The run tests its own release build, so other release builds wait for the whole run; debug
		// builds go ahead between its own Cargo commands, which hold the whole target (lead, 3 Nov).
		const needsBuild = !affected || affectedPlan.selection.some(item => item.id === "build" && item.status === "run");
		try { if (needsBuild && !signal.aborted) releaseHold = await holdRelease(SHARED_TARGET, { signal, waiting: pid => console.log(`  waiting for process ${pid}, which holds the shared target's release builds`) }); }
		catch (error) { if (!signal.aborted) throw error; }
		const ran = await withBrowserEvents(join(runDir, "browser-relaunches.jsonl"), () => runSteps(ctx, "full", { only: list(args.only), parallel }));
		const { results, ...browserEvidence } = ran;
		if (signal.aborted && browserEvidence.browserRelaunchTelemetry.status !== "fail") browserEvidence.browserRelaunchTelemetry = {...browserEvidence.browserRelaunchTelemetry, status: "unrun", reason: "Gate cancelled before browser telemetry was complete."};
		const failed = results.filter((r) => r.status === "fail");
		const telemetryFailed = browserEvidence.browserRelaunchTelemetry.status !== "pass";
		const outcomes = affected ? outcomesForSelection(affectedPlan.selection, results, browserEvidence.browserRelaunchTelemetry.status) : undefined;
		const incomplete = affected && Object.values(outcomes).some(status => status !== "pass");
		const receipt = {
			id,
			kind,
			...(affected && {base: affectedBase, preProdAtStart: affectedPreProd, greenReceipt: affectedGreen, policyHash: affectedHash, selected: Object.keys(outcomes), outcomes, selection: affectedPlan.selection, obligations: affectedPlan.obligations}),
			phase: ctx.phasePolicy.phase,
			phasePolicy: ctx.phasePolicy,
			rotation: ctx.rotation,
			ref,
			candidate: extra.candidate,
			commit,
			tree: sourceTree,
			prod: base,
			parallel,
			steps: results,
			...browserEvidence,
			registry: ctx.registry,
			captures: ctx.captures,
			snapshot: ctx.snapshotFiles,
			captureDiffs: ctx.captureDiffs?.filter((d) => d.status !== "same"),
			changes: rangeChanges.map((entry) => entry.receipt),
			...(signal.aborted && {cancelled: true, cancellationSignal: received()}),
			...(ctx.unknownWorkerDrains?.length && {operatorBlocked: true, unresolvedWorkerDrains: ctx.unknownWorkerDrains}),
			result: signal.aborted || failed.length || telemetryFailed || incomplete ? "fail" : "pass",
			at: new Date().toISOString(),
		};
		writeReceipt(receipt);
		console.log(`\nReceipt ${id}: ${receipt.result} (${failed.length} failed${telemetryFailed ? ", browser telemetry failed" : ""}, ${results.filter((r) => r.status === "unrun").length} unrun)`);
		console.log(`Browser startup relaunches: ${browserEvidence.browserRelaunches.length}`);
		if (browserEvidence.previewReloads.length) console.log(previewReloadLine(results));
		if (telemetryFailed) console.log(browserEvidence.browserRelaunchTelemetry.reason);
		printNotRun(results);
		return { receipt, ctx };
	} finally {
		if (ctx?.unknownWorkerDrains?.length) {
			const diagnostic = join(ctx.runDir, "operator-blocked.json");
			const text = JSON.stringify({receipt: run.current, result:"fail", cancelled:true, incomplete:true, unpromotable:true, unresolved:ctx.unknownWorkerDrains}, null, 2) + "\n";
			await waitForWorkerGroups(ctx.unknownWorkerDrains, {
				alert: records => alertUnresolvedWorkers(run.current, records),
				persist: () => { const temp = `${diagnostic}.tmp`; writeFileSync(temp, text); renameSync(temp, diagnostic); },
			});
		}
		releaseHold();
		// Housekeeping runs under the lock, so no other run is using the gate checkout; a failure
		// in it never keeps the lock.
		try {
			if (!signal.aborted) finishRun(run);
		} finally {
			release();
		}
	}
}

function cutCandidate() {
	const cwd = workCheckout("work");
	gitIn(cwd, "fetch", "-q", "origin", "--prune");
	const existing = sh("git", ["ls-remote", "--heads", "origin", "candidate/*"], { cwd })
		.split("\n")
		.map((l) => Number(l.match(/candidate\/(\d+)$/)?.[1]))
		.filter(Number.isFinite);
	const n = (existing.length ? Math.max(...existing) : 0) + 1;
	// Candidates come from what a release run passed (DEL-81).
	const preProd = gitIn(cwd, "rev-parse", "origin/pre-prod");
	sh("git", ["push", "-q", "origin", `${preProd}:refs/heads/candidate/${n}`], { cwd, env: gateEnv({ RV_GATE_PUSH: "candidate" }) });
	console.log(`Cut candidate/${n} at ${preProd.slice(0, 10)}.`);
	return n;
}

// ---------------------------------------------------------------------------------------------

/**
 * The gate and its detector run from a trusted copy, never from the PR or candidate under test
 * (DEL-36). Commands that write receipts re-run themselves from the gate checkout. Dev-tier commands
 * use origin/dev's copy, including combined release diagnostics. Prod and candidate runs use
 * the copy on origin/pre-prod once it has a release gate (`gateRefFor`). `--bootstrap` runs the
 * local copy while the gate is not on dev yet.
 */
function fromDevCopy(command, args) {
	if (!["dev", "release", "full", "affected", "prod", "candidate", "hotfix", "merge-prod", "report"].includes(command)) return undefined;
	if (args.bootstrap || process.env.RV_GATE_REEXEC === "1") return undefined;
	const gate = workCheckout("gate");
	gitIn(gate, "fetch", "-q", "origin", "dev", "pre-prod");
	const hasReleaseGate = (ref) => sh("git", ["cat-file", "-e", `${ref}:tools/rv-gate/lib/release.mjs`], { cwd: gate, allowFail: true }).status === 0;
	const { ref, note } = gateRefFor(command, hasReleaseGate);
	if (note) console.log(note);
	gitIn(gate, "checkout", "-q", "-f", "--detach", ref);
	const script = join(gate, "tools/rv-gate/rv-gate.mjs");
	if (!existsSync(script)) throw new Error(`rv-gate is not on ${ref} yet; run with --bootstrap (DEL-07 bootstrap exception).`);
	const result = spawnSync(process.execPath, [script, ...process.argv.slice(2)], { stdio: "inherit", env: { ...process.env, RV_GATE_REEXEC: "1" } });
	return result.status ?? 1;
}

async function main() {
	const [command, ...rest] = process.argv.slice(2);
	const args = parseArgs(rest);
	forbidInheritedCases(command, readPhase().phase);
	const delegated = fromDevCopy(command, args);
	if (delegated !== undefined) return delegated;
	switch (command) {
		case "fast": {
			const { fastInPhase } = await import("./lib/fast-phase.mjs");
			return fastInPhase({
				cwd: resolve(args.cwd ?? process.cwd()),
				target: SHARED_TARGET,
				base: args.base ?? "origin/dev",
				all: Boolean(args.all),
				plan: Boolean(args.plan),
				dependentTests: Boolean(args["dependent-tests"]),
			});
		}
		case "pre-pr":
			return localTier(command, args);
		case "dev":
			return devGate(args._[0], args);
		case "release":
			return releaseRun(args);
		case "note": {
			const [id, kind, file] = args._;
			if (!id || !kind || !file) throw new Error("Usage: rv-gate note <receipt> <kind> <file>");
			readReceipt(id);
			const path = writeReceiptNote(id, kind, { text: readFileSync(resolve(file), "utf8") });
			console.log(`Recorded ${path}.`);
			return 0;
		}
		case "full": {
			const { receipt } = await fullTier(args._[0] ?? "origin/dev", args);
			return receipt.result === "pass" ? 0 : 1;
		}
		case "affected": {
			const { receipt } = await fullTier(args._[0] ?? "origin/dev", args, {affected: true});
			return receipt.result === "pass" ? 0 : 1;
		}
		case "candidate":
			cutCandidate();
			return 0;
		case "hotfix": {
			const { runHotfix } = await import("./lib/hotfix-run.mjs");
			const cwd = workCheckout("hotfix");
			sh("git", ["worktree", "lock", "--reason", "active hotfix gate", cwd], { cwd, allowFail: true });
			const gitDir = gitIn(cwd, "rev-parse", "--absolute-git-dir");
			if (!existsSync(join(gitDir, "locked"))) throw new Error("Cannot lock the hotfix gate checkout.");
			return runHotfix({ source: args._[0], cwd, target: SHARED_TARGET, prepare: prepareCheckout });
		}
		case "prod": {
			const name = args._[0];
			if (!/^candidate\/\d+$/.test(name ?? "")) throw new Error("Usage: rv-gate prod candidate/<n>");
			const { receipt } = await fullTier(`origin/${name}`, args, { candidate: name });
			const { writeGoCard } = await import("./lib/report.mjs");
			const card = writeGoCard({ receipt, cwd: workCheckout("work") });
			console.log(`Go card: ${card}`);
			return receipt.result === "pass" ? 0 : 1;
		}
		case "merge-prod": {
			const { mergeProd } = await import("./lib/report.mjs");
			return mergeProd({ candidate: args._[0], go: args.go, cwd: workCheckout("work") });
		}
		case "report": {
			const { writeReport } = await import("./lib/report.mjs");
			const path = await writeReport({ cwd: workCheckout("work"), from: args.from ?? "origin/prod", to: args.to ?? "origin/pre-prod" });
			console.log(`Report: ${path}`);
			return 0;
		}
		case "audit": {
			const cwd = resolve(args.cwd ?? process.cwd());
			sh("git", ["fetch", "-q", "origin"], { cwd });
			const result = checks.audit({ cwd });
			console.log(line({ id: "audit", ...result }));
			return result.status === "pass" ? 0 : 1;
		}
		case "value": {
			if (args._.length) throw new Error("Usage: rv-gate value [--since YYYY-MM-DD]");
			const result = writeValue({since: args.since, steps: STEPS});
			console.log(readFileSync(result.markdown, "utf8"));
			console.log(`Value ledger: ${result.json}\nMarkdown: ${result.markdown}`);
			return 0;
		}
		case "surface": {
			const cwd = resolve(args.cwd ?? process.cwd());
			// --bin takes any build folder holding `richview`: a debug build or a kept release build.
			const files = generateSurface(args.bin ? resolve(String(args.bin)) : join(SHARED_TARGET, "release"));
			if (args.write) {
				writeSurface(join(cwd, "docs/surface"), files);
				console.log(`Wrote ${Object.keys(files).length} snapshot files to docs/surface.`);
			} else {
				const stale = diffSurface(files, readCommitted(join(cwd, "docs/surface")));
				console.log(stale.length ? `Out of date: ${stale.join(", ")}` : "docs/surface is current.");
				return stale.length ? 1 : 0;
			}
			return 0;
		}
		case "install-hooks": {
			const { installHooks } = await import("./lib/hooks.mjs");
			installHooks();
			return 0;
		}
		case "steps":
			for (const step of STEPS) console.log(`${step.tier.padEnd(12)} ${step.id.padEnd(15)} ${step.unrun ? "unrun" : "built"}  ${step.cards.join(",")}  ${step.title}`);
			return 0;
		default:
			console.log(readFileSync(fileURLToPath(import.meta.url), "utf8").split("\n\n")[0].split("\n").slice(1).join("\n").replaceAll("// ", ""));
			return command ? 2 : 0;
	}
}

main().then(
	(code) => process.exit(code ?? 0),
	(error) => {
		console.error(`rv-gate: ${error.message}`);
		process.exit(1);
	},
);

