// Host reason (ARC-03): protect complete case coverage when browser suites run in separate processes.
import assert from "node:assert/strict";
import test from "node:test";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { runShardedCommand, verificationShard, verifyShardCoverage } from "../lib/shards.mjs";
import { gateEnv } from "../lib/proc.mjs";
import { runFullSteps } from "../lib/full.mjs";

test("a gate cannot inherit standalone partial selection; only its dispatcher can assign a shard", t => {
	const original = process.env.RICHVIEW_VERIFY_SHARD;
	const parent = process.env.RICHVIEW_VERIFY_PARENT;
	const cases = process.env.RICHVIEW_VERIFY_CASES;
	t.after(() => {
		if (original === undefined) delete process.env.RICHVIEW_VERIFY_SHARD; else process.env.RICHVIEW_VERIFY_SHARD = original;
		if (parent === undefined) delete process.env.RICHVIEW_VERIFY_PARENT; else process.env.RICHVIEW_VERIFY_PARENT = parent;
		if (cases === undefined) delete process.env.RICHVIEW_VERIFY_CASES; else process.env.RICHVIEW_VERIFY_CASES = cases;
	});
	process.env.RICHVIEW_VERIFY_SHARD = "1/2";
	process.env.RICHVIEW_VERIFY_PARENT = "no-js";
	process.env.RICHVIEW_VERIFY_CASES = '["one"]';
	assert.equal("RICHVIEW_VERIFY_SHARD" in gateEnv(), false);
	assert.equal("RICHVIEW_VERIFY_PARENT" in gateEnv(), false);
	assert.equal("RICHVIEW_VERIFY_CASES" in gateEnv(), false);
	assert.equal(gateEnv({ RICHVIEW_VERIFY_SHARD: "0/2" }).RICHVIEW_VERIFY_SHARD, "0/2");
});

test("whole verification and strict deterministic shards retain stable case identities", t => {
	const original = process.env.RICHVIEW_VERIFY_SHARD;
	t.after(() => { if (original === undefined) delete process.env.RICHVIEW_VERIFY_SHARD; else process.env.RICHVIEW_VERIFY_SHARD = original; });
	delete process.env.RICHVIEW_VERIFY_SHARD;
	const cases = ["bar", "line", "pie", "map", "flow"];
	const options = { key: value => value };
	assert.deepEqual(verificationShard(cases, options), { cases, shard: { index: 0, count: 1, cohort: cases, selected: cases } });
	const reports = [];
	for (const [selector, expected] of [["0/3", ["bar", "map"]], ["1/3", ["line", "flow"]], ["2/3", ["pie"]]]) {
		process.env.RICHVIEW_VERIFY_SHARD = selector;
		const selected = verificationShard(cases, options);
		assert.deepEqual(selected.cases, expected);
		reports.push({ shard: selected.shard, completed: expected });
	}
	assert.deepEqual(verifyShardCoverage(reports), cases);
	for (const value of ["", "1", "-1/2", "2/2", "0/0", "01/2", "0/1.5", "0/9007199254740992", " 0/2"]) {
		process.env.RICHVIEW_VERIFY_SHARD = value;
		assert.throws(() => verificationShard(cases, options), /index\/count/, value);
	}
	process.env.RICHVIEW_VERIFY_SHARD = "0/1";
	assert.throws(() => verificationShard(["bar", "bar"], options), /unique/);
});

test("metadata alone cannot pass missing, duplicated, changed or unexecuted shard cases", () => {
	const full = () => [
		{ shard: { index: 0, count: 2, cohort: ["bar", "line", "pie"], selected: ["bar", "pie"] }, completed: ["bar", "pie"] },
		{ shard: { index: 1, count: 2, cohort: ["bar", "line", "pie"], selected: ["line"] }, completed: ["line"] },
	];
	for (const change of [
		reports => reports.pop(),
		reports => { reports[1] = reports[0]; },
		reports => { reports[1].shard.cohort[0] = "different"; },
		reports => { reports[0].shard.selected.pop(); },
		reports => { reports[0].completed.pop(); },
		reports => { reports[0].completed = ["bar", "bar"]; },
		reports => { reports[0].completed = ["bar", "line"]; },
		reports => { delete reports[0].completed; },
	]) {
		const reports = full(); change(reports);
		assert.throws(() => verifyShardCoverage(reports), /[Ss]hard/);
	}
	assert.throws(() => verifyShardCoverage([]), /No shard reports/);
	assert.throws(() => verifyShardCoverage([{ shard: { index: 0, count: 1, cohort: [], selected: [] }, completed: [] }]), /nonempty/);
});

