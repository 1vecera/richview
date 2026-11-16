// Tests for the contract step's pure parts (ARC-08, TYP-53): normalization, the diagnostic diff, the
// LSP mapping and the browser-stage split. None of them needs a binary.

import assert from "node:assert/strict";
import test from "node:test";
import { pathToFileURL } from "node:url";
import {
	CORE_FIELDS,
	compareGroup,
	compareHtml,
	diffDiagnostics,
	discoverFixtures,
	envelopeDiffs,
	firstDifference,
	isBrowserMeasured,
	levelOfSeverity,
	lspToDiagnostic,
	nativeOnlySurfaces,
	normalizeDiagnostic,
	normalizeText,
	okOf,
	parseFrames,
	splitBrowserStage,
	wasmValidate,
} from "../contract.mjs";
import { STEPS } from "../lib/steps.mjs";

const d = (over = {}) => ({ level: "warning", code: "block.title", message: "Add a title", path: "chart.title", line: 11, tag: "chart", ...over });

test("absolute report paths become <report>, in every spelling", () => {
	const root = "/work/out/fixtures/examples/a";
	assert.equal(normalizeText(`Asset 'x.csv' is absent from '${root}'. Add it.`, [root]), "Asset 'x.csv' is absent from '<report>'. Add it.");
	assert.equal(normalizeText(`see ${pathToFileURL(root).href}/report.rmd`, [root]), "see <report>/report.rmd");
	assert.equal(normalizeText("no path here", [root]), "no path here");
	const out = normalizeDiagnostic({ level: "error", code: "c", message: `in ${root}`, details: { dir: root }, line: null, extra: 1 }, [root]);
	assert.deepEqual(out, { level: "error", code: "c", message: "in <report>", details: { dir: "<report>" } });
});

test("LSP ranges map back to 1-based lines and unlocated ones to none", () => {
	const range = (line, character, endLine = line, endCharacter = character + 3) => ({ start: { line, character }, end: { line: endLine, character: endCharacter } });
	assert.deepEqual(lspToDiagnostic({ range: range(10, 0), severity: 2, code: "block.title", message: "m" }), { level: "warning", code: "block.title", message: "m", line: 11 });
	assert.equal(lspToDiagnostic({ range: range(0, 4), severity: 1, code: "c", message: "m" }).line, 1);
	// The fields LSP has no slot for come back from `data`; a null there means absent.
	assert.deepEqual(lspToDiagnostic({ range: range(2, 0), severity: 1, code: "block.schema", message: "m", data: { path: "chart.type", tag: "chart", expected: null } }), { level: "error", code: "block.schema", message: "m", line: 3, path: "chart.type", tag: "chart" });
	assert.equal("line" in lspToDiagnostic({ range: range(0, 0, 0, 0), severity: 1, code: "c", message: "m" }), false);
	assert.deepEqual([1, 2, 3, 4, undefined].map(levelOfSeverity), ["error", "warning", "info", "info", "info"]);
});

test("equal diagnostics in any order differ only by order", () => {
	const a = [d(), d({ code: "chart.clipping", line: 3 })];
	assert.deepEqual(diffDiagnostics(a, [...a]), []);
	const order = diffDiagnostics(a, [a[1], a[0]]);
	assert.equal(order.length, 1);
	assert.equal(order[0].kind, "order");
});

test("a dropped, an added and a changed diagnostic are told apart", () => {
	const base = [d(), d({ code: "chart.clipping", line: 3 }), d({ code: "data.missing", line: 9 })];
	const other = [d({ message: "Add a different title" }), d({ code: "chart.clipping", line: 3 }), d({ code: "authoring.key", line: 4 })];
	const diffs = diffDiagnostics(base, other);
	const kinds = diffs.map((x) => x.kind).sort();
	assert.deepEqual(kinds, ["added", "changed", "missing"]);
	const changed = diffs.find((x) => x.kind === "changed");
	assert.deepEqual(changed.changes, [{ field: "message", base: "Add a title", other: "Add a different title" }]);
	assert.equal(diffs.find((x) => x.kind === "missing").diagnostic.code, "data.missing");
	assert.equal(diffs.find((x) => x.kind === "added").diagnostic.code, "authoring.key");
});

