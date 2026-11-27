// The cross-style step of the release run (DEL-52, DEL-80): every input renders in every builtin
// style at 390 and 1440 px with reduced motion, and the values a reader can get are collected and
// judged three ways.
//
// Inputs: examples/every-block, the INT-55 golden inputs (GOLDEN_LIST of golden.mjs) and the
// precision fixture under tools/rv-gate/fixtures/cross-style/, whose source values carry more
// digits than any element shows.
//
// Value sources read for each chart, through the report runtime in Chromium:
//  - the text outside charts: big numbers, metrics, table cells and totals;
//  - the data labels of the geometry model (`window.__richview.charts()`, DEL-72): every text item
//    that is not an axis label or axis title (axis ticks are scale marks, not values);
//  - the model's summary, the frame's accessible name and the model's readouts;
//  - the keyboard readouts: focus the frame, Home, then ArrowRight through the marks;
//  - the Table view and the CSV download.
// Skipped: the chart image download (FEA-CH-47 is open, and an image carries no text to compare) and
// the SVG download (FEA-CH-46 is open and the build has no such button). The report PDF is checked
// in the prod-candidate gate (DEL-33).
//
// Assertions:
//  (a) every value is identical across the styles and widths of an input;
//  (b) in the precision fixture, every value equals its source rounded by the author's precision
//      rule (precision.json: three significant digits by default; four at most, with a scale word,
//      when the author sets it per chart or element; the format has no such setting yet);
//  (c) DEL-80's disclosure check: no output carries a fixture value with more precision than its
//      element allows (the HTML with its embedded data and attributes, the model's values, the
//      readouts, Table, CSV and accessible names).
//
// Out of scope, DEL-80's open points: mark coordinates (the model's positions are exact and are not
// scanned) and the raw source (`/raw/...`, TYP-65's `source`), which this step does not fetch.
//
// Usage: node tools/rv-gate/cross-style.mts --bin target/release/richview --out <dir>
//        [--public <dist/web>] [--only <text>] [--jobs <n>] [--cwd <repo>]

