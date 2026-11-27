// Per chart type, which FEA-CH-75 evidence one release run shows (DEL-18, DEL-81).
//
// FEA-CH-75 accepts a chart type into `pre-prod`, and into the public gallery, only when the release run
// shows all of its items for that type. The steps write results per example, per report or per chart, not
// per type, so this tool maps each result to the chart types of the example behind it and tabulates every
// item as `pass`, `fail` or `none` (no evidence). Missing evidence counts against a type and is never
// assumed to pass.
//
// Outputs in `--out`:
//   type-evidence.json   per type, per item: status, source file and the failing example or chart
//   type-evidence.md     one row per type
//   accepted.txt.draft   the types with no fail and no missing item, for docs/public/charts/accepted.txt
//
// Where a step writes data per example (contract.json, golden.json, label-truth.json, cross-style.json,
// the chart-review manifest, perf.json, a11y.json, exports/types.json) it is read as data. One source
// exists only as log text and is parsed strictly: the rust-tests log (`$ <target>/deps/<type>_contract-<hash>`
// or cargo's `Running tests/<type>_contract.rs`, followed by its `test result:` line with at least one test run). The exports log's pagination lines can only fail a type. A line
// that does not match is ignored, so a changed format turns into `none`, never into `pass`.
//
// A type passes an item only on evidence about that type: texts judged (label-truth), keys compared
// (cross-style), its own charts in the PDF check (pdf). Missing or vacuous evidence stays `none` with a note.
//
// The chart type list is the registry of richview-core (`--registry`, default
// crates/richview-core/src/registry.rs), united with every type a step reports.
//
// Usage: node tools/rv-gate/type-evidence.mts [--run <run folder>] --out <dir>
//        [--registry <registry.rs>] [--require <item,item,...>] [--lab <benchmarks/compare run folder>]

import { existsSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { evidenceDir } from "./lib/store.mts";

import type { PathLike, PathOrFileDescriptor } from "node:fs";

type Item = {id: string; step: string | null; card: string; title: string; disclosureOnly?: boolean};
type Cell = {status: "none" | "pass" | "fail"; source?: string; failing: string[]; notes: string[]};
type Receipt = {id?: string; commit?: string; at?: string; result?: string; steps?: {id: string; status: string; reason?: string}[]};
type ChartBlock = {type: string; title?: string; start: number; end: number};
type ChartHint = {title?: string; index?: number; chart?: number; line?: number};
type ContractEvidence = {fixtures?: {id: string; kind: string; ok?: boolean; diffs?: {surface?: string; against?: string}[]; surfaces?: {"cli.check"?: {charts?: {chartId: string; chartType: string; checks?: {kind: string; severity: string}[]; failure?: string; summary?: string}[]}}}[]};
type GoldenEvidence = {styles?: string[]; unrun?: {input: string; reason: string}[]; failures?: {input: string; line?: number; style?: string; code?: string}[]};
type GeometryEvidence = {views?: {type?: string | null; report: string; style: string; width: number; chart: string | number; ok?: boolean; failures?: string[]}[]};
type MatrixEvidence = {captures?: {case: string; theme: string; width: number; overflow?: boolean; errors?: unknown[]; charts?: {title?: string; collisions?: unknown[]; clips?: unknown[]}[]}[]};
type CapturesEvidence = {charts: {name: string; type: string}[]};
type CrossStyleEvidence = {inputs?: string[]; problems?: unknown[]; pageErrors?: unknown[]; comparedCharts?: (ChartHint & {input: string; keys: number; cells: number})[]; mismatches?: {input: string; key: string}[]; rounding?: {where: string; shown: string}[]; disclosure?: {where: string; shown: string}[]};
type LabelEvidence = {reports?: string[]; problems?: unknown[]; judged?: {report: string}[]; judgedTexts?: (ChartHint & {report: string; texts: number})[]; findings?: {report: string; text: string; chart: number; title?: string; tableValue?: unknown}[]};
type InteractionEvidence = {results?: {type: string; ok?: boolean; index: number; error?: string}[]; errors?: string[]};
type MotionEvidence = {charts?: {type: string; ok?: boolean; id: string; failures?: string[]}[]};
type A11yEvidence = {pages: number; violations?: number; keyboardFailures?: unknown[]; problems?: unknown[]};
type ExportEvidence = {type?: string; ok?: boolean; index: number; title?: string; a?: boolean; b?: boolean; c?: boolean; labels_tried?: number; largest_raster?: number};
type PerfEvidence = {allTypes?: {entries?: {type: string; index: number; status: string; native_ms?: number; limit_native_ms?: number; mount_ms?: number; limit_mount_ms?: number}[]}; failures?: {name: string; reason?: string; value?: number; limit?: number}[]};
type LabManifest = {richview?: {commit?: string}; types?: {type: string; id: string; libraries?: unknown[]}[]};
type LabVerdict = {verdict: string; summary?: string};
type EvidenceReport = ReturnType<typeof buildEvidence>;

const HERE = dirname(fileURLToPath(import.meta.url));

/** The widths and styles FEA-CH-75 and DEL-27 name for geometry and captures. */
const GEOMETRY_WIDTHS = [320, 390, 1440];
const CAPTURE_WIDTHS = [390, 1440];

/**
 * Every FEA-CH-75 item, with the step that evidences it. An item with `step: null` has no evidence in
 * any release run: it stays a "no evidence" column so the gap is visible, and it is required all the same,
 * so no type is accepted until the release agent waives it with `--require`. Per-type timing is
 * disclosure only under Daniel's 29 October perf decision; the full run still gates overall perf.
 */
export const ITEMS: Item[] = [
	{ id: "checks", step: "contract", card: "FEA-CH-69", title: "The type's charts pass the cross-surface check run on every golden example" },
	{ id: "type-tests", step: "rust-tests", card: "FEA-CH-69", title: "Its <type>_contract test file ran and passed (a check-fires test is not tracked per type)" },
	{ id: "golden", step: "golden", card: "DEL-60", title: "Golden example and docs snippet render in every style without an undeclared warning" },
	{ id: "geometry", step: "geometry", card: "FEA-CH-75, DEL-72", title: "Geometry model holds and the canvas draws it at 320, 390 and 1440 px in every style (the visual matrix adds collisions and clips as failures)" },
	{ id: "cross-style", step: "cross-style", card: "FEA-CH-37, DEL-52", title: "Values at the author's precision across styles and widths" },
	{ id: "label-truth", step: "label-truth", card: "TYP-36, FEA-CH-37", title: "On-chart value texts match the Table" },
	{ id: "interaction", step: "chromium", card: "FEA-CH-39", title: "Keyboard, pointer, and zoom or legend toggle where the policy offers them, per every-block chart (chart-interaction suite)" },
	{ id: "motion", step: "motion", card: "DEL-75", title: "Entrance on a stepped clock per every-block chart: progress, still parts, last frame, reduced motion, no replay (motion step)" },
	{ id: "a11y-summary", step: "contract", card: "FEA-CH-55", title: "Every chart of the type carries an accessible summary" },
	{ id: "a11y", step: "a11y", card: "DEL-58", title: "Accessibility audit and keyboard pass over the every-block fixture" },
	{ id: "pdf", step: "exports", card: "FEA-RP-43", title: "Vector output in the PDF pagination fixtures" },
	{ id: "captures", step: "captures", card: "DEL-27", title: "A chart of the type captured in the every-block fixture per style at 390 and 1440 px" },
	{ id: "library-comparison", step: null, card: "ARC-64, INT-12", title: "A review agent's verdict on the side-by-side sheets of benchmarks/compare against Highcharts and ECharts, for this run's commit (--lab)" },
	{ id: "perf", step: "perf", card: "PRF-07", disclosureOnly: true, title: "Per-type timing disclosure; Daniel's 29 October decision removes it from type acceptance" },
];

/** Read a JSON file, or return undefined when it is missing or not JSON. */
export function readJson<Value = unknown>(path: PathOrFileDescriptor): Value | undefined {
	try {
		return JSON.parse(readFileSync(path, "utf8")) as Value;
	} catch {
		return undefined;
	}
}

/** Read a text file, or return undefined when it is missing. */
function readText(path: PathOrFileDescriptor) {
	try {
		return readFileSync(path, "utf8");
	} catch {
		return undefined;
	}
}

/** The names of a folder's entries, sorted, or an empty list when it is missing. */
function listDir(path: PathLike) {
	try {
		return readdirSync(path).sort();
	} catch {
		return [];
	}
}

/** Read the chart type names out of the text of core's registry. */
export function parseRegistry(text: string) {
	return [...text.matchAll(/ChartType\s*\{\s*name:\s*"([^"]+)"/g)].map((match) => match[1]);
}

/** Remove one level of YAML quotes from a plain scalar, or return undefined for a block scalar. */
function unquote(raw: string) {
	const text = raw.trim();
	if (text.startsWith(">") || text.startsWith("|")) return undefined;
	if (text.startsWith("'") && text.endsWith("'") && text.length >= 2) return text.slice(1, -1).replaceAll("''", "'");
	if (text.startsWith('"') && text.endsWith('"') && text.length >= 2) return text.slice(1, -1).replaceAll('\\"', '"');
	return text;
}

/**
 * List the chart blocks of a Rich Markdown source in order, each with its type, title and the first
 * and last line of its fence. Only top-level `type:` and `title:` lines of a ```chart fence are read.
 */
export function chartBlocks(text: string) {
	const blocks: ChartBlock[] = [];
	let current: Omit<ChartBlock, "type"> & {type?: string} | undefined;
	text.split("\n").forEach((line: string, index: number) => {
		const number = index + 1;
		if (!current) {
			if (/^```chart\s*$/.test(line)) current = { type: undefined, title: undefined, start: number, end: number };
			return;
		}
		if (/^```\s*$/.test(line)) {
			current.end = number;
			if (current.type) blocks.push(current as ChartBlock);
			current = undefined;
			return;
		}
		const type = /^type:\s*(.+)$/.exec(line);
		if (type && current.type === undefined) current.type = unquote(type[1]);
		const title = /^title:\s*(.+)$/.exec(line);
		if (title && current.title === undefined) current.title = unquote(title[1]);
	});
	return blocks;
}

/** Read one folder's report.rmd as chart blocks, or return undefined when the folder has none. */
function blocksOf(dir: string) {
	const text = readText(join(dir, "report.rmd"));
	return text === undefined ? undefined : chartBlocks(text);
}

/** The blocks of an example folder in a step's `inputs/` store, keyed by the example id (`a__b` is `a/b`). */
function inputStore(dir: string) {
	const store = new Map<string, ChartBlock[]>();
	for (const name of listDir(dir)) {
		const blocks = blocksOf(join(dir, name));
		if (blocks) store.set(name.replaceAll("__", "/"), blocks);
	}
	return store;
}

/** The unique chart types of a list of blocks. */
function typesOf(blocks: ChartBlock[]) {
	return [...new Set(blocks.map((block) => block.type))];
}

/**
 * Pick the chart blocks a finding is about: the block whose title equals `title`, else the one at
 * `index`, else the block whose lines contain `line`; with none of these, every block of the example,
 * because a finding that cannot be placed counts against all of them.
 */
export function blocksFor(blocks: ChartBlock[], { title, index, line }: ChartHint = {}) {
	if (title !== undefined) {
		const byTitle = blocks.filter((block) => block.title === title);
		if (byTitle.length) return byTitle;
	}
	if (line !== undefined) {
		const byLine = blocks.filter((block: { start: number; end: number; }) => line >= block.start && line <= block.end);
		if (byLine.length) return byLine;
	}
	if (Number.isInteger(index) && blocks[index!]) return [blocks[index!]];
	return blocks;
}

/** An evidence table: per type, per item, a status, its sources and the failing examples or charts. */
export class Table {
	declare items: Item[];
	declare cells: Map<string, Map<string, Cell>>;
	declare unattributed: {item: string; source: string; text: string}[];

	/** Create an empty table for the given items. */
	constructor(items: Item[]) {
		this.items = items;
		this.cells = new Map();
		this.unattributed = [];
	}

	/** Get the cell for a type and item, creating it as `none`. */
	cell(type: string, item: string) {
		if (!this.cells.has(type)) this.cells.set(type, new Map());
		const row = this.cells.get(type)!;
		if (!row.has(item)) row.set(item, { status: "none", source: undefined, failing: [], notes: [] });
		return row.get(item)!;
	}

	/** Record a pass for a type; a fail already recorded stays a fail. */
	pass(type: string, item: string, source: string) {
		const cell = this.cell(type, item);
		if (cell.status === "none") cell.status = "pass";
		cell.source ??= source;
	}

	/** Record a fail for a type, naming the failing example or chart. */
	fail(type: string, item: string, source: string, failing: string) {
		const cell = this.cell(type, item);
		cell.status = "fail";
		cell.source = source;
		if (failing && !cell.failing.includes(failing)) cell.failing.push(failing);
	}

	/** Record why a type has no evidence for an item yet, without changing its status. */
	note(type: string, item: string, source: string, note: string) {
		const cell = this.cell(type, item);
		cell.source ??= source;
		if (note && !cell.notes.includes(note)) cell.notes.push(note);
	}

	/** Record a finding that no example or type could be found for. */
	lost(item: string, source: string, text: string) {
		this.unattributed.push({ item, source, text });
	}
}

/** The step's receipt status ("pass", "fail", "unrun" ...), or undefined when no receipt names it. */
function stepStatus(receipt: Receipt | undefined, id: string) {
	return receipt?.steps?.find((step: { id: string; }) => step.id === id)?.status;
}

/** Whether a step's data should be read: a step the receipt calls unrun left nothing to read. */
function stepRan(receipt: Receipt | undefined, id: string) {
	return stepStatus(receipt, id) !== "unrun";
}

/** Add the contract step's results: cross-surface checks and accessible summaries per chart. */
function addContract(table: Table, run: string, receipt: Receipt | undefined) {
	const source = "contract/contract.json";
	const contract = stepRan(receipt, "contract") ? readJson<ContractEvidence>(join(run, source)) : undefined;
	if (!contract?.fixtures) return;
	for (const fixture of contract.fixtures) {
		// Broken fixtures fail on purpose; only golden examples evidence a type.
		if (fixture.kind !== "golden") continue;
		const charts = fixture.surfaces?.["cli.check"]?.charts ?? [];
		// A cross-surface diff belongs to the whole fixture, not to a chart: with one chart it fails that
		// chart's type, with several it is reported as a finding no type could be found for.
		const diff = fixture.ok === false ? `${fixture.id}: surfaces disagree (${(fixture.diffs ?? []).map((entry) => `${entry.surface} vs ${entry.against}`).join(", ") || "see contract.json"})` : undefined;
		if (diff && charts.length !== 1) table.lost("checks", source, diff);
		for (const chart of charts) {
			const where = `${fixture.id} ${chart.chartId}`;
			const errors = (chart.checks ?? []).filter((check: { severity: string; }) => check.severity === "error");
			if (diff && charts.length === 1) table.fail(chart.chartType, "checks", source, diff);
			else if (chart.failure || errors.length) table.fail(chart.chartType, "checks", source, `${where}: ${chart.failure ?? errors.map((check) => check.kind).join(", ")}`);
			else table.pass(chart.chartType, "checks", source);
			if (typeof chart.summary === "string" && chart.summary.trim()) table.pass(chart.chartType, "a11y-summary", source);
			else table.fail(chart.chartType, "a11y-summary", source, `${where}: no summary`);
		}
	}
}

/**
 * Add the rust-tests log's `<type>_contract` tests, parsed strictly from the log text. A crate's
 * integration tests are one `integration` binary whose tests read `test <type>_contract::<name> ... ok`;
 * each such line counts for its type within the binary that printed it. An older log has one binary
 * per file: the gate ran it itself and logged `$ <target>/deps/<name>-<hash>` before its output, or
 * cargo printed `Running tests/<name>.rs`, and its `test result:` line speaks for the type. A binary
 * whose tests all ran ignored shows nothing, and a line that does not match is ignored.
 */
function addTypeTests(table: Table, run: string, receipt: Receipt | undefined, known: ReadonlySet<string>) {
	if (!stepRan(receipt, "rust-tests")) return;
	const name = (type: string) => type.replaceAll("_", "-");
	const file = (type: string) => `tests/${type.replaceAll("-", "_")}_contract.rs`;
	for (const log of listDir(join(run, "logs")).filter((entry) => /^rust-tests-\d+\.log$/.test(entry))) {
		const source = `logs/${log}`;
		let binary: string | undefined;
		// The merged binary's tests so far, by type: how many passed, failed or stayed ignored.
		let tally = new Map<string, {passed: number; failed: number}>();
		const flush = () => {
			for (const [type, count] of tally) {
				if (!known.has(type)) continue;
				if (count.failed) table.fail(type, "type-tests", source, file(type));
				else if (count.passed) table.pass(type, "type-tests", source);
				else table.note(type, "type-tests", source, `${file(type)} ran no test`);
			}
			tally = new Map();
		};
		for (const line of readFileSync(join(run, source), "utf8").split("\n")) {
			const running = /^\s*Running tests\/([a-z0-9_]+)_contract\.rs\b/.exec(line) ?? /^\$ \S*\/deps\/([a-z0-9_]+)_contract-[0-9a-f]{16}$/.exec(line);
			if (running) {
				flush();
				binary = name(running[1]);
				continue;
			}
			if (/^\s*(Running|Doc-tests)\b/.test(line) || /^\$ /.test(line)) {
				flush();
				binary = undefined;
				continue;
			}
			const merged = /^test ([a-z0-9_]+)_contract::\S.* \.\.\. (ok|FAILED|ignored)\b/.exec(line);
			if (merged) {
				const type = name(merged[1]);
				const count = tally.get(type) ?? { passed: 0, failed: 0 };
				if (merged[2] === "ok") count.passed++;
				else if (merged[2] === "FAILED") count.failed++;
				tally.set(type, count);
				continue;
			}
			const result = /^test result: (ok|FAILED)\. (\d+) passed;/.exec(line);
			if (result && binary) {
				if (known.has(binary)) {
					if (result[1] === "FAILED") table.fail(binary, "type-tests", source, file(binary));
					else if (Number(result[2]) > 0) table.pass(binary, "type-tests", source);
					else table.note(binary, "type-tests", source, `${file(binary)} ran no test`);
				}
				binary = undefined;
			}
		}
		flush();
	}
}

/** Add the golden step: each golden example and docs snippet rendered in every style. */
function addGolden(table: Table, run: string, receipt: Receipt | undefined) {
	const source = "golden/golden.json";
	const golden = stepRan(receipt, "golden") ? readJson<GoldenEvidence>(join(run, source)) : undefined;
	if (!golden) return;
	const styles = golden.styles ?? [];
	const unrun = new Map((golden.unrun ?? []).map((entry) => [entry.input, entry.reason]));
	for (const [id, blocks] of inputStore(join(run, "golden/inputs"))) {
		const dir = join(run, "golden/inputs", id.replaceAll("/", "__"), "Rendered");
		const missing = styles.filter((style) => !existsSync(join(dir, `${style}.html`)));
		const failures = (golden.failures ?? []).filter((failure) => failure.input === id);
		for (const failure of failures) {
			for (const block of blocksFor(blocks, { line: failure.line })) table.fail(block.type, "golden", source, `${id} (${failure.style}): ${failure.code}`);
		}
		for (const type of typesOf(blocks)) {
			if (unrun.has(id)) table.note(type, "golden", source, `${id} not rendered: ${unrun.get(id)}`);
			else if (!styles.length || missing.length) table.note(type, "golden", source, `${id} has no render in ${missing.join(", ") || "any style"}`);
			else table.pass(type, "golden", source);
		}
	}
}

/** The styles of the every-block fixture a run rendered: the folders under `fixtures/`. */
function fixtureStyles(run: string) {
	return listDir(join(run, "fixtures")).filter((name) => existsSync(join(run, "fixtures", name, "report.rmd")));
}

/**
 * Add the geometry step: a type passes when it has a view in every fixture style at 320, 390 and 1440 px
 * and none of its views failed. A failed view of an example fails the type too. Without geometry.json
 * (a run from before the step wrote it) the item has no evidence.
 */
function addGeometry(table: Table, run: string, receipt: Receipt | undefined) {
	const source = "geometry/geometry.json";
	const data = stepRan(receipt, "geometry") ? readJson<GeometryEvidence>(join(run, source)) : undefined;
	if (!data?.views) return;
	const styles = fixtureStyles(run);
	const seen = new Map<string, Set<string>>();
	for (const view of data.views) {
		// A page that could not be read has no chart to name; its types simply lack that view.
		if (view.type == null) {
			table.lost("geometry", source, `${view.report} ${view.style}@${view.width}: ${(view.failures ?? []).join("; ") || "no view"}`);
			continue;
		}
		if (!seen.has(view.type)) seen.set(view.type, new Set());
		if (view.style !== "default") seen.get(view.type)!.add(`${view.style}@${view.width}`);
		if (view.ok === false) table.fail(view.type, "geometry", source, `${view.report} ${view.style}@${view.width} chart ${view.chart}: ${(view.failures ?? []).slice(0, 2).join("; ") || "failed"}`);
	}
	for (const [type, cells] of seen) {
		const missing = styles.flatMap((style) => GEOMETRY_WIDTHS.map((width) => `${style}@${width}`)).filter((cell) => !cells.has(cell));
		if (!styles.length) table.note(type, "geometry", source, "the run has no fixtures/<style> folders to name the styles");
		else if (missing.length) table.note(type, "geometry", source, `no view at ${missing.join(", ")}`);
		else table.pass(type, "geometry", source);
	}
}

/**
 * Add the visual matrix's collisions and clips as further geometry failures. The matrix covers a few
 * cases only, so it never makes a type pass.
 */
function addMatrix(table: Table, run: string, receipt: Receipt | undefined) {
	const source = "chart-review/manifest.json";
	const manifest = stepRan(receipt, "visual-matrix") ? readJson<MatrixEvidence>(join(run, source)) : undefined;
	if (!manifest?.captures) return;
	const reports = new Map<string, ChartBlock[] | undefined>();
	for (const capture of manifest.captures) {
		if (!reports.has(capture.case)) reports.set(capture.case, blocksOf(join(run, "chart-review/reports", capture.case)));
		const blocks = reports.get(capture.case);
		if (!blocks) continue;
		(capture.charts ?? []).forEach((chart, index: number) => {
			const picked = blocks.length === capture.charts!.length ? [blocks[index!]] : blocksFor(blocks, { title: chart.title });
			const problems = [...(chart.collisions ?? []).map(() => "collision"), ...(chart.clips ?? []).map(() => "clip")];
			if ((capture.errors ?? []).length || capture.overflow) problems.push("page error or overflow");
			for (const block of problems.length ? picked : []) {
				table.fail(block.type, "geometry", source, `${capture.case} ${capture.theme}@${capture.width}: ${chart.title} (${[...new Set(problems)].join(", ")})`);
			}
		});
	}
}

/**
 * Add the captures step: a type passes when the step passed and `captures-index.json` places one of its
 * charts in the every-block capture of every style at 390 and 1440 px, whose image file exists. A step that
 * did not run, such as one with no baseline yet, leaves the item without evidence and says why; one that
 * failed fails every type of the fixture.
 */
function addCaptures(table: Table, run: string, receipt: Receipt | undefined) {
	const source = "captures-index.json";
	const status = stepStatus(receipt, "captures");
	const styles = fixtureStyles(run);
	const index = readJson<CapturesEvidence>(join(run, source));
	const types = new Set((index?.charts ?? []).filter((entry) => entry.name.startsWith("fixture/") && entry.type).map((entry) => entry.type));
	for (const style of styles) for (const block of blocksOf(join(run, "fixtures", style)) ?? []) types.add(block.type);
	const reason = receipt?.steps?.find((step: { id: string; }) => step.id === "captures")?.reason;
	for (const type of types) {
		if (status === "fail") table.fail(type, "captures", source, `the captures step failed${reason ? `: ${reason}` : ""}`);
		else if (status !== "pass") table.note(type, "captures", source, `the captures step is ${status ?? "not in a receipt"}${reason ? `: ${reason}` : ""}`);
		else if (!index) table.note(type, "captures", source, "no captures-index.json says which capture holds the type");
		else {
			const held = new Set(index.charts.filter((entry) => entry.type === type).map((entry) => entry.name));
			const missing = styles.flatMap((style) => CAPTURE_WIDTHS.map((width) => `fixture/${style}/${width}`)).filter((name) => !held.has(name) || !existsSync(join(run, "captures", `${name.replaceAll("/", "__")}.png`)));
			if (missing.length || !styles.length) table.note(type, "captures", source, styles.length ? `no capture of the type at ${missing.join(", ")}` : "the run has no fixtures/<style> folders to name the styles");
			else table.pass(type, "captures", source);
		}
	}
}

/** The block a per-chart result belongs to: the one with its title, else the one at its index; never a guess across all blocks. */
function placeChart(blocks: ChartBlock[], { title, chart, index }: ChartHint) {
	const at = chart ?? index;
	const byTitle = title === undefined ? [] : blocks.filter((block) => block.title === title);
	if (byTitle.length === 1) return byTitle[0];
	if (byTitle.length > 1 && Number.isInteger(at)) return byTitle.find((block) => blocks.indexOf(block) === at) ?? byTitle[0];
	return Number.isInteger(at) ? blocks[at!] : undefined;
}

/** Add the cross-style step: each compared example's mismatches, placed on their charts. */
function addCrossStyle(table: Table, run: string, receipt: Receipt | undefined) {
	const source = "cross-style/cross-style.json";
	const data = stepRan(receipt, "cross-style") ? readJson<CrossStyleEvidence>(join(run, source)) : undefined;
	if (!data?.inputs) return;
	const store = inputStore(join(run, "cross-style/inputs"));
	const broken = (data.problems ?? []).length || (data.pageErrors ?? []).length;
	// A type passes only on keys compared for its own charts: `comparedCharts` names each chart's compared
	// keys and cells. A run without it, or a type none of whose charts had a key compared across two cells,
	// has no evidence for the type.
	const compared = data.comparedCharts;
	for (const id of data.inputs) {
		const blocks = store.get(id) ?? [];
		for (const type of typesOf(blocks)) {
			if (broken) {
				table.fail(type, "cross-style", source, `${id}: ${(data.problems ?? []).length} problems, ${(data.pageErrors ?? []).length} page errors`);
				continue;
			}
			if (!Array.isArray(compared)) {
				table.note(type, "cross-style", source, "cross-style.json names no per-chart compared keys (comparedCharts)");
				continue;
			}
			const own = compared.filter((entry) => entry.input === id).filter((entry) => placeChart(blocks, entry)?.type === type);
			if (own.some((entry) => entry.keys > 0 && entry.cells > 1)) table.pass(type, "cross-style", source);
			else table.note(type, "cross-style", source, `${id}: no key of a ${type} chart was compared across two cells`);
		}
		if (!store.has(id)) table.lost("cross-style", source, `${id}: no report.rmd in the run folder`);
	}
	for (const mismatch of data.mismatches ?? []) {
		const blocks = store.get(mismatch.input);
		if (!blocks) {
			table.lost("cross-style", source, `${mismatch.input}: ${mismatch.key}`);
			continue;
		}
		const match = /^chart (\d+) \((.*)\) /.exec(mismatch.key);
		const picked = blocksFor(blocks, { title: match?.[2], index: match ? Number(match[1]) : undefined });
		for (const block of picked) table.fail(block.type, "cross-style", source, `${mismatch.input}: ${mismatch.key}`);
	}
	for (const kind of ["rounding", "disclosure"] as const) {
		for (const entry of data[kind] ?? []) table.lost("cross-style", source, `${kind}: ${entry.where} (${entry.shown})`);
	}
}

/** Add one label-truth pass over a folder of reports, placing each finding on its chart. */
function addLabelTruthRun(table: Table, run: string, folder: string, data: LabelEvidence, source: string) {
	const store = inputStore(join(run, folder, "inputs"));
	const broken = (data.problems ?? []).length;
	const judged = new Set((data.judged ?? []).map((entry) => entry.report));
	const texts = data.judgedTexts;
	for (const report of data.reports ?? []) {
		const blocks = store.get(report);
		if (!blocks) {
			table.lost("label-truth", source, `${report}: no report.rmd in the run folder`);
			continue;
		}
		for (const type of typesOf(blocks)) {
			if (broken) table.fail(type, "label-truth", source, `${report}: ${data.problems!.length} problems`);
			else if (!judged.has(report)) table.note(type, "label-truth", source, `${report} was listed but never judged`);
			else if (!Array.isArray(texts)) table.note(type, "label-truth", source, "label-truth.json counts no judged texts per chart (judgedTexts)");
			else {
				// A type passes only when value texts of one of its own charts were judged.
				const count = texts.filter((entry) => entry.report === report && placeChart(blocks, entry)?.type === type).reduce((sum, entry) => sum + entry.texts, 0);
				if (count > 0) table.pass(type, "label-truth", source);
				else table.note(type, "label-truth", source, `${report}: no value text of a ${type} chart was judged`);
			}
		}
	}
	for (const finding of data.findings ?? []) {
		const blocks = store.get(finding.report);
		if (!blocks) {
			table.lost("label-truth", source, `${finding.report}: ${finding.text}`);
			continue;
		}
		for (const block of blocksFor(blocks, { title: finding.title, index: finding.chart })) {
			table.fail(block.type, "label-truth", source, `${finding.report} chart ${finding.chart}: "${finding.text}" against Table ${finding.tableValue}`);
		}
	}
}

/** Add the label-truth step: the examples run and the step's own fixtures. */
function addLabelTruth(table: Table, run: string, receipt: Receipt | undefined) {
	if (!stepRan(receipt, "label-truth")) return;
	for (const folder of ["label-truth", "label-truth-fixtures"]) {
		const source = `${folder}/label-truth.json`;
		const data = readJson<LabelEvidence>(join(run, source));
		if (data) addLabelTruthRun(table, run, folder, data, source);
	}
}

/** Add the chart-interaction suite: keyboard, pointer and zoom or toggle per every-block chart (`interaction/types.json`). */
function addInteraction(table: Table, run: string, receipt: Receipt | undefined) {
	const source = "interaction/types.json";
	const data = stepRan(receipt, "chromium") ? readJson<InteractionEvidence>(join(run, source)) : undefined;
	if (!data?.results) return;
	for (const entry of data.results) {
		if (entry.ok) table.pass(entry.type, "interaction", source);
		else table.fail(entry.type, "interaction", source, `every-block chart ${entry.index}: ${entry.error ?? "keyboard, pointer or toggle failed"}`);
	}
	for (const error of data.errors ?? []) table.lost("interaction", source, error);
}

/** Add the motion step: each every-block chart's entrance checks (`motion/types.json`). */
function addMotion(table: Table, run: string, receipt: Receipt | undefined) {
	const source = "motion/types.json";
	const data = stepRan(receipt, "motion") ? readJson<MotionEvidence>(join(run, source)) : undefined;
	if (!data?.charts) return;
	for (const chart of data.charts) {
		if (chart.ok) table.pass(chart.type, "motion", source);
		else table.fail(chart.type, "motion", source, `every-block ${chart.id}: ${(chart.failures ?? []).join("; ") || "a motion check failed"}`);
	}
}

/** Add the accessibility audit, which covers every chart type of the every-block fixture. */
function addA11y(table: Table, run: string, receipt: Receipt | undefined) {
	const source = "a11y/a11y.json";
	const audit = stepRan(receipt, "a11y") ? readJson<A11yEvidence>(join(run, source)) : undefined;
	if (!audit || !(audit.pages > 0)) return;
	const types = new Set<string>();
	for (const style of listDir(join(run, "a11y/fixtures"))) {
		for (const block of blocksOf(join(run, "a11y/fixtures", style)) ?? []) types.add(block.type);
	}
	// A violation row is not tied to a chart, so any finding counts against every type of the fixture.
	const findings = (audit.violations ?? 0) + (audit.keyboardFailures ?? []).length + (audit.problems ?? []).length;
	for (const type of types) {
		if (findings) table.fail(type, "a11y", source, `${audit.violations ?? 0} violations, ${(audit.keyboardFailures ?? []).length} keyboard failures, ${(audit.problems ?? []).length} problems`);
		else table.pass(type, "a11y", source);
	}
}

/**
 * Add the exports step. A type passes only on its own charts in `exports/types.json`, the per-chart PDF
 * check of the every-block report (title in the PDF text, a copyable label or vector drawing in the chart's
 * box, no raster over it): every chart of the type must be `ok`. The pagination fixtures' log lines
 * (`<example>/<style>: N pages ... ; pass`) can only fail the types of their example, never pass one.
 */
function addExports(table: Table, run: string, receipt: Receipt | undefined) {
	if (!stepRan(receipt, "exports")) return;
	const source = "exports/types.json";
	const entries = readJson<ExportEvidence[]>(join(run, source));
	if (!Array.isArray(entries)) table.lost("pdf", source, "no per-chart PDF check in the run folder");
	else {
		const seen = new Map<string, number>();
		for (const entry of entries) {
			if (!entry?.type) continue;
			if (!seen.has(entry.type)) seen.set(entry.type, 0);
			if (entry.ok === true) seen.set(entry.type, seen.get(entry.type)! + 1);
			else table.fail(entry.type, "pdf", source, `every-block chart ${entry.index}${entry.title ? ` (${entry.title})` : ""}: ${[!entry.a && "title not in the PDF text", entry.a && !entry.b && (entry.labels_tried ? "no drawn label is copyable text" : "no vector drawing in the chart's box"), entry.a && !entry.c && `a raster covers ${Math.round((entry.largest_raster ?? 0) * 100)}%`].filter(Boolean).join("; ") || "failed"}`);
		}
		for (const [type, ok] of seen) if (ok > 0) table.pass(type, "pdf", source);
	}
	const examples = inputStore(join(run, "golden/inputs"));
	for (const name of listDir(join(run, "logs")).filter((file) => /^exports-\d+\.log$/.test(file))) {
		const logSource = `logs/${name}`;
		const results = new Map<string, string[]>();
		for (const line of readFileSync(join(run, logSource), "utf8").split("\n")) {
			const match = /^([a-z0-9][a-z0-9-]*)\/([a-z][a-z-]*): .*?(; pass)?$/.exec(line);
			if (!match || !/\b\d+ pages?\b/.test(line) || match[3]) continue;
			if (!results.has(match[1])) results.set(match[1], []);
			results.get(match[1])!.push(match[2]);
		}
		for (const [example, failed] of results) {
			const blocks = examples.get(`examples/${example}`);
			if (!blocks) table.lost("pdf", logSource, `${example}: no golden example of that name to read its chart types from`);
			else for (const type of typesOf(blocks)) table.fail(type, "pdf", logSource, `${example}: ${failed.join(", ")}`);
		}
	}
}

/**
 * Add the perf step. A type passes only on its own entry in PRF-07's all-types fixture
 * (`perf.json` `allTypes.entries`): "pass" passes, "fail" fails, and "no baseline yet" stays
 * `none` with that note. A failed budget of another fixture (small, flagship, dense) fails every
 * type that fixture holds; a failure that names no fixture is run-wide and placed on no type.
 */
function addPerf(table: Table, run: string, receipt: Receipt | undefined) {
	const source = "perf/perf.json";
	const perf = stepRan(receipt, "perf") ? readJson<PerfEvidence>(join(run, source)) : undefined;
	if (!perf) return;
	for (const entry of perf.allTypes?.entries ?? []) {
		const where = `all-types chart ${entry.index}`;
		if (entry.status === "pass") table.pass(entry.type, "perf", source);
		else if (entry.status === "fail") table.fail(entry.type, "perf", source, `${where}: native ${entry.native_ms} ms (limit ${entry.limit_native_ms}), mount ${entry.mount_ms} ms (limit ${entry.limit_mount_ms})`);
		else table.note(entry.type, "perf", source, `${where}: ${entry.status}`);
	}
	const fixtures = inputStore(join(run, "perf/inputs"));
	const names = [...fixtures.keys()];
	const failures = perf.failures ?? [];
	for (const [fixture, blocks] of fixtures) {
		if (fixture === "all-types") continue;
		const own = failures.filter((failure) => String(failure.name).split(".").includes(fixture));
		if (!own.length) continue;
		for (const type of typesOf(blocks)) table.fail(type, "perf", source, `${fixture}: ${own.map((failure) => failure.name).join(", ")}`);
	}
	for (const failure of failures) {
		if (String(failure.name).startsWith("allTypes.")) continue;
		if (!names.some((fixture) => String(failure.name).split(".").includes(fixture))) table.lost("perf", source, `${failure.name}: ${failure.reason ?? `${failure.value} over ${failure.limit}`}`);
	}
}

/**
 * Add the ARC-64 lab's verdicts (`--lab <run folder of benchmarks/compare>`): `comparison.json` names the
 * commit RichView was rendered from and the libraries per type, and `verdicts/<id>.json` holds a review
 * agent's verdict. Only "withstands" passes, and only when the lab rendered this run's commit; a lab of
 * another commit leaves the item without evidence, since the comparison judges the drawing itself. A
 * type no reference library draws stays without evidence, with that note.
 */
function addLibraryComparison(table: Table, lab: string | undefined, commit: string | undefined) {
	if (!lab) return;
	const manifest = readJson<LabManifest>(join(lab, "comparison.json"));
	if (!manifest?.types) {
		table.lost("library-comparison", lab, "no comparison.json in the lab folder");
		return;
	}
	const labCommit = String(manifest.richview?.commit ?? "");
	const same = labCommit && commit && (commit.startsWith(labCommit) || labCommit.startsWith(commit));
	for (const entry of manifest.types) {
		const source = `${lab}/verdicts/${entry.id}.json`;
		if (!entry.libraries?.length) {
			table.note(entry.type, "library-comparison", lab, `${entry.id}: neither reference library draws this type`);
			continue;
		}
		if (!same) {
			table.note(entry.type, "library-comparison", lab, `${entry.id}: the lab rendered ${labCommit || "an unknown commit"}, not ${commit}`);
			continue;
		}
		const verdict = readJson<LabVerdict>(join(lab, "verdicts", `${entry.id}.json`));
		if (!verdict) table.note(entry.type, "library-comparison", source, `${entry.id}: not judged yet`);
		else if (verdict.verdict === "withstands") table.pass(entry.type, "library-comparison", source);
		else table.fail(entry.type, "library-comparison", source, `${entry.id}: ${verdict.verdict}: ${verdict.summary ?? ""}`);
	}
}

/** Find the release receipt of a run: beside the run folder's own copy, else in the evidence store. */
function findReceipt(run: string) {
	const own = readJson<Receipt>(join(run, "receipt.json"));
	if (own) return own;
	return readJson<Receipt>(join(evidenceDir(), "receipts", `${basename(run)}.json`));
}

/** Find the newest release run folder in the evidence store, or undefined when there is none. */
export function newestReleaseRun(root = join(evidenceDir(), "runs")) {
	const runs = listDir(root).filter((name) => name.startsWith("release-") && statSync(join(root, name)).isDirectory());
	return runs.length ? join(root, runs[runs.length - 1]) : undefined;
}

/**
 * Build the evidence table of a release run: every chart type of the registry (and of the run), every
 * item, and for each the status. `require` limits the items acceptance waits for; by default every item
 * a step can evidence is required, except the disclosure-only per-type timings.
 */
export function buildEvidence({ run, registry, require, lab }: {run: string; registry: string[]; require?: string[]; lab?: string}) {
	const receipt = findReceipt(run);
	const table = new Table(ITEMS);
	const known = new Set(registry);
	addContract(table, run, receipt);
	addTypeTests(table, run, receipt, known);
	addGolden(table, run, receipt);
	addGeometry(table, run, receipt);
	addMatrix(table, run, receipt);
	addCaptures(table, run, receipt);
	addCrossStyle(table, run, receipt);
	addLabelTruth(table, run, receipt);
	addInteraction(table, run, receipt);
	addMotion(table, run, receipt);
	addA11y(table, run, receipt);
	addExports(table, run, receipt);
	addPerf(table, run, receipt);
	addLibraryComparison(table, lab, receipt?.commit ?? /-([0-9a-f]{8,40})$/.exec(basename(run))?.[1]);
	const types = [...new Set([...registry, ...table.cells.keys()])].sort();
	const disclosures = ITEMS.filter((item) => item.disclosureOnly).map((item) => item.id);
	const required = (require ?? ITEMS.map((item) => item.id)).filter((id: string) => !disclosures.includes(id));
	const waived = ITEMS.map((item) => item.id).filter((id) => !required.includes(id) && !disclosures.includes(id));
	const rows = types.map((type) => {
		const items = Object.fromEntries(
			ITEMS.map((item) => {
				const cell = table.cell(type, item.id);
				return [item.id, { status: cell.status, source: cell.source ?? null, failing: cell.failing, ...(cell.notes.length ? { notes: cell.notes } : {}) }];
			}),
		);
		const blockers = required.filter((id: string|number) => items[id]?.status !== "pass");
		return { type, items, accepted: blockers.length === 0, blockers };
	});
	const id = receipt?.id ?? basename(run);
	const fromName = /-([0-9a-f]{8,40})$/.exec(basename(run))?.[1];
	const stamp = /^release-(\d{4})(\d\d)(\d\d)T(\d\d)(\d\d)(\d\d)Z-/.exec(basename(run));
	return {
		run: id,
		commit: receipt?.commit ?? fromName ?? "unknown",
		at: receipt?.at ?? (stamp ? `${stamp[1]}-${stamp[2]}-${stamp[3]}T${stamp[4]}:${stamp[5]}:${stamp[6]}Z` : null),
		receiptResult: receipt?.result ?? null,
		required,
		waived,
		disclosures,
		items: ITEMS.map(({ id: itemId, step, card, title, disclosureOnly }) => ({ id: itemId, step, card, title, ...(disclosureOnly ? { disclosureOnly } : {}), stepStatus: step ? (stepStatus(receipt, step) ?? null) : null })),
		types: rows,
		unattributed: table.unattributed,
	};
}

/** Count how often each item is the reason a type is not accepted, most common first. */
export function failingItems(report: EvidenceReport) {
	const counts = new Map<string, number>();
	for (const row of report.types) {
		for (const [id, cell] of Object.entries(row.items)) {
			if (cell.status === "fail") counts.set(id, (counts.get(id) ?? 0) + 1);
		}
	}
	return [...counts].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]));
}

