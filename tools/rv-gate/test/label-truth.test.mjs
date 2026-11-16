// Tests for the label-truth step's judging function (TYP-36, FEA-CH-37). None needs a browser.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";
import vm from "node:vm";
import { gridSpans, judgeDerivedReadouts, judgeDrawn, judgeMarkText, judgePlacement, judgeValueText, markNumbers, playbackKeyColumns, shownColumns, signedNumbers, tableColumns, tableNumbers, valueAxis } from "../label-truth.mjs";
import * as labelTruth from "../label-truth.mjs";

const hits = (text, table) => judgeValueText(text, table).map((h) => h.raw);

test("a heatmap label formatted from a quantised position is a near miss of the Table value", () => {
	assert.deepEqual(hits("0.488", [0.5]), ["0.488"]);
	assert.deepEqual(hits("0.5", [0.5]), []);
});

test("a funnel count one off the Table is a near miss", () => {
	assert.deepEqual(hits("2,901", [2900]), ["2,901"]);
	assert.deepEqual(hits("639", [640]), ["639"]);
	assert.deepEqual(hits("2,900", [2900]), []);
});

test("extra digits on a Table value are a near miss, a coarser rounding is not", () => {
	assert.deepEqual(hits("9.01", [9]), ["9.01"]);
	assert.deepEqual(hits("3.8", [3.81]), []);
	assert.deepEqual(hits("4", [3.8]), []);
});

test("a scale word is read in base units", () => {
	assert.deepEqual(hits("72.2 million", [72000000]), ["72.2 million"]);
	assert.deepEqual(hits("72 million", [72000000]), []);
});

test("a value printed at zero precision against a Table zero is a sign flip", () => {
	assert.deepEqual(hits("−0.0733", [0, 0.03, -3]), ["−0.0733"]);
	assert.deepEqual(hits("0.05", [0, 0.03, -3]), ["0.05"]);
	assert.deepEqual(hits("0", [0, 0.03, -3]), []);
	// Not a rounding of any value, but above 3 % of the column's largest magnitude: nothing to judge by.
	assert.deepEqual(hits("0.5", [0, 0.03, -3]), []);
});

test("a value printed at zero precision against a Table zero is a near miss of the zero", () => {
	assert.deepEqual(hits("−0.0733", [0, 0.03, -3, 1000]), ["−0.0733"]);
	assert.deepEqual(hits("0", [0, 0.03, -3, 1000]), []);
});

test("a sign flip is a finding, a same-sign rounding is not", () => {
	assert.deepEqual(hits("−3", [3, 10]), ["−3"]);
	assert.deepEqual(hits("3", [3, 10]), []);
	assert.deepEqual(hits("−3", [-3, 10]), []);
	// The span here is 1,003, so eight grid steps (about 2) cover the 0.0433 between 0.0733 and 0.03.
	assert.deepEqual(hits("−0.0733", [0.03, -3, 1000]), ["−0.0733"]);
});

test("the grid tolerance follows the column span, not the value", () => {
	const column = [-1, 0.03, 1000, 0.5];
	assert.deepEqual(hits("0", [0.01, 100]), ["0"]);
	assert.deepEqual(hits("0.0733", [0.01, 0.07, 100, 0.03]), ["0.0733"]);
	assert.deepEqual(hits("0.0244", [0.01, 0.07, 100, 0.03]), ["0.0244"]);
	assert.deepEqual(hits("−0.879", column), ["−0.879"]);
	assert.deepEqual(hits("0.464", column), ["0.464"]);
	assert.deepEqual(hits("0.5", column), []);
});

test("a column's sum and the shares of it are the author's only where the chart prints them", () => {
	const pie = [40, 25, 20, 10, 6];
	const shares = { shares: true };
	for (const text of ["39.6 %", "24.8 %", "19.8 %", "9.9 %", "5.9 %", "101", "39.6", "0.396"]) assert.deepEqual(judgeValueText(text, [pie], [], shares), [], text);
	assert.deepEqual(judgeValueText("39.2 %", [pie], [], shares).map((h) => h.raw), ["39.2 %"]);
	// A chart type that prints no shares gets no excuse from one.
	assert.deepEqual(hits("39.6", [pie]), ["39.6"]);
	assert.deepEqual(hits("Goal 3.05", [[3, 400]]), ["3.05"]);
});