import { spawn } from "node:child_process";
import { cpSync, existsSync, mkdirSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { cpus } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { GOLDEN_LIST } from "./golden.mts";
import { launchBrowser } from "./lib/browser-launch.mts";
import { builtinStyles } from "./lib/captures.mts";
import type { BrowserHarness } from "./lib/captures.mts";
import { SKIP_COPY } from "./lib/connectors.mts";
import { displayedReadout, figureDownload } from "../../scripts/chart-interaction-checks.ts";
import type { Download, JSHandle, Locator, Page } from "playwright";

type NumericToken = ReturnType<typeof parseNumbers>[number];
type OtherValue = string | number;
type PrecisionLimits = { sig: number; maxSig?: number; overrides?: Record<string, number> };
type PrecisionRule = PrecisionLimits & { values: number[]; derived: number[]; other: OtherValue[] };
type PrecisionVerdict = { status: "ok"; expected: number; source?: undefined } | { status: "excess" | "coarse"; expected: number; source: number } | { status: "unmatched"; expected?: undefined; source?: undefined };
type RawProbe = ReturnType<typeof pageProbe>;
type RawChart = RawProbe["charts"][number];
type LabelValues = Pick<RawChart, "labels" | "dropped"> & Partial<Pick<RawChart, "valueLabels" | "valueDropped" | "keyLabels">>;
type CollectedChart = LabelValues & Pick<RawChart, "title" | "name" | "summary" | "readouts" | "marks"> & Partial<Omit<RawChart, keyof LabelValues | "title" | "name" | "summary" | "readouts" | "marks">> & { keyboard: string[]; table: string[]; csv: string; skipped?: string[] };
type CollectedProbe = Omit<RawProbe, "charts"> & { charts: (RawChart & Required<Pick<CollectedChart, "keyboard" | "table" | "csv" | "skipped">>)[]; errors?: string[]; retried?: boolean };
type Walk = { texts: string[]; keys: string[]; stop(): void };
export type RuntimeSettle = (page: Page, charts?: number, options?: { models?: boolean }) => Promise<unknown>;
type Diagnostic = { level?: string; code: string; message: string };
type Input = { id: string; dir: string };
type MatrixTask = { input: Input; style: string; width: number };
type MatrixProblem = { input: string; style: string; width?: number; message: string };


export const WIDTHS = [390, 1440];
export const FIXTURE = "tools/rv-gate/fixtures/cross-style";
export const EVERY_BLOCK = "examples/every-block";
/** The keyboard readout walks at most this many marks of a chart, from its first. */
export const MAX_KEY_STEPS = 24;
/** The folders a copy of a report leaves out; one set for every step (see lib/connectors.mjs), exported here for compatibility. */
export { SKIP_COPY };
/** Geometry-model text that is scale, not data. */
export const SCALE_ROLES = new Set(["axis-label", "axis-title"]);
/**
 * Text roles whose numbers a reader gets as values, compared across cells: data labels, key entries
 * (a pie's labels give way to a key of "<name> <share>%" at phone width) and the author's annotations
 * and panel titles. Axis ticks are left out, since their count follows the axis length and the font
 * (chart engine, 28 Oct); every text is still judged for precision.
 */
export const VALUE_ROLES = new Set(["label", "key", "annotation", "panel-title"]);

// ---------------------------------------------------------------------------------------------
// Pure parts: numbers as a reader sees them, and the precision rule.
// ---------------------------------------------------------------------------------------------

const SCALES: Record<string, number> = { k: 1e3, thousand: 1e3, m: 1e6, million: 1e6, b: 1e9, billion: 1e9, t: 1e12, trillion: 1e12 };
const ISO_DATE = /\d{4}-\d{2}(?:-\d{2})?(?:[Tt ]\d{2}:\d{2}(?::\d{2}(?:[.,]\d+)?)?(?:[Zz]|[+-]\d{2}(?::?\d{2})?)?)?/g;
/**
 * A written date: "Mar 1, 2024", "1 March 2024", "Jul 2024". Its day and year are a position on a
 * time axis, not values a reader could take for a source value, so they are not judged (release
 * 1zc judged "Mar 1" as a coarse 1.41).
 */
// Whole month names or their abbreviations only, so "Marketing 4" or "Decline 7" stays a value (review 1h
// should-fix 1). A day before a month is a whole number: the "14" of "3.14" before a line that opens with
// "Mar 2" is no day (release 1zd judged "3." left behind as a coarse 3).
// What a date can hide is bounded: a day is a whole number from 1 to 31 with no decimals and a year is
// 1500 to 2199, so a series named "May 4" or "Jun 2024" can hide only such a whole number from judging,
// never an over-precise one from the disclosure scan, and the cross-cell comparison still sees it in
// every style (review 1i note 1; "May 1234" is judged, review 1j note 1).
const MONTH = "(?:Jan(?:uary)?|Feb(?:ruary)?|Mar(?:ch)?|Apr(?:il)?|May|June?|July?|Aug(?:ust)?|Sep(?:t(?:ember)?)?|Oct(?:ober)?|Nov(?:ember)?|Dec(?:ember)?)(?:\\.|(?![A-Za-z]))";
const DAY = "(?:[12]\\d|3[01]|0?[1-9])";
const YEAR = "(?:1[5-9]|2[01])\\d{2}(?!\\d)";
const WRITTEN_DATE = new RegExp(`\\b${MONTH}\\s+(?:${DAY}(?![\\d.%]|,\\d)(?:,?\\s+${YEAR})?|${YEAR})|(?<![\\d.,])\\b${DAY}\\s+${MONTH}(?:\\s+${YEAR})?`, "g");
const NUMBER = /(?<![\w.])([$€£¥])?(\d{1,3}(?:,\d{3})+|\d+)(\.\d+)?(?:[eE]([+-]?\d+))?(?:(\s?(?:%|thousand|million|billion|trillion))|([KMBTk])(?![A-Za-z]))?/g;

/** Round to `digits` significant digits; 0 and non-finite numbers stay. */
export function roundSig(value: number, digits: number) {
	if (!Number.isFinite(value) || value === 0) return value;
	return Number(value.toPrecision(digits));
}

/** Significant digits a number's text shows: leading zeros never count, nor trailing zeros of a whole number. */
export function sigDigits(whole: string, fraction = "") {
	const digits = (whole + fraction.replace(/^\./, "")).replace(/^0+/, "");
	return (fraction ? digits : digits.replace(/0+$/, "")).length || 1;
}

/**
 * The numbers in a text, as a reader sees them: `{raw, value, percent, scale, sig, index}`.
 * `value` is the shown number in base units (`1.23 million` is 1230000; `12.3%` is 12.3),
 * `sig` its significant digits. ISO dates are blanked first so their parts are not read as numbers.
 */
export function parseNumbers(text: string) {
	const blanked = String(text).replace(ISO_DATE, (m) => " ".repeat(m.length)).replace(WRITTEN_DATE, (m) => " ".repeat(m.length));
	const found = [];
	for (const m of blanked.matchAll(NUMBER)) {
		const [raw, , whole, fraction = "", exponent, word, letter] = m;
		const unit = (word ?? letter ?? "").trim();
		const percent = unit === "%";
		const scale = percent || !unit ? 1 : SCALES[unit.toLowerCase()];
		const mantissa = Number(`${whole.replaceAll(",", "")}${fraction}${exponent ? `e${exponent}` : ""}`);
		found.push({ raw: raw.trim(), value: mantissa * scale, percent, scale: scale !== 1, sig: sigDigits(whole.replaceAll(",", ""), fraction), index: m.index });
	}
	return found;
}

export const same = (a: number, b: number) => a === b || Math.abs(a - b) <= 1e-9 * Math.max(Math.abs(a), Math.abs(b));

/**
 * The author's precision rule (FEA-CH-37) for one element: `rule = {sig, maxSig, overrides}`.
 * The default is `sig` significant digits (three); `overrides` maps a chart title or element name to
 * the digits the author set there, at most `maxSig` (four, with a scale word).
 */
export function digitsFor(rule: PrecisionLimits, element: string) {
	const set = rule.overrides?.[element];
	return Math.min(set ?? rule.sig, rule.maxSig ?? 4);
}

/**
 * The most digits an element may show: what the author set there, or else `maxSig` (four).
 * Daniel's FEA-CH-37 answer reads "three digits, at max four", and FEA-CH-22's recommended
 * option prints whole numbers below 10,000 whole, so 1,235 and 167.5 thousand pass by default
 * (lockdown lead, 2026-10-28; in WAITING.md for Daniel to confirm).
 */
export function maxDigitsFor(rule: PrecisionLimits, element: string) {
	const set = rule.overrides?.[element];
	return set ?? rule.maxSig ?? 4;
}

/** The values a source number can show as: itself, and times 100 for a percentage. */
const candidates = (source: number, token: Pick<NumericToken, "percent">) => (token.percent ? [source * 100] : [source]);

/**
 * Judge one shown number against the fixture's source values at `limit` significant digits:
 *  - `ok`: it is a source value rounded to `limit` digits (or exactly representable in fewer);
 *  - `excess`: it is a source value rounded to more than `limit` digits (a disclosure, DEL-80);
 *  - `coarse`: it is a source value rounded to fewer than `limit` digits, so not the author's precision;
 *  - `unmatched`: it is no rounding of any source value (a count, a year, a derived number).
 * `expected` is the source rounded to `limit` digits.
 */
export function judgeNumber(token: Pick<NumericToken, "percent" | "sig" | "value">, sources: readonly number[], limit: number, max = limit): PrecisionVerdict {
	for (const source of sources) {
		for (const candidate of candidates(source, token)) {
			for (let digits = limit; digits <= Math.max(limit, max); digits++) {
				if (same(token.value, roundSig(candidate, digits))) return { status: "ok", expected: token.value };
			}
		}
	}
	// A one-digit number is a coarse rounding of a source value too (FEA-CH-37): "3" for 3.14159265.
	// Counts are told apart by their noun (`isCount`), never by a declaration.
	for (const source of sources) {
		for (const candidate of candidates(source, token)) {
			if (same(token.value, roundSig(candidate, token.sig))) {
				return { status: token.sig > Math.max(limit, max) ? "excess" : "coarse", expected: roundSig(candidate, limit), source };
			}
		}
	}
	return { status: "unmatched" };
}

/**
 * Nouns that make the whole number before them a count, not a value a reader could take for a
 * source value: "3 categories", "21 observations". Such a number is not judged (FEA-CH-37, review
 * 1f should-fix 16). The list holds each noun's plural and singular, and no noun that can be a unit
 * of measure (points, values, steps, years, months, days): "4.88 points" or "3 years" is a value
 * (review 1g should-fix 11).
 */
export const COUNT_NOUNS = [
	"categories", "category", "series", "rows", "row", "items", "item", "bars", "bar", "slices", "slice",
	"groups", "group", "observations", "observation", "nodes", "node", "links", "link", "cells", "cell",
	"bins", "bin", "regions", "region", "words", "word", "panels", "panel", "pages", "page",
];
const COUNT_NOUN = new RegExp(`^\\s+(?:${COUNT_NOUNS.join("|")})(?![A-Za-z])`, "i");

/** Whether `token` is a whole number, written without a decimal point, directly followed by a count noun in `text`. */
export function isCount(token: NumericToken, text: string) {
	if (token.percent || token.scale || !Number.isInteger(token.value) || /[.eE]/.test(token.raw)) return false;
	return COUNT_NOUN.test(String(text).slice(token.index + token.raw.length));
}

/**
 * Whether an unmatched number is one the fixture declares in `other`: a number equals the shown
 * value, a string equals the shown text exactly ("2 million"). One-digit numbers get no leave.
 */
export function declaredOther(token: Pick<NumericToken, "value" | "raw">, other: OtherValue[] = []) {
	return other.some((value) => (typeof value === "string" ? value === token.raw : same(token.value, value)));
}

/**
 * Judge every number in `text` and return the ones that are not `ok`, with their verdict. A count
 * (a number followed by a count noun) is not judged. A number that is no rounding of any source
 * value fails unless `other` declares it: a wrong value (a truncation, thousands scaled as
 * millions) is never a rounding of the source (DEL-52, FEA-CH-37). `other` never excuses a
 * coarse rounding of a source value; that is the hole the declared counts used to reopen.
 */
export function judgeText(text: string, sources: readonly number[], limit: number, max = limit, other: OtherValue[] = []) {
	const bad = [];
	for (const token of parseNumbers(text)) {
		if (isCount(token, text)) continue;
		const verdict = judgeNumber(token, sources, limit, max);
		if (verdict.status === "ok") continue;
		if (verdict.status === "unmatched" && declaredOther(token, other)) continue;
		bad.push({ raw: token.raw, ...verdict });
	}
	return bad;
}

/**
 * DEL-80's disclosure scan over one output: the numbers that are a source value shown with more
 * precision than `limit` digits. Returns `[{raw, source, sig}]`, each number once.
 */
export function scanDisclosure(text: string, sources: readonly number[], limit: number) {
	const seen = new Set();
	const found = [];
	for (const token of parseNumbers(text)) {
		if (token.sig <= limit || seen.has(token.raw)) continue;
		// A double holds at most 17 significant digits, so a longer number is compared at 17.
		const digits = Math.min(token.sig, 17);
		for (const source of sources) {
			if (candidates(source, token).some((c) => same(token.value, roundSig(c, digits)))) {
				seen.add(token.raw);
				found.push({ raw: token.raw, source, sig: token.sig });
				break;
			}
		}
	}
	return found;
}

/**
 * The HTML a reader's browser parses, without the stylesheet and runtime code; embedded data and
 * attributes stay. A chart payload's values are spaced apart, so `[120,132]` reads as two numbers
 * rather than the grouped 120,132; its strings keep their commas, so `"1,234,567.891"` stays one.
 */
export function scannableHtml(html: string) {
	return html
		.replace(/<style[\s\S]*?<\/style>/gi, " ")
		.replace(/<script(?![^>]*data-rv-payload)[\s\S]*?<\/script>/gi, " ")
		.replace(/(<script[^>]*data-rv-payload[^>]*>)([\s\S]*?)(<\/script>)/gi, (all, open: string, body, close: string) => open + spacedJson(body) + close);
}

/** JSON text with a space after each comma between values; commas inside strings stay as they are. */
function spacedJson(json: string) {
	let out = "";
	let string = false;
	for (let i = 0; i < json.length; i++) {
		const c = json[i];
		out += c;
		if (string) {
			if (c === "\\") out += json[++i] ?? "";
			else if (c === '"') string = false;
		} else if (c === '"') string = true;
		else if (c === ",") out += " ";
	}
	return out;
}

/** Where in the HTML a text first occurs: an attribute, the embedded chart data or the text. */
export function locate(html: string, raw: string) {
	const at = html.indexOf(raw);
	if (at < 0) return "markup";
	const open = html.lastIndexOf("<", at);
	if (open >= 0 && html.lastIndexOf(">", at) < open) {
		const tag = html.slice(open, at);
		const attribute = [...tag.matchAll(/([\w:-]+)="[^"]*$/g)].at(-1)?.[1];
		return attribute ? `attribute ${attribute}` : "markup";
	}
	const script = html.lastIndexOf("<script", at);
	if (script >= 0 && html.lastIndexOf("</script>", at) < script && /data-rv-payload/.test(html.slice(script, html.indexOf(">", script)))) return "embedded chart data";
	return "text";
}

