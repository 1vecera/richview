// Host reason (ARC-03): real child artifacts must prove full no-JS coverage under explicit admission.
import assert from "node:assert/strict";
import test from "node:test";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { gateEnv } from "../lib/proc.mts";
import { reportCohort } from "../lib/report-inputs.mts";
import { runShardedCommand } from "../lib/shards.mts";

const producer = `
const fs=require('node:fs'),path=require('node:path');
const [out,scenario]=process.argv.slice(1),[index,count]=process.env.RICHVIEW_VERIFY_SHARD.split('/').map(Number);
if(process.env.RICHVIEW_VERIFY_PARENT!=='no-js')throw Error('No parent admission');
if(scenario==='missing-file'&&index===1)process.exit(0);
const cohort=scenario==='partial-inventory'?['sales-report','forms']:['alpha','sales-report','forms'];
const selected=cohort.filter((_,p)=>p%count===index),widths=[1280,390],modes=['live','no-js','no-webgl'];
const report={engine:'chromium',admission:'no-js',shard:{index,count,cohort,selected},completed:[...selected],failures:[],cases:selected.flatMap(name=>widths.map(width=>({name,width,figures:1}))),views:selected.flatMap(name=>widths.flatMap(width=>modes.map(mode=>({name,width,mode}))))};
if(selected.includes('sales-report'))report.recovery={missing:'The report runtime did not start',present:'passed'};
if(index===0){
 if(scenario==='missing-width')report.cases.pop();
 if(scenario==='missing-mode')report.views.pop();
 if(scenario==='duplicate-mode')report.views[1]=report.views[0];
 if(scenario==='wrong-width')report.views[0].width='1280';
 if(scenario==='no-completion')report.completed.pop();
 if(scenario==='foreign-drill')report.recovery={missing:'The report runtime did not start',present:'passed'};
 if(scenario==='no-admission')report.admission=null;
 if(scenario==='engine')report.engine='firefox';
 if(scenario==='failed'){report.failures.push('alpha: original assertion failed');process.exitCode=1;}
}
if(scenario==='missing-drill')delete report.recovery;
if(scenario==='drill-failed'&&report.recovery)report.recovery.present='The report runtime did not start';
fs.writeFileSync(path.join(out,'no-js-'+index+'-of-'+count+'.json'),JSON.stringify(report));
`;

test("no-JS admission requires every actual report mode, width and single Sales drill", async t => {
	const cwd = mkdtempSync(join(tmpdir(), "rv-no-js-shards-")), out = join(cwd, "out");
	t.after(() => rmSync(cwd, { recursive: true, force: true })); mkdirSync(out);
	for (const name of ["sales-report", "alpha", "sql-report", "metricflow-report", "not-a-report"]) {
		const dir = join(cwd, "examples", name); mkdirSync(dir, { recursive: true });
		if (name !== "not-a-report") writeFileSync(join(dir, "report.rmd"), "# Input\n");
	}
	assert.deepEqual(reportCohort(cwd), ["alpha", "sales-report", "forms"]);
	assert.deepEqual(reportCohort(cwd, { only: ["sales-report"], extra: ["custom"] }), ["sales-report", "custom"]);
	const run = (scenario: string) => runShardedCommand(process.execPath, ["-e", producer, out, scenario], { cwd, env: gateEnv(), logPath: join(cwd, scenario + ".log"), parallel: 2, scheduleCommand: <T,>(work: () => T) => work(), shard: { kind: "no-js", out } });
	const canonical = join(out, "no-js.json");
	assert.equal((await run("good")).status, "pass");
	const report = JSON.parse(readFileSync(canonical, "utf8"));
	assert.deepEqual(report.completed, ["alpha", "sales-report", "forms"]);
	assert.deepEqual(report.cases.map((item: {name: string; width: number}) => `${item.name}@${item.width}`), ["alpha@1280", "sales-report@1280", "forms@1280", "alpha@390", "sales-report@390", "forms@390"]);
	assert.equal(report.views.length, 18); assert.deepEqual(report.failures, []);
	for (const scenario of ["partial-inventory", "missing-width", "missing-mode", "duplicate-mode", "wrong-width", "no-completion", "foreign-drill", "missing-drill", "drill-failed", "no-admission", "engine", "missing-file"]) {
		writeFileSync(canonical, '{"stale":"pass"}');
		assert.equal((await run(scenario)).status, "fail", scenario);
		assert.equal(existsSync(canonical), false, scenario + " retained stale complete evidence");
	}
	assert.equal((await run("failed")).status, "fail");
	assert.deepEqual(JSON.parse(readFileSync(canonical, "utf8")).failures, ["alpha: original assertion failed"], "complete evidence must preserve the original assertion failure");
});