// Separate Node processes implement the file protocol; the parent must admit their evidence,
// rather than treating a zero exit or a pre-existing canonical file as complete coverage.
const producer = String.raw`
const fs = require('node:fs'), path = require('node:path');
const [out, scenario, kind] = process.argv.slice(1);
const [index, count] = process.env.RICHVIEW_VERIFY_SHARD.split('/').map(Number);
if (process.env.RV_TEST_SHARD_CALLS) fs.appendFileSync(process.env.RV_TEST_SHARD_CALLS, index + ':' + (process.env.RV_GATE_SHARD_ATTEMPT || '1') + '\n');
if (scenario.startsWith('relaunch')) fs.appendFileSync(process.env.RICHVIEW_BROWSER_EVENT_LOG, JSON.stringify({ version: 1, event: 'browser-startup-relaunch', surface: 'playwright', engine: 'chromium', attempt: 2, maxAttempts: 2, timeoutMs: 15000 }) + '\n');
let cohort = kind === 'runtime' ? ['bar', 'line', 'pie', 'forms', 'histogram-legend'] : ['bar', 'line', 'pie'];
if (kind === 'runtime' && process.env.RICHVIEW_VERIFY_CASES) cohort.push('wide-chart', 'legend-cues', 'numeric-series-names');
const original = [...cohort];
if (process.env.RICHVIEW_VERIFY_CASES) { const wanted = JSON.parse(process.env.RICHVIEW_VERIFY_CASES); cohort = cohort.filter(key => wanted.includes(key)); }
if (scenario === 'partial-inventory') cohort = cohort.filter(key => key !== 'line');
const selected = cohort.filter((_, position) => position % count === index);
if (scenario === 'missing' && index === 1) process.exit(0);
const completed = [...selected];
if (scenario === 'skipped' && index === 0) completed.pop();
const results = completed.map(key => ({ caseKey: key, type: key, index: cohort.indexOf(key), ok: !(scenario === 'failed' && key === 'bar') }));
const report = { browser: 'fixture', style: 'sleek', width: 1440, model_count: 3, ...(process.env.RICHVIEW_VERIFY_CASES ? {case_scope:{cohort:original,selected:cohort}} : {}), errors: [], results, completed, shard: { index, count, cohort, selected } };
if (scenario === 'wrong-scope' && report.case_scope) report.case_scope.cohort.pop();
if (kind === 'runtime') {
  report.engine = 'chromium'; delete report.style; delete report.width; delete report.model_count;
  for (const result of results) result.views = [1280, 390].map(width => ({ name: result.caseKey, width, charts: 1 }));
  if (scenario === 'missing-width' && index === 0) results[0].views.pop();
  if (scenario === 'wrong-name' && index === 0) results[0].views[0].name = 'different';
  report.cases = results.flatMap(result => result.views);
  if (scenario === 'case-mismatch' && index === 0) report.cases.pop();
  report.failures = results.some(result => !result.ok) ? ['assertion failed'] : [];
}
if (scenario === 'no-result' && index === 0) report.results.pop();
if (scenario === 'duplicate' && index === 1) report.shard.index = 0;
if (scenario === 'different' && index === 1) report.shard.cohort[0] = 'different';
if (scenario === 'viewport' && index === 1) report.width = 390;
fs.writeFileSync(path.join(out, (kind === 'runtime' ? 'report-runtime' : 'types') + '-' + index + '-of-' + count + '.json'), JSON.stringify(report));
if (results.some(result => !result.ok)) process.exitCode = 1;
if (scenario === 'relaunch-failed' && index === 0) process.exitCode = 1;
if (scenario === 'flaky-once' && index === 1 && process.env.RV_GATE_SHARD_ATTEMPT === '1') process.exitCode = 1;
`;

