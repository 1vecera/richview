// Tests for the type-evidence tool: which FEA-CH-75 items one release run shows per chart type.
// The synthetic run in fixtures/type-evidence has three types: bar passes every item the fixture
// carries, line has one failing chart, and pie has no label-truth evidence.
import assert from "node:assert/strict";
import test from "node:test";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { blocksFor, buildEvidence as buildEvidenceImplementation, chartBlocks, main, parseRegistry, renderAccepted, renderMarkdown } from "../type-evidence.mts";

const FIXTURE = join(dirname(fileURLToPath(import.meta.url)), "../fixtures/type-evidence");
const REGISTRY = parseRegistry(readFileSync(join(FIXTURE, "registry.rs"), "utf8"));
const buildEvidence = buildEvidenceImplementation as (options: { run: string; registry: string[]; require?: string[]; lab?: string }) => ReturnType<typeof buildEvidenceImplementation>;
const REQUIRE = ["checks", "golden", "label-truth"];

/** Copy the synthetic run to a temp folder, where the rendered pages (a gitignored folder) are written. */
function syntheticRun() {
	const root = mkdtempSync(join(tmpdir(), "type-evidence-"));
	const run = join(root, "release-20261028T000000Z-abcdef0123");
	cpSync(FIXTURE, run, { recursive: true });
	for (const name of ["bar", "line", "pie"]) {
		const rendered = join(run, "golden/inputs", `examples__${name}`, "Rendered");
		mkdirSync(rendered, { recursive: true });
		writeFileSync(join(rendered, "sleek.html"), "<html></html>");
	}
	return { root, run };
}

/** The status of one item of one type. */
type Report = ReturnType<typeof buildEvidence>;
function typeRow(report: Report, type: string) {
	const row = report.types.find(value => value.type === type);
	assert.ok(row, `Expected evidence for ${type}`);
	return row;
}
function statusOf(report: Report, type: string, item: string) {
	return typeRow(report, type).items[item].status;
}

test("a type with one failing chart is not accepted, and the failing chart is named", () => {
	const { root, run } = syntheticRun();
	try {
		const report = buildEvidence({ run, registry: REGISTRY, require: REQUIRE });
		const line = typeRow(report, "line");
		assert.equal(line.accepted, false);
		assert.equal(statusOf(report, "line", "checks"), "fail");
		assert.deepEqual(line.items.checks.failing, ["examples/line rv-chart-1: source-error"]);
		assert.equal(line.items.checks.source, "contract/contract.json");
		assert.equal(statusOf(report, "line", "golden"), "pass", "the other items of the type still show their own result");
	} finally {
		rmSync(root, { recursive: true, force: true });
	}
});

test("a type with an item lacking evidence is not accepted", () => {
	const { root, run } = syntheticRun();
	try {
		const report = buildEvidence({ run, registry: REGISTRY, require: REQUIRE });
		const pie = typeRow(report, "pie");
		assert.equal(statusOf(report, "pie", "label-truth"), "none");
		assert.equal(pie.accepted, false);
		assert.deepEqual(pie.blockers, ["label-truth"]);
		// By default every item is required, the ones no step evidences included, so nothing is accepted.
		const strict = buildEvidence({ run, registry: REGISTRY });
		assert.equal(typeRow(strict, "bar").accepted, false);
		assert.ok(typeRow(strict, "bar").blockers.includes("interaction"));
		assert.ok(!strict.required.includes("perf"));
		assert.ok(!typeRow(strict, "bar").blockers.includes("perf"));
		assert.deepEqual(strict.waived, []);
		assert.match(renderAccepted(strict), /Waived by --require, so not checked: none\./);
	} finally {
		rmSync(root, { recursive: true, force: true });
	}
});