/**
 * Assertion (a): `cells` is `[{id, values}]` with `values` a flat `{key: string}`. A key whose
 * value is not identical in every cell is a mismatch: `{key, groups: [{value, cells: [id]}]}`.
 */
export function compareCells(cells: { id: string; values: Record<string, string> }[]) {
	const keys = new Set(cells.flatMap((c) => Object.keys(c.values)));
	const mismatches = [];
	for (const key of [...keys].sort()) {
		const groups = new Map<string, string[]>();
		for (const cell of cells) {
			const value = key in cell.values ? cell.values[key] : "(absent)";
			// A value the collector could not read is reported once as its cell's problem, not as a difference.
			if (value === NOT_COLLECTED) continue;
			groups.set(value, [...(groups.get(value) ?? []), cell.id]);
		}
		if (groups.size > 1) mismatches.push({ key, groups: [...groups.entries()].map(([value, ids]) => ({ value, cells: ids })) });
	}
	return mismatches;
}

/** The numbers of a text as one comparable string, for sources whose wording is a style's own. */
export const numbersOf = (text: string) => parseNumbers(text).map((t) => t.raw).join(" | ");

/**
 * Read `precision.json` next to a fixture: `{sig, maxSig, overrides, values, derived}`.
 * `values` are the source numbers; `derived` the numbers computed from them (totals).
 */
/**
 * The fixture's rule: `sig` and `maxSig` digits, per-element `overrides`, the source `values`, the
 * `derived` values (totals) and `other`, the numbers it shows that are no value at all (the year in a
 * date readout such as "Mar 1, 2024"). Any other number in a judged output fails.
 */
