// Host reason (ARC-03): execute held Rust cohorts while retaining canonical per-shard logs.
import {existsSync, mkdirSync, readFileSync, renameSync} from "node:fs";
import {basename, join} from "node:path";
import {runLogged, ignoredTests, unrunLines} from "./proc.mts";
import {missingEnv} from "./connectors.mts";
import {expandCohort, selection, withCohortHold} from "./rust-tests.mts";
import {runAffectedCohort} from "./affected-cohort.mts";
import type {Cohort, CohortCommand, CohortResult} from "./affected-cohort.mts";
import type {RetriedCommand} from "./affected-retry.mts";
import type {CommandVerdict, StepOutcome} from "./results.mts";
import type {GateStep} from "./steps.mts";

interface CohortContext { cwd: string; runDir: string; target?: string; signal?: AbortSignal; cargoMark?: number }
interface CohortOptions {
	target: string; env: NodeJS.ProcessEnv; parallel: number; selected?: readonly string[];
	schedule: (work: () => Promise<CohortResult>, permits?: number) => Promise<CohortResult>;
	expand?: (step: GateStep, ctx: CohortContext) => Cohort | Promise<Cohort>;
	hold?: (kind: "rust-debug", ctx: CohortContext, work: () => Promise<CohortResult>) => Promise<CohortResult>;
	run?: typeof runLogged;
}

/** Scoped Cargo invocations keep the original executor until it supports scoped inventories. */
export function cohortApplicable(step: GateStep) {
	return step.cohort === "rust-debug" && step.run?.length === 1 && step.run[0].cmd === "cargo" && Boolean(selection(step.run[0].args));
}

/** Prepare under one debug hold; final successful shard logs keep the canonical evidence names. */
export async function runCohortStep(step: GateStep, ctx: CohortContext, {target, env, schedule, parallel, selected, expand = expandCohort, hold = withCohortHold, run = runLogged}: CohortOptions): Promise<StepOutcome> {
	const previous = new Map<string, CommandVerdict>();
	const unrun: { name: string; reason: string }[] = [], ignored: ReturnType<typeof ignoredTests> = [], unrunInputs: ReturnType<typeof unrunLines> = [];
	mkdirSync(join(ctx.runDir, "logs"), {recursive: true});
	const execute = async (command: CohortCommand, attempt: number) => {
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
		if (absent) unrun.push(...command.whenEnvMissing!.unrun);
		const args = [...command.args, ...(absent ? command.whenEnvMissing!.args : [])];
		let outcome = await run(command.cmd, args, {cwd: ctx.cwd, env: commandEnv, logPath: path, signal: ctx.signal});
		if (ctx.signal?.aborted) outcome = {...outcome, status: "unrun", reason: "Gate cancelled during Rust command."};
		const log = existsSync(path) ? readFileSync(path, "utf8") : "";
		ignored.push(...ignoredTests(log)); unrunInputs.push(...unrunLines(log));
		const result = {...outcome, logPath: path, ...(outcome.exitCode === 75 && log.includes("rv-gate Rust cohort environment-retry: missing test executable ") && {environmentRetry: true})};
		previous.set(command.id, result);
		return result;
	};
	const checkedExpand = async (step: GateStep, context: CohortContext) => {
		const value = await expand(step, context), expected = Array.from({length: 4}, (_, index) => `shard-${index}-of-4`);
		if (!Array.isArray(value?.shards) || value.shards.length !== expected.length || expected.some(id => !value.shards.some(shard => shard.id === id))) throw new Error("Rust cohort must contain all four stable shards");
		return value;
	};
	const result = await runAffectedCohort({step, ctx: {...ctx, target}, parallel, schedule, expand: checkedExpand, hold, execute, selected, signal: ctx.signal});
	ctx.cargoMark = Date.now();
	const unique = <T,>(values: readonly T[]) => [...new Map(values.map(value => [JSON.stringify(value), value])).values()];
	const outcomes: (CommandVerdict | RetriedCommand | undefined)[] = [result.prepare, result.environmentPreparation, ...result.shards];
	const retried = (value: CommandVerdict | RetriedCommand): value is RetriedCommand => "attempts" in value;
	const attempts = outcomes.flatMap<CommandVerdict & { number?: number }>(value => {
		if (value === undefined) return [];
		return retried(value) ? value.attempts : [value];
	});
	for (const attempt of attempts) {
		if (!attempt.logPath || !attempt.number) continue;
		const archived = join(ctx.runDir, "attempts", step.id, String(attempt.number), "logs", basename(attempt.logPath));
		if (existsSync(archived)) attempt.logPath = archived;
	}
	return {status: result.status, ...(result.status !== "pass" && {reason: "Rust cohort preparation or one or more selected shards failed."}),
		logs: attempts.map(value => value.logPath).filter((path): path is string => Boolean(path)),
		details: {commands: result.shards, preparation: result.prepare, ...(result.environmentRetry && {environmentRetry: true, environmentPreparation: result.environmentPreparation}),
			ignored: unique(ignored), ...(unrun.length && {unrunTests: unique(unrun)}), ...(unrunInputs.length && {unrunInputs: unique(unrunInputs)})}};
}