test("every compared field counts: level, code, message, path, line and tag", () => {
	for (const [field, value] of [["level", "error"], ["message", "x"], ["path", "chart.y"], ["line", 12], ["tag", "table"]]) {
		const diffs = diffDiagnostics([d()], [d({ [field]: value })], CORE_FIELDS);
		assert.equal(diffs.length, 1, field);
		assert.deepEqual(diffs[0].changes.map((c) => c.field), [field]);
	}
	const code = diffDiagnostics([d()], [d({ code: "other" })], CORE_FIELDS);
	assert.deepEqual(code.map((x) => x.kind).sort(), ["added", "missing"]);
	// A field outside the compared set is not judged.
	assert.deepEqual(diffDiagnostics([d({ column: 1 })], [d({ column: 2 })], CORE_FIELDS), []);
	assert.equal(diffDiagnostics([d({ column: 1 })], [d({ column: 2 })]).length, 1);
});

test("the envelope differs when a result has no ok or a different one", () => {
	assert.equal(okOf({ ok: true, diagnostics: [] }), true);
	assert.equal(okOf([]), null);
	assert.equal(okOf({ html: "" }), null);
	assert.deepEqual(envelopeDiffs({ cli: true, mcp: true }, "cli"), []);
	assert.deepEqual(envelopeDiffs({ cli: false, mcp: true, wasm: null }, "cli").map((x) => [x.surface, x.other]), [["mcp", true], ["wasm", null]]);
});

test("browser-measured diagnostics are compared only where both sides ran the stage", () => {
	const measured = d({ code: "chart.overlap", details: { measured: "browser" } });
	const plain = d();
	assert.equal(isBrowserMeasured(measured), true);
	assert.equal(isBrowserMeasured(d({ message: "Labels overlap (measured in a browser)" })), true);
	assert.equal(isBrowserMeasured(plain), false);
	const one = splitBrowserStage([plain, measured], [plain]);
	assert.deepEqual([one.ran, one.compared, one.native.length, one.wasm.length], [{ native: true, wasm: false }, false, 1, 1]);
	const both = splitBrowserStage([plain, measured], [plain, measured]);
	assert.deepEqual([both.compared, both.native.length, both.wasm.length], [true, 2, 2]);
	assert.equal(splitBrowserStage([plain], [plain]).compared, false);
});

test("geometry and HTML report the first place they differ", () => {
	assert.equal(firstDifference({ a: [1, { b: 2 }] }, { a: [1, { b: 2 }] }), null);
	assert.deepEqual(firstDifference({ a: [1, { b: 2 }] }, { a: [1, { b: 3 }] }), { path: "$.a[1].b", native: 2, wasm: 3 });
	assert.deepEqual(firstDifference([1, 2], [1]), { path: "$.length", native: 2, wasm: 1 });
	assert.equal(compareHtml("<p>a</p>", "<p>a</p>").equal, true);
	const miss = compareHtml("<p>abc</p>", "<p>abd</p>");
	assert.equal(miss.equal, false);
	assert.equal(miss.firstDifferenceAt, 5);
	assert.notEqual(miss.native.sha256, miss.wasm.sha256);
});

test("LSP frames split on Content-Length and keep a partial tail", () => {
	const body = (m) => Buffer.from(JSON.stringify(m));
	const frame = (m) => Buffer.concat([Buffer.from(`Content-Length: ${body(m).length}\r\n\r\n`), body(m)]);
	const whole = Buffer.concat([frame({ id: 1 }), frame({ id: 2, text: "žluťoučký" })]);
	const cut = whole.subarray(0, whole.length - 4);
	const first = parseFrames(cut);
	assert.deepEqual(first.messages, [{ id: 1 }]);
	assert.deepEqual(parseFrames(Buffer.concat([first.rest, whole.subarray(whole.length - 4)])).messages, [{ id: 2, text: "žluťoučký" }]);
});

