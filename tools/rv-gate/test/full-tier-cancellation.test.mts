// Host reason (ARC-03): exercise fullTier's actual partial-receipt boundary with fake work.
import test from "node:test";
import assert from "node:assert/strict";
import {cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, renameSync, rmSync, writeFileSync} from "node:fs";
import {spawn} from "node:child_process";
import {createHash} from "node:crypto";
import {setTimeout as delay} from "node:timers/promises";
import {join} from "node:path";
import {pathToFileURL} from "node:url";
import vm from "node:vm";
import {withRunCancellation} from "../lib/cancellation.mts";
import {outcomesForSelection, validateAffectedHistory, outstandingCoverage} from "../lib/affected-coverage.mts";
import type { AffectedReceipt as ValidatedAffectedReceipt } from "../lib/affected-coverage.mts";
import * as affectedCoverage from "../lib/affected-coverage.mts";
import {ensureStore, readReceipt, writeReceipt, type StoredReceipt} from "../lib/store.mts";
import {readPhase} from "../lib/phase.mts";
import {waitForWorkerGroups, type BlockedWorker, type WorkerContext} from "../lib/check-process.mts";
import {snapshotOwnedGroup} from "../lib/proc.mts";
import {runFullSteps} from "../lib/full.mts";
import type { GateStatus } from "../lib/results.mts";
import { readTestSource } from "./test-source.mts";

const source=readTestSource(new URL("../rv-gate.mts",import.meta.url));
const start=source.indexOf("async function fullTier("),end=source.indexOf("\nfunction cutCandidate(",start);
assert.ok(start>=0 && end>start);

type AffectedReceipt = ValidatedAffectedReceipt & {
	base: string;
	preProdAtStart: string;
	greenReceipt: string;
	outcomes: Record<string, GateStatus>;
	cancelled?: boolean;
	cancellationSignal?: NodeJS.Signals;
	operatorBlocked?: boolean;
	browserRelaunchTelemetry: { status: GateStatus; reason?: string };
};

function fullTierInVm(globals: Record<string, unknown>) {
	const context = vm.createContext(globals);
	vm.runInContext(source.slice(start, end) + "\nglobalThis.execute=fullTier;", context);
	// The shipping function installed by the slice returns the affected receipt under test.
	const execute: (head: string, options: { parallel: number }, mode: { affected: true }) => Promise<{ receipt: AffectedReceipt }> = context.execute;
	return execute;
}