test("shares are taken per column, so another column's sum and shares do not excuse this one", () => {
	const columns = [[0.5, 0.6], [0.5, 0.525]];
	const shares = { shares: true };
	// 0.5 / 1.025 = 0.4878 is the second column's share; the first column's 0.5 is where 0.488 is a grid text.
	assert.deepEqual(judgeValueText("0.488", columns, [], shares).map((h) => h.raw), ["0.488"]);
	assert.deepEqual(judgeValueText("0.488", columns).map((h) => h.raw), ["0.488"]);
	// Alone, the second column's share is the author's.
	assert.deepEqual(judgeValueText("0.488", [columns[1]], [], shares), []);
});

test("a stacked chart's totals and their shares are the stack's own", () => {
	const column = [3, 4, 30, 40];
	const groups = [["A", "A", "B", "B"]];
	assert.deepEqual(judgeValueText("7", [column], [], { shares: true, groups }), []);
	assert.deepEqual(judgeValueText("70", [column], [], { shares: true, groups }), []);
	// 40.5 is near the 40 and no stack's total or share.
	assert.deepEqual(judgeValueText("40.5", [column], [], { shares: true, groups }).map((h) => h.raw), ["40.5"]);
});

test("a midpoint is excused only for a key entry", () => {
	const population = [40_000_000, 60_000_000, 83_000_000];
	assert.deepEqual(judgeValueText("61.5 million", [population], [], { key: true }), []);
	assert.deepEqual(judgeValueText("61.5 million", [population]).map((h) => h.raw), ["61.5 million"]);
	assert.deepEqual(judgeValueText("0.488", [[0.476, 0.5]]).map((h) => h.raw), ["0.488"]);
	assert.deepEqual(judgeValueText("0.488", [[0.476, 0.5]], [], { key: true }), []);
});

test("playback size keys use the selected cohort across every frame, retaining wrong-midpoint detection", () => {
	const chart = {
		dataColumns: ["year", "region", "pop"],
		dataRows: [[1960, "Europe", 0], [2000, "Europe", 1], [2000, "Europe", 2], [2000, "Europe", 3], [1960, "Africa", 4], [2000, "Asia", 5]],
		shown: { pop: [30e6, 83e6, 57e6, 60e6, 18e6, 1410e6] },
		filters: [{ field: "region", value: "Europe" }],
	};
	const values = playbackKeyColumns(["pop"], chart);
	assert.deepEqual(values, [[30e6, 83e6, 57e6, 60e6]]);
	assert.deepEqual(judgeValueText("56.5 million", values, [], { key: true }), []);
	assert.deepEqual(judgeValueText("61.5 million", values, [], { key: true }).map(h => h.raw), ["61.5 million"]);
	assert.ok(judgeValueText("56.5 million", [chart.shown.pop], [], { key: true }).length, "the old unfiltered cohort falsely rejected the valid key");
	assert.deepEqual(playbackKeyColumns(["pop"], { ...chart, filters: [{ field: "region", value: null }] }), [chart.shown.pop]);
});

test("playback key cohorts combine active filters and reject unknown or misaligned data", () => {
	const chart = { dataColumns: ["region", "kind"], dataRows: [[" Europe ", "A"], ["Europe", "B"], ["Asia", "A"]], shown: { pop: [30, 83, 100] }, filters: [{ field: "region", value: "Europe" }, { field: "kind", value: "A" }] };
	assert.deepEqual(playbackKeyColumns(["pop"], chart), [[30]]);
	assert.throws(() => playbackKeyColumns(["pop"], { ...chart, filters: [{ field: "missing", value: "A" }] }), /filter column missing/);
	assert.throws(() => playbackKeyColumns(["pop"], { ...chart, shown: { pop: [30] } }), /not aligned/);
});

test("columns on one grid share its span, so a coarse column does not hide the fine one's grid text", () => {
	const columns = [[0.03, 0.5, 2.1], [2.5, 1000]];
	// Judged on its own span (2.07) the first column tolerates only 0.004, so none of these is near.
	for (const text of ["0", "0.24", "0.48"]) assert.deepEqual(judgeValueText(text, columns, [], { spans: [1000, 1000] }).map((h) => h.raw), [text], text);
	assert.deepEqual(judgeValueText("0.48", columns), []);
	assert.deepEqual(judgeValueText("0.5", columns, [], { spans: [1000, 1000] }), []);
});