test("the fixtures are the golden inputs plus the broken reports", () => {
	const repo = new URL("../../..", import.meta.url).pathname;
	const fixtures = discoverFixtures(repo);
	const broken = fixtures.filter((f) => f.kind === "broken").map((f) => f.id);
	for (const name of ["unknown-field", "missing-channel", "bad-yaml", "missing-data", "escaping-path"]) assert.ok(broken.includes(`contract/${name}`), name);
	assert.ok(fixtures.some((f) => f.id === "examples/hello-world" && f.kind === "golden"));
});

test("Wasm validate reads the shared envelope, passes the report folder, and a bare array has no ok", () => {
	let seen;
	const wasm = { validate: (source, options) => ((seen = options), { ok: false, status: "issues", diagnostics: [{ level: "error", code: "report.asset.missing", message: "m", path: "p" }] }) };
	const result = wasmValidate({ wasm }, "/r", "x", ["data/a.csv"]);
	assert.deepEqual(seen.report_folder.files, ["data/a.csv"]);
	assert.equal(result.ok, false);
	assert.equal(result.diagnostics[0].code, "report.asset.missing");
	assert.equal(wasmValidate({ wasm: { validate: () => [d()] } }, "/r", "x", []).ok, null);
	const thrown = { validate: () => { throw new Error(JSON.stringify([d()])); } };
	assert.equal(wasmValidate({ wasm: thrown }, "/r", "x", []).diagnostics.length, 1);
});

test("the contract step is built, needs the release build and runs the contract script", () => {
	const step = STEPS.find((s) => s.id === "contract");
	assert.equal(step.needs, "build");
	assert.equal(step.unrun, undefined);
	assert.equal(step.run[0].entry, "tools/rv-gate/contract.mjs");
	assert.ok(step.run[0].args.includes("{run}/contract") || JSON.stringify(step.run[0]).includes("{run}/contract"));
});

test("editor.lint is compared on the full diagnostic record (review 1e should-fix 29)", () => {
	const full = { ...d(), fix: "Add a title", expected: "a string" };
	const surface = (diagnostics) => ({ ok: false, diagnostics });
	for (const dropped of ["fix", "expected"]) {
		const { [dropped]: _gone, ...rest } = full;
		const diffs = compareGroup("validate", "cli.validate", { "cli.validate": surface([full]), "editor.lint": { envelope: false, ok: null, diagnostics: [rest] } }, {}, new Set());
		assert.equal(diffs.length, 1, dropped);
		assert.deepEqual(diffs[0].changes?.map((c) => c.field), [dropped]);
	}
	assert.deepEqual(compareGroup("validate", "cli.validate", { "cli.validate": surface([full]), "editor.lint": { envelope: false, ok: null, diagnostics: [full] } }, {}, new Set()), []);
});

test("connector fixtures are native only on every Wasm surface, and missing-data on Wasm check and generate", () => {
	const connector = nativeOnlySurfaces({ id: "examples/sql-report", files: ["report.rmd", "report.connectors.yaml"] });
	for (const surface of ["wasm.validate", "wasm.check", "wasm.generate"]) assert.ok(connector.surfaces.includes(surface), surface);
	assert.match(connector.reason, /connectors/);
	assert.ok(!connector.surfaces.includes("cli.validate"), "the native connector failures stay visible");
	assert.ok(!connector.surfaces.includes("editor.lint"));
	const missing = nativeOnlySurfaces({ id: "contract/missing-data", files: ["report.rmd"] });
	assert.deepEqual(missing.surfaces.sort(), ["wasm.check", "wasm.generate"]);
	assert.match(missing.reason, /data file/);
	assert.equal(nativeOnlySurfaces({ id: "contract/unknown-field", files: ["report.rmd"] }), undefined);
});
