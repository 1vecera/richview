// Host reason (ARC-03): execute the actual CLI step runner against real disposable Node commands.
import assert from "node:assert/strict";
import {mkdtempSync, rmSync, readFileSync} from "node:fs";
import {tmpdir} from "node:os";
import {join} from "node:path";
import vm from "node:vm";
import test from "node:test";
import * as fs from "node:fs";
import * as checks from "../lib/checks.mts";
import {stepsForTier} from "../lib/steps.mts";
import {runFullSteps} from "../lib/full.mts";
import {runPool} from "../lib/release.mts";
import {gateEnv, ignoredTests, unrunLines} from "../lib/proc.mts";
import {runScheduledCommand} from "../lib/command.mts";
import {withRerun, archiveAttempt, archiveBuildSnapshot} from "../lib/rerun.mts";
import {prelaunchException, browserGap, CHROMIUM_ONLY_REASON, type Phase, type PhaseRules} from "../lib/phase.mts";
import {retryCommands} from "../lib/affected-retry.mts";
import {commandId} from "../lib/selection.mts";
import {executeCaseCommand} from "../lib/case-executor.mts";
import {cohortApplicable, runCohortStep} from "../lib/cohort-run.mts";
import {rotationFromHistory, rotationReceipts} from "../lib/value.mts";
import {executionMeter} from "../lib/step-time.mts";
import {freshCheck, type WorkerContext} from "../lib/check-process.mts";
import {BUILD_MANIFEST} from "../lib/build-manifest.mts";
import {SOURCE_OVERRIDES,withGateSourceEnvironment} from "../lib/source-env.mts";
import type {AffectedReceipt} from "../lib/affected-coverage.mts";
import type {SelectionItem} from "../lib/selection.mts";
import type {CaseEvidence} from "../lib/cases.mts";
import type {GateStatus, StepOutcome} from "../lib/results.mts";
import {outstandingCoverage, validateAffectedHistory} from "../lib/affected-coverage.mts";
import { readTestSource } from "./test-source.mts";

const source=readTestSource(process.env.PRELAUNCH_SOURCE_UNDER_TEST||new URL("../rv-gate.mts",import.meta.url));
const start=source.indexOf("async function runSteps("),end=source.indexOf("\n// ---------------------------------------------------------------------------------------------\n// Checks",start);
assert.ok(start>=0&&end>start);
const receiptTime = "2026-11-01T00:00:00Z";
type CommandSample = { status: GateStatus; pid?: number; attempts: { status: string }[]; cases: CaseEvidence };
type ProbeDetails = { commands: CommandSample[]; processPid?: number; unrunInputs: { what: string; reason: string }[] };
type ProbeResult = {
	id: string; status: GateStatus; reason: string; flaky: boolean;
	runMs: number; queuedMs: number; durationMs: number;
	units: { id: string; status: GateStatus }[];
	rotation: { ordinal: number; cycle: number };
	exception: StepOutcome["exception"];
	attempts: { status: GateStatus; logs: string[]; reason: string; details: ProbeDetails; buildSnapshotArchive: string }[];
	logs: string[];
	details: ProbeDetails;
};
type CliStep = { id: string; title: string; cards: string[]; tier: string; [field: string]: unknown };
type SnapshotContext = {tempDir: string; snapshot?: string; snapshotFiles?: unknown[]};
function runner(steps: CliStep[],dir: string,overrides: Record<string, unknown>={}){
	const globals={...fs,join,console:{log:()=>{}},Date,Map,Set,Number,AbortController,
		SHARED_TARGET:join(dir,"target"),GATED_MODES:new Set(["full","prod","dev"]),WAIT_FLOOR_MODES:new Set(),
		registryFor:()=>({steps,retired:[],added:[]}),stepsForTier,gateEnv,listReceipts:()=>[],rotationFromHistory,rotationReceipts,prelaunchException,browserGap,CHROMIUM_ONLY_REASON,retryCommands,cohortApplicable,runCohortStep,commandId,executeCaseCommand,withGateSourceEnvironment,
		executionMeter,withRerun,archiveAttempt,archiveBuildSnapshot,freshCheck,runScheduledCommand,runPool,runFullSteps,ignoredTests,unrunLines,
		workspaceBuiltAt:()=>0,special:{},checks,line:(r: Pick<ProbeResult, "status">)=>r.status,...overrides};
	const context=vm.createContext(globals);vm.runInContext(source.slice(start,end)+"\nglobalThis.execute=runSteps;",context);
	// The VM installs the shipping step runner. The probes assert the fields from its own receipt.
	const execute: (ctx: Record<string, unknown>, tier: string, options?: { parallel?: number }) => Promise<ProbeResult[]> = context.execute;
	return execute;
}