test("gridSpans maps a Table header to the payload column and its grid", () => {
	const quant = { sales: { min: 55, max: 91, steps: 4104 }, goal: { min: 55, max: 91, steps: 4104 } };
	assert.deepEqual(gridSpans(["Sales", "goal", "Share"], quant), [36, 36, undefined]);
	assert.deepEqual(gridSpans(["Sales ($)"], quant), [36]);
	assert.deepEqual(gridSpans(["x"], quant), [undefined]);
	assert.deepEqual(gridSpans(["Sales"], undefined), [undefined]);
});

test("a minus sign after a letter signs the number, a hyphen does not", () => {
	assert.deepEqual(signedNumbers("Δ−3").map((t) => t.signed), [-3]);
	assert.deepEqual(hits("Δ−3", [3, 10]), ["−3"]);
	assert.deepEqual(signedNumbers("Wi-Fi 6").map((t) => t.signed), [6]);
});

test("tableColumns names its columns and labels each value by its row's first text cell", () => {
	const { names, groups } = tableColumns(["k\ts\tv", "a\tx\t1", "a\ty\t2", "b\tx\t3"]);
	assert.deepEqual(names, ["v"]);
	assert.deepEqual(groups, [["a", "a", "b"]]);
});

test("tableColumns takes numeric columns apart from text cells", () => {
	const { columns, loose } = tableColumns(["Region\tYear\tSales", "North\tQ1 2024\t1,200", "South\tQ2 2024\t−3.5 million"]);
	assert.deepEqual(columns, [[1200, -3500000]]);
	assert.deepEqual(loose, [2024, 2024]);
});

test("a percentage is compared with the Table value as a fraction or as a percent", () => {
	assert.deepEqual(hits("48.8%", [0.5]), ["48.8%"]);
	assert.deepEqual(hits("50%", [0.5]), []);
	assert.deepEqual(hits("12.3%", [12.3]), []);
});

test("a derived number near no Table value and a count are not judged", () => {
	assert.deepEqual(hits("12.3 %", [100, 250, 40]), []);
	assert.deepEqual(hits("3 categories", [3.1]), []);
});

test("tableNumbers reads every cell of every row", () => {
	assert.deepEqual(tableNumbers(["Region\tSales", "North\t1,200", "South\t3.5 million"]), [1200, 3500000]);
});

test("a key's midpoint between the column's ends is the author's, while a grid-shifted end is not", () => {
	const population = [40_000_000, 60_000_000, 83_000_000];
	assert.deepEqual(judgeValueText("61.5 million", [population], [], { key: true }), []);
	assert.deepEqual(judgeValueText("39.9 million", [population], [], { key: true }).map((f) => f.raw), ["39.9 million"]);
});

test("a number of a value chart that is in no row of the Table is wrong data, only when asked", () => {
	const south = [5, 5, 5, 5, 80];
	const strict = { shares: true, strict: true };
	assert.deepEqual(judgeValueText("39.6%", [south], [], strict).map((h) => h.raw), ["39.6%"]);
	assert.deepEqual(judgeValueText("5%", [south], [], strict), []);
	assert.deepEqual(judgeValueText("39.6%", [south], [], { shares: true }), []);
	assert.deepEqual(judgeValueText("60", [south], [], strict), []);
	assert.deepEqual(judgeValueText("300", [south], [], strict).map((h) => h.raw), ["300"]);
});

test("a readout heading that names another x than its mark's is a finding", () => {
	const marks = (xs, names) => ({ points: xs.map((x, i) => ({ heading: names[i], x })) });
	assert.deepEqual(judgePlacement(marks([1, 2, 3], ["d 1", "d 2", "d 3"])), []);
	assert.deepEqual(judgePlacement(marks([5, 6, 7], ["d 1", "d 2", "d 3"])).map((f) => f.raw), ["d 1", "d 2", "d 3"]);
	assert.deepEqual(judgePlacement(marks([5, 6], ["Alice", "Bob"])), []);
	const dated = { calendar: true, width: 4e10, points: [{ heading: "Jan 2024", x: Date.UTC(2024, 0, 1) }, { heading: "Feb 2024", x: Date.UTC(2024, 1, 1) }] };
	assert.deepEqual(judgePlacement(dated), []);
	assert.deepEqual(judgePlacement({ ...dated, points: dated.points.map((p, i) => ({ ...p, x: Date.UTC(2025, i, 1) })) }).length, 2);
});

