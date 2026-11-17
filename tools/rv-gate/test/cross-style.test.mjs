// Tests for the cross-style step's pure parts (DEL-52, DEL-80): the precision rule, the
// comparison across cells and the disclosure scan. None of them needs a browser or the release CLI.

import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import test from "node:test";
import { chartProbe } from "../label-truth.mjs";
import { fileURLToPath } from "node:url";
import { pool, FIXTURE, NOT_COLLECTED, SCALE_ROLES, VALUE_ROLES, pageProbe, collectPage, compareCells, retryOnTimeout, timedOut, unreadReasons, digitsFor, maxDigitsFor, flatten, inputList, judgeFixtureCell, judgeNumber, judgeText, locate, keyboardOutsideModel, labelNumbers, numbersOf, walkedAll, parseNumbers, parsePrecision, roundSig, scanDisclosure, scannableHtml, sigDigits } from "../cross-style.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const first = (text) => parseNumbers(text)[0];

test("roundSig rounds to significant digits", () => {
	assert.equal(roundSig(1234567.891, 3), 1230000);
	assert.equal(roundSig(0.045678912, 3), 0.0457);
	assert.equal(roundSig(98.7654321, 3), 98.8);
	assert.equal(roundSig(0, 3), 0);
	assert.equal(roundSig(-167543.217, 4), -167500);
});

test("sigDigits ignores leading zeros and a whole number's trailing zeros", () => {
	assert.equal(sigDigits("1230000"), 3);
	assert.equal(sigDigits("0", ".0457"), 3);
	assert.equal(sigDigits("98", ".80"), 4);
	assert.equal(sigDigits("50"), 1);
	assert.equal(sigDigits("0"), 1);
});

test("parseNumbers reads currency, separators, percent and scale words as a reader sees them", () => {
	assert.deepEqual(parseNumbers("Revenue $98,765 and 12.3% and 1.23 million and 167.5K and 168 thousand").map((t) => [t.raw, t.value, t.sig, t.percent]), [
		["$98,765", 98765, 5, false],
		["12.3%", 12.3, 3, true],
		["1.23 million", 1230000, 3, false],
		["167.5K", 167500, 4, false],
		["168 thousand", 168000, 3, false],
	]);
	assert.equal(first("98.8 thousand").scale, true);
	assert.equal(first("1,235").value, 1235);
});

test("parseNumbers does not read the parts of a date as numbers", () => {
	assert.deepEqual(parseNumbers("on 2024-03-15 and 2024-03-15T10:30:45.123Z, in 2024-03").map((t) => t.raw), []);
	assert.deepEqual(parseNumbers("Mar 1, 2024: 3.14").map((t) => t.raw), ["3.14"], "a written date is blanked too (release 1zc)");
});

test("the default is three significant digits, four at most with an author override", () => {
	const rule = { sig: 3, maxSig: 4, overrides: { "Tooltip chart": 4, "Greedy chart": 9 } };
	assert.equal(digitsFor(rule, "Plain chart"), 3);
	assert.equal(digitsFor(rule, "Tooltip chart"), 4);
	assert.equal(digitsFor(rule, "Greedy chart"), 4, "the cap holds");
});

test("judgeNumber accepts the source rounded to the rule and names the other cases", () => {
	const sources = [1234567.891, 0.045678912, 98765.4321];
	const judge = (text, limit = 3) => judgeNumber(first(text), sources, limit);
	assert.equal(judge("1.23 million").status, "ok");
	assert.equal(judge("4.57%").status, "ok");
	assert.equal(judge("98.8 thousand").status, "ok");
	assert.equal(judge("98,765").status, "excess");
	assert.equal(judge("98,765").expected, 98800);
	assert.equal(judge("1.2M").status, "coarse");
	assert.equal(judge("1.2M").expected, 1230000);
	assert.equal(judge("4.6%").status, "coarse");
	assert.equal(judge("1.235 million", 4).status, "ok", "four digits pass when the author sets four");
	assert.equal(judge("1.235 million", 3).status, "excess");
	assert.equal(judge("3 categories").status, "unmatched", "a count is no value");
	assert.equal(judge("5000").status, "unmatched");
});

test("a percentage is the source times 100 and a plain number is not", () => {
	const sources = [0.123456789];
	assert.equal(judgeNumber(first("12.3%"), sources, 3).status, "ok");
	assert.equal(judgeNumber(first("0.123"), sources, 3).status, "ok");
	assert.equal(judgeNumber(first("12.35%"), sources, 3).status, "excess");
});

