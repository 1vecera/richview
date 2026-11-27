// Host reason (ARC-03): partition independent verification cases and require complete shard evidence.
import { appendFileSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { join } from "node:path";
import { runLogged } from "./proc.mts";
import { reportCohort } from "./report-inputs.mts";
import { mergeNoJsShards } from "./no-js-shards.mts";
import { DOCS_PHASES, docsInventory, mergeDocsPhases, verifyDocsPhases } from "./docs-shards.mts";
import { retryCommands } from "./affected-retry.mts";
import type { RetriedCommand } from "./affected-retry.mts";
import type { CaseEvidence, CaseScope } from "./cases.mts";
import type { CommandVerdict } from "./results.mts";

export interface ShardSelection { index: number; count: number; cohort: string[]; selected: string[] }
interface ShardProof { shard?: ShardSelection; completed: string[] }
interface RuntimeView { width: number; name: string }
interface ShardResult { caseKey: string; ok: boolean; views: RuntimeView[]; type?: string }
interface ShardReport extends ShardProof {
	shard: ShardSelection;
	case_scope?: CaseScope;
	browser: string;
	engine: string;
	style?: string;
	width?: number;
	model_count?: number;
	failed?: number;
	errors?: unknown[];
	failures?: unknown[];
	cases: RuntimeView[];
	results: ShardResult[];
}
export type ShardSchedule = (work: () => Promise<CommandVerdict>, permits?: number) => Promise<CommandVerdict>;
interface ShardOptions {
	cwd?: string;
	env?: NodeJS.ProcessEnv;
	logPath: string;
	signal?: AbortSignal;
	parallel: number;
	scheduleCommand: ShardSchedule;
	shard: { kind: "runtime" | "docs" | "interaction" | "no-js"; out: string; count?: number; cases?: readonly string[] };
}

/** The desktop and phone views that together complete one runtime report case. */
export const RUNTIME_WIDTHS = Object.freeze([1280, 390]);

const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);
const keysOf = (values: unknown, label: string): string[] => {
	if (!Array.isArray(values) || values.some(key => typeof key !== "string" || !key.length) || new Set(values).size !== values.length)
		throw new Error(`${label} must contain unique nonempty case keys`);
	return values;
};

/** Select independent cases by stable position; no selector means the complete suite. */
export function verificationShard<T>(cases: readonly T[], { key = (_, index) => String(index) }: { key?: (item: T, index: number) => string } = {}) {
	const value = process.env.RICHVIEW_VERIFY_SHARD ?? "0/1";
	const match = /^(0|[1-9]\d*)\/([1-9]\d*)$/.exec(value);
	const index = Number(match?.[1]), count = Number(match?.[2]);
	if (!match || !Number.isSafeInteger(index) || !Number.isSafeInteger(count) || index >= count)
		throw new Error("RICHVIEW_VERIFY_SHARD must be index/count with zero-based index < count");
	const cohort = keysOf(cases.map(key), "Shard cohort");
	const selected = cohort.filter((_, position) => position % count === index);
	return { cases: cases.filter((_, position: number) => position % count === index), shard: { index, count, cohort, selected } };
}

/** Require every shard and every completed case before a parent can publish a full-suite result. */
export function verifyShardCoverage(reports: readonly ShardProof[]) {
	if (!reports.length) throw new Error("No shard reports");
	const cohort = keysOf(reports[0].shard?.cohort, "Shard cohort");
	const count = reports[0].shard?.count!;
	if (!cohort.length || !Number.isSafeInteger(count) || count < 1 || reports.length !== count)
		throw new Error("Shard reports do not cover a nonempty complete cohort");
	const indices = new Set();
	for (const { shard, completed } of reports) {
		if (!shard || shard.count !== count || !same(shard.cohort, cohort) || !Number.isSafeInteger(shard.index) || shard.index < 0 || shard.index >= count || indices.has(shard.index))
			throw new Error("Shard reports disagree on the cohort or repeat a shard index");
		indices.add(shard.index);
		const expected = cohort.filter((_, position: number) => position % count === shard.index);
		if (!same(shard.selected, expected)) throw new Error(`Shard ${shard.index} selected the wrong cases`);
		keysOf(completed, `Shard ${shard.index} completed cases`);
		if (completed.length !== expected.length || completed.some(key => !expected.includes(key)))
			throw new Error(`Shard ${shard.index} did not complete every selected case`);
	}
	return cohort;
}

