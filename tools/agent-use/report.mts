#!/usr/bin/env node
// Prints the score history as one table per task and product over time, and flags drops.
//
//   node tools/agent-use/report.mjs [--task <id>] [--product claude|codex] [--drop 15] [--history <file>]
//
// Agents are not deterministic, so a run is compared with the mean of up to three earlier runs
// of the same task and product. A flag is a prompt to look at the transcript, not a verdict.

import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { historyPath, readHistory } from "./history.mts";

import type { HistoryRow } from "./history.mts";

/** Groups runs by task and product, oldest first, with each run's drop against the recent mean. */
export function series(entries: HistoryRow[], { drop = 15, window = 3 } = {}) {
	const groups = new Map<string, HistoryRow[]>();
	for (const e of [...entries].sort((a, b) => String(a.at).localeCompare(String(b.at)))) {
		const key = `${e.task} / ${e.product}`;
		if (!groups.has(key)) groups.set(key, []);
		groups.get(key)!.push(e);
	}
	return [...groups].map(([key, runs]) => ({
		key,
		runs: runs.map((run, i) => {
			const before = runs.slice(Math.max(0, i - window), i);
			const mean = before.length ? before.reduce((s, r) => s + r.score, 0) / before.length : null;
			const delta = mean === null ? null : Math.round((run.score - mean) * 10) / 10;
			const buildChanged = i > 0 && runs[i - 1].build?.commit !== run.build?.commit;
			return { ...run, delta, buildChanged, flagged: delta !== null && delta <= -drop };
		}),
	}));
}

const pad = (v: unknown, n: number) => String(v ?? "-").slice(0, n).padEnd(n);

export function render(groups: ReturnType<typeof series>) {
	const lines = [];
	for (const { key, runs } of groups) {
		lines.push(`${key}`, `  ${pad("when", 16)} ${pad("build", 9)} ${pad("model", 10)} ${pad("score", 6)} ${pad("pass", 5)} ${pad("clean@", 6)} ${pad("calls", 5)} ${pad("facts", 6)} ${pad("vs recent", 9)} notes`);
		for (const r of runs) {
			const notes = [r.flagged ? "DROP" : "", r.buildChanged ? "new build" : "", r.misread?.length ? `misread ${r.misread.join(",")}` : "", r.facts?.failed?.length ? `failed ${r.facts.failed.join(",")}` : ""].filter(Boolean).join("; ");
			lines.push(`  ${pad(String(r.at ?? "").slice(0, 16).replace("T", " "), 16)} ${pad(r.build?.commit?.slice(0, 8), 9)} ${pad(r.model, 10)} ${pad(r.score, 6)} ${pad(r.pass ? "yes" : "no", 5)} ${pad(r.firstCleanAttempt ?? "never", 6)} ${pad(r.toolCalls, 5)} ${pad(r.facts ? `${r.facts.passed}/${r.facts.total}` : "-", 6)} ${pad(r.delta === null ? "-" : (r.delta > 0 ? "+" : "") + r.delta, 9)} ${notes}`);
		}
		lines.push("");
	}
	return lines.join("\n");
}

if (process.argv[1] && (resolve(process.argv[1]) === fileURLToPath(import.meta.url) ||
	resolve(process.argv[1]) === fileURLToPath(import.meta.url.replace(/\.mts$/, ".mjs")))) {
	const argv = process.argv.slice(2);
	const opt = (name: string) => (argv.includes(name) ? argv[argv.indexOf(name) + 1] : null);
	let entries = readHistory(opt("--history") ?? historyPath());
	if (opt("--task")) entries = entries.filter((e) => e.task === opt("--task"));
	if (opt("--product")) entries = entries.filter((e) => e.product === opt("--product"));
	const groups = series(entries, { drop: Number(opt("--drop") ?? 15) });
	if (!groups.length) console.log(`No runs in ${opt("--history") ?? historyPath()}.`);
	else {
		console.log(render(groups));
		const flagged = groups.flatMap((g) => g.runs.filter((r) => r.flagged).map((r) => `${g.key} at ${r.at} (${r.delta})`));
		if (flagged.length) console.log(`Drops: ${flagged.join("; ")}`);
	}
}
