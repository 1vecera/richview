// Library-comparison lab corpus (ARC-64): reads the chart blocks of examples/every-block/report.rmd
// and their data rows, the same-data inputs of the comparison. Development tooling only (ARC-03).
// Needs the `yaml` package from the evidence install, never from the repository.

import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';

export type Cell = string | number | null | boolean;
export type Row = Record<string, Cell | undefined>;

type Field = 'x' | 'y' | 'z' | 'color' | 'size' | 'label' | 'detail' | 'facet' | 'upper'
	| 'lower' | 'target' | 'parent' | 'source' | 'line' | 'sets' | 'open' | 'low' | 'high'
	| 'close' | 'start' | 'measure' | 'end' | 'angle';
export type Encoding = Partial<Record<Field, string>> & { dimensions?: string[] };

export interface ChartBlock {
	type: string;
	title?: string;
	subtitle?: string;
	encoding?: Encoding;
	data?: { csv?: string; inline?: Row[] };
	axes?: Record<string, { label?: string; unit?: string; format?: string } | undefined>;
	annotations?: { kind: string; at?: { y?: Cell }; text?: string }[];
	stack?: boolean;
	binning?: { count?: number; measure?: string };
	style?: { min?: number; max?: number; bands?: { to: number; label?: string }[]; ranges?: number[] };
}

interface Yaml {
	parse(text: string): unknown;
	stringify(value: unknown, options?: { lineWidth: number }): string;
}

export interface ChartCase {
	id: string;
	type: string;
	block: ChartBlock;
	rows: Row[];
	csv: string | null;
}

/** The yaml package, resolved from the evidence install's reference folder. */
export function loadYaml(referenceDir: string): Yaml {
	return createRequire(path.join(referenceDir, 'package.json'))('yaml') as Yaml;
}

/** Rows of a CSV text (RFC 4180 quoting, comma delimiter), numbers typed, leading-zero ids kept as text. */
export function parseCsv(text: string): Row[] {
	const rows: string[][] = [];
	let row: string[] = [];
	let cell = '';
	let quoted = false;
	const src = text.replace(/^﻿/, '');
	for (let i = 0; i < src.length; i++) {
		const c = src[i];
		if (quoted) {
			if (c === '"' && src[i + 1] === '"') {
				cell += '"';
				i++;
			} else if (c === '"') quoted = false;
			else cell += c;
		} else if (c === '"') quoted = true;
		else if (c === ',') {
			row.push(cell);
			cell = '';
		} else if (c === '\n' || c === '\r') {
			if (c === '\r' && src[i + 1] === '\n') i++;
			row.push(cell);
			rows.push(row);
			row = [];
			cell = '';
		} else cell += c;
	}
	if (cell !== '' || row.length) {
		row.push(cell);
		rows.push(row);
	}
	const [head, ...body] = rows.filter((r) => r.some((x) => x.trim() !== ''));
	const typed = (v: string) => {
		const t = v.trim();
		if (t === '') return null;
		return /^-?(0|[1-9]\d*)(\.\d+)?$/.test(t) ? Number(t) : t;
	};
	return body.map((r) => Object.fromEntries(head.map((h, i) => [h.trim(), typed(r[i] ?? '')])));
}

/**
 * The chart cases of a report: one per chart fence, id = type, later repeats of a type get -2, -3.
 * Each case is { id, type, block, rows, csv } where csv is the data path relative to the report.
 */
export function readCases(rmdPath: string, yaml: Yaml): ChartCase[] {
	const dir = path.dirname(rmdPath);
	const text = fs.readFileSync(rmdPath, 'utf8');
	const seen = new Map<string, number>();
	const cases = [];
	for (const m of text.matchAll(/```chart\n([\s\S]*?)```/g)) {
		// These fences are the validated examples/every-block corpus, parsed by the local reference install.
		const block = yaml.parse(m[1]) as ChartBlock;
		const n = (seen.get(block.type) ?? 0) + 1;
		seen.set(block.type, n);
		const csv = block.data?.csv ?? null;
		const rows = csv ? parseCsv(fs.readFileSync(path.join(dir, csv), 'utf8')) : (block.data?.inline ?? []);
		cases.push({ id: n === 1 ? block.type : `${block.type}-${n}`, type: block.type, block, rows, csv });
	}
	return cases;
}
