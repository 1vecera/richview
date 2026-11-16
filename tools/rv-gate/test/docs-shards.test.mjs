// Host reason (ARC-03): prove complete docs admission and canonical evidence through real children.
import assert from "node:assert/strict";
import test from "node:test";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { docsInventory, mergeDocsPhases, verifyDocsPhases } from "../lib/docs-shards.mjs";
import { runShardedCommand, verifyShardCoverage } from "../lib/shards.mjs";
import { gateEnv } from "../lib/proc.mjs";
import { runFullSteps } from "../lib/full.mjs";

function fixture(t) {
	const root = mkdtempSync(join(tmpdir(), "rv-docs-phases-"));
	t.after(() => rmSync(root, { recursive: true, force: true }));
	mkdirSync(join(root, "docs/public/charts"), { recursive: true });
	for (const name of ["index.md", "authoring.md", "charts/bar.md", "charts/line.md", "charts/unaccepted.md"])
		writeFileSync(join(root, "docs/public", name), "# Docs");
	writeFileSync(join(root, "docs/public/charts/accepted.txt"), "# Accepted charts\nline\nbar\n");
	const phases = ["native", "main", "desktop", "phone", "no-js"];
	const statics = ["index", "authoring"], types = ["bar", "line"];
	const navigation = [1440, 1024, 768, 390, 320].flatMap(width => ["index", "authoring", "charts", "reference", "charts/bar", "charts/line"].map(name => `${width}:${name}`));
	const observations = {
		native: ["editor-commands", "init", "downloadable-examples", "authoring-snippet", "learn", "report-export", "csv-data", "json-data", "failure-recovery", "style-exports", "mcp", "live-preview", "publication"].map(name => `native/${name}`),
		main: [...navigation.map(key => `main/navigation/${key.replace(":", "/")}`), "main/internal-links", "main/playground", "main/style-contract"],
		desktop: ["desktop/bar", "desktop/line"], phone: ["phone/bar", "phone/line"], "no-js": ["no-js/index", "no-js/authoring"],
	};
	const reports = phases.map((phase, index) => {
		const width = phase === "desktop" ? 1440 : phase === "phone" ? 390 : null;
		const chart_pages = width ? types.map(type => ({ width, type, ok: true, original_assertion_result: `${width}:${type}`, downloads: { csv: 12, png: 24 } })) : [];
		return {
			ok: true, admission: "docs", engine: "chromium", browser_version: "actual-fixture-version",
			shard: { index, count: 5, cohort: phases, selected: [phase] }, completed: [phase], results: [{ caseKey: phase, ok: true }],
			observations: observations[phase].map(caseKey => ({ caseKey, ok: true })), errors: [],
			navigation_cohort: navigation, navigation_views: phase === "main" ? navigation : [],
			static_cohort: statics, static_views: phase === "no-js" ? statics : [],
			chart_cohort: [1440, 390].flatMap(width => types.map(type => `${width}:${type}`)),
			chart_pages, completed_views: chart_pages.map(row => `${row.width}:${row.type}`),
			cli: phase === "native" ? { generate: { output: "/private/actual/report.html" } } : {},
			mcp: phase === "native" ? { render_validate_check_learn: true } : {},
			publication: phase === "native" ? { retained_source: true } : {},
			failures: phase === "native" ? { missing_resource_detected: true, repaired_and_regenerated: true } : {},
			browser: phase === "main" ? { original_main_result: true } : phase === "no-js" ? { static_syntax_highlighting: true } : {},
		};
	});
	return { root, reports, expected: { types, statics, navigation, observations } };
}