test("judgeText returns only the numbers that are wrong", () => {
	const bad = judgeText("Alpha 1.23 million, Beta 167.5 thousand, 3 series", [1234567.891, 167543.217], 3);
	assert.deepEqual(bad.map((b) => [b.raw, b.status, b.expected]), [["167.5 thousand", "excess", 168000]]);
});

test("a wrong value fails even though it is no rounding of the source; declared counts pass", () => {
	const sources = [1234567.891, 167543.217, 4321.98765];
	// Rounded the wrong way, scaled wrongly or truncated: none is a rounding of a source value.
	for (const wrong of ["169 thousand", "1.24 million", "4,323", "1.23 thousand"]) {
		assert.equal(judgeText(wrong, sources, 3).length, 1, wrong);
	}
	assert.deepEqual(judgeText("Total 21", sources, 3, 4, [21]), []);
	assert.equal(judgeText("Total 21", sources, 3).length, 1, "an undeclared number fails");
	assert.deepEqual(judgeText("21 observations", sources, 3), [], "a count needs no declaration");
});

test("scanDisclosure finds a source value shown with more digits than allowed", () => {
	const sources = [1234567.891, 0.123456789];
	const html = '<td data-rv-sort-value="1234567.891">1.23M</td><p>0.123</p><script data-rv-payload>{"rows":[[0.123456789]]}</script>';
	assert.deepEqual(scanDisclosure(html, sources, 3).map((f) => f.raw), ["1234567.891", "0.123456789"]);
	assert.deepEqual(scanDisclosure("1.23 million and 12.3%", sources, 3), []);
	assert.equal(scanDisclosure("1.235 million", sources, 4).length, 0, "four digits are allowed when the author said so");
	assert.equal(scanDisclosure("1.235 million", sources, 3).length, 1);
});

test("scanDisclosure reads a number once and ignores unrelated long numbers", () => {
	assert.equal(scanDisclosure("1234567.891 and 1234567.891", [1234567.891], 3).length, 1);
	assert.deepEqual(scanDisclosure("width 5123456.789 px", [1234567.891], 3), []);
});

test("scannableHtml reads a payload's array values apart and keeps a grouped number in its strings", () => {
	const html = '<script type="application/json" data-rv-payload>{"shown":{"v":[120,132,140]},"x":[123,456.789123],"t":"1,234,567.891"}</script>';
	const found = scanDisclosure(scannableHtml(html), [456.789123, 1234567.891], 4).map((f) => f.raw);
	assert.deepEqual(found.sort(), ["1,234,567.891", "456.789123"]);
});

test("scanDisclosure compares a number longer than a double at 17 digits", () => {
	const glued = Array.from({ length: 40 }, (_, i) => String(100 + i)).join(",");
	assert.deepEqual(scanDisclosure(glued, [1.5], 3), []);
	assert.equal(scanDisclosure("1.234567890123456789", [1.234567890123456789], 3).length, 1);
});

test("scannableHtml drops the stylesheet and runtime code but keeps embedded data and attributes", () => {
	const html = '<style>a{width:1234567.891px}</style><script>var x=1234567.891</script><script type="application/json" data-rv-payload>{"v":1234567.891}</script><i title="1234567.891"></i>';
	const text = scannableHtml(html);
	assert.equal(scanDisclosure(text, [1234567.891], 3).length, 1);
	assert.ok(text.includes('data-rv-payload'));
	assert.ok(text.includes('title="1234567.891"'));
	assert.ok(!text.includes("var x"));
	assert.ok(!text.includes("width:1234567"));
});

test("compareCells passes identical cells and reports each differing value with its cells", () => {
	const cell = (id, values) => ({ id, values });
	assert.deepEqual(compareCells([cell("a@390", { name: "x", csv: "1" }), cell("b@1440", { name: "x", csv: "1" })]), []);
	const mismatches = compareCells([cell("a@390", { csv: "1.23" }), cell("b@390", { csv: "1.2" }), cell("a@1440", { csv: "1.23" })]);
	assert.deepEqual(mismatches, [{ key: "csv", groups: [{ value: "1.23", cells: ["a@390", "a@1440"] }, { value: "1.2", cells: ["b@390"] }] }]);
});

test("compareCells treats a value that one cell lacks as a difference", () => {
	const mismatches = compareCells([{ id: "a", values: { labels: "x" } }, { id: "b", values: {} }]);
	assert.equal(mismatches.length, 1);
	assert.equal(mismatches[0].groups[1].value, "(absent)");
});

