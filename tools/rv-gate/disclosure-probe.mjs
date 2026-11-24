// The disclosure-probe step of the release run (DEL-80, FEA-CH-37): every example renders once
// with each measured value written to nine significant digits, and no number in the HTML a
// reader's browser parses (embedded chart data and attributes included) may round one of those
// values at more than four digits. Row positions on a grid refined past the base step count are
// checked by TYP-36's rule instead: each is its row's shown value or a base step, which can sit on
// a source value's fifth digit by chance (refinedPositions). The cross-style step proves the author's precision across
// styles over its own fixture; this one reaches every chart type's payload and derived records,
// through examples/every-block, and the scales the other examples build, where a table's sort
// keys and a contour's grid (S27-830, S27-831) and a refined grid's positions (S27-832) once
// carried source digits.
//
// Values written finer: the numbers in the rows under a block's `inline:` data, in the report's
// CSV files and under the keys of its JSON data files, and a metric's `value` and `delta`. Never
// keys that place or name a mark (PLACING: dates, coordinates, identifiers), nor author settings
// such as style bands or axis domains. An example that fails to render afterwards fails the step,
// so the probe keeps covering every type; the examples that read a live connector (CONNECTED)
// cannot render on a gate host and are skipped. A passing example's folder is deleted after its
// scan; a failing one's stays for inspection.
//
// Usage: node tools/rv-gate/disclosure-probe.mjs --bin target/release/richview --out <dir> [--cwd <repo>] [--only <example>]

import { execFileSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { locate, scanDisclosure, scannableHtml } from "./cross-style.mjs";

export const INPUT = "examples";
/** Examples that read a live connector, which no gate host configures, and why. */
export const CONNECTED = new Map([
	["metricflow-report", "reads a MetricFlow runtime from connectors.toml"],
	["parquet-report", "reads a Parquet file its seed script writes"],
	["sql-report", "reads a registered SQL connector"],
]);
/** Significant digits a reader may see at most (FEA-CH-37: three, four with a scale word). */
export const LIMIT = 4;
/** Keys and columns that place or name a mark rather than measure it: kept as written. */
export const PLACING = new Set(["month", "year", "date", "day", "hour", "weekday", "week", "quarter", "call", "id", "rank", "step", "order", "lat", "lon", "x", "y", "row", "col", "column", "index", "frame", "start", "end", "level", "depth"]);

/** `text` as a number written to nine significant digits, recorded in `values`; zero stays. */
function finer(text, values) {
	const v = Number(text);
	if (!Number.isFinite(v) || v === 0) return text;
	const written = Number((v * (1 + 0.000123457)).toPrecision(9));
	values.push(written);
	return String(written);
}

/**
 * The report source with its measured values written finer: rows under an `inline:` key of a
 * chart, table or metric fence, and a metric's `value` and `delta`. Returns `{ text, values }`.
 */
export function finerReport(source) {
	const values = [];
	const lines = source.split("\n");
	let fence = null;
	let keys = [];
	for (let i = 0; i < lines.length; i++) {
		const line = lines[i];
		const open = line.match(/^```(chart|table|metric)\b/);
		if (!fence && open) {
			fence = open[1];
			keys = [];
			continue;
		}
		if (fence && /^```\s*$/.test(line)) {
			fence = null;
			continue;
		}
		if (!fence) continue;
		const indent = line.length - line.trimStart().length;
		const key = line.match(/^\s*([\w-]+):\s*$/);
		if (key) {
			keys = keys.filter((k) => k.indent < indent).concat({ indent, name: key[1] });
			continue;
		}
		// A row may sit at its key's own indent, a block sequence YAML allows (`inline:` then `- {…}`).
		const row = /^\s*- \{.*\}\s*$/.test(line);
		const parent = keys.filter((k) => k.indent <= indent).at(-1);
		if (row && parent?.name === "inline") {
			lines[i] = line.replace(/\b([\w-]+): (-?\d+(?:\.\d+)?)(?=\s*[,}])/g, (all, name, num) => (PLACING.has(name.toLowerCase()) ? all : `${name}: ${finer(num, values)}`));
		} else if (fence === "metric") {
			lines[i] = line.replace(/^(value|delta): (-?\d+(?:\.\d+)?)\s*$/, (all, name, num) => `${name}: ${finer(num, values)}`);
		}
	}
	return { text: lines.join("\n"), values };
}

