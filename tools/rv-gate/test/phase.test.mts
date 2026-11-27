import assert from "node:assert/strict";
import {mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync} from "node:fs";
import {tmpdir} from "node:os";
import {join} from "node:path";
import test from "node:test";
import {planRotation, qualifiesForRotation, readPhase, validatePhase, prelaunchException, browserGap, CHROMIUM_ONLY_REASON, type Phase, type PhasePolicy} from "../lib/phase.mts";
import {readReceipt, writeReceipt, writeOnce, receiptPath, ensureStore} from "../lib/store.mts";
import {rotationReceipts} from "../lib/value.mts";

const policy = {phase: "prelaunch" as const, since: "2026-10-30", by: "Daniel", rotation: ["a", "b", "c", "d"]};
test("the personal-quota pause is dated and cannot affect another check, launch or prod",()=>{
	const configured={...policy,freshAgentPause:{until:"2026-10-31T18:00:00.581308Z",reason:"Daniel: personal quota resets"}};
	assert.deepEqual(validatePhase(configured),configured);
	const before={mode:"full",id:"fresh-agent",now:"2026-10-31T18:00:00.580Z"};
	assert.deepEqual(prelaunchException(configured,before),{type:"fresh-agent-quota",...configured.freshAgentPause});
	for(const change of [{id:"chromium"},{mode:"prod"},{mode:"release"},{now:configured.freshAgentPause.until},{now:"2026-11-01T00:00:00Z"},{now:"bad"}])assert.equal(prelaunchException(configured,{...before,...change}),null);
	assert.equal(prelaunchException({...configured,phase:"launch"},before),null);
	for(const freshAgentPause of [{until:"bad",reason:"why"},{until:configured.freshAgentPause.until,reason:""}])assert.throws(()=>validatePhase({...policy,freshAgentPause}),/pause/);
});
test("phase parsing fails closed and a historical checkout stays strict", t => {
	const dir = mkdtempSync(join(tmpdir(), "gate-phase-")); t.after(() => rmSync(dir, {recursive:true,force:true}));
	const file = join(dir, "phase.json"); assert.equal(readPhase(file).phase, "launch");assert.deepEqual(validatePhase(readPhase(file)),readPhase(file));
	writeFileSync(file, JSON.stringify(policy)); assert.deepEqual(readPhase(file), policy);
	for (const change of [(p: { phase: string; })=>p.phase="unknown",(p: { since: string; })=>p.since="2026-02-30",(p: { by: string; })=>p.by="",(p: { rotation: string[]; })=>p.rotation.push("a"),(p: { rotation: string[]; })=>p.rotation=["../build"]]) {
		const broken=structuredClone(policy); change(broken); // Send a deliberately malformed JSON value through the runtime validator.
		assert.throws(()=>validatePhase(broken as Partial<PhasePolicy>));
	}
});
test("rotation needs twenty truly green runs and measured cost, never queue duration", () => {
	const passes: Parameters<typeof qualifiesForRotation>[0][number][] = Array.from({length:20},()=>({status:"pass",runMs:200_000,durationMs:900_000}));
	assert.equal(qualifiesForRotation(passes),true);
	assert.equal(qualifiesForRotation(passes.slice(1)),false);
	assert.equal(qualifiesForRotation(passes.map(({runMs,...step})=>step)),false);
	const incomplete=structuredClone(passes);delete incomplete[0].runMs;assert.equal(qualifiesForRotation(incomplete),false);
	for (const change of [(s)=>s.status="fail",(s)=>s.flaky=true,(s)=>s.attempts=[{status:"fail"},{status:"pass"}]] satisfies ((observation: typeof passes[number])=>unknown)[]) {
		const broken=structuredClone(passes);change(broken[0]);assert.equal(qualifiesForRotation(broken),false);
	}
	assert.equal(qualifiesForRotation(passes.map(s=>({...s,runMs:18_000}))),false);
});
test("new receipts bind the frozen phase while historical receipts remain untouched",t=>{
	const dir=mkdtempSync(join(tmpdir(),"gate-phase-store-")),previous=process.env.RV_GATE_HOME;
	process.env.RV_GATE_HOME=dir;t.after(()=>{if(previous===undefined)delete process.env.RV_GATE_HOME;else process.env.RV_GATE_HOME=previous;rmSync(dir,{recursive:true,force:true});});
	const id="full-20261030T120000Z-abcdef0123";
	writeReceipt({id,phase:"prelaunch",phasePolicy:policy,steps:[]});
	assert.equal(readReceipt(id).phase,"prelaunch");assert.deepEqual(readReceipt(id).phasePolicy,policy);
	assert.throws(()=>writeReceipt({id,phase:"prelaunch",phasePolicy:policy}),/EEXIST/);
	ensureStore();const legacy="full-20261029T120000Z-abcdef0123";writeOnce(receiptPath(legacy),{id:legacy,kind:"full",steps:[]});
	assert.equal(readReceipt(legacy).phase,undefined);
	assert.throws(()=>writeReceipt({id:"full-20261030T120001Z-abcdef0123",phase:"launch",phasePolicy:policy}),/differs/);
});
test("every approved eligible step runs by each third full; launch and prod run all", () => {
	const steps=[...policy.rotation.map(id=>({id})),{id:"build"},{id:"captures"},{id:"packaging"},{id:"vscode",after:["packaging"]}];
	const configured={...policy,rotation:[...policy.rotation,"build","captures","packaging"]};
	const receipts: NonNullable<Parameters<typeof planRotation>[1]["receipts"]>[number][] = [];
	const seen=new Map<string, number[]>(policy.rotation.map(id=>[id,[]]));
	for(let n=0;n<9;n++){
		const plan=planRotation(configured,{mode:"full",steps,receipts,eligible:configured.rotation});
		assert.ok(!plan.skipped.some(id=>["build","captures","packaging"].includes(id)));
		for(const id of plan.selected){const times=seen.get(id);assert.ok(times);times.push(n);}
		receipts.push({id:String(n),kind:"full",phase:"prelaunch" as const,at:`2026-10-30T12:00:0${n}Z`,steps:steps.map(({id})=>({id,status:plan.skipped.includes(id)?"unrun":"pass",reason:plan.skipped.includes(id)?"prelaunch rotation":undefined}))});
	}
	for(const times of seen.values()){assert.equal(times.length,3);assert.ok(times.slice(1).every((n,i)=>n-times[i]===3));}
	for(const [phase,mode]of [["launch","full"],["prelaunch","prod"]] satisfies [Phase, string][])assert.deepEqual(planRotation({...configured,phase},{mode,steps,receipts,eligible:configured.rotation}).skipped,[]);
	assert.throws(()=>planRotation({...policy,rotation:["missing"]},{mode:"full",steps}),/Unknown/);
});
test("an approval-list change cannot defer a twice-skipped step again",()=>{
	for(const reason of ["prelaunch rotation","The build step failed."]){
		const receipts=[0,1].map(i=>({id:String(i),kind:"full",phase:"prelaunch" as const,at:`2026-10-30T12:00:0${i}Z`,steps:[{id:"a",status:"unrun",reason}]}));
		const plan=planRotation(policy,{mode:"full",steps:policy.rotation.map(id=>({id})),receipts,eligible:["a"]});
		assert.ok(plan.forced.includes("a"));assert.ok(plan.selected.includes("a"));assert.ok(!plan.skipped.includes("a"));
	}
});

