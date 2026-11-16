// Host reason (ARC-03): execute held Rust cohorts while retaining canonical per-shard logs.
import {existsSync, mkdirSync, readFileSync, renameSync} from "node:fs";
import {basename, join} from "node:path";
import {runLogged, ignoredTests, unrunLines} from "./proc.mjs";
import {missingEnv} from "./connectors.mjs";
import {expandCohort, selection, withCohortHold} from "./rust-tests.mjs";
import {runAffectedCohort} from "./affected-cohort.mjs";

/** Scoped Cargo invocations keep the original executor until it supports scoped inventories. */
export function cohortApplicable(step) {
	return step.cohort === "rust-debug" && step.run?.length === 1 && step.run[0].cmd === "cargo" && Boolean(selection(step.run[0].args));
}

/** Prepare under one debug hold; final successful shard logs keep the canonical evidence names. */
export async function runCohortStep(step, ctx, {target, env, schedule, parallel, selected, expand = expandCohort, hold = withCohortHold, run = runLogged} = {}) {
	const previous = new Map(), unrun = [], ignored = [], unrunInputs = [];
	mkdirSync(join(ctx.runDir, "logs"), {recursive: true});
	const execute = async (command, attempt) => {
		const number = /^shard-(\d+)-of-4$/.exec(command.id)?.[1];
		if (command.id !== "prepare" && number === undefined) throw new Error(`Invalid Rust cohort command id: ${command.id}`);
		const path = join(ctx.runDir, "logs", `${step.id}-${number ?? "prepare"}.log`);
		if (attempt > 1 && existsSync(path)) {
			const archive = join(ctx.runDir, "attempts", step.id, String(attempt - 1), "logs");
			mkdirSync(archive, {recursive: true});
			const saved = join(archive, `${step.id}-${number ?? "prepare"}.log`);
			if (existsSync(saved)) throw new Error(`Cohort attempt archive already exists: ${saved}`);
			renameSync(path, saved);
			const prior = previous.get(command.id);
			if (prior) prior.logPath = saved;
		}
		const commandEnv = {...env, ...command.env, CARGO_TARGET_DIR: target, RICHVIEW_VERIFY_WORKERS: "1"};
		const absent = command.whenEnvMissing && missingEnv(command.whenEnvMissing.vars, commandEnv).length > 0;
		if (absent) unrun.push(...command.whenEnvMissing.unrun);
		const args = [...command.args, ...(absent ? command.whenEnvMissing.args : [])];
		let outcome = await run(command.cmd, args, {cwd: ctx.cwd, env: commandEnv, logPath: path, signal: ctx.signal});
		if (ctx.signal?.aborted) outcome = {...outcome, status: "unrun", reason: "Gate cancelled during Rust command."};
		const log = existsSync(path) ? readFileSync(path, "utf8") : "";
		ignored.push(...ignoredTests(log)); unrunInputs.push(...unrunLines(log));
		const result = {...outcome, logPath: path, ...(outcome.exitCode === 75 && log.includes("rv-gate Rust cohort environment-retry: missing test executable ") && {environmentRetry: true})};
		previous.set(command.id, result);
		return result;
	};
	const checkedExpand = async (step, context) => {
		const value = await expand(step, context), expected = Array.from({length: 4}, (_, index) => `shard-${index}-of-4`);
		if (!Array.isArray(value?.shards) || value.shards.length !== expected.length || expected.some(id => !value.shards.some(shard => shard.id === id))) throw new Error("Rust cohort must contain all four stable shards");
		return value;
	};
	const result = await runAffectedCohort({step, ctx: {...ctx, target}, parallel, schedule, expand: checkedExpand, hold, execute, selected, signal: ctx.signal});
	ctx.cargoMark = Date.now();
	const unique = values => [...new Map(values.map(value => [JSON.stringify(value), value])).values()];
	const attempts = [result.prepare, result.environmentPreparation, ...result.shards].filter(Boolean).flatMap(value => value.attempts ?? [value]);
	for (const attempt of attempts) {
		if (!attempt.logPath || !attempt.number) continue;
		const archived = join(ctx.runDir, "attempts", step.id, String(attempt.number), "logs", basename(attempt.logPath));
		if (existsSync(archived)) attempt.logPath = archived;
	}
	return {status: result.status, ...(result.status !== "pass" && {reason: "Rust cohort preparation or one or more selected shards failed."}),
		logs: attempts.map(value => value.logPath).filter(Boolean),
		details: {commands: result.shards, preparation: result.prepare, ...(result.environmentRetry && {environmentRetry: true, environmentPreparation: result.environmentPreparation}),
			ignored: unique(ignored), ...(unrun.length && {unrunTests: unique(unrun)}), ...(unrunInputs.length && {unrunInputs: unique(unrunInputs)})}};
}