test("judgment pre-prod keeps the genuinely green coverage base and earlier failures", async t => {
	const scratch=join(process.cwd(),"tmp");mkdirSync(scratch,{recursive:true});const dir=mkdtempSync(join(scratch,"gate-green-base-"));
	const previous=process.env.RV_GATE_HOME;process.env.RV_GATE_HOME=dir;
	t.after(()=>{if(previous===undefined)delete process.env.RV_GATE_HOME;else process.env.RV_GATE_HOME=previous;rmSync(dir,{recursive:true,force:true});});
	const base="a".repeat(40),preProd="b".repeat(40),head="c".repeat(40),treeOf=(commit: string)=>({[base]:"d".repeat(40),[preProd]:"e".repeat(40),[head]:"f".repeat(40)})[commit];
	const order=[base,preProd,head],contains=(later: string,earlier: string)=>order.indexOf(later)>=order.indexOf(earlier);
	const green={id:"full-green",kind:"full",commit:base,tree:treeOf(base),result:"pass"};
	const red={id:"affected-red",kind:"affected",base,commit:preProd,tree:treeOf(preProd),policyHash:"policy",selected:["pins"],outcomes:{pins:"fail"},steps:[{id:"pins",status:"fail"}],result:"fail"};
	const notes=(id: string)=>[{kind:id===green.id?"promotion":"lead-judgment-promotion",commit:id===green.id?base:preProd,tree:treeOf(id===green.id?base:preProd)}];
	const plans: { base: string; history: (typeof red)[] }[] = [];
	const globals={withRunCancellation,readPhase,fullStepGroups:()=>{},ensureStore,acquireLock:()=>()=>{},workCheckout:()=>dir,
		gitIn:(_cwd: string,command: string,ref: string)=>command==="rev-parse"?(String(ref).endsWith("^{tree}")?treeOf(ref.slice(0,-7)):ref==="origin/prod"?base:head):"",
		spawnSync:(_command: string,args: string[])=>({status:contains(args[3],args[2])?0:1}),
		prepareCheckout:()=>{},newReceiptId:()=>"affected-test-judgment-base",makeRunDir:()=>dir,cleanCheckoutTmp:()=>{},runTemp:()=>dir,sourceChanges:()=>[],batchFields:()=>({}),readDev:()=>({base:preProd}),
		fullReceipts:function*(){yield green;},affectedReceipts:function*(){yield red;},requireGreenBase:affectedCoverage.requireGreenBase,lastGreenBase:affectedCoverage.lastGreenBase,receiptNotes:notes,stepsForTier:()=>[],
		readFileSync,join,here:new URL("..",import.meta.url).pathname,failureCounts:()=>new Map(),policyFingerprint:()=>"policy",
		affectedContext:(input: (typeof plans)[number])=>{plans.push(input);return {selection:[{id:"build",step:"build",status:"run"},{id:"pins",step:"pins",status:"run"}],pending:[],commits:[],obligations:[]};},
		holdRelease:async()=>()=>{},SHARED_TARGET:dir,withBrowserEvents:async(_path: string,run: () => unknown)=>({results:await run(),browserRelaunches:[],previewReloads:[],browserRelaunchTelemetry:{status:"pass"}}),
		runSteps:async()=>[{id:"build",status:"pass"},{id:"pins",status:"fail"}],outcomesForSelection,writeReceipt,finishRun:()=>{},printNotRun:()=>{},previewReloadLine:()=>"",list:()=>[],console:{log:()=>{}},Date};
	const execute = fullTierInVm(globals);
	const {receipt}=await execute(head,{parallel:1},{affected:true});
	const [planned] = plans;
	assert.ok(planned);
	assert.equal(planned.base,base);assert.equal(planned.history.length,1);assert.equal(planned.history[0],red);
	assert.equal(receipt.base,base);assert.equal(receipt.preProdAtStart,preProd);assert.equal(receipt.greenReceipt,green.id);
	assert.equal(receipt.outcomes.pins,"fail");assert.equal(receipt.result,"fail");
	assert.equal(readReceipt<AffectedReceipt>(receipt.id).base,base);
});