test("compareCells leaves out a value its collector could not read, so a crashed cell is one problem, not a difference per chart", () => {
	const cells = [{ id: "a@390", values: { csv: "1", table: "t" } }, { id: "b@390", values: { csv: NOT_COLLECTED, table: NOT_COLLECTED } }, { id: "b@1440", values: { csv: "1", table: "t" } }];
	assert.deepEqual(compareCells(cells), []);
	cells[2].values.csv = "2";
	assert.deepEqual(compareCells(cells), [{ key: "csv", groups: [{ value: "1", cells: ["a@390"] }, { value: "2", cells: ["b@1440"] }] }]);
});

test("flatten marks a failed read as not collected and keeps a closed tab panel's empty value", () => {
	const chart = (skipped) => ({ title: "T", name: "N", summary: "S", labels: [], dropped: [], readouts: [], keyboard: [], marks: 2, table: [], csv: "", skipped });
	const crashed = flatten({ text: "", cells: [], metrics: [], charts: [chart(["keyboard: frame.focus: Target crashed", "table: Target crashed", "csv: Target crashed"])] });
	assert.equal(crashed["chart 0 (T) keyboard readouts"], NOT_COLLECTED);
	assert.equal(crashed["chart 0 (T) table"], NOT_COLLECTED);
	assert.equal(crashed["chart 0 (T) csv"], NOT_COLLECTED);
	const hidden = flatten({ text: "", cells: [], metrics: [], charts: [chart(["keyboard: chart not visible (closed tab panel)", "table: chart not visible (closed tab panel)", "csv: chart not visible (closed tab panel)"])] });
	assert.equal(hidden["chart 0 (T) table"], "");
	assert.equal(hidden["chart 0 (T) csv"], "");
	assert.deepEqual(unreadReasons(chart(["table: chart not visible (closed tab panel)", "csv: Target crashed"])), []);
	// Review 1g should-fix 10: a style that hides the Table button at 390 px is compared and counted, not left out.
	const missing = chart(["table: locator.click: Timeout 5000ms exceeded"]);
	assert.equal(flatten({ text: "", cells: [], metrics: [], charts: [missing] })["chart 0 (T) table"], "");
	assert.deepEqual(unreadReasons(missing), ["table: locator.click: Timeout 5000ms exceeded"]);
	// A timed-out read is re-read once after the pool; a crash, a closed panel or a clean page is not.
	assert.equal(timedOut({ charts: [missing] }), true);
	assert.equal(timedOut({ charts: [chart(["csv: Target crashed"]), chart(["table: chart not visible (closed tab panel)"]), chart([])] }), false);
	assert.equal(timedOut(undefined), false);
});

test("data label numbers read the labels and the key, a repeated number twice, and leave axis ticks out", () => {
	const base = { dropped: [], labels: [] };
	const wide = labelNumbers({ ...base, valueLabels: ["12.3%", "23.5%", "64.2%"], keyLabels: ["North 12.3%", "South 23.5%", "West 64.2%"] });
	const phone = labelNumbers({ ...base, valueLabels: [], keyLabels: ["North 12.3%", "South 23.5%", "West 64.2%"] });
	assert.equal(wide, phone, "a pie's labels give way to its key at phone width");
	assert.equal(labelNumbers({ ...base, labels: ["0", "50", "100"], valueLabels: [] }), "", "ticks are not values");
	// Review 1h note 3: a label that takes another mark's number is a difference.
	assert.notEqual(labelNumbers({ ...base, valueLabels: ["12", "12", "30"] }), labelNumbers({ ...base, valueLabels: ["12", "30", "30"] }));
});