export function parsePrecision(text: string, where = "precision.json"): PrecisionRule {
	const json = JSON.parse(text) as PrecisionRule;
	const numbers = (list: unknown) => Array.isArray(list) && list.every((v) => typeof v === "number");
	if (!Number.isInteger(json.sig) || json.sig < 1) throw new Error(`${where}: sig must be a positive integer`);
	if (json.maxSig !== undefined && (!Number.isInteger(json.maxSig) || json.maxSig < json.sig)) throw new Error(`${where}: maxSig must be an integer of at least sig`);
	if (!numbers(json.values) || (json.derived !== undefined && !numbers(json.derived)) || (json.other !== undefined && !numbers(json.other))) throw new Error(`${where}: values, derived and other must be lists of numbers`);
	return { sig: json.sig, maxSig: json.maxSig ?? 4, overrides: json.overrides ?? {}, values: json.values, derived: json.derived ?? [], other: json.other ?? [] };
}

// ---------------------------------------------------------------------------------------------
// Collecting what a reader can get.
// ---------------------------------------------------------------------------------------------

/** Runs in the page: text and values outside the charts, and each chart's model, name and readouts. */
export function pageProbe({ scaleRoles, valueRoles }: { scaleRoles: string[]; valueRoles: string[] }) {
	const outside = document.body.cloneNode(true) as HTMLElement;
	for (const node of outside.querySelectorAll("figure[data-rv-chart], script, style, noscript")) node.remove();
	const cells = [...document.querySelectorAll(".rv-table td.num, .rv-table tfoot td")].filter((td) => !td.closest("figure[data-rv-chart]")).map((td) => td.textContent!.trim());
	const metrics = [...document.querySelectorAll(".rv-metric")].filter((el) => !el.closest("figure[data-rv-chart]")).map((el) => [...el.children].map((c) => c.textContent!.trim()).join(" / "));
	const models = window.__richview ? window.__richview.charts() : [];
	const figures = [...document.querySelectorAll("figure[data-rv-chart]")];
	if (models.length !== figures.length) throw new Error(`Chart model count ${models.length} does not match figure count ${figures.length}`);
	return {
		text: outside.textContent!.replace(/\s+/g, " ").trim(),
		cells,
		metrics,
		charts: figures.map((figure, i) => {
			const model = models[i];
			const geometry = model?.geometry;
			const panels = geometry?.panels ?? [];
			const texts = [...panels.flatMap(p => p.items), ...(geometry?.figure_items ?? [])].filter(it => it.kind === "text");
			return {
				id: figure.id,
				kind: geometry?.kind,
				title: figure.querySelector(".rv-chart__title")?.textContent!.trim() ?? "",
				name: figure.querySelector(".rv-chart__frame")?.getAttribute("aria-label") ?? "",
				summary: geometry?.summary ?? "",
				labels: texts.filter((it) => !scaleRoles.includes(it.role)).map((it) => it.text),
				// A key entry that is only a number is a colour scale's tick, which follows the key's length like an axis tick.
				valueLabels: texts.filter((it) => valueRoles.includes(it.role) && it.role !== "key").map((it) => it.text),
				keyLabels: texts.filter((it) => it.role === "key" && !/^[\s\d.,%+\u2212-]+$/.test(it.text)).map((it) => it.text),
				dropped: (geometry?.dropped ?? []).map((d) => d.text),
				// Explicitly typed scale drops vary with available space. Keep them in dropped for
				// precision/disclosure checks, as with visible ticks, but not in data-value equality.
				valueDropped: (geometry?.dropped ?? []).filter(d => d.kind !== "axis label" && d.kind !== "scale-bar").map(d => d.text),
				ticks: texts.filter((it) => scaleRoles.includes(it.role)).map((it) => it.text),
				readouts: panels.flatMap((p) => p.observations.map((o) => `${o.readout.heading}: ${o.readout.lines.map((l) => `${l.name} ${l.value}`).join("; ")}`)),
				readoutRecords: panels.flatMap((p) => p.observations.map((o) => ({ targeting: p.targeting, readout: o.readout }))),
				marks: panels.reduce((n, p) => n + p.observations.length, 0),
				visible: figure.querySelector(".rv-chart__frame")?.getBoundingClientRect().width! > 0,
			};
		}),
	};
}

/** Walk one chart's marks by keyboard and return each readout's text; `reason` says why it could not. */
async function keyboardReadouts(page: Page, figure: Locator, marks: number, timeout = 5000) {
	const frame = figure.locator(".rv-chart__frame");
	const count = Math.min(marks, MAX_KEY_STEPS);
	let collector: JSHandle<Walk> | undefined;
	let detached = false;
	try {
		await figure.scrollIntoViewIfNeeded({ timeout });
		await frame.focus({ timeout });
		// The runtime updates on keydown. Snapshot after each trusted keyup in the page, keeping
		// the actual keyboard walk while fetching its readouts together instead of once per mark.
		collector = await figure.evaluateHandle((node, count: number) => {
			const frame = node.querySelector<HTMLElement>(".rv-chart__frame");
			const readout = node.querySelector<HTMLElement>(".rv-chart__readout");
			if (!frame || !readout) throw new Error("Chart frame or readout missing");
			const texts: string[] = [];
			const keys: string[] = [];
			const snapshot = (event: KeyboardEvent) => {
				if (!event.isTrusted || !["Home", "ArrowRight", "Escape"].includes(event.key)) return;
				keys.push(event.key);
				if (event.key === "Escape" || count === 0) return;
				// Text nodes joined by a space, so neighbouring numbers stay apart.
				texts.push((readout.hidden ? "" : [...readout.querySelectorAll("*")].filter((e) => !e.firstElementChild).map((e) => e.textContent!.trim()).filter(Boolean).join(" ")) || "(no readout)");
			};
			frame.addEventListener("keyup", snapshot);
			return { texts, keys, stop: () => frame.removeEventListener("keyup", snapshot) };
		}, count);
		await page.keyboard.press("Home");
		for (let step = 1; step < count; step++) await page.keyboard.press("ArrowRight");
		await page.keyboard.press("Escape");
		const { texts, keys } = await collector.evaluate((walk) => {
			walk.stop();
			return { texts: walk.texts, keys: walk.keys };
		});
		detached = true;
		const expectedKeys = ["Home", ...Array(Math.max(0, count - 1)).fill("ArrowRight"), "Escape"];
		if (texts.length !== count || keys.length !== expectedKeys.length || keys.some((key, i) => key !== expectedKeys[i])) {
			throw new Error(`Keyboard walk incomplete: ${texts.length}/${count} readouts, keys ${JSON.stringify(keys)} instead of ${JSON.stringify(expectedKeys)}`);
		}
		return { texts };
	} catch (error) {
		return { texts: [], reason: (error as Error).message.split("\n")[0] };
	} finally {
		if (collector) {
			// On a failed key press, detach too. A closed/crashed page has already supplied the
			// read failure above and destroyed its listener; cleanup cannot reopen that target.
			if (!detached) await collector.evaluate((walk) => walk.stop()).catch(() => {});
			await collector.dispose().catch(() => {});
		}
	}
}

