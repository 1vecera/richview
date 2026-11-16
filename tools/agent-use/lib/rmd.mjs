// Reads a Rich Markdown report folder the way the scorer needs it: the rich blocks, the data
// behind each chart and table (inline rows or a report-local CSV or JSON file), and whether each
// chart is followed by a source note. Facts are computed from this data, never from prose.

import { existsSync, readFileSync } from "node:fs";
import { dirname, join, resolve, sep } from "node:path";
import { parseYaml } from "./yaml.mjs";

/** Parses RFC 4180 CSV into row objects; numbers stay numbers, empty cells are null. */
export function parseCsv(text) {
	const rows = [];
	let row = [];
	let cell = "";
	let quoted = false;
	const src = text.replace(/^﻿/, "");
	for (let i = 0; i < src.length; i++) {
		const c = src[i];
		if (quoted) {
			if (c === '"' && src[i + 1] === '"') {
				cell += '"';
				i++;
			} else if (c === '"') quoted = false;
			else cell += c;
		} else if (c === '"') quoted = true;
		else if (c === ",") {
			row.push(cell);
			cell = "";
		} else if (c === "\n" || c === "\r") {
			if (c === "\r" && src[i + 1] === "\n") i++;
			row.push(cell);
			cell = "";
			if (row.some((v) => v !== "")) rows.push(row);
			row = [];
		} else cell += c;
	}
	if (cell !== "" || row.length) {
		row.push(cell);
		rows.push(row);
	}
	if (!rows.length) return [];
	const header = rows[0].map((h) => h.trim());
	return rows.slice(1).map((r) => Object.fromEntries(header.map((h, i) => [h, cell2value((r[i] ?? "").trim())])));
}

function cell2value(text) {
	if (text === "") return null;
	if (/^[-+]?(\d+\.?\d*|\.\d+)([eE][-+]?\d+)?$/.test(text) && !/^[-+]?0\d/.test(text)) return Number(text);
	return text;
}

/** Splits the source into frontmatter and rich fences, with each fence's line and source note. */
export function parseBlocks(source) {
	const lines = source.replace(/\r\n?/g, "\n").split("\n");
	const blocks = [];
	let i = 0;
	if (lines[0]?.trim() === "---") {
		i = lines.indexOf("---", 1) + 1 || 0;
	}
	const rich = new Set(["chart", "table", "big", "metric", "callout", "image", "embed", "audio", "video"]);
	for (; i < lines.length; i++) {
		const open = lines[i].match(/^(`{3,}|~{3,})\s*([\w-]*)/);
		if (!open) continue;
		const fence = open[1];
		const tag = open[2];
		const start = i;
		const body = [];
		const closes = (line) => new RegExp(`^\\s*${fence[0]}{${fence.length},}\\s*$`).test(line);
		for (i++; i < lines.length && !closes(lines[i]); i++) body.push(lines[i]);
		if (!rich.has(tag)) continue;
		let spec = null;
		let error = null;
		try {
			spec = parseYaml(body.join("\n"));
		} catch (e) {
			error = e.message;
		}
		let j = i + 1;
		while (j < lines.length && lines[j].trim() === "") j++;
		const note = lines[j]?.match(/^\*\s*(Sources?:)\s*(.+?)\*\s*$/);
		blocks.push({ tag, line: start + 1, spec, error, source: note ? note[2].trim() : null });
	}
	return blocks;
}

function readData(folder, data) {
	if (!data || typeof data !== "object") return { rows: [], error: "no data" };
	if (Array.isArray(data.inline)) return { rows: data.inline.filter((r) => r && typeof r === "object") };
	for (const key of ["csv", "json"]) {
		if (typeof data[key] !== "string") continue;
		const path = resolve(folder, data[key]);
		if (!path.startsWith(resolve(folder) + sep) || !existsSync(path)) return { rows: [], error: `missing ${data[key]}` };
		const text = readFileSync(path, "utf8");
		try {
			if (key === "csv") return { rows: parseCsv(text), file: data[key] };
			const parsed = JSON.parse(text);
			return { rows: Array.isArray(parsed) ? parsed : [], file: data[key] };
		} catch (e) {
			return { rows: [], error: e.message };
		}
	}
	return { rows: [], error: "unsupported data" };
}

/** Markdown pipe tables outside fences, as row objects keyed by their header cells. */
export function markdownTables(source) {
	const lines = source.replace(/\r\n?/g, "\n").split("\n");
	const tables = [];
	let fence = null;
	const cells = (line) => line.trim().replace(/^\||\|$/g, "").split("|").map((c) => c.trim());
	for (let i = 0; i < lines.length; i++) {
		const f = lines[i].match(/^\s*(`{3,}|~{3,})/);
		if (f) fence = fence ? (f[1][0] === fence[0] ? null : fence) : f[1];
		if (fence || !/^\s*\|.*\|\s*$/.test(lines[i]) || !/^\s*\|?\s*:?-{2,}:?\s*(\|\s*:?-{2,}:?\s*)*\|?\s*$/.test(lines[i + 1] ?? "")) continue;
		const header = cells(lines[i]);
		const rows = [];
		let j = i + 2;
		for (; j < lines.length && /^\s*\|.*\|\s*$/.test(lines[j]); j++) rows.push(Object.fromEntries(header.map((h, k) => [h, cell2value(cells(lines[j])[k] ?? "")])));
		tables.push({ line: i + 1, rows, markdown: true });
		i = j - 1;
	}
	return tables;
}

/** Everything the scorer needs from `<folder>/report.rmd` (or the folder's only .rmd). */
export function readReport(folder, rmdName = "report.rmd") {
	const path = join(folder, rmdName);
	if (!existsSync(path)) return { exists: false, path, charts: [], tables: [], bigs: [], blocks: [] };
	const source = readFileSync(path, "utf8");
	const blocks = parseBlocks(source);
	const charts = [];
	const tables = [];
	const bigs = [];
	for (const block of blocks) {
		const spec = block.spec ?? {};
		if (block.tag === "chart") charts.push({ line: block.line, type: typeof spec.type === "string" ? spec.type : null, title: spec.title ?? null, source: block.source, ...readData(folder, spec.data) });
		else if (block.tag === "table") tables.push({ line: block.line, ...readData(folder, spec.data) });
		else if (block.tag === "big" || block.tag === "metric") bigs.push({ line: block.line, value: spec.value ?? null, label: spec.label ?? null });
	}
	tables.push(...markdownTables(source));
	return { exists: true, path, source, blocks, charts, tables, bigs, folder: dirname(path) };
}