/** A CSV file with its measured columns written finer; a line with quoted cells stays as written. */
export function finerCsv(csv, values) {
	const rows = csv.replace(/\n$/, "").split("\n");
	const head = rows[0].split(",").map((c) => c.trim());
	const measured = head.map((name) => !PLACING.has(name.toLowerCase()));
	for (let r = 1; r < rows.length; r++) {
		if (rows[r].includes('"')) continue;
		rows[r] = rows[r]
			.split(",")
			.map((cell, k) => (measured[k] && /^\s*-?\d+(?:\.\d+)?\s*$/.test(cell) ? finer(cell.trim(), values) : cell))
			.join(",");
	}
	return `${rows.join("\n")}\n`;
}

/** A JSON data file with the numbers under its measuring keys written finer; numbers in arrays stay. */
export function finerJson(text, values) {
	const walk = (v) => {
		if (Array.isArray(v)) return v.map(walk);
		if (!v || typeof v !== "object") return v;
		return Object.fromEntries(Object.entries(v).map(([k, x]) => [k, typeof x === "number" && !PLACING.has(k.toLowerCase()) ? Number(finer(String(x), values)) : walk(x)]));
	};
	return `${JSON.stringify(walk(JSON.parse(text)), null, 1)}\n`;
}

/** Steps of a base grid (QUANT_STEPS in richview-chart/src/payload.rs); a grid with more is refined. */
export const BASE_STEPS = 4095;

/**
 * The row positions of a chart payload that say more than TYP-36 allows, and the payload without
 * the positions it checked. On a grid refined past BASE_STEPS steps a position is the row's shown
 * value or one of the base grid's steps, to within half a refined step. The digit scan does not
 * read these positions: a base step can sit on a source value's fifth digit by chance.
 */
export function refinedPositions(payload) {
	const violations = [];
	const rows = (payload.rows ?? []).map((row) => (Array.isArray(row) ? [...row] : row));
	for (const [column, grid] of Object.entries(payload.quant ?? {})) {
		const at = (payload.columns ?? []).indexOf(column);
		if (!(grid.steps > BASE_STEPS) || at < 0) continue;
		const span = grid.max - grid.min;
		const step = span / grid.steps;
		const base = span / BASE_STEPS;
		rows.forEach((row, r) => {
			const k = Array.isArray(row) ? row[at] : undefined;
			if (typeof k !== "number") return;
			const position = grid.min + (k * span) / grid.steps;
			const shown = payload.shown?.[column]?.[r];
			const tolerance = 1e-9 * Math.max(Math.abs(position), step);
			const isShown = typeof shown === "number" && Math.abs(position - shown) <= tolerance;
			const nearest = grid.min + Math.round((position - grid.min) / base) * base;
			if (!isShown && Math.abs(position - nearest) > step / 2 + tolerance) violations.push({ column, row: r, position, shown });
			row[at] = null;
		});
	}
	return { violations, payload: { ...payload, rows } };
}

/**
 * The numbers in `html` that round a value of `values` finer than LIMIT, and the refined-grid row
 * positions finer than TYP-36 allows (`kind: "position"`); media bytes are not read.
 */