/** Open the Table view and read its cells, then close it. */
export async function tableView(figure: Locator, timeout = 5000) {
	const button = figure.locator(".rv-chart__action--table");
	try {
		await button.click({ timeout });
		const rows = await figure.locator(".rv-table tr").evaluateAll((trs) => trs.map((tr) => [...tr.children].map((c) => c.textContent!.trim()).join("\t")));
		await button.click({ timeout });
		return { rows };
	} catch (error) {
		return { rows: [], reason: (error as Error).message.split("\n")[0] };
	}
}

/** Click the CSV download and read the file. */
async function csvDownload(page: Page, figure: Locator, timeout = 5000) {
	try {
		const download: Download = await figureDownload(page, figure, "csv", { timeout: timeout + 3000 });
		const { readFileSync: read } = await import("node:fs");
		return { text: read(await download.path(), "utf8"), name: download.suggestedFilename() };
	} catch (error) {
		return { text: "", reason: (error as Error).message.split("\n")[0] };
	}
}

/** Whether a collector's failure means the browser tab itself crashed or closed. */
export const isCrash = (reason?: string | null) => /Target crashed|has been closed/.test(reason ?? "");

/** A chart's failed reads that are neither a crash nor a closed tab panel (whose chart a reader cannot see either). */
export const unreadReasons = (chart: { skipped?: string[] }) => (chart.skipped ?? []).filter(reason => !isCrash(reason) && !reason.includes("closed tab panel"));

/**
 * Read once at 5 s and, when that times out, once more at 15 s: a busy host (the release pool runs
 * several browsers) let eight reads in release 1zd time out at 5 s. A second timeout says so in the
 * reason and fails as an unread value.
 */
export async function retryOnTimeout<T extends { reason?: string }>(read: (timeout: number) => Promise<T>): Promise<T> {
	const first = await read(5000);
	if (!first.reason || !/Timeout \d+ms exceeded/.test(first.reason) || isCrash(first.reason)) return first;
	const second = await read(15000);
	return second.reason ? { ...second, reason: `${second.reason} (also after a retry at 15 s)` } : second;
}

/** Whether any chart of a collected page could not be read because a read timed out, not because the tab crashed. */
export const timedOut = (probe?: { charts?: { skipped?: string[] }[] } | null) => (probe?.charts ?? []).some((chart) => unreadReasons(chart).some((reason: string) => /Timeout \d+ms exceeded/.test(reason)));

/**
 * Collect one page: everything a reader can get from it. With `patient` off, every read gets the 5 s a
 * reader's action has and no retry: the page is then read on its own, so load cannot excuse a slow read.
 */
export async function collectPage(page: Page, settle: RuntimeSettle, { patient = true } = {}): Promise<CollectedProbe> {
	const read = <T extends { reason?: string }>(fn: (timeout: number) => Promise<T>) => (patient ? retryOnTimeout(fn) : fn(5000));
	const hasCharts = (await page.locator("figure[data-rv-chart]").count()) > 0;
	if (hasCharts) await settle(page, undefined, { models: false });
	const probe = await page.evaluate(pageProbe, { scaleRoles: [...SCALE_ROLES], valueRoles: [...VALUE_ROLES] });
	const figures = page.locator("figure[data-rv-chart]");
	let crash: string | undefined;
	for (const [i, chart] of probe.charts.entries()) {
		const figure = figures.nth(i);
		// Once the tab has crashed every later read fails the same way; record it without trying.
		const hidden = !chart.visible && "chart not visible (closed tab panel)";
		const skip = crash ?? hidden;
		const keyboard = skip ? { texts: [], reason: skip } : await read((timeout: number|undefined) => keyboardReadouts(page, figure, chart.marks, timeout));
		const table = skip ? { rows: [], reason: skip } : await read((timeout: number|undefined) => tableView(figure, timeout));
		const csv = skip ? { text: "", reason: skip } : await read((timeout: number|undefined) => csvDownload(page, figure, timeout));
		Object.assign(chart, { keyboard: keyboard.texts, table: table.rows, csv: csv.text, skipped: [keyboard.reason && `keyboard: ${keyboard.reason}`, table.reason && `table: ${table.reason}`, csv.reason && `csv: ${csv.reason}`].filter((reason): reason is string => Boolean(reason)) });
		crash ??= [keyboard.reason, table.reason, csv.reason].find((reason) => isCrash(reason));
	}
	return probe as CollectedProbe;
}

/** Strip the separators a readout's two renderings differ in. */
const squash = (text: string) => text.replace(/[\s:;|]/g, "");

/**
 * The values a reader gets from a chart's labels and key: those of the labels the model
 * dropped to avoid a collision (`geometry.dropped`) included. Which labels are drawn, and how a name
 * wraps, is presentation that varies with style and width; a number may be dropped, never changed.
 */
export function labelNumbers(chart: LabelValues) {
	const counts = (texts: string[]) => {
		const map = new Map<string, number>();
		for (const raw of texts.flatMap((t: string) => parseNumbers(t).map((n) => n.raw))) map.set(raw, (map.get(raw) ?? 0) + 1);
		return map;
	};
	// Each number as often as the labels or the key show it, whichever shows it more: a pie's shares
	// move from its labels into its key at phone width, and a repeated number still counts twice
	// (review 1h note 3).
	const labels = counts([...(chart.valueLabels ?? chart.labels), ...(chart.valueDropped ?? chart.dropped)]);
	const key = counts(chart.keyLabels ?? []);
	const all = [...new Set([...labels.keys(), ...key.keys()])].sort();
	return all.flatMap((raw) => Array(Math.max(labels.get(raw) ?? 0, key.get(raw) ?? 0)).fill(raw)).join(" | ");
}

/** Whether the keyboard walked every mark of a chart; a longer chart is judged only against its own model. */
export const walkedAll = (chart: Pick<CollectedChart, "keyboard" | "marks">) => chart.keyboard.length >= chart.marks;

/** What `flatten` records for a value its collector failed to read (a crashed page, a missing button). */
export const NOT_COLLECTED = "(not collected)";