test("a type passing everything required is accepted, and only it is written to the draft", () => {
	const { root, run } = syntheticRun();
	try {
		const report = buildEvidence({ run, registry: REGISTRY, require: REQUIRE });
		assert.deepEqual(report.types.filter((row) => row.accepted).map((row) => row.type), ["bar"]);
		const draft = renderAccepted(report);
		assert.deepEqual(draft.split("\n").filter((line) => line && !line.startsWith("#")), ["bar"]);
		assert.match(draft, /^# Draft from release run release-20261028T000000Z-abcdef0123\n# Commit abcdef0123, 2026-10-28T00:00:00Z\./);
		const table = renderMarkdown(report);
		assert.match(table, /\| bar \| yes \|/);
		assert.match(table, /\| line \| no \|/);
		// The items no step evidences stay visible as "no evidence" columns, and the draft says what was waived.
		for (const item of ["interaction", "motion", "library-comparison"]) assert.equal(statusOf(report, "bar", item), "none");
		assert.match(draft, /# Waived by --require, so not checked: type-tests, geometry, cross-style, interaction, motion, a11y-summary/);
		assert.deepEqual(report.waived.slice(0, 2), ["type-tests", "geometry"]);
	} finally {
		rmSync(root, { recursive: true, force: true });
	}
});

test("the command writes the three outputs", () => {
	const { root, run } = syntheticRun();
	const out = join(root, "out");
	const log = console.log;
	console.log = () => {};
	try {
		main(["--run", run, "--out", out, "--registry", join(FIXTURE, "registry.rs"), "--require", REQUIRE.join(",")]);
		assert.equal(JSON.parse(readFileSync(join(out, "type-evidence.json"), "utf8")).types.length, 3);
		assert.match(readFileSync(join(out, "type-evidence.md"), "utf8"), /^# Chart type evidence of release-/);
		assert.match(readFileSync(join(out, "accepted.txt.draft"), "utf8"), /\nbar\n$/);
	} finally {
		console.log = log;
		rmSync(root, { recursive: true, force: true });
	}
});

test("chart blocks read type, title and fence lines, and a finding is placed by title, line or index", () => {
	const text = "intro\n```chart\ntype: bar\ntitle: 'It''s here'\n```\n```callout\ntype: note\n```\n```chart\ntype: pie\ntitle: Second\n```\n";
	const blocks = chartBlocks(text);
	assert.deepEqual(blocks.map((block) => [block.type, block.title, block.start, block.end]), [["bar", "It's here", 2, 5], ["pie", "Second", 9, 12]]);
	assert.deepEqual(blocksFor(blocks, { title: "Second" }).map((block: { type: string }) => block.type), ["pie"]);
	assert.deepEqual(blocksFor(blocks, { line: 3 }).map((block: { type: string }) => block.type), ["bar"]);
	assert.deepEqual(blocksFor(blocks, { index: 1 }).map((block: { type: string }) => block.type), ["pie"]);
	assert.deepEqual(blocksFor(blocks, {}).map((block: { type: string }) => block.type), ["bar", "pie"], "an unplaceable finding counts against every chart");
});

/** A geometry.json view of the synthetic run. */
const view = (type: string, style: string, width: number, ok = true) => ({ report: "every-block", chart: 0, type, style, width, ok, failures: ok ? [] : ["text leaves the chart"] });

test("geometry passes a type with a view in every style at 320, 390 and 1440 px and none failed", () => {
	const { root, run } = syntheticRun();
	try {
		const styles = ["air", "sleek"];
		for (const style of styles) {
			mkdirSync(join(run, "fixtures", style), { recursive: true });
			writeFileSync(join(run, "fixtures", style, "report.rmd"), "```chart\ntype: bar\ntitle: A\n```\n```chart\ntype: pie\ntitle: B\n```\n```chart\ntype: line\ntitle: C\n```\n");
		}
		const all = (type: string, skip?: string) => styles.flatMap((style) => [320, 390, 1440].filter((width) => `${style}@${width}` !== skip).map((width) => view(type, style, width)));
		mkdirSync(join(run, "geometry"));
		writeFileSync(join(run, "geometry/geometry.json"), JSON.stringify({ views: [...all("bar"), ...all("pie", "air@320"), ...all("line"), view("line", "air", 390, false)] }));
		const report = buildEvidence({ run, registry: REGISTRY, require: ["geometry"] });
		assert.equal(statusOf(report, "bar", "geometry"), "pass");
		assert.equal(statusOf(report, "pie", "geometry"), "none", "a missing style and width is missing evidence");
		assert.deepEqual(typeRow(report, "pie").items.geometry.notes, ["no view at air@320"]);
		assert.equal(statusOf(report, "line", "geometry"), "fail");
		assert.match(typeRow(report, "line").items.geometry.failing[0], /air@390 chart 0: text leaves the chart/);
	} finally {
		rmSync(root, { recursive: true, force: true });
	}
});

test("captures pass only when the step passed and the index places a chart of the type in every style at 390 and 1440 px", () => {
	const { root, run } = syntheticRun();
	try {
		for (const style of ["air", "sleek"]) {
			mkdirSync(join(run, "fixtures", style), { recursive: true });
			writeFileSync(join(run, "fixtures", style, "report.rmd"), "```chart\ntype: bar\ntitle: A\n```\n```chart\ntype: pie\ntitle: B\n```\n");
		}
		mkdirSync(join(run, "captures"));
		const held = (type: string, name: string) => ({ name, chart: 0, type, box: { x: 0, y: 0, width: 300, height: 200 } });
		const names = ["air/390", "air/1440", "sleek/390", "sleek/1440"].map((name) => `fixture/${name}`);
		for (const name of names) writeFileSync(join(run, "captures", `${name.replaceAll("/", "__")}.png`), "");
		// The pie chart is missing from the sleek 1440 capture.
		writeFileSync(join(run, "captures-index.json"), JSON.stringify({ charts: [...names.map((name) => held("bar", name)), ...names.slice(0, 3).map((name) => held("pie", name))] }));
		const withReceipt = (status: string, reason?: string) => writeFileSync(join(run, "receipt.json"), JSON.stringify({ id: "r", commit: "abc", steps: [{ id: "captures", status, reason }] }));
		withReceipt("pass");
		let report = buildEvidence({ run, registry: REGISTRY, require: ["captures"] });
		assert.equal(statusOf(report, "bar", "captures"), "pass");
		assert.equal(statusOf(report, "pie", "captures"), "none");
		assert.deepEqual(typeRow(report, "pie").items.captures.notes, ["no capture of the type at fixture/sleek/1440"]);
		withReceipt("unrun", "No pre-prod baseline yet.");
		report = buildEvidence({ run, registry: REGISTRY, require: ["captures"] });
		assert.equal(statusOf(report, "bar", "captures"), "none");
		assert.match(typeRow(report, "bar").items.captures.notes![0], /is unrun: No pre-prod baseline yet\./);
	} finally {
		rmSync(root, { recursive: true, force: true });
	}
});

test("per-type timing remains honest disclosure without blocking acceptance", () => {
	const { root, run } = syntheticRun();
	try {
		mkdirSync(join(run, "perf"), { recursive: true });
		const entries = [
			{ index: 0, type: "bar", native_ms: 9, mount_ms: 90, limit_native_ms: 4, limit_mount_ms: 50, status: "fail" },
			{ index: 1, type: "line", native_ms: 3, mount_ms: 40, limit_native_ms: 4, limit_mount_ms: 50, status: "pass" },
			{ index: 2, type: "pie", native_ms: 3, mount_ms: 40, limit_native_ms: null, limit_mount_ms: null, status: "no baseline yet" },
		];
		writeFileSync(join(run, "perf/perf.json"), JSON.stringify({ allTypes: { entries }, failures: [{ name: "allTypes.0:bar.native_ms" }] }));
		const report = buildEvidence({ run, registry: REGISTRY, require: [...REQUIRE, "perf"] });
		assert.equal(statusOf(report, "bar", "perf"), "fail");
		assert.equal(statusOf(report, "line", "perf"), "pass");
		assert.equal(statusOf(report, "pie", "perf"), "none");
		assert.deepEqual(report.types.filter((row) => row.accepted).map((row) => row.type), ["bar"]);
		assert.deepEqual(report.required, REQUIRE);
		assert.deepEqual(report.disclosures, ["perf"]);
		assert.ok(!report.waived.includes("perf"));
		assert.deepEqual(typeRow(report, "line").blockers, ["checks"]);
		assert.match(renderAccepted(report), /Disclosure only: perf/);
		assert.equal(report.unattributed.filter((entry) => entry.item === "perf").length, 0);
	} finally {
		rmSync(root, { recursive: true, force: true });
	}
});

test("interaction passes a type only on its own every-block charts", () => {
	const { root, run } = syntheticRun();
	try {
		mkdirSync(join(run, "interaction"), { recursive: true });
		const results = [
			{ index: 0, type: "bar", ok: true },
			{ index: 1, type: "line", ok: false, error: "readout did not clear on Escape" },
		];
		writeFileSync(join(run, "interaction/types.json"), JSON.stringify({ results, errors: [] }));
		const report = buildEvidence({ run, registry: REGISTRY, require: ["interaction"] });
		assert.equal(statusOf(report, "bar", "interaction"), "pass");
		assert.equal(statusOf(report, "line", "interaction"), "fail");
		assert.equal(statusOf(report, "pie", "interaction"), "none");
		assert.deepEqual(report.types.filter((row) => row.accepted).map((row) => row.type), ["bar"]);
	} finally {
		rmSync(root, { recursive: true, force: true });
	}
});

test("motion passes a type only on its own every-block charts", () => {
	const { root, run } = syntheticRun();
	try {
		mkdirSync(join(run, "motion"), { recursive: true });
		const charts = [
			{ id: "rv-chart-1", type: "bar", ok: true, failures: [] },
			{ id: "rv-chart-2", type: "line", ok: false, failures: ["the last frame differs from the settled frame"] },
		];
		writeFileSync(join(run, "motion/types.json"), JSON.stringify({ summary: {}, charts }));
		const report = buildEvidence({ run, registry: REGISTRY, require: ["motion"] });
		assert.equal(statusOf(report, "bar", "motion"), "pass");
		assert.equal(statusOf(report, "line", "motion"), "fail");
		assert.equal(statusOf(report, "pie", "motion"), "none");
	} finally {
		rmSync(root, { recursive: true, force: true });
	}
});

test("library comparison passes only a type that withstands, in a lab of this run's commit", () => {
	const { root, run } = syntheticRun();
	const lab = join(root, "lab");
	try {
		mkdirSync(join(lab, "verdicts"), { recursive: true });
		const types = [
			{ id: "bar", type: "bar", libraries: ["highcharts", "echarts"] },
			{ id: "line", type: "line", libraries: ["highcharts"] },
			{ id: "pie", type: "pie", libraries: [] },
		];
		writeFileSync(join(lab, "comparison.json"), JSON.stringify({ richview: { commit: "abcdef0123" }, types }));
		writeFileSync(join(lab, "verdicts/bar.json"), JSON.stringify({ verdict: "withstands", summary: "" }));
		writeFileSync(join(lab, "verdicts/line.json"), JSON.stringify({ verdict: "close", summary: "720: labels heavier than Highcharts" }));
		const report = buildEvidence({ run, registry: REGISTRY, require: ["library-comparison"], lab });
		assert.equal(statusOf(report, "bar", "library-comparison"), "pass");
		assert.equal(statusOf(report, "line", "library-comparison"), "fail");
		assert.equal(statusOf(report, "pie", "library-comparison"), "none");
		// A lab of another commit is no evidence for this run.
		writeFileSync(join(lab, "comparison.json"), JSON.stringify({ richview: { commit: "1234567890" }, types }));
		const other = buildEvidence({ run, registry: REGISTRY, require: ["library-comparison"], lab });
		assert.equal(statusOf(other, "bar", "library-comparison"), "none");
	} finally {
		rmSync(root, { recursive: true, force: true });
	}
});

/** A synthetic run in the shape review 1l's te-vacuous.sh built: every item a type might pass on is vacuous. */
function vacuousRun() {
	const root = mkdtempSync(join(tmpdir(), "type-evidence-vacuous-"));
	const run = join(root, "release-20261028T000000Z-abcdef0123");
	const report = (type: string) => `---\ntitle: T\n---\n\n\`\`\`chart\ntype: ${type}\ntitle: ${type} one\n\`\`\`\n`;
	for (const folder of ["golden/inputs/examples__first-report", "golden/inputs/examples__every-block", "cross-style/inputs/examples__every-block", "label-truth/inputs/waffle"]) {
		mkdirSync(join(run, folder), { recursive: true });
		writeFileSync(join(run, folder, "report.rmd"), report(folder.includes("waffle") ? "waffle" : "bar"));
	}
	mkdirSync(join(run, "logs"), { recursive: true });
	mkdirSync(join(run, "exports"));
	writeFileSync(join(run, "receipt.json"), JSON.stringify({ id: "r", steps: [{ id: "exports", status: "fail" }, { id: "label-truth", status: "pass" }, { id: "cross-style", status: "pass" }] }));
	writeFileSync(join(run, "logs/exports-0.log"), "first-report/sleek: 2 pages, 1000 bytes, max gap 10%, browser print 2 pages; pass\nevery-block/sleek: 57 charts, 56 vector in the PDF; every-block/sleek chart 0 (bar): a raster image covers 90% of the chart on page 2\n");
	writeFileSync(join(run, "exports/types.json"), JSON.stringify([{ index: 0, type: "bar", title: "bar one", a: true, b: true, c: false, largest_raster: 0.9, ok: false }]));
	writeFileSync(join(run, "label-truth/label-truth.json"), JSON.stringify({ reports: ["waffle"], judged: [{ report: "waffle", state: "initial (1100)", charts: 1 }], judgedTexts: [], findings: [], problems: [] }));
	writeFileSync(join(run, "cross-style/cross-style.json"), JSON.stringify({ inputs: ["examples/every-block"], comparedKeys: 40, comparedCharts: [{ input: "examples/every-block", chart: 0, title: "bar one", keys: 0, cells: 1 }], mismatches: [], problems: [] }));
	return { root, run };
}

test("a chart type with no judged text, no compared key or a failing chart PDF check never passes (review 1l should-fix 4)", () => {
	const { root, run } = vacuousRun();
	try {
		const report = buildEvidence({ run, registry: ["bar", "waffle"], require: ["pdf", "label-truth", "cross-style"] });
		assert.equal(statusOf(report, "bar", "pdf"), "fail", "the per-chart check of types.json, not the pagination line, decides");
		assert.equal(statusOf(report, "waffle", "label-truth"), "none");
		assert.match(typeRow(report, "waffle").items["label-truth"].notes![0], /no value text of a waffle chart was judged/);
		assert.equal(statusOf(report, "bar", "cross-style"), "none", "a run that compared keys elsewhere is no evidence for this chart");
		assert.deepEqual(report.types.filter((row) => row.accepted).map((row) => row.type), []);
	} finally {
		rmSync(root, { recursive: true, force: true });
	}
});

test("pdf, label-truth and cross-style pass on evidence about the type, and a log line alone passes nothing", () => {
	const { root, run } = vacuousRun();
	try {
		writeFileSync(join(run, "exports/types.json"), JSON.stringify([{ index: 0, type: "bar", ok: true }, { index: 1, type: "waffle", ok: true }]));
		writeFileSync(join(run, "label-truth/label-truth.json"), JSON.stringify({ reports: ["waffle"], judged: [{ report: "waffle", state: "initial (1100)", charts: 1 }], judgedTexts: [{ report: "waffle", chart: 0, title: "waffle one", texts: 2 }], findings: [], problems: [] }));
		writeFileSync(join(run, "cross-style/cross-style.json"), JSON.stringify({ inputs: ["examples/every-block"], comparedCharts: [{ input: "examples/every-block", chart: 0, title: "bar one", keys: 7, cells: 12 }], mismatches: [], problems: [] }));
		const report = buildEvidence({ run, registry: ["bar", "waffle"] });
		assert.equal(statusOf(report, "waffle", "label-truth"), "pass");
		assert.equal(statusOf(report, "bar", "cross-style"), "pass");
		assert.equal(statusOf(report, "waffle", "pdf"), "pass");
		assert.equal(statusOf(report, "bar", "pdf"), "pass");
		writeFileSync(join(run, "logs/exports-0.log"), "first-report/sleek: 2 pages, 1000 bytes, max gap 40%; max gap over a quarter\n");
		assert.equal(statusOf(buildEvidence({ run, registry: ["bar", "waffle"] }), "bar", "pdf"), "fail", "a failing pagination line fails its example's types");
		rmSync(join(run, "exports/types.json"));
		const gone = buildEvidence({ run, registry: ["bar", "waffle"], require: ["pdf"] });
		assert.equal(statusOf(gone, "waffle", "pdf"), "none");
		assert.ok(gone.unattributed.some((entry) => entry.item === "pdf" && /no per-chart PDF check/.test(entry.text)));
		// An old label-truth.json without per-chart counts and an old cross-style.json without comparedCharts are no evidence.
		writeFileSync(join(run, "label-truth/label-truth.json"), JSON.stringify({ reports: ["waffle"], judged: [{ report: "waffle", state: "x", charts: 1 }], findings: [], problems: [] }));
		writeFileSync(join(run, "cross-style/cross-style.json"), JSON.stringify({ inputs: ["examples/every-block"], comparedKeys: 40, mismatches: [], problems: [] }));
		const old = buildEvidence({ run, registry: ["bar", "waffle"] });
		assert.equal(statusOf(old, "waffle", "label-truth"), "none");
		assert.equal(statusOf(old, "bar", "cross-style"), "none");
	} finally {
		rmSync(root, { recursive: true, force: true });
	}
});

test("type-tests read the gate's own executable log and cargo's Running lines, and an all-ignored binary shows nothing", () => {
	const { root, run } = syntheticRun();
	try {
		mkdirSync(join(run, "logs"), { recursive: true });
		const deps = "/tmp/target/debug/deps";
		const section = (name: string, result: string) => `$ ${deps}/${name}-0123456789abcdef\n\nrunning 3 tests\ntest a ... ok\n\ntest result: ${result}; 0 measured; 0 filtered out; finished in 0.01s\n\n`;
		writeFileSync(join(run, "logs/rust-tests-0.log"), [
			"$ cargo test --workspace --locked --no-fail-fast\n",
			section("bar_contract", "ok. 3 passed; 0 failed; 0 ignored"),
			section("line_contract", "FAILED. 2 passed; 1 failed; 0 ignored"),
			section("pie_contract", "ok. 0 passed; 0 failed; 3 ignored"),
			section("geometry_contracts", "ok. 9 passed; 0 failed; 0 ignored"),
		].join(""));
		writeFileSync(join(run, "logs/rust-tests-1.log"), "     Running tests/pie_contract.rs (target/debug/deps/pie_contract-0123456789abcdef)\n\nrunning 1 test\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n");
		const report = buildEvidence({ run, registry: REGISTRY, require: ["type-tests"] });
		assert.equal(statusOf(report, "bar", "type-tests"), "pass");
		assert.equal(statusOf(report, "line", "type-tests"), "fail");
		assert.deepEqual(typeRow(report, "line").items["type-tests"].failing, ["tests/line_contract.rs"]);
		assert.equal(statusOf(report, "pie", "type-tests"), "pass", "the second log's run of the binary passes it");
		const pie = typeRow(report, "pie").items["type-tests"];
		assert.deepEqual(pie.notes, ["tests/pie_contract.rs ran no test"], "the all-ignored run is disclosed, not passed");
	} finally {
		rmSync(root, { recursive: true, force: true });
	}
});

test("type-tests read each type's tests out of a crate's merged integration binary, and a failing or all-ignored type shows as such", () => {
	const { root, run } = syntheticRun();
	try {
		mkdirSync(join(run, "logs"), { recursive: true });
		const deps = "/tmp/target/debug/deps";
		writeFileSync(join(run, "logs/rust-tests-0.log"), [
			"$ cargo test --workspace --locked --no-fail-fast\n",
			`$ ${deps}/integration-0123456789abcdef\n\nrunning 8 tests\n`,
			"test bar_contract::a_bar_chart_is_valid ... ok\n",
			"test bar_contract::a_bar_has_a_summary ... ok\n",
			"test line_contract::a_line_is_valid ... ok\n",
			"test line_contract::a_line_has_a_summary ... FAILED\n",
			"test pie_contract::a_pie_is_slow ... ignored\n",
			"test geometry_contracts::everything ... ok\n",
			"test hostile::corpus ... ok\n\ntest result: FAILED. 4 passed; 1 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.10s\n\n",
			// A second merged binary, another crate's: its own run of the same type counts on its own.
			`$ ${deps}/integration-fedcba9876543210\n\nrunning 1 test\n`,
			"test pie_contract::a_pie_is_valid ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n",
		].join(""));
		const report = buildEvidence({ run, registry: REGISTRY, require: ["type-tests"] });
		assert.equal(statusOf(report, "bar", "type-tests"), "pass");
		assert.equal(statusOf(report, "line", "type-tests"), "fail");
		assert.deepEqual(typeRow(report, "line").items["type-tests"].failing, ["tests/line_contract.rs"]);
		assert.equal(statusOf(report, "pie", "type-tests"), "pass", "the other binary's run of the type passes it");
		const pie = typeRow(report, "pie").items["type-tests"];
		assert.deepEqual(pie.notes, ["tests/pie_contract.rs ran no test"], "the all-ignored run is disclosed, not passed");
	} finally {
		rmSync(root, { recursive: true, force: true });
	}
});
