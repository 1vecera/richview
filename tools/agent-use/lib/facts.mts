// The brief's checkable facts (FEA-SF-65): required chart types and tables, figures that must
// match the dataset, and a source note on every chart. Figures are computed from the task's own
// data files and looked up in the report's chart, table and big-number data, never in prose.

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { parseCsv } from "./rmd.mts";

import type { DataRow, Report } from "./rmd.mts";

type Formula = {op: string; a: Computation; b: Computation};
type Aggregate = {op?: never; file: string; where?: DataRow; column: string; agg?: string};
type Computation = Formula | Aggregate;
type Tolerance = {relative?: number; absolute?: number};
export type Task = {brief?: string; checks?: {chartTypes?: {required?: string[]; minDistinct?: number}; tables?: {min?: number}; sources?: boolean; figures?: {id: string; compute: Computation; scales?: number[]; key?: string; tolerance?: Tolerance; describe?: string}[]}};
type Fact = {id: string; kind: string; ok: boolean; detail: string; taskError?: boolean; expected?: number};

function load(taskDir: string, file: string): DataRow[] {
	const text = readFileSync(join(taskDir, file), "utf8");
	if (file.endsWith(".json")) return JSON.parse(text);
	return parseCsv(text);
}

const same = (cell: unknown, want: unknown) => String(cell).trim().toLowerCase() === String(want).trim().toLowerCase();

/**
 * Evaluates a computation over a task data file. `{file, where, column, agg}` aggregates the
 * matching rows (sum, mean, min, max, count, first, last); `{op, a, b}` combines two of them
 * (`diff` a-b, `ratio` a/b, `share` a/b*100, `pctChange` from a to b in percent).
 */
export function compute(spec: Computation, taskDir: string): number {
	if (spec.op) {
		const a = compute(spec.a, taskDir);
		const b = compute(spec.b, taskDir);
		switch (spec.op) {
			case "diff":
				return a - b;
			case "ratio":
				return a / b;
			case "share":
				return (a / b) * 100;
			case "pctChange":
				return ((b - a) / a) * 100;
			default:
				throw new Error(`unknown op ${spec.op}`);
		}
	}
	const aggregate = spec as Aggregate;
	const rows = load(taskDir, aggregate.file).filter((row) => Object.entries(aggregate.where ?? {}).every(([k, v]) => (Array.isArray(v) ? v.some((x) => same(row[k], x)) : same(row[k], v))));
	if (!rows.length) throw new Error(`no rows in ${aggregate.file} for ${JSON.stringify(aggregate.where)}`);
	const values = rows.map((row) => row[aggregate.column]).filter((v) => typeof v === "number");
	switch (aggregate.agg ?? "sum") {
		case "sum":
			return values.reduce((s, v) => s + v, 0);
		case "mean":
			return values.reduce((s, v) => s + v, 0) / values.length;
		case "min":
			return Math.min(...values);
		case "max":
			return Math.max(...values);
		case "count":
			return rows.length;
		case "first":
			return values[0];
		case "last":
			return values.at(-1)!;
		default:
			throw new Error(`unknown agg ${aggregate.agg}`);
	}
}

/** Every structured value a reader can see: chart and table rows, and big-number values. */
function observations(report: Report) {
	const out = [];
	for (const [kind, list] of [["chart", report.charts], ["table", report.tables]] as const) {
		for (const block of list) {
			for (const row of block.rows ?? []) {
				const cells = Object.values(row);
				out.push({
					kind,
					line: block.line,
					text: cells.filter((c) => typeof c === "string").map((c) => c.toLowerCase()),
					numbers: cells.map((c) => (typeof c === "number" ? c : typeof c === "string" && /^[-+]?[\d.]+$/.test(c.trim()) ? Number(c) : null)).filter((n) => n !== null),
				});
			}
		}
	}
	for (const big of report.bigs) {
		const n = typeof big.value === "number" ? big.value : Number(String(big.value ?? "").replace(/[^\d.+-]/g, ""));
		if (Number.isFinite(n) && big.value !== null) out.push({ kind: "big", line: big.line, text: [String(big.label ?? "").toLowerCase()], numbers: [n] });
	}
	return out;
}

function close(found: number, expected: number, tolerance?: Tolerance) {
	const rel = tolerance?.relative ?? 0.01;
	const abs = tolerance?.absolute ?? 0;
	return Math.abs(found - expected) <= Math.max(abs, rel * Math.abs(expected));
}

/** Scores a report against `task.checks`; returns every item with its outcome. */
export function evaluateFacts(task: Task, report: Report, taskDir: string) {
	const checks = task.checks ?? {};
	const items: Fact[] = [];
	const types = [...new Set(report.charts.map((c) => c.type).filter(Boolean))];

	for (const type of checks.chartTypes?.required ?? []) items.push({ id: `chart:${type}`, kind: "chartType", ok: types.includes(type), detail: `a ${type} chart` });
	const minDistinct = checks.chartTypes?.minDistinct ?? 0;
	if (minDistinct) items.push({ id: "chart:distinct", kind: "chartType", ok: types.length >= minDistinct, detail: `${types.length} of ${minDistinct} distinct chart types (${types.join(", ") || "none"})` });

	const minTables = checks.tables?.min ?? 0;
	if (minTables) items.push({ id: "tables", kind: "table", ok: report.tables.length >= minTables, detail: `${report.tables.length} of ${minTables} tables` });

	if (checks.sources) {
		const unsourced = report.charts.filter((c) => !c.source);
		items.push({ id: "sources", kind: "source", ok: report.charts.length > 0 && unsourced.length === 0, detail: `${report.charts.length - unsourced.length} of ${report.charts.length} charts name a source${unsourced.length ? ` (none on line ${unsourced.map((c) => c.line).join(", ")})` : ""}` });
	}

	const seen = observations(report);
	for (const figure of checks.figures ?? []) {
		let expected;
		try {
			expected = compute(figure.compute, taskDir);
		} catch (e) {
			items.push({ id: `figure:${figure.id}`, kind: "figure", ok: false, detail: `task error: ${(e as Error).message}`, taskError: true });
			continue;
		}
		const wanted = [expected, ...(figure.scales ?? []).map((s) => expected * s)];
		const key = figure.key?.toLowerCase();
		const pool = seen.filter((o) => !key || o.text.some((t) => t.includes(key)));
		const hit = pool.find((o) => o.numbers.some((n) => wanted.some((w) => close(n, w, figure.tolerance))));
		items.push({
			id: `figure:${figure.id}`,
			kind: "figure",
			ok: Boolean(hit),
			expected,
			detail: `${figure.describe ?? figure.id}: expected ${Number(expected.toPrecision(6))}${key ? ` on a row naming "${figure.key}"` : ""}${hit ? `, found in ${hit.kind} at line ${hit.line}` : ", not found in any chart, table or big number"}`,
		});
	}
	const passed = items.filter((i) => i.ok).length;
	return { items, passed, total: items.length, fraction: items.length ? passed / items.length : 1, chartTypes: types };
}
