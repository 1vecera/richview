// The label-truth step of the release run (TYP-36, FEA-CH-37): the value texts drawn on a chart
// (data labels, key entries, annotations, panel titles) show the author's values.
//
// A reader sees "0.488" on a heatmap cell whose Table says 0.5 when the text is formatted from a
// grid-quantised position instead of the shown value. So for every report under examples/ in the
// default style, each visible chart's value texts are compared with the numbers of its own Table
// view. A number that is no rounding of any Table value but a near miss of one is a finding.
//
// States per chart, each read after `window.__richview.settled()` together with that state's own
// Table view: the initial chart at 1,100 and 390 px; the second option of each filter; a playback
// stepped one frame and taken to its first frame; a zoom of two "+" presses; the first legend button
// toggled off; and, for a chart in a tab panel, all of these once its tab is open, at both widths.
//
// Two further judgements need no near miss. For the chart types that print the author's values
// (VALUE_TYPES), a data label or readout number that is in no row of the state's Table, nor a share,
// total or ratio of it, nor inside the data's range, is wrong data: a filter, frame or legend toggle
// that left the text of another state on the chart. And on a line, area or scatter chart a readout
// whose heading names another x than the one its mark is drawn at (a zoom that kept the unzoomed
// readouts) is a finding.
//
// By mark (TYP-36): a value text names its mark (`Text::mark`; a readout is its observation's), and
// that mark is judged exactly. A mark that is one row of the state ("row:<n>") is the n-th row of the
// state's Table; every number the text prints must round from that row, or be a share, sum or running
// total of the column. A bin or quartile has no row, so a label is judged against its own readout;
// a box plot's median label is judged that way too, though the box plot prints statistics.
// Nearness to the Table is only the fallback for a text with no mark (a key, an annotation, a total).
//
// Drawn against read: a rect mark of a bar-like chart (histogram, bar, Pareto, waterfall, funnel) is
// measured on the panel's value axis, read from its numeric labels and the grid lines beside them,
// and must match the readout of its observation; a Pareto or other chart whose note says "sorted"
// must read in that order; a line's hover dots are measured the same way. Histogram counts and box
// plot counts, medians and quartiles are recomputed from the state's Table rows.
//
// Usage: node tools/rv-gate/label-truth.mjs --bin target/release/richview --out <dir>
//        [--public <dist/web>] [--only <text>] [--jobs <n>] [--cwd <repo>] [--fixtures]
//
// `--fixtures` renders tools/rv-gate/fixtures/label-truth instead of examples/ and checks each
// fixture against FIXTURE_EXPECT.