export function disclosures(html, values) {
	const positions = [];
	const rest = html.replace(/(<script[^>]*data-rv-payload[^>]*>)([\s\S]*?)(<\/script>)/gi, (all, open, body, close) => {
		let payload;
		try {
			payload = JSON.parse(body);
		} catch {
			return all;
		}
		const checked = refinedPositions(payload);
		for (const v of checked.violations) positions.push({ kind: "position", raw: String(v.position), source: v.shown, where: `rows of ${v.column}, row ${v.row}` });
		return open + JSON.stringify(checked.payload).replace(/</g, "\\u003c") + close;
	});
	const text = scannableHtml(rest).replace(/data:[^"')\s]*;base64,[^"')\s]+/g, " ");
	return positions.concat(scanDisclosure(text, values, LIMIT).map((found) => ({ ...found, where: locate(text, found.raw) })));
}

/** One example copied to `folder`, written finer, rendered and scanned. */
function probeExample(source, folder, bin) {
	rmSync(folder, { recursive: true, force: true });
	cpSync(source, folder, { recursive: true });
	const report = finerReport(readFileSync(join(folder, "report.rmd"), "utf8"));
	writeFileSync(join(folder, "report.rmd"), report.text);
	const values = report.values;
	const data = join(folder, "data");
	for (const file of existsSync(data) ? readdirSync(data) : []) {
		const path = join(data, file);
		if (file.endsWith(".csv")) writeFileSync(path, finerCsv(readFileSync(path, "utf8"), values));
		else if (file.endsWith(".json")) {
			let text;
			try {
				text = finerJson(readFileSync(path, "utf8"), values);
			} catch {
				continue;
			}
			writeFileSync(path, text);
		}
	}
	let result;
	try {
		result = JSON.parse(execFileSync(bin, ["generate", "report.rmd", "-o", "probe.html", "--format", "json"], { cwd: folder, encoding: "utf8", maxBuffer: 1 << 28, stdio: ["ignore", "pipe", "pipe"] }));
	} catch (error) {
		try {
			result = JSON.parse(error.stdout || "{}");
		} catch {
			result = { diagnostics: [{ level: "error", code: "cli", line: 0, message: String(error.stderr || error.message).slice(0, 300) }] };
		}
	}
	const errors = (result.diagnostics ?? []).filter((d) => d.level === "error").map((d) => ({ code: d.code, line: d.line, message: d.message }));
	const html = join(folder, "Rendered/probe.html");
	const rendered = existsSync(html);
	const found = rendered ? disclosures(readFileSync(html, "utf8"), values) : [];
	return { values: values.length, rendered, errors, disclosures: found };
}

export function runProbe({ cwd, bin, out, only }) {
	mkdirSync(out, { recursive: true });
	const root = join(cwd, INPUT);
	const names = readdirSync(root)
		.filter((name) => existsSync(join(root, name, "report.rmd")) && (!only || name === only))
		.sort();
	const examples = names.map((name) => {
		if (CONNECTED.has(name)) return { name, skipped: CONNECTED.get(name) };
		const folder = join(out, name);
		const result = probeExample(join(root, name), folder, bin);
		const passed = result.rendered && !result.errors.length && !result.disclosures.length;
		if (passed) rmSync(folder, { recursive: true, force: true });
		return { name, passed, ...result };
	});
	const probed = examples.filter((e) => !e.skipped);
	const summary = {
		input: INPUT,
		limit: LIMIT,
		values: probed.reduce((n, e) => n + e.values, 0),
		probed: probed.length,
		skipped: examples.length - probed.length,
		failed: probed.filter((e) => !e.passed).map((e) => e.name),
		examples,
	};
	writeFileSync(join(out, "disclosure-probe.json"), `${JSON.stringify(summary, null, 2)}\n`);
	return summary;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
	const arg = (name) => {
		const at = process.argv.indexOf(name);
		return at < 0 ? undefined : process.argv[at + 1];
	};
	const bin = arg("--bin");
	const out = arg("--out");
	if (!bin || !out) {
		console.error("usage: disclosure-probe.mjs --bin <richview> --out <dir> [--cwd <repo>] [--only <example>]");
		process.exit(2);
	}
	const cwd = resolve(arg("--cwd") ?? process.cwd());
	const summary = runProbe({ cwd, bin: resolve(cwd, bin), out: resolve(out), only: arg("--only") });
	console.log(`disclosure-probe: ${summary.probed} examples with ${summary.values} values written to nine digits (${summary.skipped} skipped: they read a live connector); ${summary.failed.length} failed`);
	for (const e of summary.examples.filter((x) => !x.skipped && !x.passed)) {
		console.log(`  ${e.name}: ${e.rendered ? "rendered" : "did not render"}, ${e.errors.length} errors, ${e.disclosures.length} disclosures`);
		for (const x of e.errors.slice(0, 5)) console.log(`    error ${x.code} at line ${x.line}: ${x.message}`);
		for (const d of e.disclosures.slice(0, 5)) console.log(d.kind === "position" ? `    ${d.raw} is finer than its shown ${d.source} and the base grid (${d.where})` : `    ${d.raw} rounds ${d.source} at ${d.sig} digits (${d.where})`);
	}
	process.exit(summary.probed > 0 && !summary.failed.length ? 0 : 1);
}