test("the page probe reads figure-wide pie keys and keeps dropped scales out of value comparison only", t => {
	const frame = { getAttribute: () => "A pie", getBoundingClientRect: () => ({ width: 390 }) };
	const figure = { id: "pie", querySelector: selector => selector === ".rv-chart__title" ? { textContent: "A pie" } : frame };
	const descriptors = ["document", "window"].map(name => [name, Object.getOwnPropertyDescriptor(globalThis, name)]);
	t.after(() => { for (const [name, descriptor] of descriptors) { if (descriptor) Object.defineProperty(globalThis, name, descriptor); else delete globalThis[name]; } });
	globalThis.document = { body: { cloneNode: () => ({ querySelectorAll: () => [], textContent: "" }) }, querySelectorAll: selector => selector === "figure[data-rv-chart]" ? [figure] : [] };
	globalThis.window = { __richview: { charts: () => [{ geometry: {
		panels: [{ items: [{ kind: "text", role: "axis-label", text: "100" }], observations: [] }],
		figure_items: [{ kind: "text", role: "key", text: "North 12.3%" }, { kind: "text", role: "key", text: "South 87.7%" }],
		dropped: [{ text: "20", kind: "scale-bar" }, { text: "120", kind: "axis label" }, { text: "42", kind: "label" }],
	} }] } };
	const chart = pageProbe({ scaleRoles: [...SCALE_ROLES], valueRoles: [...VALUE_ROLES] }).charts[0];
	assert.deepEqual(chart.keyLabels, ["North 12.3%", "South 87.7%"]);
	assert.deepEqual(chart.valueDropped, ["42"]);
	assert.deepEqual(chart.dropped, ["20", "120", "42"], "scale text still reaches precision/disclosure judging");
	assert.equal(labelNumbers(chart), "12.3% | 42 | 87.7%");
	assert.notEqual(labelNumbers(chart), labelNumbers({ ...chart, keyLabels: ["North 12.4%", "South 87.7%"] }), "a changed percentage remains a data difference");
	const models = window.__richview.charts();
	for (const incomplete of [[], [...models, ...models]]) {
		window.__richview.charts = () => incomplete;
		for (const probe of [pageProbe, chartProbe]) assert.throws(() => probe({ scaleRoles: [...SCALE_ROLES], valueRoles: [...VALUE_ROLES] }), Error, "every figure has exactly one model in either value projection");
	}
});

test("collectPage stops reading charts once the tab has crashed", async () => {
	const touched = [];
	const crash = (i) => async () => {
		touched.push(i);
		throw new Error("locator.focus: Target crashed");
	};
	const figure = (i) => ({ scrollIntoViewIfNeeded: crash(i), locator: () => { const locator = { first: () => locator, isVisible: crash(i), focus: crash(i), click: crash(i), evaluate: crash(i), evaluateAll: crash(i) }; return locator; } });
	const charts = [0, 1, 2].map((i) => ({ title: `C${i}`, marks: 1, visible: true }));
	const page = {
		locator: () => ({ count: async () => 3, nth: figure }),
		evaluate: async () => ({ text: "", cells: [], metrics: [], charts }),
		keyboard: { press: async () => {} },
		waitForEvent: async () => {
			throw new Error("page.waitForEvent: Target crashed");
		},
	};
	const probe = await collectPage(page, async () => {});
	assert.ok(touched.every((i) => i === 0), "no chart after the crash is read");
	for (const chart of probe.charts) assert.ok(chart.skipped.length === 3 && chart.skipped.every((reason) => reason.includes("Target crashed")), JSON.stringify(chart.skipped));
});

/** A page whose trusted key events update the readout before keyup, like the report runtime. */
function keyboardPage({ marks = 3, omit, untrusted, fail } = {}) {
	const listeners = new Set();
	const pressed = [];
	let selected = -1;
	let handles = 0;
	const readout = {
		get hidden() { return selected < 0 || selected === 1; },
		querySelectorAll: () => [
			{ firstElementChild: {}, textContent: "wrapper must not be repeated" },
			{ firstElementChild: null, textContent: ` Mark ${selected} ` },
			{ firstElementChild: null, textContent: " Amount " },
			{ firstElementChild: null, textContent: ` ${selected}.23 ` },
			{ firstElementChild: null, textContent: " " },
		],
	};
	const frame = {
		addEventListener: (name, fn) => { assert.equal(name, "keyup"); listeners.add(fn); },
		removeEventListener: (name, fn) => { assert.equal(name, "keyup"); listeners.delete(fn); },
	};
	const node = { querySelector: (selector) => selector === ".rv-chart__frame" ? frame : readout };
	const figure = {
		scrollIntoViewIfNeeded: async () => {},
		locator: () => { const locator = { first: () => locator, isVisible: async () => true, focus: async () => {}, click: async () => {}, evaluate: async (fn) => fn(readout), evaluateAll: async () => ["Amount\t1.23"] }; return locator; },
		evaluateHandle: async (fn, arg) => {
			const value = fn(node, arg);
			handles++;
			return { evaluate: async (read) => read(value), dispose: async () => { handles--; } };
		},
	};
	const chart = { title: "Keyboard chart", marks, visible: true };
	const page = {
		locator: () => ({ count: async () => 1, nth: () => figure }),
		evaluate: async () => ({ text: "", cells: [], metrics: [], charts: [chart] }),
		keyboard: { press: async (key) => {
			pressed.push(key);
			if (key === fail) throw new Error("Keyboard transport failed");
			if (key === "Home") selected = 0;
			else if (key === "ArrowRight") selected++;
			else if (key === "Escape") selected = -1;
			if (key !== omit) for (const listener of listeners) listener({ key, isTrusted: key !== untrusted });
		} },
		waitForEvent: async () => ({ path: async () => join(root, FIXTURE, "precision.json"), suggestedFilename: () => "values.csv" }),
	};
	return { page, pressed, listeners, handles: () => handles };
}

