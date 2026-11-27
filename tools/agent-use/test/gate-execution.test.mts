// Host reason (ARC-03): exercise six actual runner subprocesses; no remote agent or credentials are used.
import assert from "node:assert/strict";
import { execFileSync, spawn } from "node:child_process";
import { once } from "node:events";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, watch, writeFileSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";
import type { TestContext } from "node:test";
import type { ChildProcess } from "node:child_process";
import type { RunRecord } from "../lib/gate.mts";
import type { HistoryRow } from "../history.mts";
import { taskPlan } from "../lib/gate.mts";

const root = process.cwd(), candidate = join(root, "tools/agent-use/gate.mjs");
type TraceRow = {event: string; index: number; pid: number; descendant?: number; product: string; task: string; argv: string[]; inputHash: string};
type FreshAgent = {at?: string; products: {details: RunRecord[]}[]};
const readFresh = (out: string): FreshAgent => JSON.parse(readFileSync(join(out, "fresh-agent.json"), "utf8"));
const jsonl = <Row = TraceRow,>(file: string): Row[] => existsSync(file) ? readFileSync(file, "utf8").split("\n").filter(Boolean).map(line => JSON.parse(line) as Row) : [];
const fakeRun = String.raw`
import fs from 'node:fs';import path from 'node:path';import{createHash}from'node:crypto';import{fileURLToPath}from'node:url';import {appendHistory,historyEntry}from './history.mts';
const argv=process.argv.slice(2),arg=name=>{const i=argv.indexOf(name);return i<0?null:argv[i+1]},product=arg('--product'),task=arg('--task'),binary=arg('--richview'),model=arg('--model');
const cfg=JSON.parse(fs.readFileSync(process.env.PROTOCOL_CONFIG)),index=cfg.plan.findIndex(row=>row.product===product&&row.task===task),trace=process.env.PROTOCOL_TRACE;
const input=createHash('sha256');const visit=dir=>{for(const name of fs.readdirSync(dir).sort()){const file=path.join(dir,name);if(fs.statSync(file).isDirectory())visit(file);else{input.update(name+'\0');input.update(fs.readFileSync(file));}}};visit(fileURLToPath(new URL('./tasks/'+task,import.meta.url)));const inputHash=input.digest('hex');
const log=row=>fs.appendFileSync(trace,JSON.stringify({...row,index,pid:process.pid,product,task})+'\n');
let descendant;
if(cfg.abort){const{spawn}=await import('node:child_process');descendant=spawn('/bin/sleep',['300'],{stdio:'ignore'});process.on('SIGTERM',()=>{});}
log({event:'start',argv,inputHash,descendant:descendant?.pid});
if(cfg.release){const released=path.join(cfg.release,String(index));if(!fs.existsSync(released))await new Promise(resolve=>{const w=fs.watch(cfg.release,()=>{if(fs.existsSync(released)){w.close();resolve();}});if(fs.existsSync(released)){w.close();resolve();}});}
const runId=product+'-'+task,runDir=path.join(cfg.runs,runId);fs.mkdirSync(runDir,{recursive:true});
const meta={runId,product,task,binary,model,exitCode:0,timedOut:cfg.mode==='timed'&&index===0,startedAt:new Date(Date.UTC(2026,9,1,0,index)).toISOString(),finishedAt:new Date(Date.UTC(2026,9,1,0,index,20)).toISOString(),runDir,build:{commit:'fixture',version:'fixture'}};
const scored={score:80+index,pass:!(cfg.mode==='quality'&&product==='claude'),clean:{final:true,attempts:1+index,firstAttempt:1},toolCalls:4+index,toolErrors:0,diagnosticsMet:{'chart.failed':{level:'error',message:'fixture diagnostic'}},misread:product==='claude'?[{code:'chart.failed',level:'error'}]:[],ignored:[],unresolved:[],invisibleDiagnostics:[],points:[],facts:{passed:1,total:1,items:[{id:'f',ok:true,detail:task}]}};
fs.writeFileSync(path.join(runDir,'run.json'),JSON.stringify(meta));
fs.writeFileSync(path.join(runDir,'transcript.jsonl'),product==='claude'?fs.readFileSync(new URL('./test/fixtures/misread.jsonl',import.meta.url)):'{"type":"fixture"}\n');
if(cfg.mode==='missing'&&index===0){console.log(JSON.stringify({runDir}));log({event:'end'});process.exit(0);}
if(cfg.mode==='nonzero'&&index===0){console.log(JSON.stringify({runDir}));log({event:'end'});process.exit(7);}
if(cfg.mode==='malformed'&&index===0){fs.writeFileSync(path.join(runDir,'score.json'),'{');console.log(JSON.stringify({runDir}));log({event:'end'});process.exit(0);}
if(cfg.mode==='metadata'&&index===0){fs.writeFileSync(path.join(runDir,'run.json'),JSON.stringify({...meta,task:'wrong-task'}));}
fs.writeFileSync(path.join(runDir,'score.json'),JSON.stringify(scored));
if(!argv.includes('--no-history'))appendHistory(historyEntry(meta,scored));
if(!(cfg.mode==='output'&&index===0))console.log(JSON.stringify({runDir}));log({event:'end'});
`;