test("a playback key is judged against every frame's values, and a mixed-frame midpoint is still a finding", () => {
	// Gapminder's Europe population: the frame 2000 Table holds 40 to 82 million; the 2020 frame holds Germany at 83 million.
	const frame = [[40e6, 57e6, 59e6, 60e6, 82e6]];
	const all = shownColumns(["Pop"], { pop: [30e6, 46e6, 72e6, 40e6, 82e6, 47e6, 83e6], region: ["Europe"] });
	assert.deepEqual(all, [[30e6, 46e6, 72e6, 40e6, 82e6, 47e6, 83e6]]);
	const everyFrame = frame.map((c, k) => [...c, ...all[k]]);
	const key = { key: true };
	assert.deepEqual(judgeValueText("83 million", everyFrame, [], key), []);
	assert.deepEqual(judgeValueText("83 million", frame, [], key).map((h) => h.raw), ["83 million"]);
	// The key's midpoint over every frame is 56.5 million; 61.5 mixes one frame's minimum with every frame's maximum.
	assert.deepEqual(judgeValueText("56.5 million", everyFrame, [], key), []);
	assert.deepEqual(judgeValueText("61.5 million", everyFrame, [], key).map((h) => h.raw), ["61.5 million"]);
	assert.deepEqual(shownColumns(["Unknown"], { pop: [1] }), [[]]);
});

// ---- By mark, drawn against read, percent tokens (review 1l blocker 1, should-fix 3) ----

/** A vertical-bar panel: a 0 to 3 axis at 80.5 px a unit, one rect per count, each in a hit box a little taller than the rect. */
const histogramPanel = (heights) => ({
	plot: { x: 15, y: 27.4, w: 500, h: 241.5 },
	labels: [0, 1, 2, 3].map((v) => ({ text: String(v), x: 7, y: 273 - v * 80.5, anchor: "end", rotate: 0 })),
	hlines: [268.9, 188.4, 107.9, 27.4],
	vlines: [],
	rects: heights.map((h, i) => ({ obs: i, series: 0, x: 16 + i * 166, y: 268.9 - h * 80.5, w: 164, h: h * 80.5 })),
});
const histogram = (drawn, read, extra = {}) => ({
	type: "histogram",
	note: "6 observations in 3 intervals",
	panels: [histogramPanel(drawn)],
	marks: read.map((count, i) => ({ panel: 0, obs: i, id: `0/-/${i}`, series: undefined, heading: `${i * 10} to ${i * 10 + 10}`, area: { x: 15 + i * 166, y: 268.9 - 3 * 80.5, w: 166, h: 3 * 80.5 }, lines: [{ name: "Count", value: String(count), derived: true }] })),
	...extra,
});

test("a text that names its mark is judged against that mark's own row, not against the column's range", () => {
	// The old judge accepts 50 as inside the range 10 to 90; by mark it is not the row's 30.
	assert.deepEqual(judgeValueText("50", [[10, 90]], [], { strict: true }), []);
	assert.deepEqual(judgeMarkText("50", { kind: "row", values: [30] }).map((h) => h.raw), ["50"]);
	assert.deepEqual(judgeMarkText("30", { kind: "row", values: [30] }), []);
	assert.deepEqual(judgeMarkText("29.97", { kind: "row", values: [29.97, 4] }), []);
	// A rounding to the digits printed is the author's value; one digit more is not.
	assert.deepEqual(judgeMarkText("30.0", { kind: "row", values: [29.97] }), []);
	assert.deepEqual(judgeMarkText("29.9", { kind: "row", values: [29.97] }).map((h) => h.raw), ["29.9"]);
	// A share or a total of the column is a derived number the chart may print on any mark.
	assert.deepEqual(judgeMarkText("44.8%", { kind: "row", values: [52] }, [44.8, 0.448]), []);
	assert.deepEqual(judgeMarkText("<0.1%", { kind: "row", values: [20] }), []);
	assert.deepEqual(judgeMarkText("3 categories", { kind: "row", values: [20] }), []);
	assert.deepEqual(judgeMarkText("120\u00a0k", { kind: "row", values: [120000] }), []);
});

test("a mark's row is the same-numbered row of the state's Table; another mark's numbers are its own readout's", () => {
	const table = ["Region\tCause\tn", "South\tAlpha\t1,004", "South\tBeta\t1,006"];
	assert.deepEqual(markNumbers("row:1", { table }).values, [1006]);
	assert.equal(markNumbers("row:5", { table }), undefined);
	const marks = [{ id: "0/-/0", lines: [{ name: "Count", value: "3" }] }];
	assert.deepEqual(markNumbers("0/-/0", { marks }), { kind: "readout", values: [3] });
	assert.equal(markNumbers("0/-/9", { marks }), undefined);
});