for (const telemetryStatus of ["pass", "fail"]) test(`fullTier writes a cancelled affected receipt with ${telemetryStatus} telemetry`, async t => {
	const scratch=join(process.cwd(),"tmp");mkdirSync(scratch,{recursive:true});const dir=mkdtempSync(join(scratch,"gate-full-cancel-"));
	const previous=process.env.RV_GATE_HOME;process.env.RV_GATE_HOME=dir;
	t.after(()=>{if(previous===undefined)delete process.env.RV_GATE_HOME;else process.env.RV_GATE_HOME=previous;rmSync(dir,{recursive:true,force:true});});
	const commit="a".repeat(40),base="b".repeat(40),tree="c".repeat(40),id=`affected-test-${telemetryStatus}`;
	let cancelled=false,cleaned=false;
	const selection=[{id:"build",step:"build",status:"run"},{id:"suite:failed",step:"suite",status:"run"},{id:"suite:active",step:"suite",status:"run"}];
	const globals={withRunCancellation,readPhase,fullStepGroups:()=>{},ensureStore,acquireLock:()=>()=>{},workCheckout:()=>dir,
		gitIn:(_cwd: string,command: string,ref: string)=>{assert.equal(cancelled,false,"No Git subprocess after cancellation");return command==="rev-parse"?(String(ref).includes("^{tree}")?tree:ref==="origin/prod"?base:commit):"";},
		prepareCheckout:()=>{},newReceiptId:()=>id,makeRunDir:()=>{const run=join(dir,"run");mkdirSync(run,{recursive:true});return run;},
		cleanCheckoutTmp:()=>{},runTemp:()=>join(dir,"temp"),sourceChanges:()=>[],batchFields:()=>({}),readDev:()=>({base}),
		fullReceipts:function*(){},affectedReceipts:function*(){},lastGreenBase:()=>({base,receipt:"green"}),receiptNotes:()=>[],stepsForTier:()=>[],
		readFileSync,join,here:new URL("..",import.meta.url).pathname,failureCounts:()=>new Map(),policyFingerprint:()=>"policy",
		affectedContext:()=>({selection,pending:[],commits:[],obligations:[]}),holdRelease:async()=>()=>{},SHARED_TARGET:dir,
		withBrowserEvents:async(_path: string,run: () => unknown)=>({results:await run(),browserRelaunches:[],previewReloads:[],browserRelaunchTelemetry:{status:telemetryStatus,...(telemetryStatus==="fail"&&{reason:"Malformed browser journal"})}}),
		runSteps:async (ctx: {signal: AbortSignal})=>{process.kill(process.pid,"SIGTERM");await new Promise<void>(resolve=>ctx.signal.addEventListener("abort",()=>resolve(),{once:true}));cancelled=true;
			return [{id:"build",status:"pass"},{id:"suite",status:"unrun",units:[{id:"suite:failed",status:"fail"},{id:"suite:active",status:"unrun"}]}];},
		outcomesForSelection,writeReceipt,finishRun:()=>{cleaned=true;},printNotRun:()=>{},previewReloadLine:()=>"",list:()=>[],console:{log:()=>{}},Date};
	const execute = fullTierInVm(globals);
	const {receipt}=await execute(commit,{parallel:1},{affected:true});const saved=readReceipt<AffectedReceipt>(id);
	assert.equal(cleaned,false,"Cancellation retains run artifacts and avoids housekeeping subprocesses");
	assert.equal(receipt.result,"fail");assert.equal(saved.cancelled,true);assert.equal(saved.cancellationSignal,"SIGTERM");
	assert.equal(saved.outcomes.build,"pass");assert.equal(saved.outcomes["suite:failed"],"fail");assert.equal(saved.outcomes["suite:active"],"unrun");
	assert.equal(saved.outcomes["browser-telemetry"],telemetryStatus==="fail"?"fail":"unrun");
	if(telemetryStatus==="fail")assert.equal(saved.browserRelaunchTelemetry.reason,"Malformed browser journal");
	const history=validateAffectedHistory({receipts:[saved],base,head:commit,policyHash:"new-policy",contains:()=>true,treeOf:()=>tree,allowPolicyChange:true});
	const pending=outstandingCoverage({obligations:[],receipts:history,contains:()=>true,head:undefined});
	assert.ok(pending.some(item=>item.unit==="suite:failed"));
	if(telemetryStatus==="fail")assert.ok(pending.some(item=>item.unit==="browser-telemetry"));
});