test("keyboard collection keeps every trusted key, leaf text and empty readout, including the walk limit", async () => {
	for (const marks of [0, 3, 27]) {
		const walk = keyboardPage({ marks });
		const probe = await collectPage(walk.page, async () => {});
		const count = Math.min(marks, 24);
		assert.deepEqual(probe.charts[0].keyboard, Array.from({ length: count }, (_, i) => i === 1 ? "(no readout)" : `Mark ${i} Amount ${i}.23`));
		assert.deepEqual(walk.pressed, ["Home", ...Array(Math.max(0, count - 1)).fill("ArrowRight"), "Escape"]);
		assert.deepEqual(probe.charts[0].skipped, []);
		assert.equal(walk.listeners.size, 0);
		assert.equal(walk.handles(), 0);
	}
});

test("missing or untrusted keyboard work fails collection instead of yielding a partial value set", async () => {
	for (const options of [{ omit: "Home" }, { omit: "ArrowRight" }, { omit: "Escape" }, { untrusted: "ArrowRight" }]) {
		const walk = keyboardPage(options);
		const probe = await collectPage(walk.page, async () => {});
		assert.deepEqual(probe.charts[0].keyboard, []);
		assert.match(unreadReasons(probe.charts[0]).join("\n"), /keyboard: Keyboard walk incomplete/);
		assert.deepEqual(probe.charts[0].table, ["Amount\t1.23"], "other value sources are still collected");
		assert.equal(walk.listeners.size, 0);
		assert.equal(walk.handles(), 0);
	}
});

test("a failed keyboard transport detaches its collector and preserves the read failure", async () => {
	const walk = keyboardPage({ fail: "ArrowRight" });
	const probe = await collectPage(walk.page, async () => {});
	assert.match(unreadReasons(probe.charts[0]).join("\n"), /keyboard: Keyboard transport failed/);
	assert.deepEqual(walk.pressed, ["Home", "ArrowRight"]);
	assert.equal(walk.listeners.size, 0);
	assert.equal(walk.handles(), 0);
});

test("numbersOf compares numbers and ignores a style's own wording", () => {
	assert.equal(numbersOf("Revenue 1.23 million"), numbersOf("REVENUE 1.23 million!"));
	assert.notEqual(numbersOf("Revenue 1.23 million"), numbersOf("Revenue 1.2 million"));
});

test("flatten keys every reader-visible source of a chart", () => {
	const probe = {
		text: "Total 1.23 million",
		cells: ["1.23M"],
		metrics: ["1.23M / Revenue"],
		charts: [{ title: "T", name: "N", summary: "S", labels: ["b 2.5%"], dropped: ["a 1.2M"], readouts: ["r"], keyboard: ["k", "j"], marks: 2, table: ["t"], csv: "c" }],
	};
	const values = flatten(probe);
	assert.equal(values["chart 0 (T) data label numbers"], "1.2M | 2.5%", "labels compare by their numbers, dropped ones included");
	assert.equal(values["chart 0 (T) keyboard readouts"], "j | k", "the order of the marks is layout");
	assert.equal(flatten({ ...probe, charts: [{ ...probe.charts[0], readouts: ["b", "a"] }] })["chart 0 (T) model readouts"], "a | b", "so is the model's order");
	assert.deepEqual(Object.keys(values).sort(), ["chart 0 (T) accessible name", "chart 0 (T) csv", "chart 0 (T) data label numbers", "chart 0 (T) keyboard readouts", "chart 0 (T) model readouts", "chart 0 (T) summary", "chart 0 (T) table", "page.cells", "page.metrics", "page.text"]);
});

test("a chart longer than the keyboard walk is judged only against its own model", () => {
	const long = { title: "L", table: [], csv: "", labels: [], dropped: [], readouts: ["A: x 1"], keyboard: ["A x 1"], marks: 100 };
	assert.equal(walkedAll(long), false);
	assert.match(flatten({ text: "", cells: [], metrics: [], charts: [long] })["chart 0 (L) keyboard readouts"], /100 marks/);
	assert.deepEqual(keyboardOutsideModel({ charts: [long] }), []);
	assert.deepEqual(keyboardOutsideModel({ charts: [{ ...long, keyboard: ["A x 2"] }] }), [{ chart: "0 (L)", readout: "A x 2" }]);
	assert.equal(labelNumbers({ labels: ["Community support"], dropped: [] }), "");
});