/** Render the report as a Markdown table with one row per chart type. */
export function renderMarkdown(report: EvidenceReport) {
	const mark = { pass: "pass", fail: "FAIL", none: "none" };
	const lines = [
		`# Chart type evidence of ${report.run}`,
		"",
		`Commit ${report.commit}${report.at ? `, ${report.at}` : ""}. Release receipt: ${report.receiptResult ?? "not found"}. Cells: pass, FAIL, or none (no evidence, which counts against the type for required items).`,
		`Acceptance requires: ${report.required.join(", ")}. Waived: ${report.waived.length ? report.waived.join(", ") : "none"}.`,
		`Disclosure only: ${report.disclosures.join(", ")}. Overall performance remains a full-run gate.`,
		"",
		"| Type | Accepted | " + report.items.map((item) => item.id).join(" | ") + " |",
		"| --- | --- | " + report.items.map(() => "---").join(" | ") + " |",
		...report.types.map((row) => `| ${row.type} | ${row.accepted ? "yes" : "no"} | ${report.items.map((item: { id: string|number; }) => mark[row.items[item.id].status]).join(" | ")} |`),
		"",
		"## Items",
		"",
		...report.items.map((item) => `- ${item.id} (${item.card}): ${item.title}. Step: ${item.step ? `${item.step} (${item.stepStatus ?? "no receipt"})` : "none, no step evidences it"}.`),
		"",
		"## Failures",
		"",
	];
	let failures = 0;
	for (const row of report.types) {
		for (const [id, cell] of Object.entries(row.items)) {
			if (cell.status !== "fail") continue;
			failures += 1;
			lines.push(`- ${row.type} / ${id} (${cell.source}): ${cell.failing.slice(0, 3).join("; ")}${cell.failing.length > 3 ? `; and ${cell.failing.length - 3} more` : ""}`);
		}
	}
	if (!failures) lines.push("None.");
	lines.push("", "## Findings no type could be found for", "");
	if (report.unattributed.length) for (const entry of report.unattributed) lines.push(`- ${entry.item} (${entry.source}): ${entry.text}`);
	else lines.push("None.");
	return `${lines.join("\n")}\n`;
}