test("with no approved rotation a run never reads the store; with one it does", () => {
	const unreadable = () => { throw new Error("a malformed receipt must not stop a run with nothing to rotate"); };
	assert.deepEqual(rotationReceipts({ ...policy, rotation: [] }, unreadable), []);
	assert.throws(() => rotationReceipts({ ...policy, rotation: ["slow"] }, unreadable), /malformed receipt/);
	const receipt = { id: "full-1", kind: "full", result: "pass", commit: "a".repeat(40), tree: "b".repeat(40), at: "2026-10-30T12:00:00Z" };
	assert.deepEqual(rotationReceipts({ ...policy, rotation: ["slow"] }, () => [receipt]), [receipt]);
});

test("a record is written whole or not at all: no temp file stays, an existing record is kept", t => {
	const dir = mkdtempSync(join(tmpdir(), "rv-write-once-"));
	t.after(() => rmSync(dir, { recursive: true, force: true }));
	const path = join(dir, "full-20261101T120000Z-abcdef0123.json");
	writeOnce(path, { id: "first" });
	assert.throws(() => writeOnce(path, { id: "second" }), /EEXIST/);
	assert.deepEqual(JSON.parse(readFileSync(path, "utf8")), { id: "first" });
	assert.deepEqual(readdirSync(dir), ["full-20261101T120000Z-abcdef0123.json"]);
});

test("prelaunch full policy is frozen and launch restores all engines", () => {
	const full = {browsers: ["chromium"], partialRetry: true};
	const configured = {...policy, prelaunchFull: full};
	assert.deepEqual(validatePhase(configured), configured);
	const step = {id: "parallel-shaders"}, command = {env: {RICHVIEW_VERIFY_BROWSER: "firefox"}};
	assert.equal(browserGap(configured, {mode: "full", step, command}), CHROMIUM_ONLY_REASON);
	assert.equal(browserGap({...configured, phase: "launch"}, {mode: "full", step, command}), null);
	assert.equal(browserGap(configured, {mode: "prod", step, command}), null);
	for (const broken of [{...full, browsers: ["firefox"]}, {...full, partialRetry: false}]) assert.throws(() => validatePhase({...policy, prelaunchFull: broken}), /prelaunch full/);
});

test("prelaunch fallback criterion can switch between failure count and real defects", () => {
	const affected = {failureThreshold:5, fallback:{mode:"failure-threshold" as const,realDefectSteps:["site","chromium"]}};
	assert.deepEqual(validatePhase({...policy, affected}).affected, affected);
	assert.equal(validatePhase({...policy, affected:{...affected,fallback:{...affected.fallback,mode:"real-defects"}}}).affected?.fallback.mode,"real-defects");
	for(const broken of [{...affected,failureThreshold:-1},{...affected,fallback:{...affected.fallback,mode:"unknown"}},{...affected,fallback:{...affected.fallback,realDefectSteps:["site","site"]}}]) assert.throws(()=>validatePhase({...policy,affected:broken} as Partial<PhasePolicy>),/fallback/);
});