test("a percent token is judged against shares, never against raw values at the span tolerance", () => {
	const options = { shares: true, cumulative: true, spans: [99980], strict: true };
	const column = [[100000, 50000, 20]];
	assert.deepEqual(judgeValueText("<0.1%", column, [], options), []);
	assert.deepEqual(judgeValueText(">99.9%", column, [], options), []);
	// Neither is the 0.1 % a near miss of the 20 that lies 195 away on a grid of this span.
	assert.deepEqual(judgeValueText("0.1%", column, [], options), []);
	// A percentage printed for a percentage of the data is still a near miss of it at the relative tolerance.
	assert.deepEqual(judgeValueText("12.3%", [[12, 50]]).map((h) => h.raw), ["12.3%"]);
	assert.deepEqual(judgeValueText("12%", [[12, 50]]), []);
});

test("a bar drawn to 3 under a readout of 2 is a finding, and bars that agree are not", () => {
	assert.deepEqual(judgeDrawn(histogram([3, 2, 1], [3, 2, 1])), []);
	const found = judgeDrawn(histogram([3, 2, 1], [2, 3, 1]));
	assert.deepEqual(found.map((f) => f.text), ["2", "3"]);
	assert.match(found[0].detail, /drawn to 3 but its readout says Count=2/);
});

test("without a labelled axis the largest readout fixes the scale", () => {
	const chart = histogram([3, 2, 1], [3, 2, 1]);
	chart.panels[0].labels = [];
	assert.deepEqual(judgeDrawn(chart), []);
	const wrong = histogram([3, 2, 1], [3, 3, 1]);
	wrong.panels[0].labels = [];
	assert.equal(judgeDrawn(wrong).length, 1);
});

test("the value axis is read from the labels beside the grid lines, and a log-like one is refused", () => {
	const panel = histogramPanel([1]);
	assert.ok(Math.abs(valueAxis(panel, true).perPixel - 1 / 80.5) < 1e-3);
	panel.labels[2].text = "4";
	assert.equal(valueAxis(panel, true), undefined);
});

/** A vertical Pareto of `values` in the drawn order, with the note the chart prints. */
const pareto = (values, drawn = values) => ({
	type: "pareto",
	note: "bars are sorted from the largest down; the line is the running share of the total, on the axis at the right",
	panels: [{ plot: { x: 15, y: 27.4, w: 500, h: 241.5 }, labels: [], hlines: [], vlines: [], rects: drawn.map((v, i) => ({ obs: i, series: 0, x: 16 + i * 100, y: 268.9 - v / 5, w: 90, h: v / 5 })) }],
	marks: values.map((v, i) => ({ panel: 0, obs: i, id: `row:${i}`, series: 0, heading: ["Alpha", "Beta", "Gamma"][i], area: { x: 15 + i * 100, y: 27.4, w: 100, h: 241.5 }, lines: [{ name: "n", value: v.toLocaleString("en"), series: 0 }] })),
});

test("a Pareto whose readouts run up where the note says down is a finding", () => {
	assert.deepEqual(judgeDrawn(pareto([1006, 1004, 20])), []);
	const found = judgeDrawn(pareto([1004, 1006, 20]));
	assert.equal(found.length, 1);
	assert.match(found[0].detail, /sorted from the largest down, but "Beta" \(1,006\) is drawn after "Alpha" \(1,004\)/);
	// Bars drawn in the wrong order under right readouts are caught by their heights.
	assert.equal(judgeDrawn(pareto([1006, 600, 20], [600, 1006, 20])).length >= 1, true);
});

test("a histogram count is judged against the rows of its bin in that state", () => {
	const chart = {
		type: "histogram",
		enc: { x: "v" },
		table: ["region\tv", "South\t1", "South\t2", "South\t10", "South\t11", "South\t12", "South\t25"],
		series: [],
		marks: [
			{ heading: "0 to 10", lines: [{ name: "Count", value: "3" }] },
			{ heading: "10 to 20", lines: [{ name: "Count", value: "4" }] },
			{ heading: "20 to 30 (inclusive)", lines: [{ name: "Count", value: "1" }] },
		],
	};
	// The 10 on the edge may fall in either bin (it is 9.996 or 10.004 at the author's precision), so 3 and 2 both read right for the first.
	assert.deepEqual(judgeDerivedReadouts(chart).map((f) => f.text), ["4"]);
	chart.marks[1].lines[0].value = "2";
	assert.deepEqual(judgeDerivedReadouts(chart), []);
	// A hidden series leaves the others' rows to be counted.
	const coloured = { ...chart, enc: { x: "v", color: "g" }, table: ["g\tv", "A\t11", "A\t12", "B\t1", "B\t2"], series: [{ name: "A", hidden: true, slot: 0 }, { name: "B", hidden: false, slot: 1 }], marks: [{ heading: "0 to 10", lines: [{ name: "Count", value: "3" }] }] };
	assert.deepEqual(judgeDerivedReadouts(coloured).map((f) => f.text), ["3"]);
});

