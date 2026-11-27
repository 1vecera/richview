// Host reason (ARC-03): keep cohort retry logs consumable by existing release evidence.
import test from "node:test";
import assert from "node:assert/strict";
import {mkdtempSync, readFileSync, rmSync, writeFileSync} from "node:fs";
import {tmpdir} from "node:os";
import {join} from "node:path";
import {cohortApplicable as cohortApplicableImplementation, runCohortStep as runCohortStepImplementation} from "../lib/cohort-run.mts";

import type {GateStep, GateCommand} from "../lib/steps.mts";
import type {runLogged} from "../lib/proc.mts";

type TestCommand = GateCommand & { id: string };
type Result = {status: string; details: {commands: {status: string; attempts: {logPath: string}[]}[]; environmentRetry?: boolean; unrunTests?: {name: string; reason: string}[]}};
const inline = <T,>(work: () => T): T => work();
const insideHold = <T,>(_name: string, _ctx: unknown, work: () => T): T => work();
const cohortApplicable = cohortApplicableImplementation as (step: Pick<GateStep, "cohort" | "run">) => boolean;
const runCohortStep = runCohortStepImplementation as unknown as (
	step: Pick<GateStep, "id" | "cohort">,
	ctx: {cwd: string; runDir: string; signal?: AbortSignal},
	options: {target: string; env: NodeJS.ProcessEnv; parallel: number; selected?: string[]; schedule: typeof inline; hold?: typeof insideHold;
		expand: () => {prepare: TestCommand; shards: TestCommand[]; maxConcurrent: number}; run?: typeof runLogged},
) => Promise<Result>;

test("scoped Cargo selections keep the original executor", () => {
	const step={cohort:"rust-debug" as const,run:[{cmd:"cargo",args:["test","--workspace","--locked","--no-fail-fast"]}]};
	assert.equal(cohortApplicable(step),true);
	assert.equal(cohortApplicable({...step,run:[{...step.run[0],args:["test","-p","richview-core","--locked","--no-fail-fast"]}]}),false);
});