function fixture(t: TestContext, { mode = "normal", hold = false, abort = false, runs = 3 } = {}) {
	mkdirSync(join(root, "tmp"), { recursive: true });
	const dir = mkdtempSync(join(root, "tmp/fresh-plan-")), tool = join(dir, "agent-use"), out = join(dir, "out"), history = join(dir, "history.jsonl"), trace = join(dir, "trace.jsonl");
	cpSync(join(root, "tools/agent-use"), tool, { recursive: true, filter: file => !file.includes("/node_modules/") });
	writeFileSync(join(tool, "gate.mjs"), readFileSync(candidate)); writeFileSync(join(tool, "run.mjs"), fakeRun);
	const tasks = readdirSync(join(tool, "tasks"), { withFileTypes: true }).filter(row => row.isDirectory() && existsSync(join(tool, "tasks", row.name, "task.json"))).map(row => row.name).sort();
	const seed = { at: "2026-09-01T00:00:00Z", runId: "prior", product: "claude", task: tasks[0], model: "sonnet", score: 90, pass: true, build: { commit: "prior" }, firstCleanAttempt: 1, toolCalls: 1, facts: { passed: 1, total: 1, failed: [] }, misread: [] };
	writeFileSync(history, JSON.stringify(seed) + "\n");
	const plan = taskPlan(tasks, ["claude", "codex"], runs, 1), release = hold ? join(dir, "release") : null;
	if (release) mkdirSync(release);
	const config = join(dir, "config.json"); writeFileSync(config, JSON.stringify({ mode, abort, plan, runs: join(dir, "runs"), release }));
	const binary = join(dir, "richview"); writeFileSync(binary, '#!/bin/sh\nprintf "fixture version\\n"\n', { mode: 0o755 });
	const children: ChildProcess[] = [];
	t.after(async () => {
		for (const child of children) if (child.exitCode === null && child.signalCode === null) { const end = once(child, "close"); child.kill("SIGTERM"); await end; }
		for (const row of jsonl(trace)) if (row.event === "start") for (const pid of [row.pid, row.descendant] .filter(pid => pid !== undefined)) { try { process.kill(pid, "SIGKILL"); } catch (error) { if ((error as NodeJS.ErrnoException).code !== "ESRCH") throw error; } }
		rmSync(dir, { recursive: true, force: true });
	});
	const start = (jobs: number | string | undefined) => {
		const child = spawn(process.execPath, [join(tool, "gate.mjs"), "--richview", binary, "--out", out, "--runs", String(runs), "--timeout-min", "2", ...(jobs === undefined ? [] : ["--jobs", String(jobs)])], { env: { ...process.env, RV_AGENT_USE_HISTORY: history, RICHVIEW_VERIFY_WORKERS: "99", PROTOCOL_CONFIG: config, PROTOCOL_TRACE: trace }, stdio: ["ignore", "pipe", "pipe"] });
		children.push(child); let stdout = "", stderr = ""; child.stdout.on("data", b => { stdout += b; }); child.stderr.on("data", b => { stderr += b; });
		const ended = new Promise<{code: number | null; stdout: string; stderr: string}>((resolve, reject) => { child.once("error", reject); child.once("close", code => resolve({ code, stdout, stderr })); });
		return { child, ended };
	};
	const until = (check: () => boolean) => new Promise<void>((resolve, reject) => {
		if (check()) { resolve(); return; }
		const watcher = watch(dir, () => { try { if (check()) { watcher.close(); resolve(); } } catch (error) { watcher.close(); reject(error); } });
		t.after(() => watcher.close()); if (check()) { watcher.close(); resolve(); }
	});
	return { dir, binary, plan, history, trace, out, start, until, release: (index: number) => writeFileSync(join(release!, String(index)), "go") };
}

/** Resolves once none of the pids is alive, then lets the gate's close handlers run. */
async function exited(pids: number[]) {
	const alive = (pid: number) => { try { process.kill(pid, 0); return true; } catch (error) { if ((error as NodeJS.ErrnoException).code === "ESRCH") return false; throw error; } };
	const deadline = Date.now() + 10_000;
	while (pids.some(alive)) {
		if (Date.now() > deadline) throw new Error(`Runners did not exit: ${pids.filter(alive).join(", ")}`);
		await new Promise(resolve => setTimeout(resolve, 10));
	}
	await new Promise(resolve => setTimeout(resolve, 100));
}