test("the managed fast-leader orphan blocks actual fullTier holds until a fresh empty inspection", {timeout:30000}, async t => {
	const scratch=join(process.cwd(),"tmp");mkdirSync(scratch,{recursive:true});const dir=mkdtempSync(join(scratch,"gate-fast-orphan-"));
	const previous=process.env.RV_GATE_HOME,previousPath=process.env.PATH;process.env.RV_GATE_HOME=dir;
	t.after(()=>{process.env.PATH=previousPath;if(previous===undefined)delete process.env.RV_GATE_HOME;else process.env.RV_GATE_HOME=previous;rmSync(dir,{recursive:true,force:true});});
	const lib=join(dir,"lib");cpSync(new URL("../lib/",import.meta.url),lib,{recursive:true});
	const runDir=join(dir,"run"),leaderFile=join(runDir,"leader.pid"),descendantFile=join(runDir,"descendant.pid"),ready=join(runDir,"ready"),hide=join(dir,"hide-unobserved");
	mkdirSync(runDir);writeFileSync(hide,"hide");
	const bin=join(dir,"bin");mkdirSync(bin);
	writeFileSync(join(bin,"ps.cjs"),`const fs=require('node:fs'),{spawnSync}=require('node:child_process');const x=spawnSync('/bin/ps',process.argv.slice(2),{encoding:'utf8'});if(x.status!==0)process.exit(1);let rows=x.stdout.split('\\n');if(fs.existsSync(${JSON.stringify(hide)})&&fs.existsSync(${JSON.stringify(leaderFile)})){const id=Number(fs.readFileSync(${JSON.stringify(leaderFile)},'utf8'));const group=rows.filter(r=>Number(r.trim().split(/\\s+/)[1])===id);if(group.length&&!group.some(r=>Number(r.trim().split(/\\s+/)[0])===id))process.exit(1);rows=rows.filter(r=>{const a=r.trim().split(/\\s+/).map(Number);return a[1]!==id||a[0]===id;});}process.stdout.write(rows.join('\\n'));\n`);
	writeFileSync(join(bin,"ps"),`#!/bin/sh\nexec '${process.execPath}' '${join(bin,"ps.cjs")}' "$@"\n`,{mode:0o755});
	const procFile=join(lib,"proc.mts"),procSource=readFileSync(procFile,"utf8"),psExpression='process.platform === "darwin" ? "/bin/ps" : "/usr/bin/ps"';
	assert.ok(procSource.includes(psExpression));writeFileSync(procFile,procSource.replace(psExpression,JSON.stringify(join(bin,"ps"))));
	process.env.PATH=`${bin}:${previousPath}`;
	const descendant=join(dir,"descendant.mjs"),launcher=join(dir,"launcher.mjs");
	writeFileSync(descendant,`import {writeFileSync} from 'node:fs';writeFileSync(${JSON.stringify(descendantFile)},String(process.pid));setInterval(()=>{},1000);`);
	writeFileSync(launcher,`import {spawn} from 'node:child_process';const c=spawn(process.execPath,[${JSON.stringify(descendant)}],{stdio:'ignore'});c.unref();`);
	writeFileSync(join(lib,"checks.mts"),`import {spawn} from 'node:child_process';import {writeFileSync} from 'node:fs';export async function active(){const c=spawn(process.execPath,[${JSON.stringify(launcher)}],{detached:true,stdio:'ignore'});writeFileSync(${JSON.stringify(leaderFile)},String(c.pid));await new Promise(r=>c.once('close',r));writeFileSync(${JSON.stringify(ready)},'ready');await new Promise(()=>setInterval(()=>{},1000));}`);
	const {freshCheck}=await import(pathToFileURL(join(lib,"check-process.mts")).href);
	const commit="a".repeat(40),base="b".repeat(40),tree="c".repeat(40),id="affected-test-fast-orphan",mergeHold=join(dir,"merge.hold"),releaseHold=join(dir,"release.hold");
	const selection=[{id:"build",step:"build",status:"run"}];let alerts=0,stepStarts=0;
	const globals={withRunCancellation,readPhase,fullStepGroups:()=>{},ensureStore,
		acquireLock:()=>{writeFileSync(mergeHold,"held");return()=>rmSync(mergeHold);},workCheckout:()=>dir,
		gitIn:(_cwd: string,command: string,ref: string)=>command==="rev-parse"?(String(ref).includes("^{tree}")?tree:ref==="origin/prod"?base:commit):"",
		prepareCheckout:()=>{},newReceiptId:()=>id,makeRunDir:()=>runDir,cleanCheckoutTmp:()=>{},runTemp:()=>join(dir,"temp"),sourceChanges:()=>[],batchFields:()=>({}),readDev:()=>({base}),
		fullReceipts:function*(){},affectedReceipts:function*(){},lastGreenBase:()=>({base,receipt:"green"}),receiptNotes:()=>[],stepsForTier:()=>[],
		readFileSync,writeFileSync,renameSync,join,here:new URL("..",import.meta.url).pathname,failureCounts:()=>new Map(),policyFingerprint:()=>"policy",
		affectedContext:()=>({selection,pending:[],commits:[],obligations:[]}),holdRelease:async()=>{writeFileSync(releaseHold,"held");return()=>rmSync(releaseHold);},SHARED_TARGET:dir,
		withBrowserEvents:async(_path: string,run: () => unknown)=>({results:await run(),browserRelaunches:[],previewReloads:[],browserRelaunchTelemetry:{status:"pass"}}),
		runSteps:(ctx: WorkerContext)=>runFullSteps([{id:"build",fn:"active",tier:"full",title:"Fixture",cards:[]}],step=>{stepStarts++;return freshCheck(step,ctx,dir,1);},1,undefined,{signal:ctx.signal}),
		waitForWorkerGroups:(records: readonly BlockedWorker[],options: Parameters<typeof waitForWorkerGroups>[1])=>waitForWorkerGroups(records,{...options,intervalMs:25}),alertUnresolvedWorkers:()=>{assert.ok(existsSync(join(runDir,"operator-blocked.json")));alerts++;},
		outcomesForSelection,writeReceipt,finishRun:()=>{},printNotRun:()=>{},previewReloadLine:()=>"",list:()=>[],console:{log:()=>{}},Date};
	const execute = fullTierInVm(globals);
	let leader: number | undefined, child: number | undefined;
	try {
		const running=execute(commit,{parallel:1},{affected:true});
		for(let i=0;i<600&&(!existsSync(ready)||!existsSync(descendantFile));i++)await delay(10);
		assert.ok(existsSync(ready)&&existsSync(descendantFile),"Fast launcher and its orphan must start");
		leader=Number(readFileSync(leaderFile,"utf8"));child=Number(readFileSync(descendantFile,"utf8"));rmSync(hide);
		process.kill(process.pid,"SIGTERM");
		const diagnostic=join(runDir,"operator-blocked.json");for(let i=0;i<1800&&!existsSync(diagnostic);i++)await delay(10);
		assert.ok(existsSync(diagnostic),"The managed gate must persist an unresolved diagnostic");
		const receipt=readReceipt<AffectedReceipt>(id);assert.equal(receipt.result,"fail");assert.equal(receipt.operatorBlocked,true);assert.equal(receipt.outcomes.build,"unrun");
		assert.ok(existsSync(mergeHold)&&existsSync(releaseHold));assert.equal(alerts,1);assert.equal(stepStarts,1);
		const orphan = child;
		assert.doesNotThrow(()=>process.kill(orphan,0),"An unverified orphan must not be signalled");
		process.kill(process.pid,"SIGTERM");await delay(50);assert.ok(existsSync(mergeHold)&&existsSync(releaseHold));assert.equal(alerts,1);
		process.kill(-leader,"SIGKILL");await running;
		assert.equal(existsSync(mergeHold),false);assert.equal(existsSync(releaseHold),false);
	} finally {
		process.env.PATH=previousPath;
		if(leader)try{process.kill(-leader,"SIGKILL");}catch(error){if((error as NodeJS.ErrnoException).code!=="ESRCH")throw error;}
		if(child)try{process.kill(child,"SIGKILL");}catch(error){if((error as NodeJS.ErrnoException).code!=="ESRCH")throw error;}
	}
});

