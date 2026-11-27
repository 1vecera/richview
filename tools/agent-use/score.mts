#!/usr/bin/env node
// Scores one agent run (FEA-SF-65, INT-69). Pure functions over a normalized transcript, the
// sandbox's report and the task's facts, plus a CLI that scores a kept run folder.
//
// It measures RichView's agent surfaces. It judges nothing about the story (INT-77), and the
// report's length is neither rewarded nor penalized (INT-13): no field below reads prose.

import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { evaluateFacts } from "./lib/facts.mts";
import { readReport } from "./lib/rmd.mts";
import { parseCheck, parseJsonl, readTranscript } from "./lib/transcript.mts";

import type { Diagnostic, ToolCall, TranscriptResult, CheckOutput } from "./lib/transcript.mts";
import type { RunMetadata } from "./history.mts";
import type { Report } from "./lib/rmd.mts";
import type { Task } from "./lib/facts.mts";

export type Score = ReturnType<typeof scoreRun>;
type Attempt = ReturnType<typeof attemptsOf>[number];
type Repeated = {code: string; level: string; attempts: number[]; fixAvailable: boolean};
type ScoreInput = {calls: ToolCall[]; result?: TranscriptResult | null; report: Report; finalCheck?: CheckOutput | null; task: Task; taskDir: string};

/** Points: a clean final check 40, its attempt 20 (first) or 10 (second), the brief's facts 40. */
export const WEIGHTS = { clean: 40, attempt1: 20, attempt2: 10, facts: 40 };
/** INT-69's bar: clean by the second attempt, at least three chart types, a table. */
export const BAR = { maxAttempt: 2, minChartTypes: 3, minTables: 1 };

const blocking = (d: Diagnostic) => d.level === "error" || d.level === "warning";

/** The check attempts in order, each with what the agent saw and whether it edited since the last. */
export function attemptsOf(calls: ToolCall[]) {
	const attempts = [];
	let edited = false;
	for (const call of calls) {
		if (call.kind === "check" && parseCheck(call).diagnostics.some((d) => d.code === "report.root.missing")) continue; // checked before saving anything: not a draft
		if (call.kind === "edit") edited = true;
		else if (call.kind === "shell" && /(?:>|tee|sed -i|mv|cp)\s/.test(String((call.input as Record<string, unknown>)?.command ?? ""))) edited = true;
		if (call.kind !== "check") continue;
		attempts.push({ n: attempts.length + 1, via: call.via, editedSincePrevious: attempts.length === 0 || edited, ...parseCheck(call) });
		edited = false;
	}
	return attempts;
}

/** Codes that came back after the agent edited (misread) or without any edit (not acted on). */
export function misreads(attempts: Attempt[]) {
	const misread: Repeated[] = [];
	const ignored: Repeated[] = [];
	for (let i = 1; i < attempts.length; i++) {
		const before = attempts[i - 1];
		const after = attempts[i];
		if (!before.visible || !after.visible) continue;
		const again = new Set(after.diagnostics.filter(blocking).map((d) => d.code));
		for (const d of before.diagnostics.filter(blocking)) {
			if (!again.has(d.code)) continue;
			const entry = { code: d.code, level: d.level, attempts: [before.n, after.n], fixAvailable: d.fix };
			(after.editedSincePrevious ? misread : ignored).push(entry);
		}
	}
	const uniq = (list: Repeated[]) => list.filter((e, i) => list.findIndex((o) => o.code === e.code) === i);
	return { misread: uniq(misread), ignored: uniq(ignored) };
}

const SOURCE_READ = /richview-v3|crates\/richview|richview-chart\/|\.rs\b|docs\/lockdown/;

/**
 * Scores a run. `finalCheck` is the runner's own `richview check` of the kept report folder
 * (`{output}` as the CLI printed it); without one the agent's last check stands in.
 */