test("a box plot's median and quartiles are judged against the rows of its box", () => {
	const chart = {
		type: "boxplot",
		enc: { x: "sample", y: "value" },
		table: ["sample\tvalue", "First\t1", "First\t2", "First\t4", "First\t8", "Second\t30"],
		marks: [{ heading: "First", lines: [{ name: "Observations", value: "4" }, { name: "Third quartile", value: "5" }, { name: "Median", value: "3" }, { name: "First quartile", value: "1.75" }] }],
	};
	assert.deepEqual(judgeDerivedReadouts(chart), []);
	// One unit of the last digit is rounding (a median of 3 read as 4 from whole-number rows); two are not.
	chart.marks[0].lines[2].value = "4";
	assert.deepEqual(judgeDerivedReadouts(chart), []);
	chart.marks[0].lines[2].value = "5";
	chart.marks[0].lines[0].value = "5";
	assert.deepEqual(judgeDerivedReadouts(chart).map((f) => f.text), ["5", "5"]);
});

test("narrow high-value box plots use Table precision for statistics and whiskers", () => {
	const chart = {
		type: "boxplot", enc: { x: "sample", y: "value" },
		table: ["sample\tvalue", ...[1001, 1003, 1004, 1006, 1009].map((value) => `North\t${value.toLocaleString("en-US")}`)],
		marks: [{ heading: "North", lines: [
			{ name: "Median", value: "1,000" }, { name: "First quartile", value: "1,000" },
			{ name: "Third quartile", value: "1,010" }, { name: "Lower whisker", value: "1,000" }, { name: "Upper whisker", value: "1,010" },
		] }],
	};
	assert.equal(judgeDerivedReadouts(chart).length, 5);
	for (const [line, value] of chart.marks[0].lines.map((line, index) => [line, [1004, 1003, 1006, 1001, 1009][index]])) line.value = value.toLocaleString("en-US");
	assert.deepEqual(judgeDerivedReadouts(chart), []);
	chart.table = ["sample\tvalue", "North\t999", "North\t1,000", "North\t1,001"];
	chart.marks[0].lines = [{ name: "Median", value: "1k" }];
	assert.match(judgeDerivedReadouts(chart)[0].detail, /less precise than.*Table/);
});

test("box-plot whiskers follow the Tukey fence rather than outliers", () => {
	const chart = { type: "boxplot", enc: { x: "group", y: "value" }, table: ["group\tvalue", ...[1, 2, 3, 4, 100].map((value) => `A\t${value}`)], marks: [{ heading: "A", lines: [{ name: "Median", value: "3" }, { name: "Lower whisker", value: "1" }, { name: "Upper whisker", value: "4" }] }] };
	assert.deepEqual(judgeDerivedReadouts(chart), []);
	chart.marks[0].lines[2].value = "100";
	assert.deepEqual(judgeDerivedReadouts(chart).map((finding) => finding.text), ["100"]);
});

const hiddenHistogram = () => ({
	type: "histogram", enc: { x: "v", color: "g" },
	table: ["g\tv", "A\t1", "A\t2", "A\t10", "B\t11", "B\t12", "B\t25"],
	series: [{ name: "A", hidden: false, slot: 0 }, { name: "B", hidden: true, slot: 1 }],
	summary: "6 observations from 0 to 30; the fullest interval, 10 to 20, holds 3.",
	marks: [
		{ heading: "0 to 10", lines: [{ name: "Count", value: "2" }, { name: "A", value: "2", series: 0 }] },
		{ heading: "10 to 20", lines: [{ name: "Count", value: "1" }, { name: "A", value: "1", series: 0 }] },
		{ heading: "20 to 30 (inclusive)", lines: [{ name: "Count", value: "0" }] },
	],
});