/** Render the draft accepted.txt: a header naming the run, its commit and the date, then the accepted types. */
export function renderAccepted(report: EvidenceReport) {
	const accepted = report.types.filter((row) => row.accepted).map((row) => row.type);
	return [
		`# Draft from release run ${report.run}`,
		`# Commit ${report.commit}, ${report.at ?? "date unknown"}.`,
		`# ${accepted.length} of ${report.types.length} chart types show every required item: ${report.required.join(", ")}.`,
		`# Waived by --require, so not checked: ${report.waived.length ? report.waived.join(", ") : "none"}.`,
		`# Disclosure only: ${report.disclosures.join(", ")}. Overall performance remains a full-run gate.`,
		...(report.unattributed.length ? [`# ${report.unattributed.length} findings could not be placed on a type; see type-evidence.md.`] : []),
		...accepted,
	].join("\n") + "\n";
}

/** Parse the command line into options. */
function parseArgs(argv: string[]) {
	const options: Record<string, string> = {};
	for (let i = 0; i < argv.length; i += 1) {
		const flag = argv[i];
		if (!["--run", "--out", "--registry", "--require", "--lab"].includes(flag) || argv[i + 1] === undefined) throw new Error(`Unknown or incomplete argument: ${flag}`);
		options[flag.slice(2)] = argv[i + 1];
		i += 1;
	}
	return options;
}