const normalize = (value: unknown, dir: string): unknown => JSON.parse(JSON.stringify(value).replaceAll(dir, "<fixture>"));
const artifacts = (out: string) => Object.fromEntries(["transcripts", "excerpts"].flatMap(folder => {
	const dir = join(out, folder); return existsSync(dir) ? readdirSync(dir).map(name => [`${folder}/${name}`, readFileSync(join(dir, name), "utf8")]) : [];
}));

test("bounded two preserves the complete six-call result, models, artifacts and original history order", { timeout: 60000, skip: process.platform === "win32" }, async t => {
	const serial = fixture(t), baseline = await serial.start(undefined).ended;
	assert.equal(baseline.code, 0, baseline.stderr);
	const parallel = fixture(t, { hold: true });
	for (let i = 1; i < 6; i++) if (i !== 3) parallel.release(i);
	const running = parallel.start(2);
	await parallel.until(() => jsonl(parallel.trace).filter(row => row.event === "start").length >= 2);
	const first = jsonl(parallel.trace).filter(row => row.event === "start"); assert.equal(first.length, 2);
	assert.deepEqual(first.map(row => row.product).sort(), ["claude", "codex"], "Each provider remains serial");
	const osChildren = execFileSync("ps", ["-axo", "pid,ppid"], { encoding: "utf8" }).trim().split("\n").slice(1).map(row => row.trim().split(/\s+/).map(Number)).filter(([, ppid]) => ppid === running.child.pid);
	assert.equal(osChildren.length, 2, "Only two real runner process roots are admitted");
	parallel.release(3);
	await parallel.until(() => jsonl(parallel.trace).filter(row => row.event === "end").length === 3);
	assert.equal(jsonl(parallel.history).length, 1, "Out-of-order finishes cannot reorder the history");
	parallel.release(0);
	const result = await running.ended; assert.equal(result.code, 0, result.stderr);
	let active = 0, peak = 0;
	for (const row of jsonl(parallel.trace)) { active += row.event === "start" ? 1 : -1; peak = Math.max(peak, active); }
	assert.equal(peak, 2); assert.equal(active, 0);
	assert.equal(jsonl(parallel.trace).filter(row => row.event === "start").length, 6);
	const inputs = (f: ReturnType<typeof fixture>) => jsonl(f.trace).filter(row => row.event === "start").map(({ product, task, inputHash }) => ({ product, task, inputHash })).sort((a, b) => `${a.product}/${a.task}`.localeCompare(`${b.product}/${b.task}`));
	assert.deepEqual(inputs(serial), inputs(parallel), "Every call retains its exact task and data bytes");
	const a = readFresh(serial.out), b = readFresh(parallel.out);
	const clockA = a.at, clockB = b.at; delete a.at; delete b.at;
	assert.deepEqual(normalize(a, serial.dir), normalize(b, parallel.dir));
	assert.deepEqual(normalize(jsonl(serial.history), serial.dir), normalize(jsonl(parallel.history), parallel.dir));
	assert.deepEqual(artifacts(serial.out), artifacts(parallel.out));
	const appendixA = readFileSync(join(serial.out, "appendix.md"), "utf8").replaceAll(serial.dir, "<fixture>").replaceAll(clockA!, "<clock>");
	const appendixB = readFileSync(join(parallel.out, "appendix.md"), "utf8").replaceAll(parallel.dir, "<fixture>").replaceAll(clockB!, "<clock>");
	assert.deepEqual(appendixA, appendixB);
	for (const row of jsonl(parallel.trace).filter(row => row.event === "start")) {
		assert.ok(row.argv.includes("--no-history")); assert.equal(row.argv[row.argv.indexOf("--timeout-min") + 1], "2");
		assert.equal(row.argv.includes("--model"), row.product === "claude");
		if (row.product === "claude") assert.equal(row.argv[row.argv.indexOf("--model") + 1], "sonnet");
	}
});

test("failed and timed-out runners retain every peer and every scored history entry; low quality stays a finding", { timeout: 60000, skip: process.platform === "win32" }, async t => {
	for (const mode of ["nonzero", "missing", "timed", "quality"]) {
		const serial = fixture(t, { mode }), parallel = fixture(t, { mode });
		const a = await serial.start(1).ended, b = await parallel.start(2).ended;
		assert.equal(a.code, mode === "quality" ? 0 : 1, a.stderr); assert.equal(b.code, a.code, b.stderr);
		assert.equal(jsonl(parallel.trace).filter(row => row.event === "start").length, 6);
		assert.deepEqual(normalize(jsonl(serial.history), serial.dir), normalize(jsonl(parallel.history), parallel.dir), mode);
		assert.equal(jsonl(parallel.history).length, ["nonzero", "missing"].includes(mode) ? 6 : 7, mode);
		const old = readFresh(serial.out), next = readFresh(parallel.out); delete old.at; delete next.at;
		assert.deepEqual(normalize(old, serial.dir), normalize(next, parallel.dir), mode);
	}
});