test("a histogram summary and accessible name are checked against that state's marks", () => {
	const chart = hiddenHistogram();
	assert.ok(judgeDerivedReadouts(chart).some((finding) => /summary.*holds 3/i.test(finding.detail)));
	chart.summary = "3 observations from 0 to 30; the fullest interval, 0 to 10, holds 2.";
	assert.deepEqual(judgeDerivedReadouts(chart), []);
	chart.summary = "3 observations from 0 to 31; the fullest interval, 0 to 10, holds 2.";
	assert.ok(judgeDerivedReadouts(chart).some(finding => /summary/.test(finding.detail)));
	chart.summary = "3 observations from 0 to 30; the fullest interval, 0 to 10, holds 2.";
	chart.accessibleSummary = "Chart. 6 observations from 0 to 30; the fullest interval, 10 to 20, holds 3.";
	assert.ok(judgeDerivedReadouts(chart).some((finding) => /accessible/i.test(finding.detail)));
});

test("legend toggles preserve each still-visible series' bin membership despite rounded Table edges", () => {
	const after = hiddenHistogram();
	const before = structuredClone(after);
	before.series[1].hidden = false;
	before.marks[0].lines = [{ name: "Count", value: "3" }, { name: "A", value: "3", series: 0 }];
	before.marks[1].lines = [{ name: "Count", value: "2" }, { name: "B", value: "2", series: 1 }];
	before.marks[2].lines = [{ name: "Count", value: "1" }, { name: "B", value: "1", series: 1 }];
	assert.ok(labelTruth.judgeLegendReplay(before, after).some((finding) => /A.*3.*2/.test(finding.detail)));
	after.marks[0].lines = [{ name: "Count", value: "3" }, { name: "A", value: "3", series: 0 }];
	after.marks[1].lines = [{ name: "Count", value: "0" }];
	assert.deepEqual(labelTruth.judgeLegendReplay(before, after), []);
});

test("hiding every group means zero visible histogram rows", () => {
	const chart = hiddenHistogram();
	chart.series[0].hidden = true;
	chart.summary = "No observations.";
	for (const mark of chart.marks) mark.lines = [{ name: "Count", value: "0" }];
	assert.deepEqual(judgeDerivedReadouts(chart), []);
});

test("density renormalization is allowed while legend bin structure remains fixed", () => {
	const before = hiddenHistogram(), after = hiddenHistogram();
	for (const [i, mark] of before.marks.entries()) mark.lines = [{ name: "Density", value: "0.2" }, { name: "A", value: "0.1", series: 0 }];
	for (const mark of after.marks) mark.lines = [{ name: "Density", value: "0.3" }, { name: "A", value: "0.3", series: 0 }];
	assert.deepEqual(labelTruth.judgeLegendReplay(before, after), []);
	after.marks[0].heading = "0 to 11";
	assert.match(labelTruth.judgeLegendReplay(before, after)[0].detail, /bin structure/);
});

// Exercise the shipping catch and its private diagnostic helper without exporting a test hook.
const collectionSource = readFileSync(new URL("../label-truth.mjs", import.meta.url), "utf8");
const diagnosticStart = collectionSource.indexOf("async function retainCollectionFailure(");
const diagnostic = diagnosticStart < 0 ? "" : collectionSource.slice(diagnosticStart, collectionSource.indexOf("/** Run the step;", diagnosticStart));
const visitStart = collectionSource.indexOf("const visit = async (where, work) =>");
const visitSource = collectionSource.slice(visitStart, collectionSource.indexOf("// A readout heading", visitStart));
const flushCollection = async () => { for (let i = 0; i < 24; i++) await Promise.resolve(); };