/** A cell's collected page as the flat `{key: string}` that assertion (a) compares. */
export function flatten(probe: Omit<RawProbe, "charts"> & { charts: CollectedChart[] }) {
	const values: Record<string, string> = { "page.text": numbersOf(probe.text), "page.cells": probe.cells.join(" | "), "page.metrics": probe.metrics.join(" | ") };
	probe.charts.forEach((c, i) => {
		const at = `chart ${i} (${c.title})`;
		values[`${at} summary`] = c.summary;
		values[`${at} accessible name`] = c.name;
		values[`${at} data label numbers`] = labelNumbers(c);
		// Like the keyboard walk, the model's mark order follows the layout (brutalist and sleek order a
		// scatter's points differently at 390 px, release 1zd); the set of readouts is the value.
		values[`${at} model readouts`] = [...c.readouts].sort().join(" | ");
		// The order of the marks follows the layout (a bar chart turns at phone width); the set does not.
		// Only a crashed tab's reads are left out; its cell fails once for the crash. Any other failed
		// read keeps its empty value and fails as its own problem (review 1g should-fix 10).
		const failed = (what: string) => (c.skipped ?? []).some((reason: string) => reason?.startsWith(`${what}: `) && isCrash(reason));
		values[`${at} keyboard readouts`] = failed("keyboard") ? NOT_COLLECTED : walkedAll(c) ? [...c.keyboard].sort().join(" | ") : `(${c.marks} marks, more than the ${MAX_KEY_STEPS} walked)`;
		values[`${at} table`] = failed("table") ? NOT_COLLECTED : c.table.join(" | ");
		values[`${at} csv`] = failed("csv") ? NOT_COLLECTED : c.csv;
	});
	return values;
}

/** Keyboard readouts that are no readout of the cell's own model: `[{chart, readout}]`. */
export function keyboardOutsideModel(probe: { charts: Pick<CollectedChart, "title" | "readouts" | "keyboard" | "readoutRecords" | "kind">[] }) {
	const model = probe.charts.map((c) => new Set((c.readoutRecords
		? c.readoutRecords.map(({ readout, targeting }) => {
			const shown = displayedReadout(readout, targeting, c.kind ?? "");
			return `${shown.heading}: ${shown.lines.map((line) => `${line.name} ${line.value}`).join("; ")}`;
		}) : c.readouts).map(squash)));
	return probe.charts.flatMap((c, i) => c.keyboard.filter((k: string) => k !== "(no readout)" && !model[i].has(squash(k))).map((readout) => ({ chart: `${i} (${c.title})`, readout })));
}

/** Every output of a cell that carries values, as `[{where, text, element}]` for (b) and (c). */
export function outputs(probe: Omit<RawProbe, "charts"> & { charts: (CollectedChart & { ticks: string[] })[] }) {
	const list: { where: string; text: string; element: string; only?: "scan" | "judge" }[] = [
		{ where: "page text", text: probe.text, element: "page", only: "scan" },
		{ where: "page table cells", text: probe.cells.join("\n"), element: "page", only: "judge" },
		{ where: "page metrics", text: probe.metrics.join("\n"), element: "page", only: "judge" },
	];
	for (const c of probe.charts) {
		const add = (where: string, text: string) => list.push({ where: `${c.title}: ${where}`, text, element: c.title });
		add("summary", c.summary);
		add("accessible name", c.name);
		add("data labels", [...c.labels, ...c.dropped].join("\n"));
		add("axis labels", c.ticks.join("\n"));
		add("model readouts", c.readouts.join("\n"));
		add("keyboard readouts", c.keyboard.join("\n"));
		add("table view", c.table.join("\n"));
		add("csv download", c.csv);
	}
	return list;
}

/** Axis labels are scale marks: they are scanned for disclosure but not judged against a value. */
const SCALE_OUTPUT = /axis labels$/;

/** (b) and (c) for one cell of the precision fixture; `rule` is the parsed precision.json. */
export function judgeFixtureCell(probe: Parameters<typeof outputs>[0], rule: PrecisionRule) {
	const sources = [...rule.values, ...rule.derived];
	const rounding = [];
	const disclosure = [];
	for (const out of outputs(probe)) {
		const limit = digitsFor(rule, out.element);
		const max = maxDigitsFor(rule, out.element);
		if (out.only !== "scan" && !SCALE_OUTPUT.test(out.where)) {
			for (const bad of judgeText(out.text, sources, limit, max, rule.other)) rounding.push({ where: out.where, shown: bad.raw, expected: bad.expected, status: bad.status, source: bad.source });
		}
		if (out.only === "judge") continue; // page text holds these cells and metrics
		for (const found of scanDisclosure(out.text, sources, max)) disclosure.push({ where: out.where, shown: found.raw, source: found.source, digits: found.sig, allowed: max });
	}
	return { rounding, disclosure };
}

// ---------------------------------------------------------------------------------------------
// The run.
// ---------------------------------------------------------------------------------------------

/** The inputs, deduplicated: every-block first, then the golden list, then the precision fixture. */
export function inputList(cwd: string, only?: string) {
	const dirs = [EVERY_BLOCK, ...GOLDEN_LIST, FIXTURE];
	return [...new Set(dirs)].filter((dir) => (!only || dir.includes(only)) && existsSync(join(cwd, dir, "report.rmd"))).map((dir) => ({ id: dir, dir }));
}

const slug = (id: string) => id.replaceAll("/", "__");

export function generate(bin: string, folder: string, style: string, publicDir?: string) {
	return new Promise<{ status: number | null; errors: Diagnostic[] }>((done) => {
		const env = { ...process.env, ...(publicDir ? { RICHVIEW_PUBLIC_DIR: publicDir } : {}) };
		const child = spawn(bin, ["generate", join(folder, "report.rmd"), "--style", style, "-o", `${style}.html`, "--format", "json"], { cwd: folder, env });
		let stdout = "";
		let stderr = "";
		child.stdout.on("data", (d) => (stdout += d));
		child.stderr.on("data", (d) => (stderr += d));
		child.on("close", (status) => {
			let errors: Diagnostic[] = [];
			try {
				errors = ((JSON.parse(stdout) as { diagnostics?: Diagnostic[] }).diagnostics ?? []).filter(d => d.level === "error");
			} catch {
				errors = [{ code: "cli", message: `exit ${status}: ${(stderr || stdout).trim().slice(0, 300)}` }];
			}
			done({ status, errors });
		});
	});
}