test("cancellation keeps a completed command failure while its active sibling becomes unrun", async t => {
	const dir=mkdtempSync(join(tmpdir(),"gate-cancel-units-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
	const commands=[{cmd:"fake",args:["red"]},{cmd:"fake",args:["active"]}];
	const step={id:"host",title:"host",tier:"full" as const,cards:[],parallel:2,run:commands};
	const ids=commands.map(command=>commandId(step,command)),selection=ids.map((id,index)=>({id,step:"host",index,status:"run"}));
	const controller=new AbortController();const {promise: active, resolve: activeStarted}=Promise.withResolvers<void>();
	const execute=runner([step],dir,{runScheduledCommand:async(_command: unknown,args: string[])=>{
		if(args[0]==="red")return {status:"fail",exitCode:1};
		activeStarted();await new Promise<void>(resolve=>controller.signal.addEventListener("abort",()=>resolve(),{once:true}));return {status:"fail",exitCode:1};
	}});
	const pending=execute({cwd:dir,runDir:dir,mode:"full",phasePolicy:{phase:"prelaunch",rotation:[],prelaunchFull:{partialRetry:true}},affectedSelection:selection,signal:controller.signal},"full");
	await active;await new Promise(resolve=>setTimeout(resolve,10));controller.abort();
	const [result]=await pending,units=Object.fromEntries(result.units.map(unit=>[unit.id,unit.status]));
	assert.equal(result.status,"fail");assert.equal(units[ids[0]],"fail");assert.equal(units[ids[1]],"unrun");
	const receipt: AffectedReceipt={at:receiptTime,id:"affected-red",kind:"affected",base:"base",commit:"a",tree:"tree-a",policyHash:"old",selected:ids,outcomes:units,steps:[result],result:"fail"};
	const history=validateAffectedHistory({receipts:[receipt],base:"base",head:"a",policyHash:"new",contains:()=>true,treeOf:()=>"tree-a",allowPolicyChange:true});
	assert.ok(outstandingCoverage({obligations:[],receipts:history,contains:()=>true,head:undefined}).some(item=>item.unit===ids[0]));
});
test("shipping quota exception skips only the dated prelaunch full and actually executes other cases",async t=>{
	for(const [phase,mode,now,id,paused]of [
		["prelaunch","full","2026-10-30T14:00:00Z","fresh-agent",true],
		["prelaunch","full","2026-10-31T18:00:00.581308Z","fresh-agent",false],
		["prelaunch","prod","2026-10-30T14:00:00Z","fresh-agent",false],
		["launch","full","2026-10-30T14:00:00Z","fresh-agent",false],
		["prelaunch","full","2026-10-30T14:00:00Z","another-step",false],
	] satisfies [Phase, string, string, string, boolean][]){
		const dir=mkdtempSync(join(tmpdir(),"gate-quota-runner-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
		const marker=join(dir,"executed"),step={id,title:id,tier:"full" as const,cards:[],run:[{cmd:process.execPath,args:["-e",`require('node:fs').writeFileSync(${JSON.stringify(marker)},'ran')`]}]};
		const phasePolicy={phase,since:"2026-10-30",by:"Daniel",rotation:[],freshAgentPause:{until:"2026-10-31T18:00:00.581308Z",reason:"Daniel: personal quota"}};
		const result=(await runner([step],dir,{prelaunchException:(p: PhaseRules,o: { mode?: string; id?: string; now?: Date|string; }|undefined)=>prelaunchException(p,{...o,now})})({cwd:dir,runDir:dir,mode,phasePolicy},"full"))[0];
		assert.equal(result.status,paused?"unrun":"pass",JSON.stringify(result));assert.equal(fs.existsSync(marker),!paused);
		if(paused){assert.equal(result.reason,phasePolicy.freshAgentPause.reason);assert.deepEqual(JSON.parse(JSON.stringify(result.exception)),{type:"fresh-agent-quota",...phasePolicy.freshAgentPause});assert.equal(result.attempts.length,1);}
	}
});
test("the shipping full runner retains both command attempts, raw logs and actual run time",async t=>{
	for(const phase of ["prelaunch","launch"]){
		const dir=mkdtempSync(join(tmpdir(),"gate-prelaunch-runner-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
		const marker=join(dir,"count"),script=`const fs=require('node:fs');const p=${JSON.stringify(marker)};let n=fs.existsSync(p)?Number(fs.readFileSync(p)):0;fs.writeFileSync(p,String(n+1));console.log(JSON.stringify({pid:process.pid,attempt:n+1}));process.exit(n===0?1:0);`;
		const step={id:"flip",title:"flip",tier:"full" as const,cards:[],run:[{cmd:process.execPath,args:["-e",script]}]};
		const result=(await runner([step],dir)({cwd:dir,runDir:dir,mode:"full",phasePolicy:{phase,since:"2026-10-30",by:"Daniel",rotation:[]}},"full"))[0];
		assert.equal(result.status,phase==="prelaunch"?"pass":"fail",JSON.stringify(result));assert.equal(result.flaky,phase==="prelaunch");
		assert.equal(result.attempts.length,phase==="prelaunch"?2:1);
		assert.ok(result.runMs>=0&&result.queuedMs>=0);assert.ok(result.runMs<=result.durationMs+1);
		assert.match(readFileSync(result.attempts[0].logs[0],"utf8"),/"attempt":1/);
		if(phase==="prelaunch"){
			assert.match(result.attempts[0].logs[0],/attempts\/flip\/1/);
			assert.match(readFileSync(result.attempts[1].logs[0],"utf8"),/"attempt":2/);
			assert.notEqual(result.attempts[0].details.commands[0].pid,result.attempts[1].details.commands[0].pid);
		}
	}
});
test("the shipping built-in rerun is a fresh process and a second deterministic failure stays red",async t=>{
	const dir=mkdtempSync(join(tmpdir(),"gate-prelaunch-builtin-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
	const step={id:"pr-fields",fn:"prFields",tier:"full" as const,cards:[],title:"fields"};
	const result=(await runner([step],dir)({cwd:dir,runDir:dir,mode:"full",phasePolicy:{phase:"prelaunch",since:"2026-10-30",by:"Daniel",rotation:[]}},"full"))[0];
	assert.equal(result.status,"fail");assert.equal(result.flaky,false);assert.equal(result.attempts.length,2,JSON.stringify(result));
	assert.ok(result.attempts[1].details.processPid!==process.pid);assert.match(result.attempts[0].reason,/No PR context/);assert.match(result.attempts[1].reason,/No PR context/);
});
test("shipping rotation retains every cohort result as unrun while prod and launch execute all",async t=>{
	for(const [phase,mode]of [["prelaunch","full"],["prelaunch","prod"],["launch","full"]]){
		const dir=mkdtempSync(join(tmpdir(),"gate-rotation-runner-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
		const ids=["slow-a","slow-b","slow-c"],steps=ids.map(id=>({id,title:id,tier:"full" as const,cards:[],run:[{cmd:process.execPath,args:["-e",`require('node:fs').writeFileSync(${JSON.stringify(join(dir,id))},'ran')`]}]}));
		const history=Array.from({length:20},(_,i)=>({id:`history-${i}`,kind:"full",at:new Date(Date.UTC(2026,9,28,i)).toISOString(),steps:ids.map(id=>({id,status:"pass",runMs:300000}))}));
		const result=await runner(steps,dir,{listReceipts:()=>history})({cwd:dir,runDir:dir,mode,phasePolicy:{phase,since:"2026-10-30",by:"Daniel",rotation:ids}},"full");
		assert.deepEqual(Array.from(result,r=>r.id),ids);
		const rotates=phase==="prelaunch"&&mode==="full";
		assert.deepEqual(Array.from(result,r=>r.status),rotates?["pass","unrun","unrun"]:["pass","pass","pass"]);
		for(const r of result)if(r.status==="unrun"){assert.equal(r.reason,"prelaunch rotation");assert.equal(r.rotation.ordinal,0);assert.equal(r.rotation.cycle,3);assert.equal(fs.existsSync(join(dir,r.id)),false);}else assert.equal(readFileSync(join(dir,r.id),"utf8"),"ran");
	}
});
test("a build-tree failure reruns build commands after a foreign-writer window and retains its first snapshot",async t=>{
	const dir=mkdtempSync(join(tmpdir(),"gate-build-rerun-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
	fs.mkdirSync(join(dir,"target/release"),{recursive:true});fs.writeFileSync(join(dir,"target",BUILD_MANIFEST),"{}");
	const marker=join(dir,"count"),script=`const f=require('node:fs'),p=${JSON.stringify(marker)};f.writeFileSync(p,String(f.existsSync(p)?Number(f.readFileSync(p))+1:1));`;
	const step={id:"build",title:"build",tier:"full" as const,cards:[],run:[{cmd:process.execPath,args:["-e",script]}]};
	const overrides={BUILD_MANIFEST,buildManifestProblems:()=>[],holdCargo:async()=>()=>{},linkTarget:()=>{},workspaceBuiltAt:()=>Date.now()+100000,
		checks:{...checks,buildTreeResult:(result: Pick<ProbeResult,"status">)=>checks.buildTreeResult(result,dir,()=>Number(readFileSync(marker))===1?["generated.json"]:[])},
		captureBuildSnapshot:({ctx,result}: {ctx: SnapshotContext; result: ProbeResult})=>{ctx.snapshot=join(ctx.tempDir,"target");ctx.snapshotFiles=[];fs.mkdirSync(ctx.snapshot,{recursive:true});fs.writeFileSync(join(ctx.snapshot,"native"),`snapshot-${readFileSync(marker,"utf8")}`);return result;}};
	const result=(await runner([step],dir,overrides)({cwd:dir,head:"a".repeat(40),runDir:dir,tempDir:join(dir,"temp"),mode:"full",phasePolicy:{phase:"prelaunch",since:"2026-10-30",by:"Daniel",rotation:[]}},"full"))[0];
	assert.equal(result.status,"pass",JSON.stringify(result));assert.equal(result.flaky,true);assert.equal(readFileSync(marker,"utf8"),"2");
	assert.equal(readFileSync(join(result.attempts[0].buildSnapshotArchive,"native"),"utf8"),"snapshot-1");
});


test("prelaunch full visibly skips non-Chromium commands and launch runs them", async t => {
	for (const phase of ["prelaunch", "launch"]) {
		const dir = mkdtempSync(join(tmpdir(), "gate-engine-policy-")); t.after(() => rmSync(dir, {recursive: true, force: true}));
		const make = (engine: string) => ({cmd: process.execPath, args: ["-e", `require('node:fs').writeFileSync(${JSON.stringify(join(dir, engine))},'ran')`], env: {RICHVIEW_VERIFY_BROWSER: engine}});
		const steps = [{id:"parallel-shaders", title:"shaders", tier:"full" as const, cards:[], parallel:3, run:[make("chromium"), make("firefox"), make("webkit")]}, {id:"firefox", title:"firefox", tier:"full" as const, cards:[], run:[make("firefox-loop")]}];
		const phasePolicy = {phase, since:"2026-11-09", by:"Daniel", rotation:[], prelaunchFull:{browsers:["chromium"], partialRetry:true}};
		const result = await runner(steps, dir)({cwd:dir,runDir:dir,mode:"full",phasePolicy}, "full");
		assert.equal(result[0].status, "pass");
		assert.deepEqual(Array.from(result[0].details.commands, item => item.status), phase === "prelaunch" ? ["pass", "unrun", "unrun"] : ["pass", "pass", "pass"]);
		assert.equal(result[1].status, phase === "prelaunch" ? "unrun" : "pass");
		if (phase === "prelaunch") assert.ok(result[0].details.unrunInputs.every((item: { reason: string; }) => item.reason === CHROMIUM_ONLY_REASON));
	}
});

test("prelaunch full retries only failed commands of a multi-command step", async t => {
	const dir = mkdtempSync(join(tmpdir(), "gate-command-retry-")); t.after(() => rmSync(dir, {recursive:true, force:true}));
	const command = (name: string, fail: boolean) => ({cmd:process.execPath, args:["-e", `const f=require('node:fs'),p=${JSON.stringify(join(dir, "COUNT"))}.replace('COUNT',${JSON.stringify(name)});const n=f.existsSync(p)?Number(f.readFileSync(p)):0;f.writeFileSync(p,String(n+1));process.exit(${fail}&&n===0?1:0);`]});
	const steps = [{id:"batch", title:"batch", tier:"full" as const, cards:[], parallel:2, run:[command("a",false),command("b",true),command("c",false)]}];
	const phasePolicy = {phase:"prelaunch",since:"2026-11-09",by:"Daniel",rotation:[],prelaunchFull:{browsers:["chromium"],partialRetry:true}};
	const [result] = await runner(steps,dir)({cwd:dir,runDir:dir,mode:"full",phasePolicy},"full");
	assert.equal(result.status,"pass",JSON.stringify(result));
	assert.deepEqual(["a","b","c"].map(name=>readFileSync(join(dir,name),"utf8")),["1","2","1"]);
	assert.deepEqual(Array.from(result.details.commands[1].attempts, item=>item.status),["fail","pass"]);
});

test("a serial producer retries before its consumer starts, and a red producer blocks its consumer", async t => {
	for (const permanentlyFail of [false, true]) {
		const dir=mkdtempSync(join(tmpdir(),"gate-serial-retry-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
		const count=join(dir,"count"), product=join(dir,"product"), consumed=join(dir,"consumed");
		const produce=`const f=require('node:fs'),p=${JSON.stringify(count)},n=f.existsSync(p)?Number(f.readFileSync(p)):0;f.writeFileSync(p,String(n+1));if(${permanentlyFail}||n===0)process.exit(1);f.writeFileSync(${JSON.stringify(product)},'ready');`;
		const consume=`const f=require('node:fs');if(!f.existsSync(${JSON.stringify(product)}))process.exit(1);f.writeFileSync(${JSON.stringify(consumed)},'done');`;
		const step={id:"publish-tests",title:"pipeline",tier:"full" as const,cards:[],run:[{cmd:process.execPath,args:["-e",produce]},{cmd:process.execPath,args:["-e",consume]}]};
		const phasePolicy={phase:"prelaunch",since:"2026-11-09",by:"Daniel",rotation:[],prelaunchFull:{browsers:["chromium"],partialRetry:true}};
		const [result]=await runner([step],dir)({cwd:dir,runDir:dir,mode:"full",phasePolicy},"full");
		assert.equal(result.status,permanentlyFail?"fail":"pass");
		assert.equal(fs.existsSync(consumed),!permanentlyFail);
		assert.equal(result.details.commands[1].status,permanentlyFail?"unrun":"pass");
	}
});


test("a failed independent serial command does not hide a later failure", async t => {
	const dir=mkdtempSync(join(tmpdir(),"gate-serial-independent-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
	const mark=join(dir,"second");
	const step={id:"independent",title:"independent",tier:"full" as const,cards:[],run:[
		{cmd:process.execPath,args:["-e","process.exit(1)"]},
		{cmd:process.execPath,args:["-e",`require('node:fs').writeFileSync(${JSON.stringify(mark)},'ran');process.exit(1);`]},
	]};
	const phasePolicy={phase:"prelaunch",since:"2026-11-09",by:"Daniel",rotation:[],prelaunchFull:{browsers:["chromium"],partialRetry:true}};
	const [result]=await runner([step],dir)({cwd:dir,runDir:dir,mode:"full",phasePolicy},"full");
	assert.equal(result.status,"fail");assert.equal(readFileSync(mark,"utf8"),"ran");
	assert.deepEqual(Array.from(result.details.commands,command=>command.status),["fail","fail"]);
});

test("prelaunch commands cannot inherit a non-Chromium engine from the host", async t => {
	const dir=mkdtempSync(join(tmpdir(),"gate-inherited-engine-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
	const marker=join(dir,"engine"),step={id:"engine",title:"engine",tier:"full" as const,cards:[],run:[{cmd:process.execPath,args:["-e",`require('node:fs').writeFileSync(${JSON.stringify(marker)},process.env.RICHVIEW_VERIFY_BROWSER)`]}]};
	const phasePolicy={phase:"prelaunch",since:"2026-11-09",by:"Daniel",rotation:[],prelaunchFull:{browsers:["chromium"],partialRetry:true}};
	const [result]=await runner([step],dir,{gateEnv:()=>({...gateEnv(),RICHVIEW_VERIFY_BROWSER:"webkit"})})({cwd:dir,runDir:dir,mode:"full",phasePolicy},"full");
	assert.equal(result.status,"pass");assert.equal(readFileSync(marker,"utf8"),"chromium");
});


test("prelaunch full delegates a declared Rust cohort through its held scheduler", async t => {
	const dir=mkdtempSync(join(tmpdir(),"gate-cohort-wiring-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
	const step={id:"rust-tests",title:"rust",tier:"full" as const,cards:[],cohort:"rust-debug",run:[{cmd:"cargo",args:["test","--workspace","--locked","--no-fail-fast"]}]};
	const phasePolicy={phase:"prelaunch",since:"2026-11-09",by:"Daniel",rotation:[],prelaunchFull:{browsers:["chromium"],partialRetry:true}};
	let called=0;
	const [result]=await runner([step],dir,{runCohortStep:async (_step: unknown,ctx: {runDir: string; cwd: string},options: {target: string; parallel: number; schedule: unknown})=>{called++;assert.equal(options.target,join(dir,"target"));assert.equal(options.parallel,1);assert.equal(typeof options.schedule,"function");assert.equal(ctx.cwd,dir);return {status:"pass",details:{commands:[]}};}})({cwd:dir,runDir:dir,mode:"full",phasePolicy},"full");
	assert.equal(called,1);assert.equal(result.status,"pass");
});

test("launch and scoped Rust selections retain the original command executor", async t => {
	for (const [phase,scoped] of [["launch",false],["prelaunch",true]]) {
		const dir=mkdtempSync(join(tmpdir(),"gate-cohort-fallback-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
		const args=scoped?["test","-p","richview-core","--locked","--no-fail-fast"]:["test","--workspace","--locked","--no-fail-fast"];
		const step={id:"rust-tests",title:"rust",tier:"full" as const,cards:[],cohort:"rust-debug",run:[{cmd:"cargo",args}]};
		const phasePolicy={phase,since:"2026-11-09",by:"Daniel",rotation:[],prelaunchFull:{browsers:["chromium"],partialRetry:true}};
		let normal=0,cohort=0;
		const overrides={GATED_MODES:new Set(),waitForCargo:async()=>{},runScheduledCommand:async()=>{normal++;return {status:"pass",exitCode:0};},runCohortStep:async()=>{cohort++;return {status:"pass"};}};
		const [result]=await runner([step],dir,overrides)({cwd:dir,runDir:dir,mode:"full",phasePolicy},"full");
		assert.equal(result.status,"pass");assert.equal(normal,1);assert.equal(cohort,0);
	}
});


test("affected execution runs only selected command units and records the rest as unrun", async t => {
	const dir=mkdtempSync(join(tmpdir(),"gate-selected-command-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
	const make=(name: string)=>({cmd:process.execPath,args:["-e",`require('node:fs').writeFileSync(${JSON.stringify(join(dir,"NAME"))}.replace('NAME',${JSON.stringify(name)}),'ran')`]});
	const step={id:"browser",title:"browser",tier:"full" as const,cards:[],parallel:2,run:[make("a"),make("b"),make("c")]};
	const other={id:"other",title:"other",tier:"full" as const,cards:[],run:[make("other")]};
	const selected=[{id:commandId(step,step.run[1]),step:"browser",index:1,status:"run"}];
	const phasePolicy={phase:"prelaunch",since:"2026-11-09",by:"Daniel",rotation:[],prelaunchFull:{browsers:["chromium"],partialRetry:true}};
	const result=await runner([step,other],dir)({cwd:dir,runDir:dir,mode:"full",phasePolicy,affectedSelection:selected},"full");
	assert.equal(result[0].status,"pass");assert.deepEqual(Array.from(result[0].units,unit=>[unit.id,unit.status]),[[selected[0].id,"pass"]]);
	assert.equal(result[1].status,"unrun");assert.equal(fs.existsSync(join(dir,"b")),true);for(const name of ["a","c","other"])assert.equal(fs.existsSync(join(dir,name)),false);
});

test("affected execution re-execs the trusted origin/dev gate", async () => {
	const {gateRefFor} = await import("../lib/release.mts");
	const begin=source.indexOf("function fromDevCopy("), finish=source.indexOf("\nasync function main(",begin);
	assert.ok(begin>=0&&finish>begin);
	const calls: (string | string[] | {env: NodeJS.ProcessEnv})[][]=[];
	const context=vm.createContext({join,gateRefFor,console,workCheckout:()=>"/trusted-gate",gitIn:(...args: (typeof calls)[number])=>calls.push(args),sh:()=>({status:0}),existsSync:()=>true,
		process:{argv:["node","rv-gate.mjs","affected","origin/dev","--parallel","5"],execPath:"node",env:{}},
		spawnSync:(...args: (typeof calls)[number])=>{calls.push(args);return {status:0};}});
	vm.runInContext(source.slice(begin,finish)+"\nglobalThis.delegate=fromDevCopy;",context);
	assert.equal(context.delegate("affected",{}),0);
	assert.ok(calls.some(args=>args.includes("checkout")&&args.includes("origin/dev")));
	const spawn=calls.find(args=>args[0]==="node");
	assert.ok(spawn && Array.isArray(spawn[1]));
	const env = spawn[2];
	assert.ok(typeof env === "object" && "env" in env);
	assert.equal(spawn[1][0],"/trusted-gate/tools/rv-gate/rv-gate.mjs");
	assert.deepEqual(Array.from(spawn[1].slice(1)),["affected","origin/dev","--parallel","5"]);
	assert.equal(env.env.RV_GATE_REEXEC,"1");
});

test("prelaunch tool-tests isolate nested harnesses from both inherited phase flags",async t=>{
	const dir=mkdtempSync(join(tmpdir(),"gate-tool-phase-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
	const names=["RV_GATE_PARTIAL_RETRY","RV_GATE_PRELAUNCH_CHROMIUM_ONLY","RICHVIEW_VERIFY_BROWSER"],before=Object.fromEntries(names.map(name=>[name,process.env[name]]));
	t.after(()=>{for(const name of names)if(before[name]===undefined)delete process.env[name];else process.env[name]=before[name];});
	const testContext=process.env.NODE_TEST_CONTEXT;t.after(()=>{if(testContext===undefined)delete process.env.NODE_TEST_CONTEXT;else process.env.NODE_TEST_CONTEXT=testContext;});delete process.env.NODE_TEST_CONTEXT;
	process.env.RV_GATE_PARTIAL_RETRY="1";process.env.RV_GATE_PRELAUNCH_CHROMIUM_ONLY="1";process.env.RICHVIEW_VERIFY_BROWSER="webkit";
	const root=new URL("../../..",import.meta.url).pathname,shards=new URL("./shards.test.mts",import.meta.url).pathname;
	const assertClean=`for(const name of ${JSON.stringify(names)})if(Object.hasOwn(process.env,name))throw Error(name+' leaked')`;
	const step={id:"tool-tests",title:"tool tests",tier:"full" as const,cards:[],run:[{cmd:process.execPath,args:["-e",assertClean]},{cmd:process.execPath,args:["--test","--test-name-pattern=real child shards",shards]}]};
	const result=(await runner([step],dir)({cwd:root,runDir:dir,mode:"full",phasePolicy:{phase:"prelaunch",since:"2026-10-30",by:"Daniel",rotation:[],prelaunchFull:{browsers:["chromium"],partialRetry:true}}},"full"))[0];
	assert.equal(result.status,"pass",JSON.stringify(result));assert.equal(result.details.commands.length,2);
	const shardLog = result.logs.find(path=>path.endsWith("tool-tests-1.log"));assert.ok(shardLog);
	assert.match(readFileSync(shardLog,"utf8"),/real child shards produce complete canonical evidence/);
});

test("prelaunch verification, journeys and perf retain their Chrome-only flags; launch strips inherited retry",async t=>{
	const dir=mkdtempSync(join(tmpdir(),"gate-phase-consumers-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
	const names=["RV_GATE_PARTIAL_RETRY","RV_GATE_PRELAUNCH_CHROMIUM_ONLY","RICHVIEW_VERIFY_BROWSER"],before=Object.fromEntries(names.map(name=>[name,process.env[name]]));
	t.after(()=>{for(const name of names)if(before[name]===undefined)delete process.env[name];else process.env[name]=before[name];});
	process.env.RV_GATE_PARTIAL_RETRY="1";process.env.RV_GATE_PRELAUNCH_CHROMIUM_ONLY="1";process.env.RICHVIEW_VERIFY_BROWSER="webkit";
	for(const [phase,mode] of [["prelaunch","full"],["launch","full"],["prelaunch","prod"]]){
		const isPartial=phase==="prelaunch"&&mode==="full",expected=isPartial?{retry:"1",chromeOnly:"1",browser:"chromium"}:{retry:null,chromeOnly:"0",browser:null};
		const script=`const assert=require('node:assert/strict');assert.deepEqual({retry:process.env.RV_GATE_PARTIAL_RETRY??null,chromeOnly:process.env.RV_GATE_PRELAUNCH_CHROMIUM_ONLY??null,browser:process.env.RICHVIEW_VERIFY_BROWSER??null},${JSON.stringify(expected)});`;
		for(const id of ["journeys","journeys-full","perf","chromium"]){
			const step={id,title:id,tier:"full" as const,cards:[],run:[{cmd:process.execPath,args:["-e",script]}]};
			const result=(await runner([step],dir)({cwd:dir,runDir:dir,mode,phasePolicy:{phase,since:"2026-10-30",by:"Daniel",rotation:[],prelaunchFull:{browsers:["chromium"],partialRetry:true}}},"full"))[0];
			assert.equal(result.status,"pass",`${phase}/${mode}/${id}: ${JSON.stringify(result)}`);
		}
	}
});

test("affected runner records only the cases validated from the selected command",async t=>{
	const {BROWSER_CASES}=await import("../lib/browser-cases.mts");
	const dir=mkdtempSync(join(tmpdir(),"gate-case-runner-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
	const marker=join(dir,"peer"),script=`const fs=require('node:fs'),path=require('node:path');const out=process.argv[process.argv.indexOf('--out')+1],wanted=JSON.parse(process.env.RICHVIEW_VERIFY_CASES),cohort=${JSON.stringify(BROWSER_CASES)},selected=cohort.filter(key=>wanted.includes(key));fs.writeFileSync(path.join(out,'browser.partial.json'),JSON.stringify({status:'passed',engine:'chromium',errors:[],case_scope:{cohort,selected},completed:selected}));`;
	const step={id:"browser",title:"browser",tier:"full" as const,cards:[],parallel:2,run:[{cmd:process.execPath,args:["-e",script,"--"]},{cmd:process.execPath,args:["-e",`require('node:fs').writeFileSync(${JSON.stringify(marker)},'ran')`]}]};
	const command=commandId(step,step.run[0]),id=`${command}:case:landing`,selection=[{id,commandId:command,step:"browser",index:0,caseKind:"browser",caseKey:"landing-hero",status:"run"}];
	const result=(await runner([step],dir)({cwd:dir,runDir:dir,mode:"full",phasePolicy:{phase:"prelaunch",since:"2026-10-30",by:"Daniel",rotation:[],prelaunchFull:{browsers:["chromium"],partialRetry:true}},affectedSelection:selection},"full"))[0];
	assert.equal(result.status,"pass",JSON.stringify(result));assert.deepEqual(JSON.parse(JSON.stringify(result.units)),[{id,status:"pass"}]);assert.equal(fs.existsSync(marker),false);
	assert.deepEqual(JSON.parse(JSON.stringify(result.details.commands[0].cases.selected)),["landing-hero"]);
});

test("source-selected browser cases execute and enter the receipt without covering their peers",async t=>{
	const {BROWSER_CASES}=await import("../lib/browser-cases.mts"),{caseCatalog,runtimeInventory}=await import("../lib/case-selection.mts");
	const {selectAffected}=await import("../lib/selection.mts"),{outcomesForSelection,validateAffectedHistory}=await import("../lib/affected-coverage.mts");
	const dir=mkdtempSync(join(tmpdir(),"gate-case-e2e-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
	const script=`const fs=require('node:fs'),path=require('node:path'),out=process.argv[process.argv.indexOf('--out')+1],wanted=JSON.parse(process.env.RICHVIEW_VERIFY_CASES),cohort=${JSON.stringify(BROWSER_CASES)},selected=cohort.filter(key=>wanted.includes(key));fs.writeFileSync(path.join(out,'browser.partial.json'),JSON.stringify({status:'passed',engine:'chromium',errors:[],case_scope:{cohort,selected},completed:selected}));`;
	const step={id:"chromium",title:"chromium",tier:"full" as const,cards:[],parallel:2,run:[{cmd:process.execPath,args:["-e",script,"--"],entry:"scripts/verify-browser.mjs"}]};
	const catalog=caseCatalog([step],runtimeInventory([]),commandId),areas={version:1 as const,areas:[{name:"hero",paths:["assets/landing/hero/"],steps:["chromium"]}]};
	const selection=selectAffected([step],["assets/landing/hero/pip.avif"],Object.assign({areas,catalog,read:()=>null}));
	const results=await runner([step],dir)({cwd:dir,runDir:dir,mode:"full",phasePolicy:{phase:"prelaunch",since:"2026-10-30",by:"Daniel",rotation:[],prelaunchFull:{browsers:["chromium"],partialRetry:true}},affectedSelection:selection},"full");
	const outcomes=outcomesForSelection(selection,results,"pass"),chosen=selection.filter(item=>item.status==="run");
	assert.deepEqual(chosen.map(item=>item.caseKey),["landing-deferred","landing-hero","browser-journey"]);
	assert.ok(chosen.every(item=>outcomes[item.id]==="pass"));
	const omitted=selection.find(item=>item.caseKey==="runtime-loader");assert.ok(omitted);assert.equal(outcomes[omitted.id],undefined);
	const receipt: AffectedReceipt={at:receiptTime,id:"affected-case",kind:"affected",base:"base",commit:"head",tree:"tree",policyHash:"p",selected:Object.keys(outcomes),selection,outcomes,steps:JSON.parse(JSON.stringify(results)),browserRelaunchTelemetry:{status:"pass"},result:"pass"};
	assert.equal(validateAffectedHistory({receipts:[receipt],base:"base",head:"head",policyHash:"p",contains:()=>true,treeOf:()=>"tree"})[0].outcomes[chosen[0].id],"pass");
});

test("affected Rust dispatch passes only selected stable shards to a fresh cohort run",async t=>{
	const {caseUnitId}=await import("../lib/case-selection.mts");
	const {validateAffectedHistory,outcomesForSelection}=await import("../lib/affected-coverage.mts");
	const dir=mkdtempSync(join(tmpdir(),"gate-rust-selected-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
	const step={id:"rust-tests",title:"rust",tier:"full" as const,cards:[],cohort:"rust-debug",run:[{cmd:"cargo",args:["test","--workspace","--no-fail-fast"]}]};
	const key="shard-2-of-4",id=caseUnitId(step.id,key),selection: SelectionItem[]=[{id,step:step.id,index:null,commandId:step.id,caseKind:"rust-cohort",caseKey:key,status:"run"}];
	let chosen: string[] | undefined;
	const results=await runner([step],dir,{cohortApplicable:()=>true,runCohortStep:async(_step: unknown,_ctx: unknown,options: {selected: string[]})=>{chosen=options.selected;return {status:"pass",details:{preparation:{status:"pass"},commands:[0,1,2,3].map(i=>({id:`shard-${i}-of-4`,status:i===2?"pass":"unrun"}))}};}})({cwd:dir,runDir:dir,mode:"full",phasePolicy:{phase:"prelaunch",since:"2026-10-30",by:"Daniel",rotation:[],prelaunchFull:{browsers:["chromium"],partialRetry:true}},affectedSelection:selection},"full");
	assert.ok(chosen);
	assert.deepEqual(Array.from(chosen),[key]);assert.deepEqual(JSON.parse(JSON.stringify(results[0].units)),[{id,status:"pass"}]);
	const outcomes=outcomesForSelection(selection,results,"pass"),receipt: AffectedReceipt={at:receiptTime,id:"affected-rust",kind:"affected",base:"base",commit:"head",tree:"tree",policyHash:"p",selected:Object.keys(outcomes),selection,outcomes,steps:JSON.parse(JSON.stringify(results)),browserRelaunchTelemetry:{status:"pass"},result:"pass"};
	assert.equal(validateAffectedHistory({receipts:[receipt],base:"base",head:"head",policyHash:"p",contains:()=>true,treeOf:()=>"tree"})[0].outcomes[id],"pass");
});

test("full, affected and candidate runs ignore ambient source and browser overrides in children and built-ins",async t=>{
	const dir=mkdtempSync(join(tmpdir(),"gate-source-env-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
	const names=["RICHVIEW_CLI","RICHVIEW_BIN","RICHVIEW_DIST","RICHVIEW_VERIFY_URL","RICHVIEW_VERIFY_PUBLIC_DIR","RICHVIEW_PUBLIC_DIR","PUBLIC_DIR","RICHVIEW_VERIFY_NO_HEADERS","CHROMIUM_PATH","RICHVIEW_CHROMIUM","RICHVIEW_BROWSER","RICHVIEW_VERIFY_BROWSER","RICHVIEW_VERIFY_WORKERS"];
	assert.deepEqual([...SOURCE_OVERRIDES].sort(),[...names].sort());
	const before=Object.fromEntries(names.map(name=>[name,process.env[name]]));
	t.after(()=>{for(const name of names)if(before[name]===undefined)delete process.env[name];else process.env[name]=before[name];});
	for(const name of names)process.env[name]=name==="RICHVIEW_VERIFY_BROWSER"?"webkit":`probe-${name}`;
	for(const [phase,mode,affected] of [["prelaunch","full",false],["prelaunch","full",true],["launch","full",false],["prelaunch","prod",false]]){
		const expected=Object.fromEntries(names.map(name=>[name,name==="RICHVIEW_VERIFY_BROWSER"&&phase==="prelaunch"&&mode==="full"?"chromium":null]));
		const script=`const assert=require('node:assert/strict'),names=${JSON.stringify(names)};assert.deepEqual(Object.fromEntries(names.map(name=>[name,process.env[name]??null])),${JSON.stringify(expected)});`;
		const built={id:"built",title:"built",tier:"full" as const,cards:[],fn:"probe"},child={id:"child",title:"child",tier:"full" as const,cards:[],run:[{cmd:process.execPath,args:["-e",script]}]};
		const chosen=affected?[{id:"built",step:"built",status:"run"},{id:"child",step:"child",status:"run"}]:undefined;
		const checksOverride={...checks,probe:()=>{for(const name of names)assert.equal(process.env[name],undefined,`built-in inherited ${name}`);return {status:"pass"};}};
		const results=await runner([built,child],dir,{checks:checksOverride})({cwd:dir,runDir:dir,mode,phasePolicy:{phase,since:"2026-10-30",by:"Daniel",rotation:[],prelaunchFull:{browsers:["chromium"],partialRetry:true}},...(chosen&&{affectedSelection:chosen})},"full");
		assert.deepEqual(Array.from(results,item=>item.status),["pass","pass"],`${phase}/${mode}/${affected}: ${JSON.stringify(results)}`);
		for(const name of names)assert.equal(process.env[name],name==="RICHVIEW_VERIFY_BROWSER"?"webkit":`probe-${name}`,`${name} restored`);
	}
});

test("a declared launch engine and worker budget survive ambient override removal",async t=>{
	const dir=mkdtempSync(join(tmpdir(),"gate-source-declared-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
	const before=process.env.RICHVIEW_VERIFY_BROWSER;process.env.RICHVIEW_VERIFY_BROWSER="webkit";t.after(()=>{if(before===undefined)delete process.env.RICHVIEW_VERIFY_BROWSER;else process.env.RICHVIEW_VERIFY_BROWSER=before;});
	const script=`const assert=require('node:assert/strict');assert.equal(process.env.RICHVIEW_VERIFY_BROWSER,'firefox');assert.equal(process.env.RICHVIEW_VERIFY_WORKERS,'2');`;
	const step={id:"engine",title:"engine",tier:"full" as const,cards:[],run:[{cmd:process.execPath,args:["-e",script],env:{RICHVIEW_VERIFY_BROWSER:"firefox",RICHVIEW_VERIFY_WORKERS:"2"}}]};
	const result=(await runner([step],dir)({cwd:dir,runDir:dir,mode:"full",phasePolicy:{phase:"launch",since:"2026-10-30",by:"Daniel",rotation:[]}},"full"))[0];
	assert.equal(result.status,"pass",JSON.stringify(result));
});

test("source override cleanup restores the calling environment even on error and leaves local tiers alone",async()=>{
	const before=process.env.RICHVIEW_CLI;process.env.RICHVIEW_CLI="probe-cli";
	try{
		await assert.rejects(withGateSourceEnvironment("full",async()=>{assert.equal(process.env.RICHVIEW_CLI,undefined);throw Error("failure");}),/failure/);
		assert.equal(process.env.RICHVIEW_CLI,"probe-cli");
		await withGateSourceEnvironment("dev",async()=>assert.equal(process.env.RICHVIEW_CLI,"probe-cli"));
	}finally{if(before===undefined)delete process.env.RICHVIEW_CLI;else process.env.RICHVIEW_CLI=before;}
});

test("release entry cleans ambient sources for its actual step environment and in-process checks",async t=>{
	const before=Object.fromEntries(SOURCE_OVERRIDES.map(name=>[name,process.env[name]]));
	t.after(()=>{for(const name of SOURCE_OVERRIDES)if(before[name]===undefined)delete process.env[name];else process.env[name]=before[name];});
	for(const name of SOURCE_OVERRIDES)process.env[name]=`probe-${name}`;
	const start=source.indexOf("async function releaseRun("),end=source.indexOf("async function releaseRunWithSourceEnvironment(",start);
	const stepStart=source.indexOf("async function runStep("),stepEnd=source.indexOf("\n/** A Cargo step",stepStart);
	assert.ok(start>=0&&end>start&&stepStart>=0&&stepEnd>stepStart);let called=0;
	const assertClean=(env: NodeJS.ProcessEnv)=>{for(const name of SOURCE_OVERRIDES)assert.equal(env[name],undefined,`release step inherited ${name}`);};
	const context=vm.createContext({withGateSourceEnvironment,executionMeter,withRerun,archiveAttempt,archiveBuildSnapshot,freshCheck,SHARED_TARGET:"/target",special:{},console:{log:()=>{}},line:()=>"",
		checks:{probe:()=>{assertClean(process.env);return {status:"pass"};}},runCommands:async(_step: unknown,_ctx: unknown,env: NodeJS.ProcessEnv)=>{assertClean(env);return {status:"pass"};},releaseRunWithSourceEnvironment:async (args: {marker: string})=>{
		called++;assert.equal(args.marker,"release");
		const env=gateEnv(),ctx={mode:"release",phasePolicy:{phase:"launch",rotation:[]}};
		assertClean(env);assertClean(process.env);
		assert.equal((await context.step({id:"builtin",fn:"probe"},ctx,env)).status,"pass");
		assert.equal((await context.step({id:"child",run:[{cmd:"fake",args:[]}]},ctx,env)).status,"pass");
		return "ok";
	}});
	vm.runInContext(source.slice(stepStart,stepEnd)+source.slice(start,end)+"\nglobalThis.execute=releaseRun;globalThis.step=runStep;",context);
	assert.equal(await context.execute({marker:"release"}),"ok");assert.equal(called,1);
	for(const name of SOURCE_OVERRIDES)assert.equal(process.env[name],`probe-${name}`,`${name} restored`);
});

test("the actual release built-in retry retains the legacy worker path even with an AbortSignal",async t=>{
	const dir=mkdtempSync(join(tmpdir(),"gate-release-retry-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
	const stepStart=source.indexOf("async function runStep("),stepEnd=source.indexOf("\n/** A Cargo step",stepStart);
	const controller=new AbortController(),ctx: WorkerContext={cwd:dir,runDir:dir,mode:"release",signal:controller.signal,phasePolicy:{phase:"prelaunch"}};
	let firstCalls=0;
	const globals={executionMeter,withRerun,archiveAttempt,archiveBuildSnapshot,freshCheck,SHARED_TARGET:dir,special:{},checks:{prFields:()=>{firstCalls++;return {status:"fail",reason:"First attempt"};}},console:{log:()=>{}},line:()=>""};
	const context=vm.createContext(globals);vm.runInContext(source.slice(stepStart,stepEnd)+"\nglobalThis.execute=runStep;",context);
	const result=await context.execute({id:"pr-fields",fn:"prFields",cards:[]},ctx,{});
	assert.equal(firstCalls,1);assert.equal(result.status,"fail");assert.equal(result.attempts.length,2);
	assert.match(result.attempts[1].reason,/No PR context/);
	assert.equal(fs.existsSync(join(dir,"attempts/pr-fields/2/groups.json")),false);
	assert.equal(ctx.unknownWorkerDrains,undefined);
});
