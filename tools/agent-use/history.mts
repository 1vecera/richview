// The score history: one JSON line per run (FEA-SF-65). It lives in the gate's evidence folder,
// outside every checkout, and is internal: a public claim needs a demonstration (INT-23).

import { appendFileSync, existsSync, mkdirSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join } from "node:path";

import type { Score } from "./score.mts";

export type BuildInfo = {version?: string | null; commit?: string | null};
export type RunMetadata = {
	runId: string; product: string; task: string; model?: string | null; profile?: string | null;
	build?: BuildInfo; binary?: string; reportDir?: string; command?: string[]; runDir: string;
	startedAt?: string; finishedAt?: string; timedOut?: boolean; exitCode?: number | null;
};
export type HistoryRow = {
	at?: string; runId?: string; product: string; task: string; score: number; pass: boolean;
	model?: string | null; profile?: string | null; build?: BuildInfo; points?: Score["points"];
	cleanFinal?: boolean; firstCleanAttempt?: number | null; attempts?: number; toolCalls?: number; toolErrors?: number;
	diagnosticCodes?: string[]; misread?: string[]; ignored?: string[]; unresolved?: string[]; invisibleDiagnostics?: number;
	facts?: {passed: number; total: number; failed: string[]}; runDir?: string; reportDir?: string;
};

export const historyPath = () => process.env.RV_AGENT_USE_HISTORY || join(homedir(), ".local/share/richview-gate/evidence/agent-use/history.jsonl");

/** What goes into the history for a scored run; `meta` is the run's run.json. */
export function historyEntry(meta: RunMetadata, scored: Score): HistoryRow {
	return {
		at: meta.startedAt,
		runId: meta.runId,
		product: meta.product,
		model: meta.model ?? null,
		profile: meta.profile ?? null,
		build: meta.build,
		task: meta.task,
		score: scored.score,
		pass: scored.pass,
		points: scored.points,
		cleanFinal: scored.clean.final,
		firstCleanAttempt: scored.clean.firstAttempt,
		attempts: scored.clean.attempts,
		toolCalls: scored.toolCalls,
		toolErrors: scored.toolErrors,
		diagnosticCodes: Object.keys(scored.diagnosticsMet),
		misread: scored.misread.map((m) => m.code),
		ignored: scored.ignored.map((m) => m.code),
		unresolved: scored.unresolved,
		invisibleDiagnostics: scored.invisibleDiagnostics,
		facts: { passed: scored.facts.passed, total: scored.facts.total, failed: scored.facts.items.filter((i) => !i.ok).map((i) => i.id) },
		runDir: meta.runDir,
		reportDir: meta.reportDir,
	};
}

export function appendHistory(entry: HistoryRow, path = historyPath()) {
	mkdirSync(dirname(path), { recursive: true });
	appendFileSync(path, `${JSON.stringify(entry)}\n`);
}

export function readHistory(path = historyPath()): HistoryRow[] {
	if (!existsSync(path)) return [];
	return readFileSync(path, "utf8")
		.split("\n")
		.filter((l) => l.trim())
		.flatMap((l) => {
			try {
				return [JSON.parse(l) as HistoryRow];
			} catch {
				return [];
			}
		});
}