function collectionScenario({ dom, io } = {}) {
	const problems = [], saved = [], writes = [], timers = new Map(), probes = [];
	let nextTimer = 0, later = 0, finishDom, finishIo;
	const overlay = { tagName: "DIV", id: "overlay", className: "notice", textContent: "Covering the legend" };
	const rect = { x: 10, y: 20, width: 100, height: 40, toJSON() { return { x: this.x, y: this.y, width: this.width, height: this.height }; } };
	const button = { tagName: "BUTTON", id: "legend", className: "rv-chart__series", textContent: "North", isConnected: true, disabled: false, getBoundingClientRect: () => rect, contains: () => false };
	const figure = { tagName: "FIGURE", id: "chart-2", className: "rv-chart", textContent: "Chart title", querySelector: () => button };
	const page = { evaluate: async (fn, index) => {
		probes.push(index);
		if (dom === "throw") throw Error("diagnostic DOM failed");
		if (dom === "pending") return new Promise(resolve => { finishDom = resolve; });
		return vm.runInNewContext(`(${fn})(${index})`, {
			document: { readyState: "complete", fonts: { status: "loaded" }, querySelectorAll: () => [null, null, figure], elementFromPoint: () => overlay },
			getComputedStyle: () => ({ display: "block", visibility: "visible", pointerEvents: "auto" }), scrollX: 0, scrollY: 120, innerWidth: 390, innerHeight: 900,
		});
	} };
	const writeFile = async (file, text, options) => {
		writes.push({ file, text, options });
		assert.equal(problems[0]?.message, "width 390: locator.click: Timeout 5000ms exceeded.", "Record the original failure before any diagnostic IO");
		if (io === "throw" || (io === "second-throw" && writes.length === 2)) throw Error("diagnostic write failed");
		if (io === "pending") await new Promise(resolve => { finishIo = resolve; });
		saved.push(JSON.parse(text));
	};
	const visit = new Function("page", "real", "report", "width", "problems", "writeFile", "join", "setTimeout", "clearTimeout", "AbortController", `${diagnostic}\n${visitSource}\nreturn visit;`)(
		page, "/diagnostics", { id: "flagship" }, 390, problems, writeFile, join,
		(fn, ms) => { timers.set(++nextTimer, { fn, ms }); return nextTimer; }, id => timers.delete(id), AbortController,
	);
	const original = Error("locator.click: Timeout 5000ms exceeded.\nCall log:\n  overlay intercepts pointer events");
	return { problems, saved, writes, probes, timers, original, fail: () => visit("chart 2", async () => { throw original; }),
		success: () => visit("chart 3", async () => { later++; }), later: () => later,
		expire: () => { assert.equal(timers.size, 1); const [id, timer] = timers.entries().next().value; assert.equal(timer.ms, 1000, "Keep the one-second diagnostic cap"); timers.delete(id); timer.fn(); },
		finishDom: value => finishDom(value), finishIo: () => finishIo() };
}

test("collection failures retain the original call log and actual DOM probe while later charts continue", async () => {
	const s = collectionScenario();
	await s.fail();
	assert.ok(s.saved.length, "Retain the full original collection error and call log");
	const record = s.saved.at(-1);
	assert.equal(record.error, s.original.message);
	assert.equal(record.stack, s.original.stack);
	assert.equal(record.current.button.id, "legend");
	assert.equal(record.current.centerHit.id, "overlay");
	assert.equal(record.current.hitIsTarget, false);
	assert.deepEqual(s.probes, [2]);
	assert.deepEqual(s.problems, [{ report: "flagship", state: "chart 2", message: "width 390: locator.click: Timeout 5000ms exceeded." }]);
	await s.success(); assert.equal(s.later(), 1); assert.equal(s.timers.size, 0);
});

test("successful collection does not probe DOM, write diagnostics or arm a timer", async () => {
	const s = collectionScenario({ dom: "throw", io: "throw" });
	await s.success();
	assert.equal(s.later(), 1);
	assert.deepEqual(s.problems, []); assert.deepEqual(s.probes, []); assert.deepEqual(s.writes, []); assert.equal(s.timers.size, 0);
});

test("diagnostic DOM and IO errors cannot replace the original failure or stop later collection", async () => {
	for (const options of [{ dom: "throw" }, { io: "throw" }, { io: "second-throw" }]) {
		const s = collectionScenario(options);
		await s.fail(); await s.success();
		assert.equal(s.problems.length, 1); assert.equal(s.problems[0].message, s.original.message.split("\n")[0].replace(/^/, "width 390: "));
		assert.equal(s.later(), 1); assert.equal(s.timers.size, 0);
		if (options.dom) assert.equal(s.saved.at(-1).current.error, "diagnostic DOM failed");
	}
});

test("the one-second cap covers pending DOM and IO without late diagnostic work", async () => {
	for (const options of [{ dom: "pending" }, { io: "pending" }]) {
		const s = collectionScenario(options), pending = s.fail();
		await flushCollection();
		assert.equal(s.problems.length, 1, "The failed collection must already be recorded while diagnostics are pending");
		s.expire(); await pending; await s.success();
		assert.equal(s.later(), 1); assert.equal(s.timers.size, 0); assert.equal(s.writes[0].options.signal.aborted, true);
		if (options.dom) s.finishDom({ late: true }); else s.finishIo();
		await flushCollection();
		assert.equal(s.writes.length, 1, "A late result cannot start another write");
		if (options.io) assert.deepEqual(s.probes, [], "A late first write cannot start a DOM probe");
	}
});
