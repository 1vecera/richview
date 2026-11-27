// Tests for the a11y step's pure parts (DEL-58) and its keyboard pass, run in Chromium on tiny
// static pages served from loopback. The audit itself runs on the every-block fixture in the step.

import assert from "node:assert/strict";
import { createServer } from "node:http";
import { dirname, join, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { AXE_TAGS, WIDTHS, focusIndicated, groupByRule, judgeKeyboard, keyboardPass, matrix, summarize } from "../a11y.mts";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const styles = ["sleek", "air"];

test("the matrix is every style at 1440 and 390 px", () => {
	assert.deepEqual(WIDTHS, [1440, 390]);
	assert.deepEqual(matrix(styles).map((m: { id: string }) => m.id), ["sleek-1440", "sleek-390", "air-1440", "air-390"]);
	assert.equal(matrix(["a", "b", "c", "d", "e"]).length, 10);
});

test("the audit runs the WCAG 2.0, 2.1 and 2.2 A and AA tags and no others", () => {
	assert.deepEqual(AXE_TAGS, ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"]);
});

const rule = (id: string, impact: string, targets: (string | string[])[]) => ({ id, impact, help: `${id} help`, helpUrl: `https://example.test/${id}`, nodes: targets.map((t) => ({ target: Array.isArray(t) ? t : [t] })) });

test("the summary has one row per node with rule, impact, selector and help URL", () => {
	const rows = summarize([
		{ style: "sleek", width: 390, violations: [rule("label", "critical", ["input.a", ["iframe", "input.b"]])] },
		{ style: "air", width: 1440, violations: [rule("color-contrast", "serious", ["summary"])] },
		{ style: "air", width: 390, violations: [] },
	]);
	assert.deepEqual(rows[1], { style: "sleek", width: 390, rule: "label", impact: "critical", selector: "iframe input.b", help: "label help", helpUrl: "https://example.test/label" });
	assert.equal(rows.length, 3);
	const groups = groupByRule(rows);
	assert.deepEqual(groups.map((g) => [g.rule, g.count, g.pages]), [["label", 2, ["sleek-390"]], ["color-contrast", 1, ["air-1440"]]]);
});

test("a focus indicator is an outline, shadow, border, fill or underline that differs from the unfocused element", () => {
	const none = { outlineStyle: "none", outlineWidth: "0px", outlineColor: "red", outlineOffset: "0px", boxShadow: "none", borderColor: "black", borderWidth: "1px", backgroundColor: "white", color: "black", textDecorationLine: "none" };
	const ring0 = { ...none, outlineStyle: "solid", outlineWidth: "2px" };
	assert.equal(focusIndicated(none, { ...none }), false);
	assert.equal(focusIndicated(none, { ...none, outlineStyle: "solid", outlineWidth: "2px" }), true);
	assert.equal(focusIndicated(none, { ...none, outlineStyle: "solid", outlineWidth: "0px" }), false);
	assert.equal(focusIndicated(none, { ...none, boxShadow: "0 0 0 2px blue" }), true);
	assert.equal(focusIndicated(none, { ...none, textDecorationLine: "underline" }), true);
	assert.equal(focusIndicated(undefined, none), false);
	assert.equal(focusIndicated(undefined, ring0), true);
	// A painted ::after that appears on focus, such as the chart frame's ring, is an indicator; one that goes is not.
	const after = "::after solid 2px rgb(0, 0, 255) none none rgba(0, 0, 0, 0)";
	assert.equal(focusIndicated({ ...none, pseudo: "" }, { ...none, pseudo: after }), true);
	assert.equal(focusIndicated({ ...none, pseudo: after }, { ...none, pseudo: after }), false);
	assert.equal(focusIndicated({ ...none, pseudo: after }, { ...none, pseudo: "" }), false);
	assert.equal(focusIndicated(undefined, { ...none, pseudo: after }), true);
});

const style = { outlineStyle: "none", outlineWidth: "0px", outlineColor: "c", outlineOffset: "0px", boxShadow: "none", borderColor: "b", borderWidth: "1px", backgroundColor: "w", color: "k", textDecorationLine: "none" };
const ring = { ...style, outlineStyle: "solid", outlineWidth: "2px" };
const stop = (id: number|null, ok = true) => ({ id, label: `el${id}`, before: style, after: ok ? ring : style });
const chart = { id: "rv-chart-0", title: "Sales rose", name: "Sales rose. 3 categories; highest A at 5. Arrow keys step through the data.", tabbable: true };

test("a clean pass has no failures", () => {
	const expected = [{ id: 0, label: "el0" }, { id: 1, label: "el1" }];
	assert.deepEqual(judgeKeyboard({ expected, visits: [stop(0), stop(1)], wrapped: true, charts: [chart] }), []);
});

test("judgeKeyboard reports unreachable elements, wrong order, missing focus rings, loops and unnamed charts", () => {
	const expected = [{ id: 0, label: "el0" }, { id: 1, label: "el1" }, { id: 2, label: "el2" }];
	const kinds = (input: { expected: { id: number; label: string }[]; visits: (ReturnType<typeof stop> & { native?: boolean })[]; wrapped: boolean; charts: (typeof chart)[] }) => judgeKeyboard(input).map((f) => f.kind);
	assert.deepEqual(kinds({ expected, visits: [stop(0), stop(1)], wrapped: true, charts: [] }), ["unreachable"]);
	assert.deepEqual(kinds({ expected, visits: [stop(0), stop(2), stop(1)], wrapped: true, charts: [] }), ["order"]);
	assert.deepEqual(kinds({ expected, visits: [stop(0), stop(1, false), stop(2)], wrapped: true, charts: [] }), ["focus-visible"]);
	assert.deepEqual(kinds({ expected, visits: [stop(0), stop(1), stop(2)], wrapped: false, charts: [] }), ["trap"]);
	assert.deepEqual(kinds({ expected, visits: [stop(0), stop(1), stop(0)], wrapped: false, charts: [] }), ["trap", "order", "unreachable", "trap"]);
	assert.deepEqual(kinds({ expected: expected.slice(0, 1), visits: [{ ...stop(null), native: true, after: style }, stop(0)], wrapped: true, charts: [] }), []);
	const bad = (patch: { tabbable?: boolean; name?: string; }) => judgeKeyboard({ expected: [], visits: [], wrapped: true, charts: [{ ...chart, ...patch }] }).map((f) => f.message);
	assert.equal(bad({ tabbable: false }).length, 1);
	assert.match(bad({ name: "" })[0], /no accessible name/);
	assert.match(bad({ name: "Other" })[0], /does not start with its title/);
	assert.match(bad({ name: "Sales rose." })[0], /title alone/);
});

const serve = (html: string) =>
	new Promise<{ url: string; close: () => void }>((ok) => {
		const server = createServer((req, res) => {
			res.writeHead(200, { "content-type": "text/html" });
			res.end(html);
		}).listen(0, "127.0.0.1", () => {
			const address = server.address();
			assert.ok(address && typeof address !== "string");
			ok({ url: `http://127.0.0.1:${address.port}/`, close: () => server.close() });
		});
	});

async function passOn(html: string) {
	const { chromium } = await import(join(root, "node_modules/playwright/index.mjs"));
	const site = await serve(html);
	const browser = await chromium.launch({ headless: true });
	try {
		const page = await browser.newPage();
		await page.goto(site.url);
		return await keyboardPass(page);
	} finally {
		await browser.close();
		site.close();
	}
}

const good = `<!doctype html><title>t</title><style>a:focus-visible,button:focus-visible,.f:focus-visible{outline:2px solid blue}</style>
<h1>Page</h1><a href="#x">One</a><button>Two</button><details><summary>Three</summary><button>Hidden until open</button></details>
<figure data-rv-chart id="c0"><figcaption><p class="rv-chart__title">Sales rose</p></figcaption><div class="rv-chart__frame f" tabindex="0" aria-label="Sales rose. 3 categories; highest A at 5.">Chart</div></figure><button disabled>Off</button>`;

test("keyboard pass: a chart frame whose ring is a painted ::after on focus passes", async () => {
	const ring = await passOn(`<!doctype html><title>t</title><style>.f{position:relative;outline:none}.f::after{position:absolute;inset:-6px;border:2px solid blue}.f:focus-visible::after{content:""}</style>
<figure data-rv-chart id="c0"><figcaption><p class="rv-chart__title">Sales rose</p></figcaption><div class="rv-chart__frame f" tabindex="0" aria-label="Sales rose. 3 categories; highest A at 5.">Chart</div></figure>`);
	assert.deepEqual(ring.failures, []);
});

test("keyboard pass: a small page passes; closed details content is not expected", async () => {
	const pass = await passOn(good);
	assert.deepEqual(pass.failures, []);
	assert.equal(pass.expected.length, 4);
	assert.equal(pass.wrapped, true);
	assert.equal(pass.charts.length, 1);
});

test("keyboard pass: no focus ring, an unfocusable chart frame and a focus trap are each found", async () => {
	const noRing = await passOn(`<!doctype html><title>t</title><style>button{outline:none}</style><button>Bare</button>`);
	assert.deepEqual(noRing.failures.map((f) => f.kind), ["focus-visible"]);
	const unnamed = await passOn(`<!doctype html><title>t</title><figure data-rv-chart id="c0"><p class="rv-chart__title">T</p><div class="rv-chart__frame">Chart</div></figure>`);
	assert.deepEqual(unnamed.failures.map((f) => f.kind).sort(), ["chart", "chart"]);
	const unpainted = await passOn(`<!doctype html><title>t</title><style>button{outline:none}button:focus::after{content:""}</style><button>Bare</button>`);
	assert.deepEqual(unpainted.failures.map((f) => f.kind), ["focus-visible"]);
	const trap = await passOn(`<!doctype html><title>t</title><button id="a">A</button><button id="b" onkeydown="if(event.key==='Tab'){event.preventDefault();a.focus()}">B</button>`);
	assert.ok(trap.failures.some((f) => f.kind === "trap"));
	assert.equal(trap.wrapped, false);
});