test("docs admission requires every source-derived journey and keeps original assertion payloads", t => {
	const { root, reports, expected } = fixture(t);
	assert.deepEqual(docsInventory(root), expected);
	const merge = children => mergeDocsPhases(children, { cohort: verifyShardCoverage(children), inventory: docsInventory(root), engine: "chromium" });
	const canonical = merge(reports);
	assert.deepEqual(canonical.cli, reports[0].cli);
	assert.deepEqual(canonical.mcp, reports[0].mcp);
	assert.deepEqual(canonical.publication, reports[0].publication);
	assert.deepEqual(canonical.failures, reports[0].failures);
	assert.deepEqual(canonical.browser, { original_main_result: true, static_syntax_highlighting: true });
	assert.deepEqual(canonical.chart_pages, [...reports[2].chart_pages, ...reports[3].chart_pages], "Keep actual values, output paths and download timings unchanged");
	for (const [name, change] of [
		["missing-phase", rows => rows.pop()],
		["duplicate-phase", rows => { rows[4] = rows[0]; }],
		["missing-completion", rows => rows[2].completed.pop()],
		["admission", rows => { rows[0].admission = null; }],
		["failed-phase", rows => { rows[0].ok = false; }],
		["engine", rows => { rows[0].engine = "webkit"; }],
		["missing-version", rows => { delete rows[0].browser_version; }],
		["mixed-version", rows => { rows[1].browser_version = "other"; }],
		["failed-result", rows => { rows[1].results[0].ok = false; }],
		["browser-error", rows => rows[1].errors.push("actual failure")],
		["missing-journey", rows => rows[0].observations.pop()],
		["duplicate-journey", rows => rows[0].observations.push(rows[0].observations[0])],
		["missing-downloads", rows => { rows[0].observations = rows[0].observations.filter(row => row.caseKey !== "native/downloadable-examples"); }],
		["duplicate-downloads", rows => rows[0].observations.push({ caseKey: "native/downloadable-examples", ok: true })],
		["failed-journey", rows => { rows[0].observations[0].ok = false; }],
		["route", rows => rows[1].navigation_views.pop()],
		["static-guide", rows => rows[4].static_views.pop()],
		["chart-result", rows => rows[2].chart_pages.pop()],
		["failed-chart", rows => { rows[2].chart_pages[0].ok = false; }],
		["chart-completion", rows => rows[2].completed_views.pop()],
		["native-payload", rows => { delete rows[0].publication; }],
		["smaller-inventory", rows => rows.forEach(row => row.chart_cohort.pop())],
	]) {
		const rows = structuredClone(reports); change(rows);
		assert.throws(() => merge(rows), undefined, name);
	}
	writeFileSync(join(root, "docs/public/new-guide.md"), "# Newly required guide");
	assert.throws(() => merge(reports), /journey|inventory/i, "Children cannot omit a newly required source page");
});

const replay = `
const assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path');
const [out, scenario] = process.argv.slice(1);
assert.equal(process.env.RICHVIEW_VERIFY_PARENT, scenario === 'partial' ? 'affected-docs' : 'docs');
const [index, count] = process.env.RICHVIEW_VERIFY_SHARD.split('/').map(Number);
assert.equal(count, 5);
if (scenario === 'missing' && index === 2) process.exit(0);
const row = JSON.parse(fs.readFileSync(path.join(out, 'fixture.json')))[index];
if (scenario === 'partial') {const wanted=JSON.parse(process.env.RICHVIEW_VERIFY_CASES);row.case_scope={cohort:['native','main','desktop','phone','no-js'],selected:['native','main','desktop','phone','no-js'].filter(key=>wanted.includes(key))};fs.appendFileSync(path.join(out,'started.log'),String(index)+'\\n');}
if (scenario === 'incomplete' && index === 1) row.navigation_views.pop();
fs.writeFileSync(path.join(out, 'docs-' + index + '-of-' + count + '.json'), JSON.stringify(row));
if (scenario === 'exit' && index === 4) process.exitCode = 1;
`;