import { launchBrowser } from "./lib/browser-launch.mjs";
import { cpSync, existsSync, mkdirSync, readdirSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { writeFile } from "node:fs/promises";
import { cpus } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { SCALE_ROLES, VALUE_ROLES, generate, isCount, parseNumbers, pool, retryOnTimeout, roundSig, same, tableView } from "./cross-style.mjs";
import { SKIP_COPY } from "./lib/connectors.mjs";

/** The style every report is rendered in: the product default (DEFAULT_STYLE of core). */
export const STYLE = "sleek";
/** Reports that need a connector and cannot render offline. */
export const SKIP_REPORTS = new Set(["metricflow-report", "sql-report"]);
/** The kinds of state a chart is read in; the run names each one, such as "filter South (1100)" or "frame 1960 (1100)". */
export const STATES = ["initial", "filter <option>", "frame <label>", "first frame", "zoom", "legend off"];
/** The chart types whose derived readouts (shares, totals, ratios) the judge can recompute from the Table alone. */
export const MODELLED = new Set(["pie", "donut", "funnel", "pareto"]);
/**
 * The chart types whose readouts and data labels print the author's values (or the MODELLED shares),
 * so a number of them that is in no row of the Table is wrong data. The other types print statistics,
 * bin edges, ranges and totals the judge cannot recompute, and keep to the near-miss judgement of their texts.
 */
export const VALUE_TYPES = new Set([...MODELLED, "bar", "line", "area", "scatter", "lollipop", "heatmap"]);
/** The chart types whose readout headings name the x their mark is drawn at. */
export const PLACED = new Set(["line", "area", "scatter"]);
/** The text roles asserted to be the Table's numbers (for a VALUE_TYPES chart), not only not to be near misses of them. */
export const STRICT = new Set(["label", "readout"]);
/** Readout lines that count rows or marks, whose number is no Table value. */
export const COUNT_NAMES = /^(n|count|rows?|observations?|items?)$/i;
/** The widths every page is read at; the interactive states are read at the desktop one. */
export const WIDTHS = [1100, 390];
/** The chart types whose texts print a column's sum and the shares of it (a stacked chart's totals are added by its `stack` option). */
export const SHARE_TYPES = new Set(["pie", "donut", "waffle", "funnel", "pareto", "treemap", "sunburst", "icicle"]);
/**
 * The step's own fixtures (review 1j): reports whose correct rendering has no near miss, so each must
 * raise no finding. The judge's catching of wrong text is the unit tests' job; a fixture that expected
 * findings would fail the day the product is fixed.
 */
export const FIXTURES = "tools/rv-gate/fixtures/label-truth";
export const FIXTURE_EXPECT = {
	"heat-pass": "right",
	"heat-nozero": "right",
	"bar-labels": "right",
	"pie-shares": "right",
	"filter-states": "right",
	"playback-frames": "right",
	"zoom-line": "right",
	"regrid-histogram": "right",
	"regrid-hidden": "right",
	"regrid-pareto": "right",
	"box-narrow": "right",
};
/**
 * Correct texts the judge cannot tell from grid text, each with its reason. Empty: texts that name a mark are
 * judged by it, and the gapminder size key's midpoint needs no entry.
 */
export const ALLOWED = [];
/** A near miss lies within this relative distance of a Table value, or within a column-span tolerance, whichever is larger. */
export const NEAR = 0.03;
/** A grid step is about span / 4,095, so span / 512 is about eight steps. */
export const GRID_DIVISOR = 512;

// ---------------------------------------------------------------------------------------------
// Pure part: is a shown number the author's?
// ---------------------------------------------------------------------------------------------

const MINUS = /^[-−–]$/;

/** A text with the no-break space the product prints before a one-letter scale ("120 k") removed, so the scale is read. */
export const normalise = (text) => String(text).replace(/[\u00a0\u202f]([KMBTk])(?![A-Za-z])/g, "$1");

/**
 * The numbers of a text as `{...token, signed}`: the sign is a minus directly before the number that
 * follows a space, bracket or the start, or a true minus sign after a letter ("Δ−3"). A hyphen after a
 * letter is a word's ("Wi-Fi 6") and stays unsigned.
 */
export function signedNumbers(input) {
	const text = normalise(input);
	return parseNumbers(text).map((token) => {
		const before = String(text)[token.index - 1];
		const lead = String(text)[token.index - 2];
		const negative = before !== undefined && MINUS.test(before) && (lead === undefined || /[\s([:;,/]/.test(lead) || (before === "−" && /\p{L}/u.test(lead)));
		return { ...token, negative, signed: negative ? -token.value : token.value, raw: negative ? `−${token.raw}` : token.raw };
	});
}

/** Whether `token` (unsigned magnitude `shown`) is a rounding of `value` to the token's own significant digits. */
const roundsTo = (token, value) => same(token.signed, roundSig(value, token.sig));

/** The values a Table number can show as: itself, and times 100 for a percentage token. */
const asShown = (token, value) => (token.percent ? [[value, 1], [value * 100, 100]] : [[value, 1]]);

/**
 * The tolerance of a near miss of `candidate` on a grid of the given span; `factor` is 100 for a percentage token.
 * A percent token has no grid: it is a share or a rounded value, so only the relative tolerance applies, never the column's span.
 */
const tolerance = (candidate, span, factor, percent = false) => (percent ? NEAR * Math.abs(candidate) : Math.max(NEAR * Math.abs(candidate), (span * factor) / GRID_DIVISOR));

/** Whether the token is a bound ("<0.1%", ">99.9%", "≥ 5"): it names a limit, not a value, and is no near miss of one. */
const isBound = (token, text) => /[<>≤≥]\s*$/.test(String(text).slice(0, token.index + 1).trimEnd()) || /[<>≤≥]\s*$/.test(String(text).slice(0, token.index));

/**
 * The numbers a column's values can be shown as besides themselves, only where the text prints them:
 * the midpoint of its range for a size or colour key entry (`key`); and, for a chart that prints
 * shares or totals (`shares`), its sum and each value's share of the sum as a percentage and as a
 * fraction. A stacked chart passes `groups`, a label per value, so each stack's total and shares count too.
 * A funnel (`ratios`) also prints shares of the first and the previous stage, a Pareto (`cumulative`)
 * its running share.
 */
function derivedValues(values, { key = false, shares = false, groups, ratios = false, cumulative = false } = {}) {
	const derived = [];
	const percents = (numbers) => numbers.flatMap((n) => [n * 100, n]);
	// A funnel prints each stage as a share of the first and of the previous stage, in row order.
	if (ratios) derived.push(...percents(values.flatMap((v, i) => [values[0] !== 0 ? v / values[0] : v, ...(i && values[i - 1] !== 0 ? [v / values[i - 1]] : [])])));
	// A Pareto prints the running share of the values ranked from the largest.
	if (cumulative) {
		const total = values.reduce((a, b) => a + b, 0);
		let run = 0;
		for (const v of [...values].sort((a, b) => b - a)) {
			run += v;
			derived.push(run, ...(total !== 0 ? percents([run / total]) : []));
		}
	}
	if (key && values.length) derived.push((Math.min(...values) + Math.max(...values)) / 2);
	if (shares) {
		const buckets = [values.map((_, i) => i)];
		if (groups) for (const label of new Set(groups)) buckets.push(values.flatMap((_, i) => (groups[i] === label ? [i] : [])));
		for (const bucket of buckets) {
			const sum = bucket.reduce((a, i) => a + values[i], 0);
			derived.push(sum);
			if (sum !== 0) derived.push(...bucket.flatMap((i) => [(values[i] / sum) * 100, values[i] / sum]));
		}
	}
	return derived;
}

/**
 * Judge the numbers of one value text against the Table's numeric columns (signed, in base units)
 * and return `[{raw, near}]` for each that is a near miss. `columns` is a list of columns, or one
 * flat list of numbers; `loose` are further numbers (those in text cells) a token may round.
 *
 * `options` says what the text and the chart can print:
 * - `spans[k]` is the grid span of column k, `payload.quant`'s max - min for the value channel the
 *   column shows; channels that share a grid share it. A column without one falls back to its own
 *   span, which is the grid's only where the column has the grid to itself.
 * - `shares` is true for a chart type that prints a column's sum and shares of it, and `groups[k]`
 *   labels the rows of column k by stack when the chart is stacked.
 * - `key` is true for a size or colour key entry, the one text that prints a column's midpoint.
 *
 * Everything is judged per column. A token near a value of column c is excused by c's own values and
 * by the numbers derived from c alone, never by another column's sum, share or midpoint. It is a
 * near miss of a value v in c when |x - v| is at most max(3 % of |v|, span_c / 512), which is the
 * zero rule for a v of 0, or when its sign differs from v and ||x| - |v|| is within the same
 * tolerance (a sign flip, which includes |x| rounding to |v|). A column holding a 0 also catches a
 * non-zero x below 3 % of its largest |v| that rounds nothing of the column: grid text for the zero
 * (a value printed at zero precision). A count ("3 categories") and a number near no Table value are
 * not judged. An annotation such as "Goal 3.05" beside a 3 is no rounding of anything and, within
 * tolerance, is flagged: that is accepted, since without mark ids nearness is all there is to judge by.
 */
export function judgeValueText(input, columns, loose = [], options = {}) {
	const text = normalise(input);
	const cols = (typeof columns[0] === "number" ? [columns] : columns).filter((c) => c.length);
	const { spans = [], shares = false, key = false, groups = [], strict = false, ratios = false, cumulative = false } = options;
	const accepted = cols.map((c, k) => [...c, ...derivedValues(c, { key, shares, groups: groups[k], ratios, cumulative })]);
	const gridSpan = cols.map((c, k) => spans[k] ?? Math.max(...c) - Math.min(...c));
	const found = [];
	for (const token of signedNumbers(text)) {
		if (isCount(token, text) || isBound(token, text)) continue;
		if (loose.some((value) => asShown(token, value).some(([c]) => roundsTo(token, c)))) continue;
		let near;
		let nearest = Infinity;
		const rounds = (value) => asShown(token, value).some(([c]) => roundsTo(token, c));
		// A rounding of any column's own value is that value; only derived numbers (sums, shares, midpoints) are a column's alone.
		const pooled = cols.some((column) => column.some(rounds));
		const excusedBy = (k) => pooled || accepted[k].some(rounds);
		for (const [k, column] of cols.entries()) {
			if (excusedBy(k)) continue;
			const bigger = Math.max(...column.map(Math.abs));
			for (const value of column) {
				for (const [c, factor] of asShown(token, value)) {
					const tol = tolerance(c, gridSpan[k], factor, token.percent);
					const off = Math.abs(token.signed - c);
					const flipped = c !== 0 && Math.sign(token.signed) !== Math.sign(c) && Math.abs(Math.abs(token.signed) - Math.abs(c)) <= tol;
					// A column holding a 0 prints it as 0, so a small non-zero number is the zero's grid text, whatever the span.
					const zero = value === 0 && token.signed !== 0 && Math.abs(token.signed) < NEAR * bigger * factor;
					if (off <= tol || flipped || zero) {
						const distance = flipped ? Math.abs(Math.abs(token.signed) - Math.abs(c)) : off;
						if (distance < nearest) [near, nearest] = [value, distance];
					}
				}
			}
		}
		if (near !== undefined) found.push({ raw: token.raw, near });
		else if (strict && !cols.some((_, k) => excusedBy(k))) {
			// Far from every Table value and outside every column's range, widened by the grid tolerance: no row of this state prints it.
			const inRange = cols.some((column, k) => asShown(token, 0).some(([, factor]) => token.signed >= (Math.min(...column) - gridSpan[k] / GRID_DIVISOR) * factor && token.signed <= (Math.max(...column) + gridSpan[k] / GRID_DIVISOR) * factor && (!token.percent || factor === 100)));
			if (!inRange) found.push({ raw: token.raw, near: undefined });
		}
	}
	return found;
}

/**
 * Judge where a chart's readouts sit: each `{heading, x}` names an x (a number, or a date on a calendar
 * axis) and `x` is where its mark is drawn. Returns the headings that name another x than their mark's,
 * by more than half the closest spacing of the marks or 1 % of the visible axis, which is a readout
 * kept from a layout with a different zoom or hidden set. A heading that is no single number or date is not judged.
 */
export function judgePlacement({ calendar = false, width = 0, points }) {
	const named = points.flatMap((point) => {
		const dated = calendar ? Date.parse(`${point.heading} UTC`) : NaN;
		const numbers = calendar ? [] : signedNumbers(point.heading);
		const value = calendar ? dated : numbers.length === 1 ? numbers[0].signed : NaN;
		return Number.isFinite(value) ? [{ ...point, value }] : [];
	});
	const sorted = [...new Set(named.map((point) => point.x))].sort((a, b) => a - b);
	const gaps = sorted.slice(1).map((x, i) => x - sorted[i]).filter((gap) => gap > 0);
	if (!gaps.length) return [];
	const tol = Math.max(Math.min(...gaps) / 2, width / 100);
	return named.filter((point) => Math.abs(point.value - point.x) > tol).map((point) => ({ raw: point.heading, at: point.x, named: point.value }));
}

/** Whether a Table cell is only a number (with its sign, unit or currency). */
const numericCell = (cell) => {
	const tokens = signedNumbers(cell);
	return tokens.length === 1 && cell.replace(tokens[0].raw.replace("−", ""), "").replace(/[-−–+\s]/g, "") === "";
};

/**
 * A Table view (header row, then rows, tab-separated) as `{columns, loose, names, groups}`: each
 * numeric column's signed values, and the numbers in other cells. `names[k]` is the header of column
 * k and `groups[k]` labels each of its values by the row's first text cell (the stack, in a stacked chart).
 */
export function tableColumns(rows) {
	const header = (rows[0] ?? "").split("\t");
	const body = rows.slice(1).map((row) => row.split("\t"));
	const columns = [];
	const loose = [];
	const names = [];
	const groups = [];
	for (let c = 0; c < Math.max(0, ...body.map((r) => r.length)); c++) {
		const cells = body.map((r) => r[c] ?? "").filter(Boolean);
		if (cells.length && cells.every(numericCell)) {
			columns.push(cells.map((cell) => signedNumbers(cell)[0].signed));
			names.push(header[c] ?? "");
			groups.push(body.filter((r) => r[c]).map((r) => r.find((cell) => cell && !numericCell(cell)) ?? ""));
		} else loose.push(...cells.flatMap((cell) => signedNumbers(cell).map((t) => t.signed)));
	}
	return { columns, loose, names, groups };
}

/**
 * The grid span of each Table column: the max - min of the `quant` entry of the payload column the
 * header names (compared without case or punctuation), so columns on one grid share its span; a
 * column that names none gets `undefined` and the judge falls back to its own span.
 */
export function gridSpans(names, quant = {}) {
	const squash = (name) => String(name).toLowerCase().replace(/[^a-z0-9]/g, "");
	const entries = Object.entries(quant ?? {}).map(([name, grid]) => [squash(name), grid]);
	return names.map((header) => {
		const want = squash(header);
		const grid = entries.find(([name]) => name === want)?.[1] ?? entries.filter(([name]) => name.length >= 3 && want.startsWith(name)).sort((a, b) => b[0].length - a[0].length)[0]?.[1];
		return grid ? grid.max - grid.min : undefined;
	});
}

/**
 * The values of each Table column across every row of the chart, at the author's precision
 * (`payload.shown`, matched to the header as `gridSpans` matches grids), or an empty list. A playback
 * chart's size and colour keys are fitted over every frame (d104343b), so a key entry is judged
 * against these, not only against the current frame's Table.
 */
export function shownColumns(names, shown = {}) {
	const squash = (name) => String(name).toLowerCase().replace(/[^a-z0-9]/g, "");
	const entries = Object.entries(shown ?? {}).filter(([, values]) => Array.isArray(values)).map(([name, values]) => [squash(name), values.filter((v) => typeof v === "number" && Number.isFinite(v))]);
	return names.map((header) => {
		const want = squash(header);
		return entries.find(([name]) => name === want)?.[1] ?? entries.filter(([name]) => name.length >= 3 && want.startsWith(name)).sort((a, b) => b[0].length - a[0].length)[0]?.[1] ?? [];
	});
}

/** A playback key spans every frame of the active filters, without pooling other cohorts. */
export function playbackKeyColumns(names, chart) {
	const { dataColumns = [], dataRows = [], filters = [], shown = {} } = chart;
	const keep = dataRows.map((_, i) => i).filter(i => filters.every(({ field, value }) => {
		if (value === null) return true;
		const at = dataColumns.indexOf(field);
		if (at < 0) throw new Error(`Playback key filter column ${field} is missing`);
		const cell = shown[field]?.[i] ?? dataRows[i][at];
		return cell !== null && cell !== undefined && String(cell).trim() === String(value).trim();
	}));
	return shownColumns(names, Object.fromEntries(Object.entries(shown).map(([field, values]) => {
		if (!Array.isArray(values) || values.length !== dataRows.length) throw new Error(`Playback key column ${field} is not aligned with its rows`);
		return [field, keep.map(i => values[i])];
	})));
}

/** The numbers a Table view shows: every number of every row, as `parseNumbers` reads them. */
export function tableNumbers(rows) {
	return rows.flatMap((row) => row.split("\t").flatMap((cell) => parseNumbers(cell).map((t) => t.value)));
}

// ---------------------------------------------------------------------------------------------
// Pure part: texts judged by their mark, marks judged against their readouts.
// ---------------------------------------------------------------------------------------------

/** Half a unit of the last printed digit of `token`, in base units: the most a rounding to the author's precision can move a value. */
export const halfUnit = (token) => (token.value ? 0.5 * 10 ** (Math.floor(Math.log10(Math.abs(token.value))) - token.sig + 1) : 0);

/**
 * Half a unit of the last digit printed, counting every digit shown: "10" is exact to 0.5, "10.0" to 0.05, "120 k" to 500.
 * For a bin edge, which has no trailing zeros to be unsure of; `halfUnit` is the looser reading for a value whose zeros may be rounding.
 */
export const halfPrinted = (token) => {
	const [whole, fraction = ""] = token.raw.replace(/[^\d.]/g, "").split(".");
	const mantissa = Number(`${whole}.${fraction || "0"}`);
	return 0.5 * 10 ** -fraction.length * (mantissa ? Math.abs(token.value) / mantissa : 1);
};

/** The pixels a drawn extent may differ from its scale by: antialiasing, the 1 px outline of a mark. */
export const PIXELS = 1.5;

/** The chart types whose rect marks are drawn to a value on a linear value axis, and read out by it. */
export const BAR_TYPES = new Set(["histogram", "bar", "column", "pareto", "waterfall", "funnel"]);
/** The chart types whose readouts are statistics of the state's rows (counts, quartiles), judged by `judgeDerivedReadouts`. */
export const DERIVED_TYPES = new Set(["histogram", "boxplot"]);

const squash = (name) => String(name).toLowerCase().replace(/[^a-z0-9]/g, "");

/** Whether the centre of `rect` lies in `area`. */
const inside = (area, rect) => rect.x + rect.w / 2 >= area.x && rect.x + rect.w / 2 <= area.x + area.w && rect.y + rect.h / 2 >= area.y && rect.y + rect.h / 2 <= area.y + area.h;

/** A text and a number it prints, `{value, tolerance}` in base units; undefined when it prints none or several. */
const readNumber = (text) => {
	const tokens = signedNumbers(text);
	return tokens.length === 1 && !tokens[0].percent ? { value: tokens[0].signed, tolerance: halfUnit(tokens[0]), token: tokens[0] } : undefined;
};

/**
 * The numbers of the mark a text names. A mark whose id is `row:<n>` is the n-th row of the state
 * (`Geometry::mark_ids`; a filter or frame renumbers them), so its numbers are the cells of row n of
 * the state's Table view `table` (a header, then tab-separated rows), at the precision the Table
 * prints. A mark with another id (a bin, a quartile) has no row; its numbers are those its own
 * readout prints. Returns `{kind, values}` or undefined when the mark's numbers cannot be read.
 */
export function markNumbers(id, { table = [], marks = [] } = {}) {
	const row = /^row:(\d+)$/.exec(id ?? "");
	if (row) {
		const cells = table.slice(1)[Number(row[1])];
		const values = cells === undefined ? [] : cells.split("\t").flatMap((cell) => signedNumbers(cell).map((t) => t.signed));
		return values.length ? { kind: "row", values } : undefined;
	}
	const mark = marks.find((m) => m.id === id);
	const values = (mark?.lines ?? []).flatMap((line) => signedNumbers(line.value).map((t) => t.signed));
	return values.length ? { kind: "readout", values } : undefined;
}

/**
 * The chart types whose label may name a rowless mark that is a statistic of several rows: a box's
 * median. `judgeDerivedReadouts` recomputes that mark's readout from the Table rows.
 */
export const STATISTIC_LABEL_TYPES = new Set(["boxplot"]);

/**
 * The numbers a text is judged against by its mark (see `markNumbers`), or undefined when nearness to
 * the Table is all there is to judge by. `strict` is a label or readout of a chart that prints the
 * author's values. A label of a box plot that names a rowless mark prints a statistic of that box,
 * so it is judged by the box's own readout, exactly: the median of an even sample lies between two
 * rows and may be near a third, which makes nearness to the rows the wrong question for it.
 */
export function ownNumbers({ role, mark }, chart, strict) {
	if (!mark) return undefined;
	if (strict) return markNumbers(mark, chart);
	return role === "label" && STATISTIC_LABEL_TYPES.has(chart.type) && !/^row:\d+$/.test(mark) ? markNumbers(mark, chart) : undefined;
}

/**
 * Judge the numbers of a text that names its mark (`Text::mark`, or the observation a readout is
 * of) against that mark's own numbers, `own` (see `markNumbers`), instead of against anything near
 * it: every number printed must be a rounding of one of the mark's own numbers, or one of `derived`,
 * the sums, shares and running totals of the chart's columns (a share is of the whole column, so it
 * cannot be told from the mark's row). A count ("3 categories") and a bound ("<0.1%") are not judged.
 * Returns `[{raw, near}]`, `near` the mark's number closest to the one shown.
 */
export function judgeMarkText(input, own, derived = []) {
	const text = normalise(input);
	const found = [];
	for (const token of signedNumbers(text)) {
		if (isCount(token, text) || isBound(token, text)) continue;
		const rounds = (value) => asShown(token, value).some(([c]) => roundsTo(token, c));
		if (own.values.some(rounds) || derived.some(rounds)) continue;
		const distance = (value) => Math.min(...asShown(token, value).map(([c]) => Math.abs(token.signed - c)));
		found.push({ raw: token.raw, near: [...own.values].sort((a, b) => distance(a) - distance(b))[0] });
	}
	return found;
}

/** The readout line that is the value a mark of `series` is drawn to: its own series' line, else a count or total, else the only plain one. */
export function valueLine(lines, series) {
	const numbered = lines.flatMap((line) => {
		const one = readNumber(line.value);
		return one ? [{ line, ...one }] : [];
	});
	const own = numbered.filter((n) => series !== undefined && n.line.series === series);
	if (own.length) return own.sort((a, b) => Number(Boolean(a.line.derived)) - Number(Boolean(b.line.derived)))[0];
	const named = numbered.find((n) => COUNT_NAMES.test(n.line.name) || /^(total|value)$/i.test(n.line.name));
	return named ?? (numbered.length === 1 ? numbered[0] : undefined);
}

/**
 * The value axis of a panel as `{perPixel, at(px)}`: the numeric axis labels paired with the grid
 * or baseline line each sits beside give value = a + b * position, checked on every pair to a pixel.
 * `vertical` reads the left axis' labels against horizontal lines (bars standing up); otherwise the
 * bottom labels against vertical lines. Undefined when fewer than two labels pair or they are not linear.
 */
export function valueAxis(panel, vertical) {
	const { plot, labels = [], hlines = [], vlines = [] } = panel;
	const numeric = labels.flatMap((label) => {
		const one = readNumber(label.text);
		return one && !label.rotate && /^[^\p{L}]*\p{N}[^\p{L}]*$|^[^\p{L}]*[\d.,]+\s?[KMBTkmbt]?$/u.test(normalise(label.text)) ? [{ ...label, value: one.value }] : [];
	});
	const chosen = vertical ? numeric.filter((l) => l.anchor === "end" && l.x < plot.x + plot.w / 2) : numeric.filter((l) => l.anchor === "middle" && l.y > plot.y + plot.h);
	const lines = vertical ? hlines : vlines;
	const pairs = chosen.flatMap((label) => {
		const at = vertical ? label.y : label.x;
		const nearest = lines.map((line) => [line, Math.abs(line - at)]).sort((a, b) => a[1] - b[1])[0];
		return nearest && nearest[1] <= (vertical ? 9 : 2) ? [{ position: nearest[0], value: label.value }] : [];
	});
	const unique = [...new Map(pairs.map((p) => [p.value, p])).values()];
	if (unique.length < 2) return undefined;
	const first = unique[0];
	const last = unique[unique.length - 1];
	const slope = (last.value - first.value) / (last.position - first.position);
	if (!Number.isFinite(slope) || slope === 0) return undefined;
	const at = (position) => first.value + (position - first.position) * slope;
	if (unique.some((p) => Math.abs(at(p.position) - p.value) > PIXELS * Math.abs(slope))) return undefined;
	return { perPixel: Math.abs(slope), at };
}

/**
 * Judge the marks a chart draws against what its readouts say. For the bar-like types (BAR_TYPES)
 * each rect mark's drawn extent, on the panel's value axis, must equal the value its readout prints
 * for that mark to the author's precision (half a unit of its last digit) and a pixel; where the
 * chart's note says it is sorted, the readouts must also run in that order along the axis, and the
 * drawn extents with them. A line chart's hover dots (`dots`) are judged the same way against the
 * line of their series. Without a labelled axis the largest readout fixes the scale and the others
 * are judged against it. Returns `[{detail, text}]`.
 */
export function judgeDrawn(chart) {
	const found = [];
	// A rect names its observation by `obs`, but a hidden series leaves the other marks' `obs` as they were while the observations are renumbered, so the join is by place: the observation whose hit box holds the rect's centre.
	const observationOf = (p, rect) => {
		const here = chart.marks.filter((m) => m.panel === p && m.area && inside(m.area, rect));
		return here.find((m) => m.series === rect.series) ?? here.sort((a, b) => a.area.w * a.area.h - b.area.w * b.area.h)[0];
	};
	for (const [p, panel] of chart.panels.entries()) {
		if (BAR_TYPES.has(chart.type)) {
			const marks = new Map(panel.rects.map((r) => [r, observationOf(p, r)]));
			const rects = panel.rects.filter((r) => marks.get(r));
			const spread = (key) => (rects.length ? (Math.max(...rects.map((r) => r[key])) - Math.min(...rects.map((r) => r[key]))) / Math.max(...rects.map((r) => r[key]), 1e-9) : 0);
			// A histogram's bins differ in width and a funnel's bands in height only; every other type is read by which extent varies.
			const vertical = chart.type === "histogram" ? true : chart.type === "funnel" ? false : spread("h") >= spread("w");
			const extent = (r) => (vertical ? r.h : r.w);
			const read = rects.flatMap((rect) => {
				const mark = marks.get(rect);
				const line = valueLine(mark.lines, rect.series);
				return line ? [{ rect, mark, ...line }] : [];
			});
			if (read.length) {
				const axis = valueAxis(panel, vertical);
				const nonNegative = read.every((r) => r.value >= 0);
				const truncated = axis && nonNegative && chart.type !== "waterfall" && Math.abs(axis.at(vertical ? panel.plot.y + panel.plot.h : panel.plot.x)) > 1e-9 * Math.max(...read.map((r) => Math.abs(r.value)), 1);
				let perPixel = axis?.perPixel;
				if (!axis) {
					const anchor = [...read].sort((a, b) => Math.abs(b.value) - Math.abs(a.value))[0];
					perPixel = anchor.value ? Math.abs(anchor.value) / extent(anchor.rect) : undefined;
				}
				if (perPixel && !truncated) {
					for (const r of read) {
						const drawn = extent(r.rect) * perPixel;
						if (Math.abs(drawn - Math.abs(r.value)) > r.tolerance + PIXELS * perPixel) {
							found.push({ detail: `the mark of "${r.mark.heading}" is drawn to ${Number(drawn.toPrecision(4))} but its readout says ${r.line.name}=${r.line.value}`, text: r.line.value });
						}
					}
				}
				const order = /sorted from the (?:largest|biggest|highest)|descending/i.test(chart.note ?? "") ? -1 : /sorted from the (?:smallest|lowest)|ascending/i.test(chart.note ?? "") ? 1 : 0;
				if (order) {
					const along = (r) => (vertical ? r.rect.x : r.rect.y);
					const sorted = [...read].sort((a, b) => along(a) - along(b));
					for (let i = 1; i < sorted.length; i++) {
						const [a, b] = [sorted[i - 1], sorted[i]];
						const wrong = (b.value - a.value) * order < -(a.tolerance + b.tolerance);
						const drawnWrong = (extent(b.rect) - extent(a.rect)) * order < -PIXELS;
						if (wrong || drawnWrong) found.push({ detail: `the chart says it is sorted ${order < 0 ? "from the largest down" : "from the smallest up"}, but "${b.mark.heading}" (${b.line.value}) is drawn after "${a.mark.heading}" (${a.line.value})`, text: b.line.value });
					}
				}
			}
		}
		if (chart.type === "line" && !chart.stack) {
			const axis = valueAxis(panel, true);
			const rightLabels = (panel.labels ?? []).some((l) => l.anchor === "start" && l.x > panel.plot.x + panel.plot.w && readNumber(l.text));
			if (axis && !rightLabels) {
				for (const mark of chart.marks.filter((m) => m.panel === p)) {
					for (const dot of mark.dots ?? []) {
						const line = valueLine(mark.lines.filter((l) => l.series === dot.series), dot.series);
						if (!line) continue;
						const drawn = axis.at(dot.at[1]);
						if (Math.abs(drawn - line.value) > line.tolerance + PIXELS * axis.perPixel) found.push({ detail: `the point of "${mark.heading}" is drawn at ${Number(drawn.toPrecision(4))} but its readout says ${line.line.name}=${line.line.value}`, text: line.line.value });
					}
				}
			}
		}
	}
	return found;
}

/** The number of a Table cell (with its unit, "7 ms"), or undefined when it holds none or several. */
const cellNumber = (cell) => (signedNumbers(cell ?? "").length === 1 ? signedNumbers(cell)[0] : undefined);

/** A linear-interpolated quantile of an ascending sample (the engine's own method). */
const quantile = (sorted, p) => {
	const at = p * (sorted.length - 1);
	const [lo, hi] = [Math.floor(at), Math.ceil(at)];
	return sorted[lo] + (sorted[hi] - sorted[lo]) * (at - lo);
};

/**
 * Judge the readouts of the derived chart types against the state's own rows (`chart.table`, tab
 * separated, the state's Table view): a histogram's count against the rows of that bin (the bin's
 * edges are read from the heading and may be off by half a unit of their last digit, so a row that
 * close to an edge may fall on either side) and, with a colour, the count of each visible series;
 * a box plot's observation count, median and quartiles against the rows of that box (the heading's
 * parts, such as "A · Control", each name a cell of the row), to one unit of the last printed digit.
 * Faceted charts and shares are not judged. Returns `[{detail, text}]`.
 */
export function judgeDerivedReadouts(chart) {
	if (!DERIVED_TYPES.has(chart.type) || chart.enc?.facet) return [];
	const rows = (chart.table ?? []).map((row) => row.split("\t"));
	const header = rows[0] ?? [];
	const body = rows.slice(1);
	const column = (name) => {
		const at = header.findIndex((h) => squash(h) === squash(name ?? ""));
		return at >= 0 ? at : header.findIndex((h) => squash(h).startsWith(squash(name ?? "")) && squash(name ?? "").length >= 3);
	};
	const x = column(chart.enc?.x);
	const y = column(chart.enc?.y);
	const color = column(chart.enc?.color);
	const found = [];
	if (chart.type === "histogram" && x >= 0) {
		const values = body.flatMap((row) => {
			const n = cellNumber(row[x]);
			return n ? [{ value: n.signed, half: halfUnit(n), group: color >= 0 ? row[color] : undefined }] : [];
		});
		const visible = new Set((chart.series ?? []).filter((s) => !s.hidden).map((s) => s.name));
		for (const mark of chart.marks) {
			const edges = signedNumbers(mark.heading);
			if (edges.length !== 2) continue;
			const [lo, hi] = edges;
			const inclusive = /inclusive/i.test(mark.heading);
			const [slackLo, slackHi] = [halfPrinted(lo), halfPrinted(hi)];
			const loose = (rowsOf) => rowsOf.filter((r) => r.value >= lo.signed - slackLo && r.value <= hi.signed + slackHi).length;
			const sure = (rowsOf) => rowsOf.filter((r) => r.value > lo.signed + slackLo && r.value < hi.signed - slackHi).length;
			for (const line of mark.lines) {
				const one = readNumber(line.value);
				if (!one) continue;
				const own = line.series !== undefined && line.series !== null ? (chart.series ?? []).find((s) => s.slot === line.series)?.name : undefined;
				const group = COUNT_NAMES.test(line.name) ? undefined : own ?? line.name;
				if (!COUNT_NAMES.test(line.name) && (color < 0 || !values.some((r) => r.group === group))) continue;
				const rowsOf = values.filter((r) => (group === undefined ? color < 0 || !(chart.series?.length) || visible.has(r.group) : r.group === group));
				const [few, most] = [sure(rowsOf), loose(rowsOf)];
				if (one.value < few || one.value > most) found.push({ detail: `"${mark.heading}" reads ${line.name}=${line.value}, but ${few === most ? few : `${few} to ${most}`} rows of this state fall in it`, text: line.value });
			}
		}
	}
	if (chart.type === "boxplot" && y >= 0) {
		for (const mark of chart.marks) {
			const parts = mark.heading.split(" · ");
			if (/\brow \d+$/.test(mark.heading) || !mark.lines.some((l) => /^median$/i.test(l.name))) continue;
			const inBox = body.filter((row) => parts.every((part) => row.some((cell) => cell.trim() === part)));
			const values = inBox.flatMap((row) => (cellNumber(row[y]) ? [cellNumber(row[y])] : [])).sort((a, b) => a.signed - b.signed);
			if (!values.length || values.length !== inBox.length) continue;
			const sorted = values.map((v) => v.signed);
			const tableHalf = Math.max(...values.map(halfPrinted));
			const unit = tableHalf * 2;
			const q1 = quantile(sorted, 0.25), q3 = quantile(sorted, 0.75);
			const fences = [q1 - 1.5 * (q3 - q1), q3 + 1.5 * (q3 - q1)];
			const inside = sorted.filter(value => value >= fences[0] && value <= fences[1]);
			const expect = { observations: sorted.length, median: quantile(sorted, 0.5), "first quartile": q1, "third quartile": q3 };
			// A rounded Table value at a fence can legitimately switch outlier membership.
			if (inside.length && !sorted.some(value => fences.some(fence => Math.abs(value - fence) <= unit))) {
				expect["lower whisker"] = inside[0];
				expect["upper whisker"] = inside.at(-1);
			}
			for (const line of mark.lines) {
				const one = readNumber(line.value);
				const want = expect[line.name.toLowerCase()];
				if (!one || want === undefined) continue;
				const slack = line.name.toLowerCase() === "observations" ? 0 : /whisker$/i.test(line.name) ? tableHalf : unit;
				if (Math.abs(one.value - want) > slack + 1e-9) found.push({ detail: `"${mark.heading}" reads ${line.name}=${line.value}, but the rows of this box give ${want}`, text: line.value });
				else if (line.name.toLowerCase() !== "observations" && halfPrinted(one.token) > tableHalf + 1e-9) found.push({ detail: `"${mark.heading}" reads ${line.name}=${line.value}, less precise than its Table rows`, text: line.value });
			}
		}
	}
	if (chart.type === "histogram") found.push(...judgeHistogramSummary(chart));
	return found;
}

/** A histogram's narrative and accessible name must describe the same counts and edges as its marks. */
function judgeHistogramSummary(chart) {
	const bins = chart.marks.flatMap(mark => {
		const edges = signedNumbers(mark.heading);
		const count = mark.lines.find(line => COUNT_NAMES.test(line.name));
		const value = count && readNumber(count.value);
		return edges.length === 2 && value ? [{ edges, count: value.value }] : [];
	});
	if (!bins.length) return [];
	const found = [];
	const total = bins.reduce((n, bin) => n + bin.count, 0), max = Math.max(...bins.map(bin => bin.count));
	const same = (token, other) => Math.abs(token.signed - other.signed) <= 1e-12 * Math.max(1, Math.abs(token.signed), Math.abs(other.signed));
	for (const [kind, text] of [["summary", chart.summary], ["accessible summary", chart.accessibleSummary]]) {
		const match = [...String(text ?? "").matchAll(/([\d,]+) observations from (.+?) to (.+?); the fullest interval, (.+?) to (.+?), holds ([\d,]+)\./g)].at(-1);
		if (!match) continue;
		const [n, lo, hi, binLo, binHi, count] = match.slice(1).map(readNumber);
		if (![n, lo, hi, binLo, binHi, count].every(Boolean)) continue;
		const fullest = bins.find(bin => same(binLo.token, bin.edges[0]) && same(binHi.token, bin.edges[1]) && bin.count === max);
		if (n.value !== total || count.value !== max || !fullest || !same(lo.token, bins[0].edges[0]) || !same(hi.token, bins.at(-1).edges[1])) {
			found.push({ detail: `${kind} "${match[0]}" disagrees with this state's bin marks (${total} observations, fullest count ${max})`, text: match[0] });
		}
	}
	return found;
}

/** Hiding a legend series cannot change the bins or counts of another still-visible series. */
export function judgeLegendReplay(before, after) {
	if (before.type !== "histogram" || after.type !== "histogram" || before.enc?.facet || after.enc?.facet) return [];
	const found = [];
	const visible = (after.series ?? []).filter(series => !series.hidden && (before.series ?? []).some(old => old.slot === series.slot && !old.hidden));
	const groups = (mark, series) => mark?.lines.find(line => line.series === series.slot && !COUNT_NAMES.test(line.name));
	if (before.marks.length !== after.marks.length || before.marks.some((mark, i) => mark.heading !== after.marks[i]?.heading)) {
		return [{ detail: "legend toggle changed the authored histogram bin structure", text: "bin edges" }];
	}
	// Share/density readouts may renormalize as visibility changes; only literal counts are invariant.
	if (![...before.marks, ...after.marks].every(mark => mark.lines.some(line => COUNT_NAMES.test(line.name)))) return [];
	for (const [i, mark] of after.marks.entries()) for (const series of visible) {
		const old = groups(before.marks[i], series), now = groups(mark, series);
		// Absent group readouts represent zero, including empty bins.
		const was = old ? readNumber(old.value)?.value : 0, value = now ? readNumber(now.value)?.value : 0;
		if (was !== undefined && value !== undefined && was !== value) found.push({ detail: `legend toggle changed ${series.name} in "${mark.heading}" from ${was} to ${value}`, text: now?.value ?? "0" });
	}
	return found;
}

// ---------------------------------------------------------------------------------------------
// The run.
// ---------------------------------------------------------------------------------------------

/** The reports under `base` (examples/, or the step's fixtures) the step renders, as `{id, dir}`. */
export function reportList(cwd, only, base = "examples") {
	return readdirSync(join(cwd, base))
		.filter((name) => !SKIP_REPORTS.has(name) && (!only || name.includes(only)) && existsSync(join(cwd, base, name, "report.rmd")))
		.sort()
		.map((name) => ({ id: name, dir: join(base, name) }));
}

/**
 * Runs in the page: each chart's value texts, from the panels' items and from `figure_items`, where
 * the size and colour keys live, and what the judge needs to know of the chart (its type and stack
 * option, and the payload's Table columns and grids). Axis texts are left out; a key entry that is
 * only a number is kept, since a colour key's ends are values a reader reads. With `only`, just that
 * figure is read.
 */
export function chartProbe({ valueRoles, only }) {
	const models = window.__richview ? window.__richview.charts() : [];
	const figures = [...document.querySelectorAll("figure[data-rv-chart]")];
	if (models.length !== figures.length) throw new Error(`Chart model count ${models.length} does not match figure count ${figures.length}`);
	return figures.flatMap((figure, i) => {
		if (only !== undefined && only !== i) return [];
		const geometry = models[i]?.geometry;
		const items = [...(geometry?.panels ?? []).flatMap((p) => p.items), ...(geometry?.figure_items ?? [])];
		let payload = {};
		try {
			payload = JSON.parse(figure.querySelector("script[data-rv-payload]")?.textContent ?? "{}");
		} catch {
			// A chart whose payload does not parse is judged on its Table alone.
		}
		// The id of each observation as `Geometry::mark_ids` makes it: `row:<n>`, else `<panel>/<series>/<ordinal>`.
		const marks = (geometry?.panels ?? []).map((panel, p) => {
			const counts = new Map();
			return panel.observations.map((o) => {
				const n = counts.get(o.series ?? null) ?? 0;
				counts.set(o.series ?? null, n + 1);
				return { id: o.row !== undefined && o.row !== null ? `row:${o.row}` : `${p}/${o.series ?? "-"}/${n}` };
			});
		});
		const straight = (it) => (it.kind === "line" && it.path?.cmds?.length === 2 && it.path.cmds.every((c) => c.p?.length === 2) ? it.path.cmds.map((c) => c.p) : undefined);
		const playback = (payload.block?.controls ?? []).some(control => control.kind === "playback");
		const filters = [];
		let keyReason;
		for (const [k, control] of (playback ? payload.block.controls : []).entries()) {
			if (control.kind !== "filter") continue;
			const group = figure.querySelector(`.rv-chart__controls [data-rv-control="${k}"]`);
			const select = group?.querySelector("select");
			const selected = select ? Number(select.value) : Number(group?.querySelector('.rv-chart__option[aria-pressed="true"]')?.getAttribute("data-rv-option") ?? NaN);
			const options = control.all ? [null, ...(control.options ?? [])] : (control.options ?? []);
			if (!Number.isInteger(selected) || selected < 0 || selected >= options.length) keyReason = `Playback key filter ${control.id} has no readable active option`;
			else filters.push({ field: control.field, value: options[selected] });
		}
		return [
			{
				i,
				title: figure.querySelector(".rv-chart__title")?.textContent.trim() ?? "",
				texts: [
					...items.filter((it) => it.kind === "text" && valueRoles.includes(it.role)).map((it) => ({ text: it.text, role: it.role, mark: it.mark })),
					...(geometry?.panels ?? []).flatMap((p, pi) => p.observations.flatMap((o, k) => o.readout.lines.map((line) => ({ text: line.value, role: "readout", name: line.name, derived: Boolean(line.derived), mark: marks[pi][k].id })))),
				],
				visible: figure.querySelector(".rv-chart__frame")?.getBoundingClientRect().width > 0,
				type: payload.block?.type ?? "",
				stack: Boolean(payload.block?.stack),
				quant: payload.quant ?? {},
				playback,
				shown: payload.shown ?? {},
				dataColumns: playback ? (payload.columns ?? []) : [],
				dataRows: playback ? (payload.rows ?? []) : [],
				filters,
				keyReason,
				zoomed: Boolean(models[i]?.zoom),
				note: geometry?.note ?? "",
				summary: geometry?.summary ?? "",
				accessibleSummary: figure.querySelector(".rv-chart__frame")?.getAttribute("aria-label") ?? "",
				enc: payload.block?.encoding ?? {},
				series: (geometry?.series ?? []).map((s) => ({ name: s.name, hidden: Boolean(s.hidden), slot: s.slot })),
				// Every observation (a mark with a readout) and the rect marks and axis of each panel, for the drawn-against-read checks.
				marks: (geometry?.panels ?? []).flatMap((panel, p) => panel.observations.map((o, k) => ({ panel: p, obs: k, id: marks[p][k].id, series: o.series, heading: o.readout.heading, area: o.area ?? undefined, lines: o.readout.lines.map((l) => ({ name: l.name, value: l.value, series: l.series ?? undefined, derived: Boolean(l.derived) })), dots: (o.dots ?? []).map((d) => ({ at: d.at, series: d.series })) }))),
				panels: (geometry?.panels ?? []).map((panel) => ({
					plot: panel.plot,
					rects: panel.items.filter((it) => it.kind === "rect" && it.role === "mark" && it.obs !== undefined).map((it) => ({ obs: it.obs, series: it.series, ...it.rect })),
					labels: panel.items.filter((it) => it.kind === "text" && it.role === "axis-label").map((it) => ({ text: it.text, x: it.x, y: it.y, anchor: it.anchor, rotate: it.rotate ?? 0 })),
					hlines: panel.items.flatMap((it) => { const pts = straight(it); return pts && pts[0][1] === pts[1][1] ? [pts[0][1]] : []; }),
					vlines: panel.items.flatMap((it) => { const pts = straight(it); return pts && pts[0][0] === pts[1][0] ? [pts[0][0]] : []; }),
				})),
				// Where each readout's mark sits on a linear x axis, in data units, for the placement check.
				placed: (geometry?.panels ?? []).flatMap((p) => {
					const x = p.x;
					if (!x?.domain || !x.range || !p.observations?.length || Object.keys(x).some((key) => !["calendar", "domain", "extent", "range"].includes(key))) return [];
					const at = (px) => x.domain[0] + ((px - x.range[0]) / (x.range[1] - x.range[0])) * (x.domain[1] - x.domain[0]);
					return [{ calendar: Boolean(x.calendar), width: Math.abs(x.domain[1] - x.domain[0]), points: p.observations.map((o) => ({ heading: o.readout.heading, x: at(o.x) })) }];
				}),
			},
		];
	});
}

/** Wait for every chart of the page to settle. */
const settled = (page) => page.evaluate(() => window.__richview.settled());

/** Read the visible charts of the settled page that are not in `seen` (or just chart `only`, seen or not): their value texts and Table; `seen` gains their indexes. */
async function readCharts(page, seen = new Set(), only) {
	const probe = await page.evaluate(chartProbe, { valueRoles: [...VALUE_ROLES], only });
	const figures = page.locator("figure[data-rv-chart]");
	const charts = [];
	for (const chart of probe) {
		// A chart in a closed tab panel is not on screen, so no reader has its labels until the tab opens.
		if (!chart.visible || (only === undefined && seen.has(chart.i))) continue;
		seen.add(chart.i);
		const table = await retryOnTimeout((timeout) => tableView(figures.nth(chart.i), timeout));
		charts.push({ ...chart, table: table.rows, reason: table.reason });
	}
	return charts;
}

/** The control groups of a figure as `{kind, label, options, current}`; `kind` is "filter" (buttons or a select) or "playback". */
function controlsProbe(figure) {
	return [...figure.querySelectorAll("[data-rv-control]")].map((group) => {
		const label = group.querySelector("legend, .rv-chart__control-label, label")?.textContent.trim() ?? "";
		const scrubber = group.querySelector(".rv-chart__scrubber");
		if (scrubber) return { kind: "playback", label, options: Number(scrubber.max) + 1, current: Number(scrubber.value), text: scrubber.getAttribute("aria-valuetext") ?? scrubber.value };
		const select = group.querySelector("select");
		if (select) return { kind: "filter", label, options: select.options.length, current: select.selectedIndex, names: [...select.options].map((o) => o.textContent.trim()) };
		const buttons = [...group.querySelectorAll(".rv-chart__option")];
		return { kind: "filter", label, options: buttons.length, current: Math.max(0, buttons.findIndex((b) => b.getAttribute("aria-pressed") === "true")), names: buttons.map((b) => b.textContent.trim()) };
	});
}

/** Choose option `index` of control `k` of figure `i`: a button, a select or a playback's scrubber. */
async function choose(page, i, k, index) {
	const group = page.locator("figure[data-rv-chart]").nth(i).locator("[data-rv-control]").nth(k);
	const scrubber = group.locator(".rv-chart__scrubber");
	if (await scrubber.count()) {
		await scrubber.evaluate((input, value) => {
			input.value = String(value);
			input.dispatchEvent(new Event("input", { bubbles: true }));
			input.dispatchEvent(new Event("change", { bubbles: true }));
		}, index);
	} else if (await group.locator("select").count()) await group.locator("select").selectOption({ index }, { timeout: 5000 });
	else await group.locator(".rv-chart__option").nth(index).click({ timeout: 5000 });
	await settled(page);
}

/** Press "+" twice on chart `i`'s frame; returns whether it zoomed (only line, area and scatter charts do). */
async function zoomIn(page, i) {
	const figure = page.locator("figure[data-rv-chart]").nth(i);
	await figure.locator(".rv-chart__frame").focus({ timeout: 5000 });
	await page.keyboard.press("+");
	await page.keyboard.press("+");
	await settled(page);
	return page.evaluate((n) => Boolean(window.__richview.charts()[n]?.zoom), i);
}

/**
 * Put chart `i`, which is on screen, through every state a reader can put it in, calling
 * `judge(name, charts)` after each. Each change is undone before the next, except the legend, which
 * is left off last. `label` prefixes the state names of a chart in a tab.
 */
async function exercise(page, chart, width, label, judge, interactive = width === WIDTHS[0]) {
	const i = chart.i;
	const name = (what) => `${label}${what} (${width})`;
	const figure = page.locator("figure[data-rv-chart]").nth(i);
	const controls = interactive ? await figure.evaluate(controlsProbe) : [];
	for (const [k, control] of controls.entries()) {
		if (control.kind === "filter" && control.options > 1) {
			const other = control.current === 1 ? 0 : 1;
			await choose(page, i, k, other);
			judge(name(`filter ${control.names[other]}`), await readCharts(page, new Set(), i));
			await choose(page, i, k, control.current);
		} else if (control.kind === "playback" && control.options > 1) {
			// Step towards the other end: the next frame, or the previous one when the playback starts on its last.
			await figure.locator(control.current + 1 < control.options ? ".rv-chart__control-button--next" : ".rv-chart__control-button--previous").click({ timeout: 5000 });
			await settled(page);
			judge(name(`frame ${(await figure.evaluate(controlsProbe))[k].text}`), await readCharts(page, new Set(), i));
			if (control.current === 0) continue;
			await choose(page, i, k, 0);
			judge(name(`first frame ${(await figure.evaluate(controlsProbe))[k].text}`), await readCharts(page, new Set(), i));
			await choose(page, i, k, control.current);
		}
	}
	if (interactive && (await zoomIn(page, i))) {
		judge(name("zoom"), await readCharts(page, new Set(), i));
		await figure.locator(".rv-chart__frame").press("Escape", { timeout: 5000 });
		await settled(page);
	}
	const button = figure.locator("button[data-rv-series]").first();
	if ((await button.count()) && (await button.isVisible())) {
		const before = chart.type === "histogram" ? await readCharts(page, new Set(), i) : undefined;
		await button.click({ timeout: 5000 });
		await settled(page);
		judge(name("legend off"), await readCharts(page, new Set(), i), before);
	}
}

/** Open each tab of the page in turn and read the charts that become visible; `onOpen(label, charts)` gets each tab's new charts. */
async function openTabs(page, seen, onOpen) {
	const tabs = page.locator("[role=tab]");
	for (let t = 0; t < (await tabs.count()); t++) {
		const tab = tabs.nth(t);
		const label = ((await tab.textContent()) ?? "").trim() || String(t + 1);
		await tab.click({ timeout: 5000 });
		await settled(page);
		await onOpen(`tab ${label}`, await readCharts(page, seen));
	}
}

// Best-effort evidence after a collection failure. One deadline covers DOM and IO; the initial
// record keeps the full error even when the browser never answers the passive snapshot.
async function retainCollectionFailure(page, real, report, width, where, error) {
	let timer;
	try {
		const controller = new AbortController();
		const deadline = new Promise(resolve => { timer = setTimeout(() => { controller.abort(); resolve(); }, 1000); });
		await Promise.race([(async () => {
			const file = join(real, `collection-failure-${report.replace(/[^a-zA-Z0-9_-]/g, "_")}-${width}-${where.replace(/[^a-zA-Z0-9_-]/g, "_")}.json`);
			const record = { report, width, where, at: new Date().toISOString(), error: String(error?.message ?? error).slice(0, 65536), stack: String(error?.stack ?? "").slice(0, 16384), diagnosticBudgetMs: 1000, current: { status: "pending" } };
			const save = () => writeFile(file, `${JSON.stringify(record, null, 2)}\n`, { signal: controller.signal });
			await save();
			if (controller.signal.aborted) return;
			try {
				record.current = await page.evaluate(index => {
					const figure = Number.isInteger(index) ? document.querySelectorAll("figure[data-rv-chart]")[index] : null;
					const button = figure?.querySelector("button[data-rv-series]");
					const node = element => element && ({ tag: element.tagName, id: element.id, class: String(element.className), text: element.textContent?.trim().slice(0, 160) });
					const rect = button?.getBoundingClientRect();
					const style = button && getComputedStyle(button);
					const center = rect && { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 };
					const hit = center && center.x >= 0 && center.y >= 0 && center.x < innerWidth && center.y < innerHeight ? document.elementFromPoint(center.x, center.y) : null;
					return { ready: document.readyState, fonts: document.fonts.status, scroll: { x: scrollX, y: scrollY }, viewport: { width: innerWidth, height: innerHeight }, figure: node(figure), button: node(button), connected: button?.isConnected, disabled: button?.disabled, rect: rect?.toJSON(), center, style: style && { display: style.display, visibility: style.visibility, pointerEvents: style.pointerEvents }, centerHit: node(hit), hitIsTarget: !!button && (hit === button || button.contains(hit)) };
				}, Number(where.match(/chart (\d+)/)?.[1]));
			} catch (diagnosticError) { record.current = { error: String(diagnosticError?.message ?? diagnosticError).slice(0, 4096) }; }
			if (!controller.signal.aborted) await save();
		})(), deadline]);
	} catch {
		// Diagnostics cannot change the already-recorded collection failure or stop later charts.
	} finally { clearTimeout(timer); }
}

/** Run the step; returns `{findings, problems, pages, seconds}` and writes `label-truth.json`. */
export async function runLabelTruth({ cwd, bin, out, publicDir, only, jobs, fixtures = false }) {
	const started = Date.now();
	const binary = resolve(cwd, bin);
	if (!existsSync(binary)) throw new Error(`The release CLI ${binary} does not exist; the label-truth step needs the build step first`);
	mkdirSync(out, { recursive: true });
	const real = realpathSync(out);
	const reports = reportList(cwd, only, fixtures ? FIXTURES : "examples");
	const publicFolder = publicDir ? resolve(cwd, publicDir) : [join(dirname(binary), "dist/web"), join(cwd, "dist/web")].find((p) => existsSync(p));

	// Copy each report into a real folder under `out` (a symlinked /tmp trips report.root.escape) and render it.
	const problems = [];
	const root = join(real, "inputs");
	rmSync(root, { recursive: true, force: true });
	const rendered = await pool(reports, jobs, async (report) => {
		const folder = join(root, report.id);
		cpSync(join(cwd, report.dir), folder, { recursive: true, filter: (src) => !SKIP_COPY.has(basename(src)) });
		const run = await generate(binary, folder, STYLE, publicFolder);
		if (run.status !== 0 || run.errors.length) {
			problems.push({ report: report.id, message: `generate failed: ${run.errors.map((e) => `${e.code}: ${e.message}`).join("; ") || `exit ${run.status}`}` });
			return false;
		}
		return true;
	});
	const { chromium, errors: playwrightErrors } = await import(join(cwd, "node_modules/playwright/index.mjs"));
	const { serveStatic, chromiumPath } = await import(join(cwd, "scripts/browser-harness.mjs"));
	const { settle } = await import(join(cwd, "scripts/report-runtime.mjs"));
	const server = await serveStatic({ "/": root });
	const browser = await launchBrowser(chromium, { headless: true, executablePath: chromiumPath(), args: ["--no-sandbox"] }, { TimeoutError: playwrightErrors.TimeoutError });
	const findings = [];
	const notes = [];
	let pages = 0;

	// One browser context per width; every other state follows the initial one in the same page.
	const judged = [];
	// Per chart, how many value texts with a number reached the judge: a chart with none was judged on nothing (review 1l should-fix 4).
	const judgedTexts = new Map();
	const judge = (report, state, charts, before = []) => {
		judged.push({ report, state, charts: charts.length });
		for (const chart of charts) {
			if (chart.reason) {
				problems.push({ report, state, message: `chart ${chart.i} (${chart.title}): Table not read: ${chart.reason}` });
				continue;
			}
			const { columns, loose, names, groups } = tableColumns(chart.table);
			const spans = gridSpans(names, chart.quant);
			let everyFrame = columns;
			if (chart.playback) {
				try {
					if (chart.keyReason) throw new Error(chart.keyReason);
					everyFrame = playbackKeyColumns(names, chart);
				} catch (error) {
					problems.push({ report, state, message: `chart ${chart.i} (${chart.title}): ${error.message}` });
					continue;
				}
			}
			const shares = SHARE_TYPES.has(chart.type) || chart.stack;
			const modelled = MODELLED.has(chart.type) || chart.stack;
			const found = (extra) => findings.push({ report, state, chart: chart.i, title: chart.title, ...extra });
			const ratios = chart.type === "funnel";
			const cumulative = chart.type === "pareto";
			// What a text may print besides its mark's own row: the sums, shares and running totals of the columns.
			const derived = shares ? columns.flatMap((column, k) => derivedValues(column, { shares, groups: chart.stack ? groups[k] : undefined, ratios, cumulative })) : [];
			for (const { text, role, name, derived: isDerived, mark } of chart.texts) {
				// A derived readout is a statistic, a share of a parent or a difference the judge can only recompute for the modelled types; elsewhere it is not judged.
				if (role === "readout" && (!VALUE_TYPES.has(chart.type) || (isDerived && !modelled))) continue;
				if (parseNumbers(text).length) {
					const at = `${report}|${chart.i}|${chart.title}`;
					judgedTexts.set(at, (judgedTexts.get(at) ?? 0) + 1);
				}
				const strict = STRICT.has(role) && VALUE_TYPES.has(chart.type) && !COUNT_NAMES.test(name ?? "");
				// By mark: a text that names its mark is judged against that mark's own row (or its own readout), exactly.
				const own = ownNumbers({ role, mark }, chart, strict);
				if (own) {
					for (const hit of judgeMarkText(text, own, derived)) found({ role, text, shown: hit.raw, tableValue: hit.near, mark });
					continue;
				}
				// Fallback, for a text with no mark (a key, an annotation, a total) or a mark whose numbers cannot be read: nearness to the Table is all there is to judge by.
				const options = { spans, shares, key: role === "key", strict, ratios, cumulative, groups: chart.stack ? groups : [] };
				for (const hit of judgeValueText(text, role === "key" ? everyFrame : columns, loose, options)) {
					const allowed = ALLOWED.find((a) => a.report === report && a.shown === hit.raw);
					(allowed ? notes : findings).push({ report, state, chart: chart.i, title: chart.title, role, text, shown: hit.raw, tableValue: hit.near, ...(allowed && { reason: allowed.reason }) });
				}
			}
			// Drawn against read, and the derived readouts against the state's rows.
			for (const hit of [...judgeDrawn(chart), ...judgeDerivedReadouts(chart)]) found({ role: "readout", text: hit.text, shown: hit.text, detail: hit.detail });
			const previous = before.find(old => old.i === chart.i);
			if (previous) for (const hit of judgeLegendReplay(previous, chart)) found({ role: "readout", text: hit.text, shown: hit.text, detail: hit.detail });
			for (const axis of PLACED.has(chart.type) ? chart.placed : []) {
				for (const hit of judgePlacement(axis)) findings.push({ report, state, chart: chart.i, title: chart.title, role: "readout heading", text: hit.raw, shown: hit.raw, placed: hit });
			}
		}
	};
	try {
		const todo = reports.filter((_, i) => rendered[i]);
		await pool(todo.flatMap((report) => WIDTHS.map((width) => ({ report, width }))), jobs, async ({ report, width }) => {
			const context = await browser.newContext({ viewport: { width, height: 900 }, deviceScaleFactor: 1, reducedMotion: "reduce", colorScheme: "light" });
			try {
				const page = await context.newPage();
				const response = await page.goto(`${server.base}/${report.id}/Rendered/${STYLE}.html`, { waitUntil: "load" });
				if (!response || response.status() >= 400) throw new Error(`HTTP ${response?.status() ?? "no response"}`);
				if ((await page.locator("figure[data-rv-chart]").count()) === 0) return;
				await settle(page, undefined, { models: false });
				const seen = new Set();
				// A state a chart cannot be put in is a problem of that chart; the others are still read.
				const visit = async (where, work) => {
					try {
						await work();
					} catch (error) {
						problems.push({ report: report.id, state: where, message: `width ${width}: ${error.message.split("\n")[0]}` });
						await retainCollectionFailure(page, real, report.id, width, where, error);
					}
				};
				// A readout heading names its mark's x only if it does in the initial state ("Point 1" names a point, not an x), so only then is it judged in the others.
				const placed = new Set();
				const states = async (label, charts) => {
					for (const chart of charts) if (chart.placed.every((axis) => !judgePlacement(axis).length)) placed.add(chart.i);
					judge(report.id, `${label}initial (${width})`, charts.map((chart) => ({ ...chart, placed: [] })));
					const later = (state, found, before) => judge(report.id, state, found.map((chart) => (placed.has(chart.i) ? chart : { ...chart, placed: [] })), before);
					for (const chart of charts) await visit(`${label}chart ${chart.i}`, () => exercise(page, chart, width, label, later));
				};
				await states("", await readCharts(page, seen));
				pages++;
				await openTabs(page, seen, (label, charts) => states(`${label}: `, charts));
			} catch (error) {
				problems.push({ report: report.id, message: `width ${width}: ${error.message.split("\n")[0]}` });
			} finally {
				await context.close();
			}
		});
	} finally {
		await browser.close();
		await server.close();
	}
	findings.sort((a, b) => a.report.localeCompare(b.report) || a.state.localeCompare(b.state) || a.chart - b.chart);
	const summary = { fixtures, reports: reports.map((r) => r.id), style: STYLE, judged, judgedTexts: [...judgedTexts].map(([at, texts]) => { const [report, chart, ...title] = at.split("|"); return { report, chart: Number(chart), title: title.join("|"), texts }; }), states: STATES, pages, seconds: (Date.now() - started) / 1000, findings, allowed: notes, problems };
	writeFileSync(join(real, "label-truth.json"), `${JSON.stringify(summary, null, 2)}\n`);
	return summary;
}

/** The finding as the line the step prints. */
export const describe = (f) => (f.detail ? `FAIL ${f.report} [${f.state}] chart ${f.chart} (${f.title}): ${f.detail}` : f.placed ? `FAIL ${f.report} [${f.state}] chart ${f.chart} (${f.title}): the readout "${f.shown}" is for the mark drawn at x = ${Number(f.placed.at.toPrecision(4))}` : `FAIL ${f.report} [${f.state}] chart ${f.chart} (${f.title}): "${f.shown}" ${f.tableValue === undefined ? "is in no row of this state's Table" : `is a near miss of ${f.tableValue} from the Table`} (${f.role} "${f.text}")`);

if (process.argv[1] === fileURLToPath(import.meta.url)) {
	const arg = (name) => {
		const at = process.argv.indexOf(name);
		return at < 0 ? undefined : process.argv[at + 1];
	};
	const bin = arg("--bin");
	const out = arg("--out");
	if (!bin || !out) {
		console.error("usage: label-truth.mjs --bin <richview> --out <dir> [--public <dist/web>] [--only <text>] [--jobs <n>] [--cwd <repo>] [--fixtures]");
		process.exit(2);
	}
	const summary = await runLabelTruth({ cwd: resolve(arg("--cwd") ?? process.cwd()), bin, out: resolve(out), publicDir: arg("--public"), only: arg("--only"), fixtures: process.argv.includes("--fixtures"), jobs: Number(arg("--jobs") ?? Math.max(2, Math.min(4, cpus().length - 1))) });
	for (const p of summary.problems) console.log(`FAIL not collected ${p.report}${p.state ? ` [${p.state}]` : ""}: ${p.message}`);
	for (const f of summary.findings) console.log(describe(f));
	for (const a of summary.allowed) console.log(`note allowed ${a.report} [${a.state}] chart ${a.chart}: "${a.shown}" (${a.reason})`);
	console.log(`label-truth: ${summary.reports.length} reports, ${summary.pages} pages in ${summary.seconds.toFixed(1)} s; ${summary.findings.length} near misses, ${summary.problems.length} not collected`);
	if (summary.fixtures) {
		// A fixture is expected to raise findings (`wrong`) or none (`right`); the exit code reports a surprise only.
		const surprises = summary.reports.filter((id) => FIXTURE_EXPECT[id] && (FIXTURE_EXPECT[id] === "wrong") !== summary.findings.some((f) => f.report === id));
		for (const id of surprises) console.log(`FAIL fixture ${id} expected ${FIXTURE_EXPECT[id] === "wrong" ? "findings" : "no findings"}`);
		process.exit(surprises.length + summary.problems.length ? 1 : 0);
	}
	process.exit(summary.findings.length + summary.problems.length ? 1 : 0);
}