test("real child shards produce complete canonical evidence, and omissions never reuse stale passes", async t => {
	const dir = mkdtempSync(join(tmpdir(), "rv-shard-command-"));
	t.after(() => rmSync(dir, { recursive: true, force: true }));
	const canonical = join(dir, "types.json");
	const run = scenario => runShardedCommand(process.execPath, ["-e", producer, dir, scenario], {
		env: gateEnv({RV_GATE_PARTIAL_RETRY:"0", RV_GATE_PRELAUNCH_CHROMIUM_ONLY:"0"}), logPath: join(dir, `${scenario}.log`), parallel: 2, scheduleCommand: run => run(), shard: { kind: "interaction", out: dir },
	});
	const good = await run("good");
	assert.equal(good.status, "pass");
	const report = JSON.parse(readFileSync(canonical, "utf8"));
	assert.deepEqual(report.results.map(result => result.type), ["bar", "line", "pie"]);
	assert.deepEqual(report.completed, ["bar", "line", "pie"]);
	assert.equal(report.cohort_sha256, createHash("sha256").update('["bar","line","pie"]').digest("hex"));
	assert.equal(report.charts, 3); assert.equal(report.failed, 0);
	for (const scenario of ["missing", "skipped", "no-result", "duplicate", "different", "viewport"]) {
		writeFileSync(canonical, '{"stale":"pass"}');
		const result = await run(scenario);
		assert.equal(result.status, "fail", scenario);
		assert.equal(existsSync(canonical), false, `${scenario} left a stale canonical pass`);
	}
	const failed = await run("failed");
	assert.equal(failed.status, "fail", "a complete cohort retains its assertion failure");
	assert.equal(JSON.parse(readFileSync(canonical, "utf8")).failed, 1);
	for (const scenario of ["relaunch", "relaunch-failed"]) {
		const outcome = await run(scenario);
		assert.equal(outcome.status, scenario === "relaunch" ? "pass" : "fail");
		assert.equal(outcome.browserRelaunches.length, 2, "Both shard relaunches reach the parent even on failure");
		assert.equal(new Set(outcome.browserRelaunches.map(event => event.journal)).size, 2);
	}
	let calls = 0, peer;
	try {
		const stopped = await runShardedCommand(process.execPath, ["-e", producer, dir, "good"], {
			env: gateEnv({RV_GATE_PARTIAL_RETRY:"0", RV_GATE_PRELAUNCH_CHROMIUM_ONLY:"0"}), logPath: join(dir, "rejected.log"), parallel: 2, shard: { kind: "interaction", out: dir },
			scheduleCommand: run => ++calls === 1 ? Promise.reject(new Error("dispatcher aborted")) : (peer = run()),
		});
		assert.equal(stopped.status, "fail");
		assert.equal(existsSync(join(dir, "types-1-of-2.json")), true, "a rejected dispatch must still await its running peer");
	} finally { await peer; }
});

test("runtime shard evidence requires every report's desktop and phone assertions", async t => {
	const dir = mkdtempSync(join(tmpdir(), "rv-runtime-shards-"));
	t.after(() => rmSync(dir, { recursive: true, force: true }));
	for (const name of ["bar", "line", "pie"]) { const folder = join(dir, "examples", name); mkdirSync(folder, { recursive: true }); writeFileSync(join(folder, "report.rmd"), "# Report"); }
	const canonical = join(dir, "report-runtime.json");
	const run = scenario => runShardedCommand(process.execPath, ["-e", producer, dir, scenario, "runtime"], {
		cwd: dir, env: gateEnv(), logPath: join(dir, `${scenario}.log`), parallel: 2, scheduleCommand: run => run(), shard: { kind: "runtime", out: dir },
	});
	assert.equal((await run("good")).status, "pass");
	const report = JSON.parse(readFileSync(canonical, "utf8"));
	assert.deepEqual(report.cases.map(view => `${view.name}@${view.width}`), ["bar@1280", "bar@390", "forms@1280", "forms@390", "histogram-legend@1280", "histogram-legend@390", "line@1280", "line@390", "pie@1280", "pie@390"]);
	for (const scenario of ["missing", "skipped", "no-result", "missing-width", "wrong-name", "case-mismatch", "partial-inventory"]) {
		writeFileSync(canonical, '{"stale":"pass"}');
		assert.equal((await run(scenario)).status, "fail", scenario);
		assert.equal(existsSync(canonical), false, scenario);
	}
	assert.equal((await run("failed")).status, "fail");
	assert.deepEqual(JSON.parse(readFileSync(canonical, "utf8")).failures, ["assertion failed"]);
});

test("a fixed five-leaf cohort keeps all leaves while sharing one or two command permits", async t => {
	const dir = mkdtempSync(join(tmpdir(), "rv-fixed-shard-count-"));
	t.after(() => rmSync(dir, { recursive: true, force: true }));
	for (const parallel of [1, 2]) {
		const out = join(dir, String(parallel)); mkdirSync(out);
		let active = 0, peak = 0, started = 0;
		const results = await runFullSteps([{ id: "build" }, { id: "fixed", run: [{ cmd: "node" }] }], async (step, budget) => {
			if (step.id === "build") return { status: "pass" };
			return runShardedCommand(process.execPath, ["-e", producer, out, "good"], {
				env: gateEnv(), logPath: join(out, "run.log"), parallel, shard: { kind: "interaction", count: 5, out },
				scheduleCommand: work => budget(async () => { started++; active++; peak = Math.max(peak, active); try { return await work(); } finally { active--; } }),
			});
		}, parallel);
		assert.equal(results[1].status, "pass"); assert.equal(started, 5); assert.equal(peak, parallel);
		for (let i = 0; i < 5; i++) assert.ok(existsSync(join(out, `types-${i}-of-5.json`)));
		assert.deepEqual(JSON.parse(readFileSync(join(out, "types.json"))).completed, ["bar", "line", "pie"]);
	}
	for (const count of [0, 1, -1, 1.5, Infinity]) {
		const canonical = join(dir, "types.json"); writeFileSync(canonical, '{"stale":"pass"}');
		const result = await runShardedCommand(process.execPath, [], { env: gateEnv(), logPath: join(dir, "invalid.log"), parallel: 2, scheduleCommand: () => assert.fail("invalid count started a child"), shard: { kind: "interaction", count, out: dir } });
		assert.equal(result.status, "fail"); assert.equal(existsSync(canonical), false);
	}
});