test("malformed or mismatched artifacts and missing output fail visibly while all other calls finish", { timeout: 60000, skip: process.platform === "win32" }, async t => {
	for (const mode of ["malformed", "metadata", "output"]) {
		const f = fixture(t, { mode }), result = await f.start(2).ended;
		assert.equal(result.code, 1, result.stderr);
		assert.equal(jsonl(f.trace).filter(row => row.event === "start").length, 6, "Every selected call runs exactly once");
		const records = readFresh(f.out).products.flatMap(product => product.details);
		assert.equal(records.length, 6);
		assert.equal(records.filter(record => record.executed).length, 5);
		assert.equal(jsonl(f.history).length, 6, "Only the five valid scored calls append history");
		assert.match(records[0].failure!, mode === "output" ? /run.mjs exited/ : /Invalid run artifacts/);
	}
});

test("interruption keeps scored out-of-order peers without starting the remaining calls", { timeout: 60000, skip: process.platform === "win32" }, async t => {
	const f = fixture(t, { hold: true });
	for (let index = 1; index < 6; index++) if (index !== 3) f.release(index);
	const running = f.start(2);
	await f.until(() => jsonl(f.trace).filter(row => row.event === "start").length === 2);
	f.release(3);
	await f.until(() => jsonl(f.trace).filter(row => row.event === "end").length === 3);
	// A runner writes its end event just before it exits; a SIGTERM in between would make the gate score it as interrupted.
	await exited(jsonl(f.trace).filter(row => row.event === "end").map(row => row.pid));
	assert.equal(jsonl(f.history).length, 1, "The finished provider waits for the original plan prefix");
	running.child.kill("SIGTERM");
	const result = await running.ended;
	assert.equal(result.code, 2, result.stderr);
	assert.equal(jsonl(f.trace).filter(row => row.event === "start").length, 4, "No remaining call starts after interruption");
	assert.deepEqual(jsonl<HistoryRow>(f.history).slice(1).map(row => [row.product, row.task]), f.plan.slice(3).map(row => [row.product, row.task]));
	assert.equal(Object.keys(artifacts(f.out)).length, 3, "Every finished peer keeps its transcript");
	assert.equal(existsSync(join(f.out, "fresh-agent.json")), false, "An interrupted gate never writes a passing result");
});

test("invalid jobs never start a runner and repeated product/task keys retain original serial execution", { timeout: 60000, skip: process.platform === "win32" }, async t => {
	for (const value of [0, 3, -1, 1.5, "bad"]) { const f = fixture(t), result = await f.start(value).ended; assert.equal(result.code, 2); assert.deepEqual(jsonl(f.trace), []); }
	const f = fixture(t, { runs: 6 }), result = await f.start(2).ended; assert.equal(result.code, 0, result.stderr);
	let active = 0, peak = 0;
	for (const row of jsonl(f.trace)) { active += row.event === "start" ? 1 : -1; peak = Math.max(peak, active); }
	assert.equal(peak, 1); assert.equal(jsonl(f.trace).filter(row => row.event === "start").length, 12);
});

test("an interrupted two-worker gate drains only its own real runner groups", { timeout: 60000, skip: process.platform === "win32" }, async t => {
	const outsider = spawn(process.execPath, ["-e", "process.stdin.resume()"], { detached: true, stdio: ["pipe", "ignore", "ignore"] });
	await once(outsider, "spawn");
	const stopOutsider = async () => { if (outsider.exitCode !== null || outsider.signalCode !== null) return; const ended = once(outsider, "close"); outsider.stdin.end(); await ended; };
	t.after(stopOutsider);
	const f = fixture(t, { hold: true, abort: true }), running = f.start(2);
	await f.until(() => jsonl(f.trace).filter(row => row.event === "start").length === 2);
	const own = jsonl(f.trace).filter(row => row.event === "start");
	running.child.kill("SIGTERM"); const result = await running.ended; assert.equal(result.code, 2, result.stderr);
	assert.equal(jsonl(f.trace).filter(row => row.event === "start").length, 2, "No new calls start after interruption");
	for (const row of own) for (const pid of [row.pid, row.descendant]) assert.throws(() => process.kill(pid!, 0), { code: "ESRCH" });
	assert.doesNotThrow(() => process.kill(outsider.pid!, 0), "An unrelated process group stays alive");
	await stopOutsider();
});