test("five real docs children share the global budget and incomplete runs remove both stale canonicals", async t => {
	const { root, reports } = fixture(t);
	writeFileSync(join(root, "fixture.json"), JSON.stringify(reports));
	const docs = join(root, "docs.json"), charts = join(root, "chart-pages.json");
	let active = 0, peak = 0, started = 0;
	const run = async scenario => {
		const result = await runFullSteps([{ id: "build" }, { id: "docs", run: [{ cmd: "node" }] }], async (step, budget) => {
			if (step.id === "build") return { status: "pass" };
			return runShardedCommand(process.execPath, ["-e", replay, root, scenario], {
				cwd: root, env: gateEnv(), logPath: join(root, `${scenario}.log`), parallel: 2, shard: { kind: "docs", count: 5, out: root },
				scheduleCommand: work => budget(async () => { started++; active++; peak = Math.max(peak, active); try { return await work(); } finally { active--; } }),
			});
		}, 2);
		return result[1];
	};
	assert.equal((await run("good")).status, "pass");
	assert.equal(started, 5); assert.equal(peak, 2); assert.equal(active, 0);
	const merged = JSON.parse(readFileSync(docs));
	assert.deepEqual(merged.chart_pages, JSON.parse(readFileSync(charts)));
	assert.deepEqual(merged.cli, reports[0].cli);
	assert.deepEqual(merged.chart_pages, [...reports[2].chart_pages, ...reports[3].chart_pages]);
	for (const scenario of ["missing", "incomplete", "exit"]) {
		writeFileSync(docs, '{"stale":"pass"}'); writeFileSync(charts, '["stale"]');
		assert.equal((await run(scenario)).status, "fail", scenario);
		assert.equal(existsSync(docs), false, scenario);
		assert.equal(existsSync(charts), false, scenario);
	}
	assert.ok(existsSync(join(root, "docs-4-of-5.json")), "Failed child evidence remains available");
	const invalid = await runShardedCommand(process.execPath, [], { env: gateEnv(), logPath: join(root, "invalid.log"), parallel: 2, shard: { kind: "docs", count: 2, out: root }, scheduleCommand: () => assert.fail("Wrong docs phase count started a child") });
	assert.equal(invalid.status, "fail");
});

test("partial docs admission verifies only requested phases against the complete source inventory", t => {
	const {root,reports}=fixture(t),selected=["desktop","phone"],rows=reports.filter(row=>selected.includes(row.shard.selected[0]));
	const result=verifyDocsPhases(rows,{cohort:["native","main","desktop","phone","no-js"],selected,inventory:docsInventory(root),engine:"chromium"});
	assert.deepEqual([...result.keys()],selected);
	for(const mutate of [values=>values.pop(),values=>values[0].observations.pop(),values=>values[1].shard.index=2]){
		const bad=structuredClone(rows);mutate(bad);assert.throws(()=>verifyDocsPhases(bad,{cohort:["native","main","desktop","phone","no-js"],selected,inventory:docsInventory(root),engine:"chromium"}));
	}
});


test("the affected docs dispatcher starts only named phases and writes partial evidence", async t => {
	const {root,reports}=fixture(t);writeFileSync(join(root,"fixture.json"),JSON.stringify(reports));
	const result=await runShardedCommand(process.execPath,["-e",replay,root,"partial"],{
		cwd:root,env:gateEnv({RV_GATE_PARTIAL_RETRY:"1"}),logPath:join(root,"partial.log"),parallel:2,
		shard:{kind:"docs",count:5,out:root,cases:["desktop","phone"]},scheduleCommand:run=>run(),
	});
	assert.equal(result.status,"pass",JSON.stringify(result));
	assert.deepEqual(readFileSync(join(root,"started.log"),"utf8").trim().split("\n").sort(),["2","3"]);
	assert.deepEqual(result.cases.selected,["desktop","phone"]);assert.deepEqual(result.cases.completed,["desktop","phone"]);
	assert.equal(existsSync(join(root,"docs.json")),false);assert.equal(existsSync(join(root,"chart-pages.json")),false);assert.equal(existsSync(join(root,"docs.partial.json")),true);
});

test("unfiltered docs merge retains the original canonical JSON bytes", t => {
	const {root,reports}=fixture(t);
	const expected={cli:reports[0].cli,mcp:reports[0].mcp,publication:reports[0].publication,browser:{...reports[1].browser,...reports[4].browser},chart_pages:[...reports[2].chart_pages,...reports[3].chart_pages],failures:reports[0].failures};
	const actual=mergeDocsPhases(reports,{cohort:verifyShardCoverage(reports),inventory:docsInventory(root),engine:"chromium"});
	assert.equal(JSON.stringify(actual),JSON.stringify(expected));
});