/** Run browser shards through the shared budget and merge only complete actual case results. */
export async function runShardedCommand(cmd: string, args: readonly string[], { cwd, env, logPath, signal, parallel, scheduleCommand, shard }: ShardOptions): Promise<CommandVerdict> {
	const started = Date.now(), command = [cmd, ...args].join(" ");
	let outcomes: (CommandVerdict & Partial<Pick<RetriedCommand, "attempts">>)[] = [];
	let caseEvidence: CaseEvidence | undefined;
	let indices: number[] = [];
	const requested = shard?.cases;
	const {RICHVIEW_VERIFY_CASES: _untrustedCases, ...cleanEnv} = env ?? {};
	const relaunches = () => outcomes.flatMap(outcome => outcome.attempts ? outcome.attempts.flatMap(attempt => attempt.browserRelaunches ?? []) : outcome.browserRelaunches ?? []);
	const shardResults = () => outcomes.map((outcome, index) => ({id: `${indices[index]}`, ...outcome}));
	try {
		const prefix = { interaction: "types", runtime: "report-runtime", "no-js": "no-js", docs: "docs" }[shard.kind];
		if (typeof prefix !== "string") throw new Error(`Unknown shard result kind: ${shard.kind}`);
		const runtime = shard.kind === "runtime";
		if (requested !== undefined && (!["interaction", "docs", "runtime"].includes(shard.kind) || !Array.isArray(requested) || !requested.length || requested.some(key => typeof key !== "string" || !key) || new Set(requested).size !== requested.length)) throw new Error("Affected cases must be unique keys for a supported suite");
		if (requested && shard.kind === "docs" && requested.some((key: string) => !(DOCS_PHASES as readonly string[]).includes(key))) throw new Error("Unknown docs phase");
		const childEnv = {...cleanEnv, RICHVIEW_VERIFY_PARENT: requested ? `affected-${shard.kind}` : shard.kind, ...(requested && {RICHVIEW_VERIFY_CASES: JSON.stringify(requested)})};
		if (!Number.isSafeInteger(parallel) || parallel < 1) throw new Error("Shard budget must be a positive integer");
		const count = shard.count ?? parallel;
		mkdirSync(shard.out, { recursive: true });
		const canonical = join(shard.out, `${prefix}${requested ? ".partial" : ""}.json`);
		if (requested) rmSync(join(shard.out, `${prefix}.json`), {force: true});
		rmSync(canonical, { force: true });
		if (shard.kind === "docs") rmSync(join(shard.out, "chart-pages.json"), { force: true });
		if (!Number.isSafeInteger(count) || count < 2) throw new Error("Shard count must be at least two");
		if (shard.kind === "docs" && count !== DOCS_PHASES.length) throw new Error("Docs dispatch requires its fixed five phases");
		indices = requested && shard.kind === "docs" ? DOCS_PHASES.flatMap((phase, index) => requested.includes(phase) ? [index] : []) : Array.from({length: count}, (_, index) => index);
		writeFileSync(logPath, `$ ${command} (${count} shards, command budget ${parallel})\n`);
		// Remove old per-shard evidence too: an exited child must never inherit a previous pass.
		for (let index = 0; index < count; index++) rmSync(join(shard.out, `${prefix}-${index}-of-${count}.json`), { force: true });
		if (env?.RV_GATE_PARTIAL_RETRY === "1") {
			outcomes = await retryCommands(indices, async (index, attempt) => {
				const part = join(shard.out, `${prefix}-${index}-of-${count}.json`);
				rmSync(part, {force: true});
				const childLog = `${logPath}.shard-${index}${attempt === 2 ? "-attempt-2" : ""}.log`;
				const outcome = await scheduleCommand(() => runLogged(cmd, args, {
					cwd, env: {...childEnv, RICHVIEW_VERIFY_SHARD: `${index}/${count}`, RV_GATE_SHARD_ATTEMPT: String(attempt)}, signal, logPath: childLog,
				}));
				if (outcome.status !== "pass") return outcome;
				try {
					const report: ShardReport = JSON.parse(readFileSync(part, "utf8")), metadata = report.shard;
					const cohort = keysOf(metadata?.cohort, "Shard cohort");
					if (!cohort.length || metadata.index !== index || metadata.count !== count || !same(metadata.selected, cohort.filter((_, position) => position % count === index))) throw new Error("Shard metadata does not match its assignment");
					keysOf(report.completed, "Shard completed cases");
					if (report.completed.length !== metadata.selected.length || report.completed.some(key => !metadata.selected.includes(key))) throw new Error("Shard did not complete its assigned cases");
					if (report.failed || report.errors?.length || report.failures?.length || report.results?.some(result => !result.ok)) throw new Error("Shard reported failed assertions");
					return outcome;
				} catch (error) { return {...outcome, status: "fail", reason: String((error as Error).message)}; }
			}, {limit: indices.length, signal});
		} else {
			const settled = await Promise.allSettled(indices.map(index => scheduleCommand(() => runLogged(cmd, args, {
				cwd, env: { ...childEnv, RICHVIEW_VERIFY_SHARD: `${index}/${count}` }, signal,
				logPath: `${logPath}.shard-${index}.log`,
			}))));
			outcomes = settled.map(result => result.status === "fulfilled" ? result.value : { status: "fail", reason: String(result.reason) });
		}
		for (const [position, index] of indices.entries()) {
			for (const suffix of ["", ...((outcomes[position].attempts?.length ?? 0) > 1 ? ["-attempt-2"] : [])]) {
				const path = `${logPath}.shard-${index}${suffix}.log`;
				if (existsSync(path)) appendFileSync(logPath, `\n--- shard ${index}/${count}${suffix} ---\n${readFileSync(path, "utf8")}\n`);
			}
		}
		const reports: ShardReport[] = indices.map(index => JSON.parse(readFileSync(join(shard.out, `${prefix}-${index}-of-${count}.json`), "utf8")));
		const partialDocs = Boolean(requested && shard.kind === "docs");
		const cohort = partialDocs ? DOCS_PHASES : verifyShardCoverage(reports);
		if (requested) {
			const scope = reports[0].case_scope;
			keysOf(scope?.cohort, "Original case cohort");
			keysOf(scope?.selected, "Requested cases");
			if (!same([...scope!.selected].sort(), [...requested].sort()) || !same(scope!.selected, scope!.cohort.filter(key => requested.includes(key))) || !same(cohort, partialDocs ? scope!.cohort : scope!.selected)
				|| reports.some(report => !same(report.case_scope, scope) || !partialDocs && !runtime && report.model_count !== scope!.cohort.length)) throw new Error("Filtered shard evidence does not match the requested cases and source cohort");
			caseEvidence = {cohort: scope!.cohort, selected: scope!.selected, completed: partialDocs ? reports.flatMap(report => report.completed) : [...cohort]};
		}
		if (shard.kind === "docs") {
			if (outcomes.some(outcome => outcome.status !== "pass")) throw new Error("A docs phase command failed");
			// These JSON reports come from the docs shard command; its verifier checks them below.
			const docsReports = reports as unknown as Parameters<typeof verifyDocsPhases>[0];
			const options = {cohort, inventory: docsInventory(cwd ?? process.cwd()), engine: env?.RICHVIEW_VERIFY_BROWSER ?? "chromium"};
			if (partialDocs) {
				verifyDocsPhases(docsReports, {...options, selected: caseEvidence!.selected});
				writeFileSync(canonical, JSON.stringify({case_scope: caseEvidence, phases: reports}, null, 2) + "\n");
				appendFileSync(logPath, `\nrv-gate: ${caseEvidence!.selected.length} selected docs phases completed their authoritative journeys\n`);
				return {status: "pass", exitCode: 0, durationMs: Date.now() - started, command, browserRelaunches: relaunches(), shards: shardResults(), cases: caseEvidence};
			}
			const report = mergeDocsPhases(docsReports, options);
			writeFileSync(join(shard.out, "chart-pages.json"), JSON.stringify(report.chart_pages, null, 2) + "\n");
			writeFileSync(canonical, JSON.stringify({ ...report, seconds: (Date.now() - started) / 1000, cohort_sha256: createHash("sha256").update(JSON.stringify(cohort)).digest("hex") }, null, 2) + "\n");
			appendFileSync(logPath, `\nrv-gate: all five docs phases completed every authoritative journey\n`);
			return { status: "pass", exitCode: 0, durationMs: Date.now() - started, command, browserRelaunches: relaunches(), shards: shardResults(), ...(caseEvidence && {cases: caseEvidence}) };
		}
		const runtimeExtras = caseEvidence ? ["forms", "histogram-legend", "wide-chart", "legend-cues", "numeric-series-names"] : ["forms", "histogram-legend"];
		if (runtime && !same(caseEvidence?.cohort ?? cohort, reportCohort(cwd ?? process.cwd(), { extra: runtimeExtras })))
			throw new Error("Runtime shards do not cover the complete report inventory");
		if (shard.kind === "no-js") {
			const report = mergeNoJsShards(reports as unknown as Parameters<typeof mergeNoJsShards>[0], { cohort: reportCohort(cwd ?? process.cwd()), engine: env?.RICHVIEW_VERIFY_BROWSER ?? "chromium" });
			writeFileSync(canonical, JSON.stringify({ ...report, seconds: (Date.now() - started) / 1000, cohort_sha256: createHash("sha256").update(JSON.stringify(cohort)).digest("hex") }, null, 2) + "\n");
			const bad = outcomes.some(outcome => outcome.status !== "pass") || report.failures.length;
			appendFileSync(logPath, `\nrv-gate: ${cohort.length} reports completed every no-JS mode at both widths\n`);
			return { status: bad ? "fail" : "pass", exitCode: bad ? 1 : 0, durationMs: Date.now() - started, command, browserRelaunches: relaunches(), shards: shardResults(), ...(caseEvidence && {cases: caseEvidence}) };
		}
		const results: ShardResult[] = [];
		for (const report of reports) {
			const dimensions: (keyof ShardReport)[] = runtime ? ["browser", "engine"] : ["browser", "style", "width", "model_count"];
			if (dimensions.some(key => report[key] !== reports[0][key]) || !runtime && report.model_count !== (caseEvidence ? caseEvidence.cohort.length : cohort.length))
				throw new Error("Shard reports disagree on browser, style, viewport or model count");
			keysOf(report.results?.map(result => result.caseKey), "Shard result cases");
			if (!same([...report.results.map(result => result.caseKey)].sort(), [...report.completed].sort()))
				throw new Error("Shard completed cases lack matching assertion results");
			if (runtime) {
				if (typeof report.browser !== "string" || !report.browser.length || report.engine !== (env?.RICHVIEW_VERIFY_BROWSER ?? "chromium"))
					throw new Error("Runtime shard browser or engine does not match the requested run");
				for (const result of report.results) {
					if (!Array.isArray(result.views) || !same(result.views.map(view => view.width).sort((a: number, b: number) => a - b), [...RUNTIME_WIDTHS].sort((a, b) => a - b)) || result.views.some(view => view.name !== result.caseKey))
						throw new Error(`Report ${result.caseKey} did not finish both runtime widths`);
				}
				if (!Array.isArray(report.cases) || !same(sortedViews(report.cases), sortedViews(report.results.flatMap(result => result.views))))
					throw new Error("Runtime view evidence does not match the completed reports");
			}
			results.push(...report.results);
		}
		const order = new Map(cohort.map((key, index) => [key, index]));
		results.sort((a, b) => order.get(a.caseKey)! - order.get(b.caseKey)!);
		const errors = reports.flatMap(report => report.errors ?? []);
		const failures = runtime ? reports.flatMap(report => report.failures ?? []) : [];
		const failed = results.filter(result => !result.ok).length;
		const seconds = (Date.now() - started) / 1000;
		writeFileSync(canonical, JSON.stringify({
			...reports[0], seconds,
			...(runtime ? { cases: sortedViews(results.flatMap(result => result.views)), failures } : { charts: results.length, types: new Set(results.map(result => result.type)).size }),
			failed, errors, results, completed: cohort, shard: { index: 0, count: 1, cohort, selected: cohort },
			cohort_sha256: createHash("sha256").update(JSON.stringify(cohort)).digest("hex"),
		}, null, 2) + "\n");
		const bad = outcomes.some(outcome => outcome.status !== "pass") || failed || errors.length || failures.length;
		appendFileSync(logPath, `\nrv-gate: ${cohort.length} cases completed across ${count} shards; ${failed} failed\n`);
		return { status: bad ? "fail" : "pass", exitCode: bad ? 1 : 0, durationMs: Date.now() - started, command, browserRelaunches: relaunches(), shards: shardResults(), ...(caseEvidence && {cases: caseEvidence}) };
	} catch (error) {
		appendFileSync(logPath, `\nrv-gate: shard evidence failed: ${(error as Error).message}\n`);
		return { status: "fail", exitCode: 1, durationMs: Date.now() - started, command, browserRelaunches: relaunches(), shards: shardResults(), ...(caseEvidence && {cases: caseEvidence}) };
	}
}

function sortedViews(views: readonly RuntimeView[]) {
	return [...views].sort((a, b) => a.name.localeCompare(b.name) || b.width - a.width);
}