export async function pool<T, R>(items: readonly T[], size: number, work: (item: T) => R | Promise<R>) {
	if (!Number.isSafeInteger(size) || size < 1) throw new RangeError("Worker count must be a positive safe integer");
	const results: Awaited<R>[] = [];
	const errors: unknown[] = [];
	let next = 0;
	await Promise.all(
		Array.from({ length: Math.min(size, items.length) }, async () => {
			while (next < items.length) {
				const index = next++;
				try {
					results[index] = await work(items[index]);
				} catch (error) {
					errors.push(error);
				}
			}
		}),
	);
	if (errors.length) throw new AggregateError(errors, `${errors.length} verification case(s) failed`, { cause: errors[0] });
	return results;
}

export async function runCrossStyle({ cwd, bin, out, publicDir, only, jobs }: { cwd: string; bin: string; out: string; publicDir?: string; only?: string; jobs: number }) {
	const started = Date.now();
	const binary = resolve(cwd, bin);
	if (!existsSync(binary)) throw new Error(`The release CLI ${binary} does not exist; the cross-style step needs the build step first`);
	mkdirSync(out, { recursive: true });
	const real = realpathSync(out);
	const styles = builtinStyles(cwd);
	const inputs = inputList(cwd, only);
	const missing = [EVERY_BLOCK, ...GOLDEN_LIST, FIXTURE].filter((dir) => !only && !existsSync(join(cwd, dir, "report.rmd")));
	if (missing.length) throw new Error(`Cross-style inputs without a report.rmd: ${missing.join(", ")}`);
	const rule = parsePrecision(readFileSync(join(cwd, FIXTURE, "precision.json"), "utf8"));
	const publicFolder = publicDir ? resolve(cwd, publicDir) : [join(dirname(binary), "dist/web"), join(cwd, "dist/web")].find((p) => existsSync(p));

	// Copy each input into a real folder under `out` (a symlinked /tmp trips report.root.escape) and render it.
	const problems: MatrixProblem[] = [];
	const inputsRoot = join(real, "inputs");
	rmSync(inputsRoot, { recursive: true, force: true });
	const rendered = await pool(inputs, jobs, async (input: { id: string; dir: string; }) => {
		const folder = join(inputsRoot, slug(input.id));
		cpSync(join(cwd, input.dir), folder, { recursive: true, filter: (src) => !SKIP_COPY.has(basename(src)) });
		const results = [];
		for (const style of styles) {
			const run = await generate(binary, folder, style, publicFolder);
			if (run.status !== 0 || run.errors.length) problems.push({ input: input.id, style, message: `generate failed: ${run.errors.map((e) => `${e.code}: ${e.message}`).join("; ") || `exit ${run.status}`}` });
			else results.push(style);
		}
		return results;
	});
	const { chromium, errors } = await import(join(cwd, "node_modules/playwright/index.mjs")) as typeof import("playwright");
	const { serveStatic, chromiumPath, waitForBenchmark } = await import(join(cwd, "scripts/browser-harness.ts")) as BrowserHarness;
	const { settle } = await import(join(cwd, "scripts/report-runtime.ts")) as { settle: RuntimeSettle };
	const server = await serveStatic({ "/": inputsRoot });
	await waitForBenchmark();
	const browser = await launchBrowser(chromium, { headless: true, executablePath: chromiumPath(), args: ["--no-sandbox"] }, { TimeoutError: errors.TimeoutError });
	const tasks = inputs.flatMap((input, i) => rendered[i].flatMap((style) => WIDTHS.map((width) => ({ input, style, width }))));
	const probes = new Map<string, CollectedProbe>();
	mkdirSync(join(real, "cells"), { recursive: true });
	const collectCell = async ({ input, style, width }: MatrixTask, retried = false) => {
		const id = `${style}@${width}`;
		const context = await browser.newContext({ viewport: { width, height: 900 }, deviceScaleFactor: 1, reducedMotion: "reduce", colorScheme: "light", acceptDownloads: true });
		try {
			const page = await context.newPage();
			const errors: string[] = [];
			page.on("pageerror", (e) => errors.push(e.message));
			const response = await page.goto(`${server.base}/${slug(input.id)}/Rendered/${style}.html`, { waitUntil: "load" });
			if (!response || response.status() >= 400) throw new Error(`HTTP ${response?.status() ?? "no response"}`);
			const probe = await collectPage(page, settle, { patient: !retried });
			probe.errors = errors;
			if (retried) probe.retried = true;
			probes.set(`${input.id}|${id}`, probe);
			const file = join(real, "cells", `${slug(input.id)}__${style}-${width}.json`);
			writeFileSync(file, `${JSON.stringify(probe, null, 1)}\n`);
		} catch (error) {
			problems.push({ input: input.id, style, width, message: (error as Error).message.split("\n")[0] });
		} finally {
			await context.close();
		}
	};
	try {
		await pool(tasks, jobs, (task) => collectCell(task));
		// A page whose reads timed out under the release pool's load is read once more on its own, after
		// the pool, when nothing else of this step competes for the host. Alone, each read gets the 5 s a
		// reader's action has and no retry, so a slow page still fails (review 1j note 7).
		const slow = tasks.filter(({ input, style, width }) => timedOut(probes.get(`${input.id}|${style}@${width}`)));
		for (const task of slow) await collectCell(task, true);
	} finally {
		await browser.close();
		await server.close();
	}

	// (a) identical values across the styles and widths of an input.
	const mismatches = [];
	const skipped: { input: string; cell: string; chart: string; why: string[] }[] = [];
	const pageErrors = [];
	let valueKeys = 0;
	// Per chart, how many of its keys were compared across how many cells, so a type is judged on its own charts (review 1l should-fix 4).
	const comparedCharts: { input: string; chart: number; title: string; keys: number; cells: number }[] = [];
	for (const input of inputs) {
		const cells = [...probes.entries()].filter(([key]) => key.startsWith(`${input.id}|`)).map(([key, probe]) => ({ id: key.split("|")[1], values: flatten(probe), probe }));
		valueKeys += Object.keys(cells[0]?.values ?? {}).length;
		(cells[0]?.probe.charts ?? []).forEach((c, i) => {
			const prefix = `chart ${i} (${c.title}) `;
			comparedCharts.push({ input: input.id, chart: i, title: c.title, keys: Object.keys(cells[0].values).filter((key) => key.startsWith(prefix)).length, cells: cells.length });
		});
		for (const m of compareCells(cells)) mismatches.push({ input: input.id, ...m });
		for (const cell of cells) {
			for (const e of cell.probe.errors!) pageErrors.push({ input: input.id, cell: cell.id, message: e });
			cell.probe.charts.forEach((c, i) => c.skipped.length && skipped.push({ input: input.id, cell: cell.id, chart: `${i} (${c.title})`, why: c.skipped }));
			// A crashed page is one failure of its cell, named as such, instead of a difference per chart value.
			const crashed = cell.probe.charts.filter((c) => (c.skipped ?? []).some(isCrash));
			const [style, width] = cell.id.split("@");
			if (crashed.length) problems.push({ input: input.id, style, width: Number(width) || undefined, message: `the page crashed while its charts were read; ${crashed.length} of ${cell.probe.charts.length} charts not collected` });
			// A read that failed for another reason is a reader who could not get the value: a failure.
			cell.probe.charts.forEach((c, i) => {
				const unread = unreadReasons(c);
				if (unread.length) problems.push({ input: input.id, style, width: Number(width) || undefined, message: `chart ${i} (${c.title}) could not be read: ${unread.join("; ")}` });
			});
		}
	}

	const keyboard = [];
	for (const [key, probe] of probes) for (const k of keyboardOutsideModel(probe)) keyboard.push({ input: key.split("|")[0], cell: key.split("|")[1], ...k });

	// (b) and (c) on the precision fixture, and (c) on the generated HTML of every style.
	const rounding = new Map<string, ReturnType<typeof judgeFixtureCell>["rounding"][number] & { cells: string[] }>();
	const disclosure = new Map<string, ReturnType<typeof judgeFixtureCell>["disclosure"][number] & { cells: string[] }>();
	const note = <Row extends object>(map: Map<string, Row & { cells: string[] }>, row: Row, cell: string) => {
		const key = JSON.stringify(row);
		map.set(key, { ...row, cells: [...(map.get(key)?.cells ?? []), cell] });
	};
	const sources = [...rule.values, ...rule.derived];
	for (const [key, probe] of probes) {
		const [input, cell] = key.split("|");
		if (input !== FIXTURE) continue;
		const judged = judgeFixtureCell(probe, rule);
		for (const row of judged.rounding) note(rounding, row, cell);
		for (const row of judged.disclosure) note(disclosure, row, cell);
	}
	if (inputs.some((i) => i.id === FIXTURE)) {
		for (const style of rendered[inputs.findIndex((i) => i.id === FIXTURE)]) {
			const file = join(inputsRoot, slug(FIXTURE), "Rendered", `${style}.html`);
			if (!existsSync(file)) continue;
			const html = scannableHtml(readFileSync(file, "utf8"));
			for (const found of scanDisclosure(html, sources, rule.maxSig ?? 4)) note(disclosure, { where: `generated HTML, ${locate(html, found.raw)}`, shown: found.raw, source: found.source, digits: found.sig, allowed: rule.maxSig ?? 4 }, style);
		}
	}
	const summary = {
		inputs: inputs.map((i) => i.id),
		styles,
		widths: WIDTHS,
		pages: probes.size,
		comparedKeys: valueKeys,
		comparedCharts,
		seconds: (Date.now() - started) / 1000,
		mismatches,
		keyboard,
		rounding: [...rounding.values()],
		disclosure: [...disclosure.values()],
		skipped,
		pageErrors,
		problems,
		retried: [...probes].filter(([, probe]) => probe.retried).map(([key]) => key.replace("|", " ")),
	};
	writeFileSync(join(real, "cross-style.json"), `${JSON.stringify(summary, null, 2)}\n`);
	return summary;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
	const arg = (name: string) => {
		const at = process.argv.indexOf(name);
		return at < 0 ? undefined : process.argv[at + 1];
	};
	const bin = arg("--bin");
	const out = arg("--out");
	if (!bin || !out) {
		console.error("usage: cross-style.mts --bin <richview> --out <dir> [--public <dist/web>] [--only <text>] [--jobs <n>] [--cwd <repo>]");
		process.exit(2);
	}
	const summary = await runCrossStyle({ cwd: resolve(arg("--cwd") ?? process.cwd()), bin, out: resolve(out), publicDir: arg("--public"), only: arg("--only"), jobs: Number(arg("--jobs") ?? Math.max(2, Math.min(4, cpus().length - 1))) });
	const cut = (s: string, n = 160) => (s.length > n ? `${s.slice(0, n)}...` : s);
	for (const p of summary.problems) console.log(`FAIL not collected ${p.input} [${p.style}${p.width ? `@${p.width}` : ""}]: ${p.message}`);
	for (const e of summary.pageErrors) console.log(`FAIL page error ${e.input} [${e.cell}]: ${e.message}`);
	for (const m of summary.mismatches) console.log(`FAIL (a) ${m.input}: ${m.key} differs: ${m.groups.map((g) => `${JSON.stringify(cut(g.value))} in ${g.cells.join(", ")}`).join(" vs ")}`);
	for (const k of summary.keyboard) console.log(`FAIL (a) ${k.input} [${k.cell}]: chart ${k.chart} keyboard readout ${JSON.stringify(cut(k.readout))} is no readout of its own model`);
	for (const r of summary.rounding) console.log(`FAIL (b) ${r.where}: shows ${r.shown}, rule gives ${r.expected} (${r.status}, source ${r.source}) in ${r.cells.length} cells`);
	for (const d of summary.disclosure) console.log(`FAIL (c) ${d.where}: ${d.shown} is source ${d.source} at ${d.digits} digits, ${d.allowed} allowed, in ${d.cells.length} cells`);
	for (const s of summary.skipped) console.log(`note skipped ${s.input} [${s.cell}] chart ${s.chart}: ${s.why.join("; ")}`);
	for (const r of summary.retried) console.log(`note re-read ${r} on its own after a read timed out in the pool`);
	const failed = summary.problems.length + summary.pageErrors.length + summary.mismatches.length + summary.keyboard.length + summary.rounding.length + summary.disclosure.length;
	console.log(`cross-style: ${summary.inputs.length} inputs x ${summary.styles.length} styles x ${summary.widths.length} widths = ${summary.pages} pages in ${summary.seconds.toFixed(1)} s; ${summary.mismatches.length} cross-cell mismatches, ${summary.keyboard.length} keyboard readouts outside the model, ${summary.rounding.length} rounding mismatches, ${summary.disclosure.length} disclosures, ${summary.problems.length} not collected`);
	process.exit(failed ? 1 : 0);
}