/** Run the tool: read the run folder, write the three outputs and print the counts. */
export function main(argv: string[]) {
	const options = parseArgs(argv);
	if (!options.out) throw new Error("Usage: type-evidence.mjs [--run <run folder>] --out <dir> [--registry <registry.rs>] [--require <items>]");
	const run = options.run ? resolve(options.run) : newestReleaseRun();
	if (!run || !existsSync(run)) throw new Error(`No release run folder: ${run ?? "none in the evidence store"}`);
	const registryPath = options.registry ?? join(HERE, "../../crates/richview-core/src/registry.rs");
	const registryText = readText(registryPath);
	if (registryText === undefined) throw new Error(`No chart type registry at ${registryPath}`);
	const require = options.require?.split(",").map((item: string) => item.trim()).filter(Boolean);
	for (const id of require ?? []) if (!ITEMS.some((item) => item.id === id)) throw new Error(`Unknown item: ${id}`);
	const report = buildEvidence({ run, registry: parseRegistry(registryText), require, lab: options.lab ? resolve(options.lab) : undefined });
	mkdirSync(options.out, { recursive: true });
	writeFileSync(join(options.out, "type-evidence.json"), `${JSON.stringify(report, null, "\t")}\n`);
	writeFileSync(join(options.out, "type-evidence.md"), renderMarkdown(report));
	writeFileSync(join(options.out, "accepted.txt.draft"), renderAccepted(report));
	const accepted = report.types.filter((row) => row.accepted).length;
	const [worst] = failingItems(report);
	console.log(`${report.types.length} types, ${accepted} accepted; most common failing item: ${worst ? `${worst[0]} (${worst[1]} types)` : "none"}`);
	return report;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
	try {
		main(process.argv.slice(2));
	} catch (error) {
		console.error((error as Error).message);
		process.exit(1);
	}
}