test("prelaunch retries only the failed browser shard and retains both outcomes", async t => {
	const dir = mkdtempSync(join(tmpdir(), "rv-shard-retry-"));
	t.after(() => rmSync(dir, {recursive: true, force: true}));
	const calls = join(dir, "calls.log");
	const outcome = await runShardedCommand(process.execPath, ["-e", producer, dir, "flaky-once"], {
		env: gateEnv({RV_GATE_PARTIAL_RETRY: "1", RV_TEST_SHARD_CALLS: calls}), logPath: join(dir, "run.log"), parallel: 2,
		scheduleCommand: run => run(), shard: {kind: "interaction", out: dir},
	});
	assert.equal(outcome.status, "pass");
	assert.deepEqual(readFileSync(calls, "utf8").trim().split("\n").sort(), ["0:1", "1:1", "1:2"]);
	assert.deepEqual(outcome.shards[1].attempts.map(attempt => attempt.status), ["fail", "pass"]);
});


test("filtered chart shards record exactly the requested cases and cannot publish complete evidence", async t => {
	const dir=mkdtempSync(join(tmpdir(),"rv-case-shard-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
	const result=await runShardedCommand(process.execPath,["-e",producer,dir,"good"],{
		env:gateEnv({RV_GATE_PARTIAL_RETRY:"1"}),logPath:join(dir,"run.log"),parallel:2,scheduleCommand:run=>run(),shard:{kind:"interaction",out:dir,cases:["bar","pie"]},
	});
	assert.equal(result.status,"pass",JSON.stringify(result));
	assert.deepEqual(result.cases.selected,["bar","pie"]);
	assert.deepEqual(result.cases.completed,["bar","pie"]);
	assert.deepEqual(result.cases.cohort,["bar","line","pie"]);
	assert.equal(existsSync(join(dir,"types.json")),false);
	assert.equal(existsSync(join(dir,"types.partial.json")),true);
});


test("filtered shards reject unknown cases instead of silently passing a smaller cohort", async t => {
	const dir=mkdtempSync(join(tmpdir(),"rv-case-unknown-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
	const result=await runShardedCommand(process.execPath,["-e",producer,dir,"good"],{
		env:gateEnv({RV_GATE_PARTIAL_RETRY:"1"}),logPath:join(dir,"run.log"),parallel:2,scheduleCommand:run=>run(),shard:{kind:"interaction",out:dir,cases:["bar","missing"]},
	});
	assert.equal(result.status,"fail");assert.equal(existsSync(join(dir,"types.json")),false);
});

test("filtered runtime shards prove selected reports and both widths without publishing a full report", async t => {
	mkdirSync(join(process.cwd(),"tmp"),{recursive:true});const dir=mkdtempSync(join(process.cwd(),"tmp/runtime-shards-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
	for(const name of ["bar","line","pie"]) {const folder=join(dir,"examples",name);mkdirSync(folder,{recursive:true});writeFileSync(join(folder,"report.rmd"),"# report");}
	const run=(scenario,cases=["bar","wide-chart"])=>runShardedCommand(process.execPath,["-e",producer,dir,scenario,"runtime"],{
		cwd:dir,env:gateEnv({RV_GATE_PARTIAL_RETRY:"1"}),logPath:join(dir,`${scenario}.log`),parallel:2,scheduleCommand:work=>work(),shard:{kind:"runtime",out:dir,cases},
	});
	const result=await run("good");assert.equal(result.status,"pass",JSON.stringify(result));
	assert.deepEqual(result.cases.cohort,["bar","line","pie","forms","histogram-legend","wide-chart","legend-cues","numeric-series-names"]);
	assert.deepEqual(result.cases.selected,["bar","wide-chart"]);assert.deepEqual(result.cases.completed,["bar","wide-chart"]);
	const partial=JSON.parse(readFileSync(join(dir,"report-runtime.partial.json"),"utf8"));
	assert.deepEqual(partial.cases.map(view=>`${view.name}@${view.width}`),["bar@1280","bar@390","wide-chart@1280","wide-chart@390"]);
	assert.equal(existsSync(join(dir,"report-runtime.json")),false);
	for(const scenario of ["missing-width","wrong-name","wrong-scope"]) {assert.equal((await run(scenario)).status,"fail",scenario);assert.equal(existsSync(join(dir,"report-runtime.partial.json")),false);}
	assert.equal((await run("good",["unknown"])).status,"fail");
});
