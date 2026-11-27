// Host reason (ARC-03): exercise cancellation of gate-owned fake processes without a browser or Cargo.
import test from "node:test";
import assert from "node:assert/strict";
import {spawn} from "node:child_process";
import {cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync} from "node:fs";
import {join} from "node:path";
import {pathToFileURL} from "node:url";
import {setTimeout as delay} from "node:timers/promises";
import {createHash} from "node:crypto";
import {runLogged, snapshotOwnedGroup, classifyOwnedGroup, signalOwnedGroup} from "../lib/proc.mts";
import {validateAffectedHistory} from "../lib/affected-coverage.mts";
import {affectedPromotion} from "../lib/affected-promotion.mts";
import {waitUntilIdle} from "../lib/target.mts";
import {drainWorkerGroups} from "../lib/check-process.mts";

test("a cancelled Cargo-idle wait stops polling promptly", async () => {
	const controller = new AbortController(); let polls = 0;
	const waiting = waitUntilIdle(() => ++polls === 1 ? [42] : [], {every: 50, signal: controller.signal});
	controller.abort();
	await assert.rejects(waiting, /abort/i);
	assert.equal(polls, 1);
});

test("an already cancelled command creates neither a process nor a log", async t => {
	const scratch = join(process.cwd(), "tmp"); mkdirSync(scratch, {recursive: true});
	const dir = mkdtempSync(join(scratch, "gate-cancel-command-")); t.after(() => rmSync(dir, {recursive: true, force: true}));
	const marker = join(dir, "ran"), log = join(dir, "log"), controller = new AbortController(); controller.abort();
	const result = await runLogged(process.execPath, ["-e", `require('node:fs').writeFileSync(${JSON.stringify(marker)},'ran')`], {logPath: log, signal: controller.signal});
	assert.equal(result.status, "unrun"); assert.equal(existsSync(marker), false); assert.equal(existsSync(log), false);
});

test("unknown inspection and recycled identities never authorize a group signal", async () => {
	assert.equal(classifyOwnedGroup({status:"unknown",members:[]},[{pid:1,start:"old"}]),"unknown");
	assert.equal(classifyOwnedGroup({status:"known",members:[]},[{pid:1,start:"old"}]),"empty");
	assert.equal(classifyOwnedGroup({status:"known",members:[{pid:1,start:"new"}]},[{pid:1,start:"old"}]),"unknown");
	const child=spawn(process.execPath,["-e","setInterval(()=>{},1000)"],{detached:true,stdio:"ignore"});
	const closed=new Promise(resolve=>child.once("close",resolve));
	try {
		await new Promise(resolve=>child.once("spawn",resolve));
		const pid=child.pid;assert.ok(pid);
		const snapshot=snapshotOwnedGroup(pid);assert.equal(snapshot.status,"known");
		const identity=snapshot.members.find(member=>member.pid===child.pid);assert.ok(identity);
		assert.equal(signalOwnedGroup(pid,[{...identity,start:identity.start+" changed"}]),"unknown");
		assert.doesNotThrow(()=>process.kill(pid,0));
		assert.equal(signalOwnedGroup(pid,[identity]),"signalled");await closed;
		assert.equal(classifyOwnedGroup(snapshotOwnedGroup(pid),[identity]),"empty");
	} finally {try{if(child.pid)process.kill(-child.pid,"SIGKILL");}catch(error){if((error as NodeJS.ErrnoException).code!=="ESRCH")throw error;}await closed;}
});