export function scoreRun({ calls, result = null, report, finalCheck = null, task, taskDir }: ScoreInput) {
	const attempts = attemptsOf(calls);
	const firstClean = attempts.find((a) => a.clean)?.n ?? null;
	const final = finalCheck ? parseCheck(finalCheck) : (attempts.at(-1) ?? null);
	const finalClean = Boolean(final?.clean) && report.exists;
	const lastCheckIndex = calls.findLastIndex((c) => c.kind === "check");
	const editedAfterLastCheck = lastCheckIndex >= 0 && calls.slice(lastCheckIndex + 1).some((c) => c.kind === "edit");

	const met: Record<string, {level: string; message: string; count: number; attempts: number[]; fixShown: boolean}> = {};
	for (const a of attempts) {
		for (const d of a.diagnostics) {
			const e = (met[d.code] ??= { level: d.level, message: d.message ?? "", count: 0, attempts: [], fixShown: false });
			e.count++;
			if (!e.attempts.includes(a.n)) e.attempts.push(a.n);
			e.fixShown ||= d.fix;
		}
	}
	const { misread, ignored } = misreads(attempts);
	const unresolved = (final?.diagnostics ?? []).filter(blocking).map((d) => d.code);

	const facts = report.exists ? evaluateFacts(task, report, taskDir) : { items: [], passed: 0, total: 0, fraction: 0, chartTypes: [] };
	const byKind: Record<string, number> = {};
	for (const c of calls) byKind[c.kind] = (byKind[c.kind] ?? 0) + 1;
	const readSource = calls.filter((c) => (c.kind === "read" || c.kind === "shell") && SOURCE_READ.test(JSON.stringify(c.input ?? {}))).length;

	const attemptPoints = firstClean === 1 ? WEIGHTS.attempt1 : firstClean === 2 ? WEIGHTS.attempt2 : 0;
	const points = {
		clean: finalClean ? WEIGHTS.clean : 0,
		attempt: finalClean ? attemptPoints : 0,
		facts: Math.round(WEIGHTS.facts * facts.fraction * 100) / 100,
	};
	const score = Math.round((points.clean + points.attempt + points.facts) * 100) / 100;
	const chartTypes = facts.chartTypes.length;
	const bar = finalClean && firstClean !== null && firstClean <= BAR.maxAttempt && chartTypes >= BAR.minChartTypes && report.tables.length >= BAR.minTables && readSource === 0;
	const pass = bar && facts.items.every((i) => i.ok);

	return {
		score,
		points,
		pass,
		bar,
		clean: { final: finalClean, firstAttempt: firstClean, attempts: attempts.length, editedAfterLastCheck },
		attempts: attempts.map(({ n, via, ok, clean, count, visible, editedSincePrevious }) => ({ n, via, ok, clean, count, visible, editedSincePrevious })),
		toolCalls: calls.length,
		toolErrors: calls.filter((c) => c.isError).length,
		toolCallsByKind: byKind,
		diagnosticsMet: met,
		misread,
		ignored,
		unresolved: [...new Set(unresolved)],
		invisibleDiagnostics: attempts.filter((a) => !a.visible).length,
		readSource,
		facts,
		agent: result && { turns: result.turns, costUsd: result.costUsd, durationMs: result.durationMs, crashed: result.isError },
	};
}

/** Scores a kept run folder (`run.json`, `transcript.jsonl`, `final-check.txt`, `sandbox/`). */
export function scoreRunFolder(runDir: string, { tasksDir = join(fileURLToPath(new URL(".", import.meta.url)), "tasks") } = {}) {
	const meta = JSON.parse(readFileSync(join(runDir, "run.json"), "utf8")) as RunMetadata;
	const taskDir = join(tasksDir, meta.task);
	const task = JSON.parse(readFileSync(join(taskDir, "task.json"), "utf8")) as Task;
	const { calls, result } = readTranscript(parseJsonl(readFileSync(join(runDir, "transcript.jsonl"), "utf8")), meta.product);
	const finalPath = join(runDir, "final-check.txt");
	const finalCheck = existsSync(finalPath) ? { output: readFileSync(finalPath, "utf8") } : null;
	const report = readReport(join(runDir, "sandbox", "report"));
	return { meta, ...scoreRun({ calls, result, report, finalCheck, task, taskDir }) };
}

if (process.argv[1] && (resolve(process.argv[1]) === fileURLToPath(import.meta.url) ||
	resolve(process.argv[1]) === fileURLToPath(import.meta.url.replace(/\.mts$/, ".mjs")))) {
	const dir = process.argv[2];
	if (!dir) {
		console.error("usage: score.mjs <run-folder> [--write]");
		process.exit(2);
	}
	const scored = scoreRunFolder(resolve(dir));
	if (process.argv.includes("--write")) writeFileSync(join(resolve(dir), "score.json"), `${JSON.stringify(scored, null, 2)}\n`);
	console.log(JSON.stringify(scored, null, 2));
}