test("keyboard readings use the visible rows while the full model remains available", () => {
	const readout = { heading: "Email and Web", lines: [{ name: "Readers", value: "210" }, { name: "Share", value: "33.9%", derived: true }, { name: "Other share", value: "38.9%", derived: true }] };
	const chart = { kind: "sankey", title: "T", readouts: ["Email and Web: Readers 210; Share 33.9%; Other share 38.9%"], readoutRecords: [{ targeting: "contain", readout }], keyboard: ["Email and Web Readers 210"] };
	assert.deepEqual(keyboardOutsideModel({ charts: [chart] }), []);
	assert.equal(keyboardOutsideModel({ charts: [{ ...chart, keyboard: ["Email and Web Readers 211"] }] }).length, 1);
	assert.equal(keyboardOutsideModel({ charts: [{ ...chart, readoutRecords: [{ targeting: "nearest-x", readout }] }] }).length, 1);
	assert.equal(keyboardOutsideModel({ charts: [{ ...chart, kind: "flow" }] }).length, 1);
	assert.equal(chart.readouts[0], "Email and Web: Readers 210; Share 33.9%; Other share 38.9%");
});

test("judgeFixtureCell judges rounding in every output and scans scale marks only for disclosure", () => {
	const rule = { sig: 3, maxSig: 4, overrides: {}, values: [1234567.891], derived: [] };
	const probe = { text: "1.23 million", cells: [], metrics: [], charts: [{ title: "T", name: "", summary: "", labels: [], dropped: [], ticks: ["1234568"], readouts: ["A: Amount 1.2 million"], keyboard: [], table: [], csv: "1234567.891\n" }] };
	const { rounding, disclosure } = judgeFixtureCell(probe, rule);
	assert.deepEqual(rounding.map((r) => [r.where, r.shown, r.status]), [["T: model readouts", "1.2 million", "coarse"], ["T: csv download", "1234567.891", "excess"]]);
	assert.deepEqual(disclosure.map((d) => [d.where, d.shown]), [["T: axis labels", "1234568"], ["T: csv download", "1234567.891"]]);
});

test("the precision fixture's rule is valid and its values appear in the fixture", () => {
	const rule = parsePrecision(readFileSync(join(root, FIXTURE, "precision.json"), "utf8"));
	assert.equal(rule.sig, 3);
	const source = readFileSync(join(root, FIXTURE, "report.rmd"), "utf8") + readFileSync(join(root, FIXTURE, "data/prices.csv"), "utf8");
	for (const value of rule.values) assert.ok(source.includes(String(value)), `${value} is in the fixture source`);
	assert.equal(sigDigits(String(rule.values[0]).split(".")[0], "." + String(rule.values[0]).split(".")[1]) > rule.sig, true, "the fixture carries more digits than shown");
	const sum = (xs) => xs.reduce((a, b) => a + b, 0);
	assert.ok(Math.abs(rule.derived[0] - sum(rule.values.slice(0, 3))) < 1e-6, "the amount total");
	assert.ok(Math.abs(rule.derived[1] - sum(rule.values.slice(3, 6))) < 1e-6, "the price total");
});

test("parsePrecision rejects a malformed rule", () => {
	assert.throws(() => parsePrecision('{"sig": 0, "values": []}'), /sig/);
	assert.throws(() => parsePrecision('{"sig": 3, "maxSig": 2, "values": []}'), /maxSig/);
	assert.throws(() => parsePrecision('{"sig": 3, "values": ["1"]}'), /lists of numbers/);
});

test("the inputs are every-block, the golden list and the fixture, once each", () => {
	const inputs = inputList(root).map((i) => i.id);
	assert.equal(inputs[0], "examples/every-block");
	assert.equal(inputs.at(-1), FIXTURE);
	assert.equal(new Set(inputs).size, inputs.length);
	assert.ok(inputs.includes("examples/openai-vs-anthropic"));
	for (const id of inputs) assert.ok(existsSync(join(root, id, "report.rmd")), id);
	assert.deepEqual(inputList(root, "every-block").map((i) => i.id), ["examples/every-block"]);
});