test("changed or missing persisted ownership cannot authorize a group signal", async t => {
	const scratch=join(process.cwd(),"tmp");mkdirSync(scratch,{recursive:true});const dir=mkdtempSync(join(scratch,"gate-owner-integrity-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
	const child=spawn(process.execPath,["-e","setInterval(()=>{},1000)"],{detached:true,stdio:"ignore"});const closed=new Promise(resolve=>child.once("close",resolve));
	try {
		await new Promise(resolve=>child.once("spawn",resolve));const pid=child.pid;assert.ok(pid);const identity=snapshotOwnedGroup(pid).members.find(member=>member.pid===child.pid);assert.ok(identity);
		const file=join(dir,"groups.json"),old=JSON.stringify([{pgid:child.pid,identities:[{...identity,start:identity.start+" old"}]}]);writeFileSync(file,old);
		const ownershipHash=createHash("sha256").update(old).digest("hex");
		writeFileSync(file,JSON.stringify([{pgid:child.pid,identities:[identity]}]));
		assert.equal((await drainWorkerGroups(file,{ownershipHash,timeoutMs:0})).status,"unknown");
		assert.equal((await drainWorkerGroups(file,{ownershipHash:null,timeoutMs:0})).status,"unknown");
		assert.doesNotThrow(()=>process.kill(pid,0));
	} finally {try{if(child.pid)process.kill(-child.pid,"SIGKILL");}catch(error){if((error as NodeJS.ErrnoException).code!=="ESRCH")throw error;}await closed;}
});

for (const how of ["signal", "worker-exit"]) test(`verified group ownership survives leader close and ${how}`, {timeout:15000}, async t => {
	const scratch=join(process.cwd(),"tmp");mkdirSync(scratch,{recursive:true});const dir=mkdtempSync(join(scratch,"gate-group-owner-"));
	t.after(()=>rmSync(dir,{recursive:true,force:true}));
	const lib=join(dir,"lib");cpSync(new URL("../lib/",import.meta.url),lib,{recursive:true});
	const descendant=join(dir,"descendant.mjs"),launcher=join(dir,"launcher.mjs"),leaderFile=join(dir,"leader.pid"),descendantFile=join(dir,"descendant.pid"),ready=join(dir,"ready"),resultFile=join(dir,"result.json");
	writeFileSync(descendant,`import {writeFileSync} from 'node:fs';writeFileSync(${JSON.stringify(descendantFile)},String(process.pid));setInterval(()=>{},1000);`);
	writeFileSync(launcher,`import {spawn} from 'node:child_process';const c=spawn(process.execPath,[${JSON.stringify(descendant)}],{stdio:'ignore'});c.unref();setTimeout(()=>{},350);`);
	writeFileSync(join(lib,"checks.mts"),`import {spawn} from 'node:child_process';import {writeFileSync} from 'node:fs';
export async function active(){const c=spawn(process.execPath,[${JSON.stringify(launcher)}],{detached:true,stdio:'ignore'});writeFileSync(${JSON.stringify(leaderFile)},String(c.pid));await new Promise(r=>c.once('close',r));writeFileSync(${JSON.stringify(ready)},'ready');${how==="worker-exit"?"process.exit(42);":"await new Promise(()=>setInterval(()=>{},1000));"}}`);
	const driver=join(dir,"driver.mjs");
	writeFileSync(driver,`import {writeFileSync} from 'node:fs';import {withRunCancellation} from ${JSON.stringify(pathToFileURL(join(lib,"cancellation.mts")).href)};import {freshCheck} from ${JSON.stringify(pathToFileURL(join(lib,"check-process.mts")).href)};
await withRunCancellation(async(signal,_received,cancel)=>{const ctx={cwd:${JSON.stringify(dir)},runDir:${JSON.stringify(dir)},mode:'full',signal,cancel};const result=await freshCheck({id:'active',fn:'active'},ctx,${JSON.stringify(dir)},1);writeFileSync(${JSON.stringify(resultFile)},JSON.stringify({result,unknown:ctx.unknownWorkerDrains}));});`);
	const parent=spawn(process.execPath,[driver],{stdio:["ignore","ignore","pipe"]});let stderr="";parent.stderr.on("data",data=>{stderr+=data;});
	const closed=new Promise(resolve=>parent.once("close",(code,signal)=>resolve({code,signal})));
	try {
		for(let i=0;i<500&&(!existsSync(ready)||!existsSync(descendantFile));i++)await delay(10);
		assert.ok(existsSync(ready)&&existsSync(descendantFile),stderr);
		if(how==="signal")parent.kill("SIGTERM");
		assert.deepEqual(await closed,{code:0,signal:null},stderr);
		const result=JSON.parse(readFileSync(resultFile,"utf8"));assert.equal(result.unknown,undefined,JSON.stringify(result));
		const pid=Number(readFileSync(descendantFile,"utf8"));assert.throws(()=>process.kill(pid,0),error=>(error as NodeJS.ErrnoException).code==="ESRCH");
		const groups=JSON.parse(readFileSync(join(dir,"attempts/active/worker-1/groups.json"),"utf8"));
		assert.ok(groups.some((group: { identities: { pid: number; }[]; })=>group.identities.some((identity: { pid: number; })=>identity.pid===pid)),"Persisted ownership includes the observed surviving descendant");
	} finally {
		if(existsSync(leaderFile))try{process.kill(-Number(readFileSync(leaderFile,"utf8")),"SIGKILL");}catch(error){if((error as NodeJS.ErrnoException).code!=="ESRCH")throw error;}
		if(existsSync(descendantFile))try{process.kill(Number(readFileSync(descendantFile,"utf8")),"SIGKILL");}catch(error){if((error as NodeJS.ErrnoException).code!=="ESRCH")throw error;}
		if(parent.exitCode===null&&parent.signalCode===null)parent.kill("SIGKILL");await closed;
	}
});

test("a terminal signal drains a detached in-process child, skips queued checks and writes a partial receipt", {timeout: 20000}, async t => {
	const scratch = join(process.cwd(), "tmp"); mkdirSync(scratch, {recursive: true});
	const dir = mkdtempSync(join(scratch, "gate-cancel-"));
	t.after(() => rmSync(dir, {recursive: true, force: true}));
	const lib = join(dir, "lib"); cpSync(new URL("../lib/", import.meta.url), lib, {recursive: true});
	if (process.env.CANCELLATION_BASELINE_FULL) cpSync(process.env.CANCELLATION_BASELINE_FULL, join(lib, "full.mts"));
	if (process.env.CANCELLATION_BASELINE_WORKER) cpSync(process.env.CANCELLATION_BASELINE_WORKER, join(lib, "check-worker.mts"));
	const marker = join(dir, "ready"), pidFile = join(dir, "child-pid"), launched = join(dir, "started"), admitted = join(dir, "admitted"), gateHome = join(dir, "home"), receiptId = "affected-fake";
	const policy = JSON.parse(readFileSync(new URL("../phase.json", import.meta.url), "utf8"));
	writeFileSync(join(lib, "checks.mts"), `import {spawn} from 'node:child_process';import {writeFileSync,appendFileSync} from 'node:fs';import {join} from 'node:path';
export function pass(ctx){appendFileSync(join(ctx.runDir,'started'),'pass\\n');return {status:'pass'};}
export async function active(ctx){appendFileSync(join(ctx.runDir,'started'),'active\\n');const pidFile=join(ctx.runDir,'child-pid');const child=spawn(process.execPath,['-e',\`require('node:fs').writeFileSync(\${JSON.stringify(pidFile)},String(process.pid));setInterval(()=>{},1000)\`],{detached:true,stdio:'ignore'});child.unref();while(!child.pid)await new Promise(r=>setTimeout(r,5));writeFileSync(join(ctx.runDir,'ready'),'ready');await new Promise(()=>setInterval(()=>{},1000));}
export function queued(ctx){appendFileSync(join(ctx.runDir,'started'),'queued\\n');return {status:'pass'};}`);
	const driver = join(dir, "driver.mjs");
	writeFileSync(driver, `import {appendFileSync} from 'node:fs';import {withRunCancellation} from ${JSON.stringify(pathToFileURL(join(lib, "cancellation.mts")).href)};
import {runFullSteps} from ${JSON.stringify(pathToFileURL(join(lib, "full.mts")).href)};
import {freshCheck} from ${JSON.stringify(pathToFileURL(join(lib, "check-process.mts")).href)};
import {outcomesForSelection} from ${JSON.stringify(pathToFileURL(join(lib, "affected-coverage.mts")).href)};
import {ensureStore,writeReceipt} from ${JSON.stringify(pathToFileURL(join(lib, "store.mts")).href)};
const root=${JSON.stringify(dir)};
const steps=['pass','active','queued'].map(id=>({id,fn:id})), selection=steps.map(({id})=>({id,status:'run'}));
await withRunCancellation(async(signal,received,cancel)=>{const ctx={cwd:root,runDir:root,mode:'full',signal,cancel};
const results=await runFullSteps(steps,step=>{appendFileSync(${JSON.stringify(admitted)},step.id+'\\n');return freshCheck(step,ctx,root,1)},1,undefined,{signal});
const outcomes=outcomesForSelection(selection,results,'unrun');ensureStore();writeReceipt({id:${JSON.stringify(receiptId)},kind:'affected',base:'base',commit:'a',tree:'tree-a',policyHash:'policy',phase:'prelaunch',phasePolicy:${JSON.stringify(policy)},selected:Object.keys(outcomes),outcomes,steps:results,browserRelaunchTelemetry:{status:'unrun'},result:signal.aborted?'fail':'pass',cancelled:signal.aborted,cancellationSignal:received()});});`);
	const child = spawn(process.execPath, [driver], {env: {...process.env, RV_GATE_HOME: gateHome}, stdio: ["ignore", "ignore", "pipe"]});
	let stderr = ""; child.stderr.on("data", chunk => { stderr += chunk; });
	const closed = new Promise(resolve => child.once("close", (code, signal) => resolve({code, signal})));
	try {
		for (let n = 0; n < 300 && (!existsSync(marker) || !existsSync(pidFile)); n++) await delay(10);
		assert.ok(existsSync(marker) && existsSync(pidFile), `Fake owned work did not start: ${stderr}`);
		const pid = Number(readFileSync(pidFile, "utf8")); child.kill("SIGTERM");
		assert.deepEqual(await closed, {code: 0, signal: null}, stderr);
		const receipt = JSON.parse(readFileSync(join(gateHome, "evidence", "receipts", `${receiptId}.json`), "utf8"));
		assert.equal(receipt.result, "fail"); assert.equal(receipt.cancelled, true); assert.equal(receipt.cancellationSignal, "SIGTERM");
		assert.deepEqual(receipt.steps.map((row: {status: string}) => row.status), ["pass", "unrun", "unrun"]);
		assert.equal(receipt.outcomes.pass, "pass"); assert.equal(receipt.outcomes.active, "unrun"); assert.equal(receipt.outcomes.queued, "unrun");
		assert.equal(validateAffectedHistory({receipts:[receipt],base:"base",head:"a",policyHash:"policy",contains:()=>true,treeOf:()=>"tree-a"})[0].outcomes.pass, "pass");
		assert.throws(() => affectedPromotion({receipt, policy: {phase:"prelaunch", since:"2026-10-30", by:"fixture", rotation:[]}, steps: []}), /passing prelaunch affected receipt/);
		assert.equal(readFileSync(launched, "utf8"), "pass\nactive\n");
		assert.equal(readFileSync(admitted, "utf8"), "pass\nactive\n");
		assert.throws(() => process.kill(pid, 0), error => (error as NodeJS.ErrnoException).code === "ESRCH");
	} finally {
		if (child.exitCode === null && child.signalCode === null) child.kill("SIGKILL");
		if (existsSync(pidFile)) try { process.kill(-Number(readFileSync(pidFile, "utf8")), "SIGKILL"); } catch (error) { if ((error as NodeJS.ErrnoException).code !== "ESRCH") throw error; }
		await closed;
	}
});