for (const uncertainty of ["identity mismatch", "inspection failure"]) test(`${uncertainty} retains live gate and disposable holds until verified empty`, {timeout:15000}, async t => {
	const scratch=join(process.cwd(),"tmp");mkdirSync(scratch,{recursive:true});const dir=mkdtempSync(join(scratch,"gate-blocked-"));
	const previous=process.env.RV_GATE_HOME,previousPath=process.env.PATH;process.env.RV_GATE_HOME=dir;
	t.after(()=>{process.env.PATH=previousPath;if(previous===undefined)delete process.env.RV_GATE_HOME;else process.env.RV_GATE_HOME=previous;rmSync(dir,{recursive:true,force:true});});
	const unrelated=spawn(process.execPath,["-e","setInterval(()=>{},1000)"],{detached:true,stdio:"ignore"});
	const closed=new Promise(resolve=>unrelated.once("close",resolve));await new Promise(resolve=>unrelated.once("spawn",resolve));
	const pid = unrelated.pid;
	assert.ok(pid);
	const identity=snapshotOwnedGroup(pid).members.find(member=>member.pid===pid);assert.ok(identity);
	const groups=join(dir,"groups.json");writeFileSync(groups,JSON.stringify([{pgid:pid,identities:[uncertainty==="identity mismatch"?{...identity,start:identity.start+" old"}:identity]}]));
	let inspectWait=waitForWorkerGroups, fakePs: string | undefined;
	if(uncertainty==="inspection failure"){
		const lib=join(dir,"lib");cpSync(new URL("../lib/",import.meta.url),lib,{recursive:true});fakePs=join(dir,"inspection-ps");
		writeFileSync(fakePs,"#!/bin/sh\nexit 1\n",{mode:0o755});
		const procFile=join(lib,"proc.mts"),procSource=readFileSync(procFile,"utf8"),psExpression='process.platform === "darwin" ? "/bin/ps" : "/usr/bin/ps"';
		assert.ok(procSource.includes(psExpression));writeFileSync(procFile,procSource.replace(psExpression,JSON.stringify(fakePs)));
		inspectWait=(await import(pathToFileURL(join(lib,"check-process.mts")).href)).waitForWorkerGroups;
	}
	const commit="a".repeat(40),base="b".repeat(40),tree="c".repeat(40),id="affected-test-blocked",mergeHold=join(dir,"merge.hold"),releaseHold=join(dir,"release.hold");
	const selection=[{id:"build",step:"build",status:"run"}];let alerts=0,stepStarts=0;
	const globals={withRunCancellation,readPhase,fullStepGroups:()=>{},ensureStore,
		acquireLock:()=>{writeFileSync(mergeHold,"held");return()=>rmSync(mergeHold);},workCheckout:()=>dir,
		gitIn:(_cwd: string,command: string,ref: string)=>command==="rev-parse"?(String(ref).includes("^{tree}")?tree:ref==="origin/prod"?base:commit):"",
		prepareCheckout:()=>{},newReceiptId:()=>id,makeRunDir:()=>{const run=join(dir,"run");mkdirSync(run,{recursive:true});return run;},
		cleanCheckoutTmp:()=>{},runTemp:()=>join(dir,"temp"),sourceChanges:()=>[],batchFields:()=>({}),readDev:()=>({base}),
		fullReceipts:function*(){},affectedReceipts:function*(){},lastGreenBase:()=>({base,receipt:"green"}),receiptNotes:()=>[],stepsForTier:()=>[],
		readFileSync,writeFileSync,renameSync,join,here:new URL("..",import.meta.url).pathname,failureCounts:()=>new Map(),policyFingerprint:()=>"policy",
		affectedContext:()=>({selection,pending:[],commits:[],obligations:[]}),holdRelease:async()=>{writeFileSync(releaseHold,"held");return()=>rmSync(releaseHold);},SHARED_TARGET:dir,
		withBrowserEvents:async(_path: string,run: () => unknown)=>({results:await run(),browserRelaunches:[],previewReloads:[],browserRelaunchTelemetry:{status:"pass"}}),
		runSteps:async (ctx: { unknownWorkerDrains: { file: string; reason: string; ownershipHash: string; }[]; cancel: () => void; })=>{stepStarts++;ctx.unknownWorkerDrains=[{file:groups,reason:uncertainty,ownershipHash:createHash("sha256").update(readFileSync(groups)).digest("hex")}];ctx.cancel();return [{id:"build",status:"unrun"}];},
		waitForWorkerGroups:(records: readonly BlockedWorker[],options: Parameters<typeof waitForWorkerGroups>[1])=>inspectWait(records,{...options,intervalMs:25}),alertUnresolvedWorkers:()=>{assert.ok(existsSync(join(dir,"run/operator-blocked.json")));alerts++;},
		outcomesForSelection,writeReceipt,finishRun:()=>{},printNotRun:()=>{},previewReloadLine:()=>"",list:()=>[],console:{log:()=>{}},Date};
	const execute = fullTierInVm(globals);
	try {
		const running=execute(commit,{parallel:1},{affected:true});
		const diagnostic=join(dir,"run/operator-blocked.json");for(let i=0;i<100&&!existsSync(diagnostic);i++)await delay(10);
		assert.ok(existsSync(diagnostic));assert.equal(readReceipt<AffectedReceipt>(id).result,"fail");assert.equal(readReceipt<AffectedReceipt>(id).operatorBlocked,true);
		assert.ok(existsSync(mergeHold)&&existsSync(releaseHold));assert.equal(alerts,1);assert.equal(stepStarts,1);
		process.kill(process.pid,"SIGTERM");process.kill(process.pid,"SIGTERM");await delay(100);
		assert.ok(existsSync(mergeHold)&&existsSync(releaseHold));assert.equal(alerts,1);assert.equal(stepStarts,1);
		assert.doesNotThrow(()=>process.kill(pid,0),"Unknown identity or inspection must never signal the group");
		if(uncertainty==="inspection failure"){assert.ok(fakePs);writeFileSync(fakePs,'#!/bin/sh\nexec /bin/ps "$@"\n',{mode:0o755});}
		else process.kill(-pid,"SIGKILL");
		await closed;await running;
		assert.equal(existsSync(mergeHold),false);assert.equal(existsSync(releaseHold),false);
		assert.equal(JSON.parse(readFileSync(diagnostic,"utf8")).unpromotable,true);
	} finally {process.env.PATH=previousPath;try{process.kill(-pid,"SIGKILL");}catch(error){if((error as NodeJS.ErrnoException).code!=="ESRCH")throw error;}await closed;}
});