test("locate says where in the HTML a value sits", () => {
	const html = '<td data-rv-sort-value="1234567.891" class="num">1.23M</td><script type="application/json" data-rv-payload>{"v":0.123456789}</script><p>2.718281</p>';
	assert.equal(locate(html, "1234567.891"), "attribute data-rv-sort-value");
	assert.equal(locate(html, "0.123456789"), "embedded chart data");
	assert.equal(locate(html, "2.718281"), "text");
});

test("up to four significant digits pass by default; five, or more than the author set, are excess", () => {
	const rule = { sig: 3, maxSig: 4, overrides: { Shares: 2 } };
	const sources = [1234.56789, 167543.217, 98765.4321];
	assert.equal(maxDigitsFor(rule, "anything"), 4);
	assert.equal(maxDigitsFor(rule, "Shares"), 2);
	assert.equal(judgeNumber(first("1,235"), sources, digitsFor(rule, "x"), maxDigitsFor(rule, "x")).status, "ok", "a whole number below 10,000 prints whole");
	assert.equal(judgeNumber(first("167.5 thousand"), sources, 3, 4).status, "ok", "four digits with a scale word");
	assert.equal(judgeNumber(first("$98,765"), sources, 3, 4).status, "excess", "five digits");
	assert.equal(judgeNumber(first("1,235"), sources, digitsFor(rule, "Shares"), maxDigitsFor(rule, "Shares")).status, "excess", "more than the author set");
	assert.deepEqual(scanDisclosure("98765.4321, 1234.6 and 1,235", sources, 4).map((f) => f.raw), ["98765.4321", "1234.6"]);
});

test("a number shown to fewer digits than the rule fails (FEA-CH-37, review 1e should-fix 28)", () => {
	const sources = [3.14159265, 0.345678912, 1234567.891, 167543.217, 4321.98765, 0.5];
	for (const shown of ["3", "0.3", "1 million", "200 thousand", "4,000"]) {
		const bad = judgeText(shown, sources, 3, 4, []);
		assert.equal(bad.length, 1, shown);
		assert.equal(bad[0].status, "coarse", shown);
	}
	assert.equal(judgeText("3", sources, 3, 4, [3]).length, 1, "a declaration never excuses a coarse rounding of a source");
	assert.deepEqual(judgeText("0.5", sources, 3, 4, []), [], "the source itself has one digit");
	assert.deepEqual(judgeText("50%", sources, 3, 4, []), []);
	assert.deepEqual(judgeText("3.14 and 4,320 and 1.23 million", sources, 3, 4, []), []);
	assert.deepEqual(judgeText("7 series", sources, 3, 4, []), [], "a number before a count noun is a count");
	assert.equal(judgeText("7", sources, 3, 4, []).length, 1, "an unmatched one-digit number fails unless declared");
	assert.deepEqual(judgeText("7", sources, 3, 4, [7]), []);
	assert.equal(judgeText("2 million", sources, 3, 4, []).length, 1);
	assert.deepEqual(judgeText("2 million", sources, 3, 4, ["2 million"]), [], "a declared shown text passes");
	assert.deepEqual(judgeText("3", sources, 1, 1, []), [], "an element the author set to one digit");
});

test("counts need no declaration: a number before a count noun is not judged", () => {
	const sources = [3.14159265, 1234567.891];
	assert.deepEqual(judgeText("3 categories; highest Alpha 1.23 million", sources, 3, 4, []), []);
	assert.deepEqual(judgeText("1 category, 24 rows, 12 bins", sources, 3, 4, []), []);
	// Review 1g should-fix 11: a decimal is never a count, and a unit is never a count noun.
	for (const [text, source] of [["4.88 points", 5], ["72.4567 years", 72.4567], ["3.14159265 categories", 3.14159265], ["1.0 days", 1.04159], ["3 years", 3.14159265]])
		assert.equal(judgeText(text, [source], 3, 4, []).length, 1, text);
	assert.equal(judgeText("3 million categories", sources, 3, 4, []).length, 1, "a scaled number is a value, not a count");
	assert.equal(judgeText("3 of 24", sources, 3, 4, []).length, 2, "no count noun, so both are judged");
	assert.equal(judgeText("3 categoriesx", sources, 3, 4, []).length, 1, "the noun is a whole word");
});