test("a passing retry owns the canonical numbered log and archives the failure", async t => {
	const dir=mkdtempSync(join(tmpdir(),"gate-cohort-logs-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
	const step={id:"rust-tests",cohort:"rust-debug" as const},ctx={cwd:dir,runDir:dir};
	const expand=()=>({prepare:{id:"prepare",cmd:"test",args:[]},shards:[0,1,2,3].map(i=>({id:`shard-${i}-of-4`,cmd:"test",args:[]})),maxConcurrent:2});
	let shard=0;
	const run: typeof runLogged=async (_cmd,_args,{logPath,env})=>{assert.equal(env?.CARGO_TARGET_DIR,"/shared");assert.equal(env?.RICHVIEW_VERIFY_WORKERS,"1");const failing=logPath.endsWith("rust-tests-0.log")&&shard++===0;writeFileSync(logPath,failing?"failed first":"passed second");return {status:failing?"fail":"pass",exitCode:failing?1:0,command:_cmd,durationMs:0};};
	const result=await runCohortStep(step,ctx,{target:"/shared",env:{RICHVIEW_VERIFY_WORKERS:"99"},parallel:5,schedule:inline,expand,hold:insideHold,run});
	assert.equal(result.status,"pass");assert.equal(readFileSync(join(dir,"logs/rust-tests-0.log"),"utf8"),"passed second");
	assert.equal(readFileSync(join(dir,"attempts/rust-tests/1/logs/rust-tests-0.log"),"utf8"),"failed first");
	assert.match(result.details.commands[0].attempts[0].logPath,/attempts\/rust-tests\/1\/logs/);
	assert.equal(readFileSync(result.details.commands[0].attempts[0].logPath,"utf8"),"failed first");
});

test("missing binaries are recorded as environment retries and prepare again after peers drain", async t => {
	const dir=mkdtempSync(join(tmpdir(),"gate-cohort-missing-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
	const step={id:"rust-tests",cohort:"rust-debug" as const},ctx={cwd:dir,runDir:dir},calls: string[]=[];
	const expand=()=>({prepare:{id:"prepare",cmd:"test",args:[]},shards:[0,1,2,3].map(i=>({id:`shard-${i}-of-4`,cmd:"test",args:[]})),maxConcurrent:2});
	const counts=new Map();
	const run: typeof runLogged=async (_cmd,_args,{logPath})=>{const key=logPath.split("/").at(-1),n=(counts.get(key)??0)+1;counts.set(key,n);calls.push(`${key}:${n}`);const missing=key==="rust-tests-0.log"&&n===1;writeFileSync(logPath,missing?"rv-gate Rust cohort environment-retry: missing test executable test":"passed");return {status:missing?"fail":"pass",exitCode:missing?75:0,command:_cmd,durationMs:0};};
	const result=await runCohortStep(step,ctx,{target:"/shared",env:{},parallel:5,schedule:inline,expand,hold:insideHold,run});
	assert.equal(result.status,"pass");assert.equal(result.details.environmentRetry,true);
	assert.deepEqual(calls,["rust-tests-prepare.log:1","rust-tests-0.log:1","rust-tests-1.log:1","rust-tests-2.log:1","rust-tests-3.log:1","rust-tests-prepare.log:2","rust-tests-0.log:2"]);
});


test("an incomplete expanded cohort fails closed", async t => {
	const dir=mkdtempSync(join(tmpdir(),"gate-cohort-incomplete-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
	await assert.rejects(()=>runCohortStep({id:"rust-tests",cohort:"rust-debug" as const},{cwd:dir,runDir:dir},{target:"/shared",env:{},parallel:5,schedule:inline,expand:()=>({prepare:{id:"prepare",cmd:"fixture",args:[]},shards:[{id:"shard-0-of-4",cmd:"fixture",args:[]}],maxConcurrent:2}),hold:insideHold}),/all four/);
});

test("missing connector declarations are recorded once across prepare and shards", async t => {
	const dir=mkdtempSync(join(tmpdir(),"gate-cohort-connector-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
	const step={id:"rust-slow",cohort:"rust-debug" as const},ctx={cwd:dir,runDir:dir};
	const whenEnvMissing={vars:["RV_TEST_ABSENT_CONNECTOR"],args:["--skip","example","--exact"],unrun:[{name:"example",reason:"connector unavailable"}]};
	const expand=()=>({prepare:{id:"prepare",cmd:"test",args:[],whenEnvMissing},shards:[0,1,2,3].map(i=>({id:`shard-${i}-of-4`,cmd:"test",args:[],whenEnvMissing})),maxConcurrent:2});
	const run: typeof runLogged=async (_cmd,args,{logPath})=>{assert.deepEqual(args,["--skip","example","--exact"]);writeFileSync(logPath,"passed");return {status:"pass",exitCode:0,command:_cmd,durationMs:0};};
	const result=await runCohortStep(step,ctx,{target:"/shared",env:{},parallel:5,schedule:inline,expand,hold:insideHold,run});
	assert.deepEqual(result.details.unrunTests,[{name:"example",reason:"connector unavailable"}]);
});

test("a later affected run freshly prepares and runs only its selected Rust shard",async t=>{
	const dir=mkdtempSync(join(tmpdir(),"gate-cohort-selected-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
	const step={id:"rust-tests",cohort:"rust-debug" as const},ctx={cwd:dir,runDir:dir},calls: (string | undefined)[]=[];
	const expand=()=>({prepare:{id:"prepare",cmd:"test",args:[]},shards:[0,1,2,3].map(i=>({id:`shard-${i}-of-4`,cmd:"test",args:[]})),maxConcurrent:2});
	const run: typeof runLogged=async(_cmd,_args,{logPath})=>{calls.push(logPath.split("/").at(-1));writeFileSync(logPath,"passed");return {status:"pass",exitCode:0,command:_cmd,durationMs:0};};
	const result=await runCohortStep(step,ctx,{target:"/shared",env:{},parallel:5,schedule:inline,expand,hold:insideHold,run,selected:["shard-2-of-4"]});
	assert.equal(result.status,"pass");assert.deepEqual(calls,["rust-tests-prepare.log","rust-tests-2.log"]);
	assert.deepEqual(result.details.commands.map(item=>item.status),["unrun","unrun","pass","unrun"]);
});

test("cancellation preserves a completed shard failure and marks the interrupted shard unrun",async t=>{
	const dir=mkdtempSync(join(tmpdir(),"gate-cohort-cancel-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
	const controller=new AbortController(),step={id:"rust-tests",cohort:"rust-debug" as const},ctx={cwd:dir,runDir:dir,signal:controller.signal};
	const expand=()=>({prepare:{id:"prepare",cmd:"test",args:[]},shards:[0,1,2,3].map(i=>({id:`shard-${i}-of-4`,cmd:"test",args:[]})),maxConcurrent:2});
	const {promise: active, resolve: ready}=Promise.withResolvers<void>();
	const run: typeof runLogged=async(_cmd,_args,{logPath})=>{
		writeFileSync(logPath,"test");
		if(logPath.endsWith("prepare.log"))return {status:"pass",exitCode:0,command:_cmd,durationMs:0};
		if(logPath.endsWith("rust-tests-0.log"))return {status:"fail",exitCode:1,command:_cmd,durationMs:0};
		ready();await new Promise<void>(resolve=>controller.signal.addEventListener("abort",()=>resolve(),{once:true}));return {status:"fail",exitCode:1,command:_cmd,durationMs:0};
	};
	const pending=runCohortStep(step,ctx,{target:"/shared",env:{},parallel:2,schedule:inline,expand,hold:insideHold,run,selected:["shard-0-of-4","shard-1-of-4"]});
	await active;await new Promise(resolve=>setTimeout(resolve,5));controller.abort();const result=await pending;
	assert.equal(result.details.commands[0].status,"fail");assert.equal(result.details.commands[1].status,"unrun");
});