test("written dates are positions on a time axis, not values (release 1zc)", () => {
	const raw = (text) => parseNumbers(text).map((n) => n.raw);
	assert.deepEqual(raw("1 series, from Mar 1, 2024 to Mar 4, 2024: Level ends at 1.73."), ["1", "1.73"]);
	assert.deepEqual(raw("Mar 3, 2024: Level 1.41"), ["1.41"]);
	assert.deepEqual(raw("4 series, from Jul 2024 to Sep 2026: r/ChatGPT ends at 5,709"), ["4", "5,709"]);
	assert.deepEqual(raw("on 1 March 2024 it was 3.14"), ["3.14"]);
	assert.deepEqual(raw("May 1.5 million"), ["1.5 million"], "a scaled number after a month name is a value");
	// Review 1h should-fix 1: a word that merely starts like a month is no date.
	for (const [text, source] of [["Q1: Marketing 4", 4.4], ["Decline 7", 7.08], ["Augsburg 3", 3.14], ["12 Margin points", 12.4], ["Novartis 88", 88.4]])
		assert.equal(judgeText(text, [source], 3, 4, []).length, 1, text);
	assert.deepEqual(raw("Sept. 30, 2025 and 31 December 2025, not March 32"), ["32"]);
	// Release 1zd: readouts joined line by line; the decimals before the next line's date stay whole.
	assert.deepEqual(raw("Mar 1, 2024: Level 3.14\nMar 2, 2024: Level 2.72\nMar 3, 2024: Level 1,234 Mar 4"), ["3.14", "2.72", "1,234"]);
	assert.deepEqual(judgeText("Mar 3, 2024: Level 1.41", [1.41421356], 3, 4, []), []);
	assert.equal(judgeText("Mar 3, 2024: Level 1", [1.41421356], 3, 4, []).length, 1, "the value after the date is still judged");
	// Review 1j note 1: a year is 1500 to 2199, so a four-digit value after a month name is judged.
	assert.deepEqual(raw("May 1234, Jun 9999"), ["1234", "9999"]);
	assert.deepEqual(raw("Dec 12, 4567"), ["4567"]);
	assert.deepEqual(raw("Jul 1960 to Jun 2024"), []);
});

test("a read that times out is retried once with a longer timeout; a second timeout says so", async () => {
	const calls = [];
	const flaky = async (timeout) => (calls.push(timeout), timeout < 10000 ? { rows: [], reason: "locator.click: Timeout 5000ms exceeded." } : { rows: ["a"] });
	assert.deepEqual(await retryOnTimeout(flaky), { rows: ["a"] });
	assert.deepEqual(calls, [5000, 15000]);
	const stuck = await retryOnTimeout(async (timeout) => ({ rows: [], reason: `locator.click: Timeout ${timeout}ms exceeded.` }));
	assert.match(stuck.reason, /Timeout 15000ms exceeded\. \(also after a retry at 15 s\)/);
	let tries = 0;
	await retryOnTimeout(async () => (tries++, { rows: [], reason: "locator.click: Target crashed" }));
	assert.equal(tries, 1, "a crash is not retried");
});

test("bounded cases preserve source order despite different completion order", async () => {
	let release, started;
	const blocked = new Promise(resolve => { release = resolve; });
	const ready = new Promise(resolve => { started = resolve; });
	let active = 0, maximum = 0;
	const completed = [];
	const pending = pool([3, 2, 1], 2, async value => {
		active++;
		maximum = Math.max(maximum, active);
		try {
			if (value === 3) { started(); await blocked; }
			completed.push(value);
			return value;
		} finally { active--; }
	});
	await ready;
	await new Promise(setImmediate);
	release();
	assert.deepEqual(await pending, [3, 2, 1]);
	assert.deepEqual(completed, [2, 1, 3]);
	assert.equal(maximum, 2);
	assert.equal(active, 0);
});

test("a failed case drains queued work and waits for active cleanup before rejecting", async () => {
	let release;
	const held = new Promise(resolve => { release = resolve; });
	let settled = false, cleaned = false;
	const seen = [];
	const pending = pool([0, 1, 2, 3], 2, async value => {
		seen.push(value);
		if (value === 0 || value === 2) throw Error(`bad case ${value}`);
		if (value === 1) { try { await held; } finally { cleaned = true; } }
		return value;
	});
	pending.then(() => { settled = true; }, () => { settled = true; });
	await new Promise(setImmediate);
	try { assert.equal(settled, false, "resource owner must not close the browser while a case is active"); }
	finally { release(); }
	await assert.rejects(pending, error => error instanceof AggregateError && error.errors.length === 2);
	assert.equal(cleaned, true);
	assert.deepEqual([...seen].sort(), [0, 1, 2, 3]);
});

test("invalid worker counts fail before a case can disappear", async () => {
	for (const size of [0, -1, NaN, Infinity, 1.5]) {
		let called = false;
		await assert.rejects(pool(["case"], size, async () => { called = true; }), RangeError);
		assert.equal(called, false);
	}
});
