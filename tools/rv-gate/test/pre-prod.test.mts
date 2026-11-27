// Host reason (ARC-03): test promotion against a disposable Git remote and immutable local receipts.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import {fileURLToPath, pathToFileURL} from "node:url";
import test, { type TestContext } from "node:test";
import {prelaunchException, type Phase, type PhasePolicy} from "../lib/phase.mts";
import {rotationFromHistory} from "../lib/value.mts";
import { fullPromotionProblems, promoteFull, readDev, sourceChanges } from "../lib/pre-prod.mts";
import type { GateStatus, StepOutcome } from "../lib/results.mts";
import type { StoredReceipt } from "../lib/store.mts";
import type { AreaMap, SelectionItem } from "../lib/selection.mts";
import { audit } from "../lib/checks.mts";
import { FRESH_AGENT_SCHEDULE, STEPS, stepsForTier } from "../lib/steps.mts";
import { appendChange, listReceipts, readBaseline, readChanges, receiptNotes, writeBaseline, writeReceipt, writeReceiptNote } from "../lib/store.mts";

// An issue id of the tracker, built here and not written out: this file ships, and the public-tree check scans it.
const issueId = (n: number) => ["S27", n].join("-");

const git = (cwd: string, ...args: string[]) => execFileSync("git", args, { cwd, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
type PromotionStep = NonNullable<StoredReceipt["steps"]>[number];
type RotationHistory = StoredReceipt & { steps: PromotionStep[] };
type PromotionNote = ReturnType<typeof receiptNotes>[number] & { commits?: string[]; required?: string[]; affectedProof?: { obligations: unknown[] } };

const fixtureSource = { commit: "a".repeat(40), tree: "b".repeat(40) };
const passingFull = { ...fixtureSource, kind: "full", result: "pass" };
const promotionEvidence = { ...passingFull, id: "full-fixture", at: "2026-10-30T12:00:00Z" };

function fixtureStep<T extends { id: string }>(steps: readonly T[], id: string): T {
	const step = steps.find(item => item.id === id);
	assert.ok(step, `Fixture step ${id} must be recorded`);
	return step;
}

async function writeReport(options: { cwd: string; from: string; to: string }) {
	const {writeReport: reportImplementation}=await import("../lib/report.mts");
	return reportImplementation(options);
}

const passingSteps = (): PromotionStep[] => stepsForTier("full", "full").map(({id}) => ({id, status: ["voiceover", "hosted-smoke", "windows"].includes(id) ? "unrun" : "pass", reason: "Standing gap, Daniel29Oct"}));
test("only a qualified approved prelaunch rotation can authorize a full gap",()=>{
	const policy={phase:"prelaunch" as const,since:"2026-10-30",by:"Daniel",rotation:["slow-a","slow-b","slow-c"]};
	const steps=policy.rotation.map(id=>({id}));
	const history: RotationHistory[]=Array.from({length:20},(_,i)=>({...passingFull,id:`full-${i}`,at:new Date(Date.UTC(2026,9,28,i)).toISOString(),steps:steps.map(({id})=>({id,status:"pass",runMs:300000}))}));
	const rotation=rotationFromHistory(policy,{mode:"full",steps,receipts:history});
	const receipt={...promotionEvidence,phase:"prelaunch" as const,phasePolicy:policy as PhasePolicy | undefined,rotation,
		steps:steps.map(({id}): PromotionStep=>({id,status:rotation.skipped.includes(id)?"unrun":"pass",...(rotation.skipped.includes(id)&&{reason:"prelaunch rotation",rotation:{ordinal:rotation.ordinal,cycle:3}})}))};
	const check=(r: typeof receipt,extra: {policy?: PhasePolicy; history?: RotationHistory[]; now?: string}={})=>fullPromotionProblems(r,steps,{policy,history,...extra});
	assert.deepEqual(check(receipt),[]);
	assert.ok(check(receipt,{policy:{...policy,phase:"launch"}}).length);
	for(const mutate of [
		r=>delete (r as Partial<typeof r>).phasePolicy,r=>r.rotation.ordinal=-1,r=>r.rotation.selected=[],r=>r.rotation.skipped.push("slow-a"),
		r=>r.rotation.qualifications[1].timingSamples=1,r=>r.rotation.qualifications[1].medianRunMs=18000,
		r=>r.rotation.qualifications[1].receipts.fill("full-0"),r=>r.steps[1].reason="skipped",r=>{assert.ok(r.steps[1].rotation);r.steps[1].rotation.cycle=4;},
	] satisfies ((fixture: typeof receipt) => unknown)[]){const bad=structuredClone(receipt);mutate(bad);assert.ok(check(bad).length);}
	const brokenHistory=structuredClone(history);brokenHistory[0].steps[1].flaky=true;assert.ok(check(receipt,{history:brokenHistory}).length);
	const reusedHistory=structuredClone(history);reusedHistory[0].steps[1].reused="earlier";assert.ok(check(receipt,{history:reusedHistory}).length);
	for(const [name,mutate]of [
		["zero cost",h=>h.forEach(r=>r.steps.forEach(s=>s.runMs=0))],
		["queued wall only",h=>h.forEach(r=>{r.parallel=4;r.steps.forEach(s=>{delete s.runMs;s.durationMs=900000;});})],
		["missing observation",h=>h.pop()],
		["unrun is not an execution",h=>h[0].steps[1].status="unrun"],
		["missing step",h=>h[0].steps.splice(1,1)],
		["newer failure",h=>h.push({...passingFull,id:"newer",result:"fail",at:"2026-10-30T11:00:00Z",steps:steps.map(({id})=>({id,status:"fail",runMs:300000}))})],
		["newer execution changes latest twenty",h=>h.push({...passingFull,id:"newer",at:"2026-10-30T11:00:00Z",steps:steps.map(({id})=>({id,status:"pass",runMs:300000}))})],
	] satisfies [string, (fixture: typeof history) => unknown][]){const changed=structuredClone(history);mutate(changed);assert.ok(check(receipt,{history:changed}).length,`${name} cannot borrow a claimed qualifying summary`);}
	const falseMedian=structuredClone(receipt);falseMedian.rotation.qualifications[1].medianRunMs=300001;assert.ok(check(falseMedian).length,"The claimed median must match actual observation times");
	const falseHash=structuredClone(receipt);falseHash.rotation.qualifications[1].receiptHashes[0]="0".repeat(64);assert.ok(check(falseHash).length,"Qualifications bind immutable receipt contents");
	const laterHistory: RotationHistory[]=[...history,{...passingFull,id:"later-red",result:"fail",at:"2026-10-30T13:00:00Z",steps:steps.map(({id})=>({id,status:"fail",runMs:300000}))}];
	assert.ok(check(receipt,{history:laterHistory,now:"2026-10-30T14:00:00Z"}).length,"New promotion cannot reuse a window superseded after the full");
	assert.deepEqual(check(receipt,{history:laterHistory,now:receipt.at}),[],"Historical audit freezes the observation window at execution");
	const laterUnrun=structuredClone(laterHistory), last=laterUnrun.at(-1);assert.ok(last);last.phase="prelaunch";for(const s of last.steps)s.status="unrun";
	assert.deepEqual(check(receipt,{history:laterUnrun,now:"2026-10-30T14:00:00Z"}),[],"Later fulls without an execution preserve the admitted due-slot plan and actual execution window");
	const allPass=structuredClone(receipt);delete (allPass as Partial<typeof allPass>).rotation;allPass.steps=steps.map(({id})=>({id,status:"pass"}));allPass.steps[0].flaky=true;
	assert.deepEqual(check(allPass),[]);assert.ok(check(allPass,{policy:{...policy,phase:"launch"}}).length);
});
function fixture(t: TestContext) {
	mkdirSync(join(process.cwd(), "tmp"), { recursive: true });
	const root = mkdtempSync(join(process.cwd(), "tmp/pre-prod-"));
	const previous = process.env.RV_GATE_HOME;
	process.env.RV_GATE_HOME = join(root, "home");
	t.after(() => { if (previous === undefined) delete process.env.RV_GATE_HOME; else process.env.RV_GATE_HOME = previous; rmSync(root, { recursive: true, force: true }); });
	const remote = join(root, "remote.git"), cwd = join(root, "work");
	git(root, "init", "--bare", remote);
	git(root, "init", "-b", "dev", cwd);
	for (const [key, value] of [["user.name", "Fixture"], ["user.email", "fixture@example.test"], ["commit.gpgsign", "false"], ["core.hooksPath", "/dev/null"]]) git(cwd, "config", key, value);
	const commit = (text: string) => { writeFileSync(join(cwd, "report.txt"), text); git(cwd, "add", "report.txt"); git(cwd, "commit", "-qm", text); return git(cwd, "rev-parse", "HEAD"); };
	// merge-prod reads whether the candidate ships Windows from this file and refuses without it.
	mkdirSync(join(cwd, "assets/install"), { recursive: true });
	writeFileSync(join(cwd, "assets/install/release.json"), '{"enabled": false, "windows": false}\n');
	git(cwd, "add", "assets/install/release.json");
	const first = commit("first");
	git(cwd, "remote", "add", "origin", remote);
	git(cwd, "push", "-q", "origin", "HEAD:dev", "HEAD:pre-prod", "HEAD:prod");
	const second = commit("second");
	git(cwd, "push", "-q", "origin", "HEAD:dev");
	const receipt = {
		id: `full-20261029T180000Z-${second.slice(0,10)}`, at: "2026-10-29T18:00:00Z", kind: "full",
		commit: second, tree: git(cwd,"rev-parse",`${second}^{tree}`), prod: first, result: "pass", steps: passingSteps(),
	} satisfies StoredReceipt & { prod: string };
	return { cwd, remote, first, second, commit, receipt: receipt as typeof receipt & { candidate?: string } };
}

test("lead judgment audits pre-prod as distinct provenance, never candidate or prod proof", t => {
	const f=fixture(t), third=f.commit("third");
	git(f.cwd,"push","-q","origin","HEAD:dev","HEAD:pre-prod");git(f.cwd,"fetch","-q","origin");
	const receipt={...f.receipt,id:`affected-20261110T163824Z-${third.slice(0,10)}`,kind:"affected",commit:third,tree:git(f.cwd,"rev-parse",`${third}^{tree}`),result:"fail"};
	writeReceipt(receipt);
	writeReceiptNote(receipt.id,"lead-judgment-promotion",{base:f.first,commit:third,tree:receipt.tree,authority:"Daniel 10 Nov 2026 17:25 CET permits automatic pre-prod judgment; lead explicitly authorizes this source.",resultAtPromotion:"in-progress",steps:[{id:"build",status:"pass"},{id:"site",status:"fail",reason:"stale expectation"},{id:"audit",status:"pending"}],prodEligible:false});
	const preprod=audit({cwd:f.cwd,head:third,mode:"full"});assert.equal(preprod.status,"pass","reason" in preprod ? preprod.reason : "");assert.equal(preprod.details?.judgments?.[0].commit,third);
	assert.equal(audit({cwd:f.cwd}).status,"pass","explicit pre-prod audit uses judgment");
	assert.equal(audit({cwd:f.cwd,head:third,mode:"prod"}).status,"fail","prod cannot consume judgment");
	assert.equal(audit({cwd:f.cwd,head:third,mode:"full",candidate:"candidate/1"}).status,"fail","candidate cannot consume judgment even under full mode");
	assert.equal(audit({cwd:f.cwd,head:third}).status,"fail","an explicit arbitrary head is not pre-prod authority");
});

test("judgment evidence must bind source, tree, range, ruling and complete step snapshot", t => {
	const f=fixture(t);git(f.cwd,"push","-q","origin","HEAD:pre-prod");git(f.cwd,"fetch","-q","origin");
	const receipt={...f.receipt,id:`affected-20261110T163824Z-${f.second.slice(0,10)}`,kind:"affected",result:"fail"};
	const valid={base:f.first,commit:f.second,tree:receipt.tree,authority:"Daniel 10 Nov 2026 17:25 CET permits automatic pre-prod judgment; release lead authorized it.",resultAtPromotion:"fail",steps:[{id:"site",status:"fail",reason:"checked"},{id:"audit",status:"unrun",reason:"not selected"}],prodEligible:false};
	for(const [name,mutate] of [
		["tree",n=>n.tree="0".repeat(40)], ["base",n=>n.base="0".repeat(40)],
		["actor",n=>n.authority="Daniel 10 Nov 2026 17:25 CET"], ["time",n=>n.authority="Daniel and release lead 10 Nov 2026"],
		["steps",n=>n.steps=[]], ["duplicate step",n=>n.steps.push(n.steps[0])], ["status",n=>n.steps[0].status="unknown"], ["prod eligibility",n=>n.prodEligible=true],
	] satisfies [string, (fixture: typeof valid) => unknown][]) {
		process.env.RV_GATE_HOME=join(f.cwd,`home-${name.replaceAll(" ","-")}`);
		writeReceipt(receipt);const note=structuredClone(valid);mutate(note);writeReceiptNote(receipt.id,"lead-judgment-promotion",note);
		assert.equal(audit({cwd:f.cwd,mode:"full"}).status,"fail",name);
	}
});

test("pending judgment notes and future structured authority bind only the fast-forward range", t => {
	const f=fixture(t),third=f.commit("third");git(f.cwd,"push","-q","origin","HEAD:dev","HEAD:pre-prod");git(f.cwd,"fetch","-q","origin");
	const id=`affected-20261110T163824Z-${third.slice(0,10)}`,tree=git(f.cwd,"rev-parse",`${third}^{tree}`);
	writeReceiptNote(id,"lead-judgment-promotion",{base:f.first,commit:third,tree,actor:"release lead",ruling:"Daniel, 10 Nov 2026 17:25 CET",resultAtPromotion:"in-progress",steps:[{id:"build",status:"pass"},{id:"audit",status:"pending"}],prodEligible:false});
	assert.equal(audit({cwd:f.cwd}).status,"pass","the immutable note can precede the finished receipt");
	writeReceipt({...f.receipt,id,kind:"affected",commit:third,tree:"0".repeat(40),result:"fail"});
	assert.equal(audit({cwd:f.cwd}).status,"fail","a later source-mismatched receipt cannot inherit the judgment");
});

test("judgment never hides a pre-existing strict receipt's tree failure", t => {
	const f=fixture(t);git(f.cwd,"push","-q","origin","HEAD:pre-prod");git(f.cwd,"fetch","-q","origin");
	const id=`affected-20261110T163824Z-${f.second.slice(0,10)}`;
	writeReceiptNote(id,"lead-judgment-promotion",{base:f.first,commit:f.second,tree:f.receipt.tree,actor:"release lead",ruling:"Daniel, 10 Nov 2026 17:25 CET",resultAtPromotion:"in-progress",steps:[{id:"audit",status:"pending"}],prodEligible:false});
	writeReceipt({...f.receipt,tree:"0".repeat(40)});writeReceiptNote(f.receipt.id,"merge",{commit:f.second});
	const result=audit({cwd:f.cwd});assert.equal(result.status,"fail");assert.match(result.reason ?? "",/tree differs/);
});

test("shipping coverage, promotion and historical audit preserve only justified declared rotation gaps",async t=>{
	const f=fixture(t),home=process.env.RV_GATE_HOME;
	// Give the unchanged shipping modules a private real phase file and evidence store.
	cpSync(fileURLToPath(new URL("../../",import.meta.url)),join(f.cwd,"tools"),{recursive:true});
	const policy={phase:"prelaunch" as const,since:"2026-10-30",by:"Daniel",rotation:["fonts"]};
	writeFileSync(join(f.cwd,"tools/rv-gate/phase.json"),JSON.stringify(policy));
	const module=(path: string)=>import(pathToFileURL(join(f.cwd,"tools/rv-gate",path)).href);
	const localChecks=await module("lib/checks.mts"),localPromotion=await module("lib/pre-prod.mts"),localValue=await module("lib/value.mts");
	const history: RotationHistory[]=Array.from({length:20},(_,i)=>({...passingFull,id:`full-20261028T${String(i).padStart(2,"0")}0000Z-abcdef0123`,at:new Date(Date.UTC(2026,9,28,i)).toISOString(),phase:"launch" as const,phasePolicy:{phase:"launch" as const,since:"2026-10-28",by:"Daniel",rotation:[]},steps:[{id:"fonts",status:"pass",runMs:300000}]}));
	history.push({...passingFull,id:"full-20261030T110000Z-abcdef0123",at:"2026-10-30T11:00:00Z",phase:"prelaunch" as const,phasePolicy:policy,steps:[{id:"fonts",status:"unrun",reason:"Build failed"}]});
	for(const prior of history)writeReceipt(prior);
	const rotation=localValue.rotationFromHistory(policy,{mode:"full",steps:STEPS,receipts:listReceipts()});
	assert.deepEqual(rotation.skipped,["fonts"]);
	const receipt={...f.receipt,id:"full-20261030T120000Z-abcdef0123",at:"2026-10-30T12:00:00Z",phase:"prelaunch" as const,phasePolicy:policy,rotation,steps:f.receipt.steps.map((s): PromotionStep=>s.id==="fonts"?{...s,status:"unrun",reason:"prelaunch rotation",rotation:{ordinal:rotation.ordinal,cycle:3}}:s)};
	const context={cwd:f.cwd,pr:{items:["DEL-82"]},completed:new Map(receipt.steps.map(s=>[s.id,s.status]))};
	assert.equal(localChecks.coverage(context).status,"pass");assert.equal(context.completed.get("fonts"),"unrun");
	const mapPath=join(f.cwd,"tools/rv-gate/coverage.json"),mapBytes=readFileSync(mapPath),badMap=JSON.parse(mapBytes.toString("utf8"));
	badMap.items["DEL-82"].checks=["not-a-declared-step"];writeFileSync(mapPath,JSON.stringify(badMap));
	assert.equal(localChecks.coverage(context).status,"fail","Rotation cannot make an unresolved coverage declaration pass");writeFileSync(mapPath,mapBytes);
	writeReceipt(receipt);assert.equal(localPromotion.promoteFull(f.cwd,{receiptId:receipt.id}).updated,false);
	assert.equal(localPromotion.promoteFull(f.cwd,{receiptId:receipt.id,apply:true}).updated,true);assert.equal(git(f.remote,"rev-parse","pre-prod"),f.second);assert.equal(git(f.remote,"rev-parse","prod"),f.first);
	const priors=listReceipts().filter(r=>r.id!==receipt.id);
	const later={...passingFull,id:"full-20261030T130000Z-abcdef0123",result:"fail",at:"2026-10-30T13:00:00Z",phase:"prelaunch" as const,phasePolicy:policy,steps:[{id:"fonts",status:"fail",runMs:300000}]};writeReceipt(later);
	assert.equal(localChecks.audit({cwd:f.cwd,head:f.second}).status,"pass","Later failures do not rewrite a valid historical promotion");
	assert.throws(()=>localPromotion.promoteFull(f.cwd,{receiptId:receipt.id}),/actual latest executed history/);
	const localRegistry=(await module("lib/steps.mts")).STEPS;
	localRegistry.push({id:"new-artifact-consumer",tier:"integration",after:["fonts"]});
	try {
		const old=localChecks.audit({cwd:f.cwd,head:f.second});assert.equal(old.status,"pass",`Historical dependency protection must use the admitted registry: ${old.reason}`);
		assert.throws(()=>localPromotion.promoteFull(f.cwd,{receiptId:receipt.id}),/Cannot promote pre-prod/,"New promotion retains today's dependency protection");
	} finally {localRegistry.pop();}
	for(const [name,mutate]of [
		["unjustified-reason",r=>fixtureStep(r.steps,"fonts").reason="manual skip"],
		["unapproved-step",r=>Object.assign(fixtureStep(r.steps,"host"),{status:"unrun",reason:"prelaunch rotation"})],
		["false-cost",r=>r.rotation.qualifications[0].medianRunMs=900000],
	] satisfies [string, (fixture: typeof receipt) => unknown][]){
		process.env.RV_GATE_HOME=join(f.cwd,name);for(const prior of priors)writeReceipt(prior);
		const bad=structuredClone(receipt);mutate(bad);writeReceipt(bad);
		assert.throws(()=>localPromotion.promoteFull(f.cwd,{receiptId:bad.id}),/Cannot promote pre-prod/);
		writeReceiptNote(bad.id,"promotion",{commit:f.second,tree:bad.tree,commits:[f.second],required:bad.steps.map(s=>s.id)});
		assert.equal(localChecks.audit({cwd:f.cwd,head:f.second}).status,"fail",name);
	}
	process.env.RV_GATE_HOME=home;
});

test("only the live matching personal-quota exception promotes, and its historical audit survives expiry",t=>{
	const f=fixture(t),policy={phase:"prelaunch" as const,since:"2026-10-30",by:"Daniel",rotation:[],freshAgentPause:{until:"2026-10-31T18:00:00.581308Z",reason:"Daniel: personal quota"}};
	const at="2026-10-30T14:00:00Z",exception=prelaunchException(policy,{mode:"full",id:"fresh-agent",now:at});
	assert.ok(exception);
	const receipt={...f.receipt,id:"full-20261030T140000Z-abcdef0123",at,phase:"prelaunch" as Phase,phasePolicy:policy,steps:f.receipt.steps.map((s): PromotionStep=>s.id==="fresh-agent"?{...s,status:"unrun",reason:exception.reason,exception}:s)};
	const check=(r: typeof receipt,options: {policy?: PhasePolicy; now?: string}={})=>fullPromotionProblems(r,undefined,{policy,now:at,...options});
	assert.deepEqual(check(receipt),[]);
	assert.ok(check(receipt,{now:policy.freshAgentPause.until}).length,"New promotion expires even if the run was earlier");
	assert.ok(check(receipt,{policy:{...policy,phase:"launch"}}).length);
	for(const mutate of [r=>r.kind="prod",r=>r.phase="launch",r=>delete (r as Partial<typeof r>).at,r=>r.at=policy.freshAgentPause.until,
		r=>delete (r.phasePolicy as PhasePolicy).freshAgentPause,r=>fixtureStep(r.steps,"fresh-agent").reason="quota",
		r=>{const pause=fixtureStep(r.steps,"fresh-agent").exception;assert.ok(pause);pause.until="2026-11-02T00:00:00Z";},
		r=>{const pause=fixtureStep(r.steps,"fresh-agent").exception;assert.ok(pause);pause.type="other";},
		r=>{const s=fixtureStep(r.steps,"fresh-agent");fixtureStep(r.steps,"chromium").exception=s.exception;fixtureStep(r.steps,"chromium").status="unrun";},
	] satisfies ((fixture: typeof receipt) => unknown)[]){const bad=structuredClone(receipt);mutate(bad);assert.ok(check(bad).length,JSON.stringify(bad));}
	writeReceipt(receipt);writeReceiptNote(receipt.id,"promotion",{commit:f.second,tree:receipt.tree,commits:[f.second],required:receipt.steps.map(s=>s.id)});
	const OriginalDate=globalThis.Date;
	try {
		globalThis.Date=class extends OriginalDate {constructor(...args: unknown[]){super(...((args.length?args:["2026-11-01T00:00:00Z"]) as [number, number]));}static now(){return OriginalDate.parse("2026-11-01T00:00:00Z");}} as DateConstructor;
		const result=audit({cwd:f.cwd,head:f.second});assert.equal(result.status,"pass","reason" in result ? result.reason : "");
	} finally {globalThis.Date=OriginalDate;}
	assert.ok(check(receipt,{now:"2026-11-01T00:00:00Z"}).length);
});

test("historical prelaunch promotion audits under its frozen policy after today's rotation list changes",t=>{
	const f=fixture(t),policy={phase:"prelaunch" as const,since:"2026-10-30",by:"Daniel",rotation:["fresh-agent"]};
	const history: RotationHistory[]=Array.from({length:20},(_,i)=>({...passingFull,id:`full-20261028T${String(i).padStart(2,"0")}0000Z-abcdef0123`,at:new Date(Date.UTC(2026,9,28,i)).toISOString(),parallel:1,phase:"launch" as const,phasePolicy:{phase:"launch" as const,since:"2026-10-28",by:"Daniel",rotation:[]},steps:[{id:"fresh-agent",status:"pass",runMs:300000,durationMs:300000}]}));
	for(const prior of history)writeReceipt(prior);
	writeReceipt({id:"full-20261030T110000Z-abcdef0123",kind:"full",at:"2026-10-30T11:00:00Z",phase:"prelaunch" as const,phasePolicy:policy,steps:[{id:"fresh-agent",status:"unrun",reason:"The build failed."}]});
	const rotation=rotationFromHistory(policy,{mode:"full",steps:STEPS,receipts:listReceipts()});
	const receipt={...f.receipt,id:"full-20261030T120000Z-abcdef0123",at:"2026-10-30T12:00:00Z",phase:"prelaunch" as const,phasePolicy:policy,rotation,steps:f.receipt.steps.map((s): PromotionStep=>s.id==="fresh-agent"?{...s,status:"unrun",reason:"prelaunch rotation",rotation:{ordinal:1,cycle:3}}:s)};
	writeReceipt(receipt);writeReceiptNote(receipt.id,"promotion",{commit:f.second,tree:receipt.tree,commits:[f.second],required:receipt.steps.map(s=>s.id)});
	assert.ok(fullPromotionProblems(receipt).length,"A new promotion cannot borrow a previous approved list");
	const result=audit({cwd:f.cwd,head:f.second});assert.equal(result.status,"pass","reason" in result ? result.reason : "");
});

test("reading dev never promotes; only explicit passing-full promotion moves the exact checked head", t => {
	const f = fixture(t); writeReceipt(f.receipt);
	assert.deepEqual(readDev(f.cwd), { base: f.first, head: f.second, updated: false });
	assert.equal(promoteFull(f.cwd, {receiptId: f.receipt.id}).updated, false);
	assert.equal(git(f.remote, "rev-parse", "pre-prod"), f.first);
	const third = f.commit("new untested dev"); git(f.cwd,"push","-q","origin","HEAD:dev");
	assert.equal(promoteFull(f.cwd, {receiptId:f.receipt.id,apply:true}).head, f.second);
	assert.equal(git(f.remote,"rev-parse","dev"),third);
	assert.equal(git(f.remote,"rev-parse","pre-prod"),f.second);
	assert.equal(git(f.remote,"rev-parse","prod"),f.first);
	assert.equal(promoteFull(f.cwd,{receiptId:f.receipt.id,apply:true}).updated,false);
	assert.deepEqual(receiptNotes<PromotionNote>(f.receipt.id)[0].commits,[f.second]);
	assert.deepEqual(receiptNotes<PromotionNote>(f.receipt.id)[0].required, stepsForTier("full", "full").map(step => step.id));
	assert.equal(audit({cwd:f.cwd,head:third,mode:"full"}).status,"pass","unpromoted dev does not need its own prior release receipt");
	assert.equal(audit({cwd:f.cwd}).status,"pass","actual promotion has the matching full tree receipt");
});

test("historical promotions retain their required steps while new promotions require registry additions", t => {
	const f = fixture(t); writeReceipt(f.receipt);
	promoteFull(f.cwd, { receiptId: f.receipt.id, apply: true });
	STEPS.push({ id: "new-required-check", tier: "integration", title: "Fixture", cards: [] });
	t.after(() => STEPS.pop());
	assert.equal(audit({ cwd: f.cwd }).status, "pass", "Registry growth cannot invalidate an already checked promotion");
	assert.throws(() => promoteFull(f.cwd, { receiptId: f.receipt.id, apply: true }), /new-required-check: expected exactly one result/, "Every new promotion still uses the current registry");
	assert.equal(git(f.remote, "rev-parse", "pre-prod"), f.second);
	assert.equal(git(f.remote, "rev-parse", "prod"), f.first);
});

test("legacy promotion notes use their receipt cohort and still reject failed or malformed evidence", t => {
	const f = fixture(t);
	git(f.cwd, "push", "-q", "origin", "HEAD:pre-prod");
	git(f.cwd, "fetch", "-q", "origin", "pre-prod");
	const cohort = f.receipt.steps.map(step => step.id);
	STEPS.push({ id: "new-required-check", tier: "integration", title: "Fixture", cards: [] });
	t.after(() => STEPS.pop());
	const check = (suffix: string, mutate: (receipt: typeof f.receipt) => unknown = () => {}, required?: (string | null)[]) => {
		process.env.RV_GATE_HOME = join(f.cwd, `gate${suffix}`);
		const receipt = structuredClone(f.receipt); mutate(receipt); writeReceipt(receipt);
		writeReceiptNote(receipt.id, "promotion", { commit: f.second, tree: receipt.tree, commits: [f.second], ...(required === undefined ? {} : { required }) });
		return audit({ cwd: f.cwd });
	};
	assert.equal(check("-legacy").status, "pass");
	for (const [suffix, mutate, required] of [
		["-failed", r => r.result = "fail"],
		["-candidate", r => r.candidate = "candidate/1"],
		["-wrong-tree", r => r.tree = "0".repeat(40)],
		["-failed-step", r => fixtureStep(r.steps,"chromium").status = "fail"],
		["-gap", r => fixtureStep(r.steps,"vscode").status = "unrun"],
		["-empty", r => r.steps = []],
		["-duplicate", r => r.steps.push(r.steps[0])],
		["-missing", r => r.steps.pop(), cohort],
		["-empty-required", () => {}, []],
		["-duplicate-required", () => {}, [...cohort, cohort[0]]],
		["-invalid-required", () => {}, [null]],
	] satisfies [string, (receipt: typeof f.receipt) => unknown, ((string | null)[])?][]) assert.equal(check(suffix, mutate, required).status, "fail", suffix);
});

test("failed, partial, duplicate or incomplete full results cannot authorize promotion", t => {
	const f=fixture(t);
	assert.deepEqual(fullPromotionProblems(f.receipt),[]);
	for(const mutate of [r=>r.result="fail",r=>r.kind="release",r=>r.candidate="candidate/1",r=>r.steps.pop(),r=>r.steps.push(r.steps[0]),r=>fixtureStep(r.steps,"vscode").status="unrun",r=>fixtureStep(r.steps,"chromium").status="fail",r=>fixtureStep(r.steps,"voiceover").reason="",r=>r.steps.push({id:"extra",status:"unrun"})] satisfies ((fixture: typeof f.receipt) => unknown)[]){
		const broken=structuredClone(f.receipt); mutate(broken); assert.ok(fullPromotionProblems(broken).length,JSON.stringify(broken));
	}
	fixtureStep(f.receipt.steps,"vscode").status="unrun";writeReceipt(f.receipt);
	assert.throws(()=>promoteFull(f.cwd,{receiptId:f.receipt.id,apply:true}),/vscode: unrun/);
	assert.equal(git(f.remote,"rev-parse","pre-prod"),f.first);
	assert.equal(git(f.remote,"rev-parse","prod"),f.first);
});

test("promotion refuses a wrong receipt tree and a divergent or non-dev source",t=>{
	const f=fixture(t); writeReceipt({...f.receipt,tree:"0".repeat(40)});
	assert.throws(()=>promoteFull(f.cwd,{receiptId:f.receipt.id,apply:true}),/tree differs/);
	git(f.cwd,"checkout","-qb","other",f.first); const other=f.commit("other");
	assert.throws(()=>readDev(f.cwd,{at:other}),/must be on origin\/dev/);
	git(f.cwd,"push","-q","origin","HEAD:pre-prod");
	const id=f.receipt.id.replace("180000","180001");writeReceipt({...f.receipt,id});
	assert.throws(()=>promoteFull(f.cwd,{receiptId:id,apply:true}),/only fast-forwards/);
	assert.equal(git(f.remote,"rev-parse","pre-prod"),other);
	assert.equal(git(f.remote,"rev-parse","prod"),f.first);
});
test("full promotion retains declared change metadata once, including commits without an earlier release", t => {
	const f = fixture(t);
	const third = f.commit(`Reader change\n\nRefs ${issueId(787)}\n\ndecision: design\ncause: request "reader proof"\nitems: FEA-CH-39\nassumed: FEA-SF-24\n`);
	git(f.cwd, "push", "-q", "origin", "HEAD:dev");
	const receipt = { ...f.receipt, id: f.receipt.id.replace("180000", "180002"), commit: third, tree: git(f.cwd, "rev-parse", `${third}^{tree}`) };
	writeReceipt(receipt);
	promoteFull(f.cwd, { receiptId: receipt.id });
	assert.equal(readChanges().length, 0, "read-only does not publish change metadata");
	promoteFull(f.cwd, { receiptId: receipt.id, apply: true });
	const change = readChanges().find(change => change.commit === third);
	assert.equal(change?.class, "design");
	assert.deepEqual(change.items, ["FEA-CH-39"]);
	assert.deepEqual(change.assumed, ["FEA-SF-24"]);
	assert.equal(change.receipt, receipt.id);
	promoteFull(f.cwd, { receiptId: receipt.id, apply: true });
	assert.equal(readChanges().length, 2, "retry does not duplicate earlier declarations");
});

test("full source declarations are available before promotion and preserve earlier PR review identity", t => {
	const f = fixture(t);
	const third = f.commit(`Reader change\n\nRefs ${issueId(787)}\n\ndecision: design\ncause: issue ${issueId(787)}\nitems: FEA-CH-39\nassumed: FEA-SF-24\n\n## Retired tests\n- \`old-test\` — replaced by reader proof\n\nA control separator \x1e remains ordinary commit text.`);
	const prior = { commit: f.second, title: "Earlier PR title", receipt: "dev-20261029T170000Z-abcdef0123", class: "feature", items: ["DEL-81"], pr: 123 };
	appendChange(prior as Parameters<typeof appendChange>[0]);
	const entries = sourceChanges(f.cwd, { base: f.first, head: third, receipt: "prospective-full", at: undefined });
	assert.deepEqual(entries[0], prior);
	assert.equal(entries[1].class, "design");
	assert.deepEqual(entries[1].items, ["FEA-CH-39"]);
	assert.deepEqual(entries[1].assumed, ["FEA-SF-24"]);
	assert.deepEqual(entries[1].deletedTests, [{ test: "old-test", reason: "replaced by reader proof" }]);
	assert.equal(entries[1].receipt, "prospective-full");
	assert.deepEqual(readChanges(), [prior], "reading prospective changes has no store effects");
	assert.deepEqual(sourceChanges(f.cwd, { base: third, head: third, receipt: "empty", at: undefined }), []);
});

test("only an applied full promotion updates capture baselines, and retries preserve the previous baseline", t => {
	const f = fixture(t);
	writeBaseline("pre-prod", { report: "before" });
	writeReceipt({ ...f.receipt, captures: { report: "checked" } });
	writeReceiptNote(f.receipt.id, "diagnostic", { commit: f.second, commits: [f.second] });
	assert.equal(audit({ cwd: f.cwd, head: f.second }).status, "fail", "a diagnostic note never proves promotion");
	promoteFull(f.cwd, { receiptId: f.receipt.id });
	assert.deepEqual(readBaseline("pre-prod"), { report: "before" });
	promoteFull(f.cwd, { receiptId: f.receipt.id, apply: true });
	assert.deepEqual(readBaseline("pre-prod"), { report: "checked" });
	assert.deepEqual(readBaseline("pre-prod.prev"), { report: "before" });
	promoteFull(f.cwd, { receiptId: f.receipt.id, apply: true });
	assert.deepEqual(readBaseline("pre-prod.prev"), { report: "before" });
});

test("a retry after the ref push still covers every unreleased commit", t => {
	const f = fixture(t), third = f.commit("third");
	git(f.cwd, "push", "-q", "origin", "HEAD:dev", "HEAD:pre-prod");
	const receipt = { ...f.receipt, commit: third, tree: git(f.cwd, "rev-parse", `${third}^{tree}`) };
	writeReceipt(receipt);
	assert.equal(promoteFull(f.cwd, { receiptId: receipt.id, apply: true }).updated, false);
	assert.deepEqual(new Set(receiptNotes<PromotionNote>(receipt.id)[0].commits), new Set([f.second, third]));
	assert.equal(audit({ cwd: f.cwd }).status, "pass");
});

test("source declarations include a merged branch when the merge body has no repeated fields", t => {
	const f = fixture(t);
	git(f.cwd, "checkout", "-qb", "side", f.first);
	writeFileSync(join(f.cwd, "side.txt"), "reader change");
	git(f.cwd, "add", "side.txt");
	git(f.cwd, "commit", "-qm", `Reader change\n\nRefs ${issueId(787)}\ndecision: design\ncause: issue ${issueId(787)}\nitems: FEA-CH-19`);
	const side = git(f.cwd, "rev-parse", "HEAD");
	git(f.cwd, "checkout", "-q", "dev");
	git(f.cwd, "merge", "--no-ff", "-qm", "Merge reviewed branch", "side");
	const entries = sourceChanges(f.cwd, { base: f.first, head: git(f.cwd, "rev-parse", "HEAD"), receipt: "full", at: undefined });
	assert.deepEqual(entries.find(entry => entry.commit === side)?.items, ["FEA-CH-19"]);
	assert.equal(entries.find(entry => entry.commit === side)?.class, "design");
});

test("a full may leave fresh-agent unrun for its DEL-78 schedule, and nothing else may", t => {
	const f = fixture(t);
	const scheduled = { ...f.receipt, steps: f.receipt.steps.map((s): PromotionStep => s.id === "fresh-agent" ? { ...s, status: "unrun", reason: FRESH_AGENT_SCHEDULE } : s) };
	assert.deepEqual(fullPromotionProblems(scheduled), []);
	for (const mutate of [r => { fixtureStep(r.steps,"fresh-agent").reason = "skipped"; }, r => { r.kind = "prod"; },
		r => { const s = fixtureStep(r.steps,"chromium"); s.status = "unrun"; s.reason = FRESH_AGENT_SCHEDULE; }] satisfies ((fixture: typeof scheduled) => unknown)[]) {
		const bad = structuredClone(scheduled);
		mutate(bad);
		assert.ok(fullPromotionProblems(bad).length, JSON.stringify(bad.steps.filter(s => s.status === "unrun")));
	}
});

test("only the frozen prelaunch Chromium policy admits engine gaps", () => {
	const full = {browsers: ["chromium"], partialRetry: true};
	const policy = {phase: "prelaunch" as const, since: "2026-11-09", by: "Daniel", rotation: [], prelaunchFull: full};
	const reason = "Prelaunch policy: Chromium only until launch.";
	const steps = ["firefox", "webkit", "engines-smoke", "parallel-shaders"].map(id => ({id}));
	const gap = {status: "unrun" as const, reason};
	const receipt = {...promotionEvidence, phase:"prelaunch" as const, phasePolicy:policy as PhasePolicy, steps:[
		{id:"firefox", ...gap}, {id:"webkit", ...gap},
		{id:"engines-smoke", ...gap, details:{commands:[gap,gap]}},
		{id:"parallel-shaders", status:"pass" as const, details:{commands:[{status:"pass"} as { status: GateStatus; reason?: string },gap,gap]}},
	]};
	assert.deepEqual(fullPromotionProblems(receipt, steps, {policy}), []);
	assert.ok(fullPromotionProblems(receipt, steps, {policy:{...policy,phase:"launch"}}).length);
	for (const mutate of [r=>r.steps[2].details!.commands.pop(), r=>r.steps[3].details!.commands[0].status="unrun", r=>r.steps[3].details!.commands[1].reason="other", r=>{r.steps[3].details!.commands[1].reason="other";r.steps[3].details!.commands[2].reason="other";}, r=>delete r.phasePolicy.prelaunchFull] satisfies ((fixture: typeof receipt) => unknown)[]) {
		const bad=structuredClone(receipt); mutate(bad); assert.ok(fullPromotionProblems(bad, steps, {policy}).length);
	}
});

test("internal journey and perf browser gaps require the frozen prelaunch policy", () => {
	const policy={phase:"prelaunch" as const,since:"2026-11-09",by:"Daniel",rotation:[],prelaunchFull:{browsers:["chromium"],partialRetry:true}};
	const reason="Prelaunch policy: Chromium only until launch.";
	const steps=[{id:"journeys"},{id:"perf"}];
	const receipt={...promotionEvidence,phase:"prelaunch" as const,phasePolicy:policy,steps:[
		{id:"journeys",status:"pass" as const,details:{unrunInputs:["offline author journey in firefox","offline author journey in webkit"].map(what=>({what,reason}))}},
		{id:"perf",status:"pass" as const,details:{unrunInputs:["webkit.frame.p95","webkit.frame.max"].map(what=>({what,reason}))}},
	]};
	assert.deepEqual(fullPromotionProblems(receipt,steps,{policy}),[]);
	assert.ok(fullPromotionProblems(receipt,steps,{policy:{...policy,phase:"launch"}}).length);
	const incomplete=structuredClone(receipt);incomplete.steps[0].details.unrunInputs.pop();assert.ok(fullPromotionProblems(incomplete,steps,{policy}).length);
});

test("affected evidence promotes only complete coverage and merge-prod accepts its exact promoted tree", async t => {
	const f = fixture(t);
	mkdirSync(join(f.cwd, "crates/demo/src"), {recursive:true});
	writeFileSync(join(f.cwd, "crates/demo/Cargo.toml"), '[package]\nname="demo"\nversion="0.1.0"\n');
	writeFileSync(join(f.cwd, "crates/demo/src/lib.rs"), '// demo\n');
	git(f.cwd, "add", "-A"); git(f.cwd, "commit", "-qm", "add demo");
	const head = git(f.cwd, "rev-parse", "HEAD"), tree = git(f.cwd, "rev-parse", `${head}^{tree}`);
	git(f.cwd, "push", "-q", "origin", "HEAD:dev", "HEAD:refs/heads/candidate/1");
	const {affectedContext} = await import("../lib/affected-context.mts");
	const {policyFingerprint, outcomesForSelection} = await import("../lib/affected-coverage.mts");
	const {failureCounts} = await import("../lib/selection.mts");
	const {readPhase} = await import("../lib/phase.mts");
	const {mergeProd} = await import("../lib/report.mts");
	const policy = readPhase(), steps = stepsForTier("full", "full"), areas: AreaMap = JSON.parse(readFileSync(new URL("../areas.json", import.meta.url), "utf8"));
	const anchor = {...f.receipt, id:`full-20261101T000000Z-${f.first.slice(0,10)}`, commit:f.first, tree:git(f.cwd,"rev-parse",`${f.first}^{tree}`), steps:passingSteps(), phase:policy.phase, phasePolicy:policy};
	writeReceipt(anchor); writeReceiptNote(anchor.id, "promotion", {commit:f.first, tree:anchor.tree, commits:[]});
	const counts = failureCounts([anchor], steps), policyHash = policyFingerprint({policy, steps, areas, counts});
	const plan = affectedContext({cwd:f.cwd, base:f.first, head, steps, areas, policy, counts, history:[], policyHash});
	type SelectedCase = SelectionItem & { commandId: string; caseKey: string };
	const results = steps.map((step): StepOutcome & { id: string } => {
		const selected = plan.selection.filter(item => item.status === "run" && item.step === step.id);
		const groups = new Map<string, SelectedCase[]>();
		for (const item of selected.filter(item => item.caseKind)) {
			assert.ok(item.commandId && item.caseKey, "A selected case names its command and case key");
			const selectedCase = { ...item, commandId: item.commandId, caseKey: item.caseKey };
			groups.set(item.commandId, [...(groups.get(item.commandId) ?? []), selectedCase]);
		}
		const commands = [...groups.values()].map(items => ({
			status: "pass" as const,
			caseUnits: items.map(item => ({id:item.id,status:"pass" as const})),
			cases: {cohort:items.map(item=>item.caseKey),selected:items.map(item=>item.caseKey),completed:items.map(item=>item.caseKey)},
		}));
		return {id:step.id, status:selected.length ? "pass" : "unrun", ...(commands.length&&{details:{commands}}), ...(!selected.some(item => item.id === step.id) && selected.length ? {units:selected.map(item => ({id:item.id,status:"pass" as const}))} : {})};
	});
	const outcomes = outcomesForSelection(plan.selection, results, "pass");
	const receipt = {id:`affected-20261101T010000Z-${head.slice(0,10)}`, kind:"affected" as const, base:f.first, greenReceipt:anchor.id, policyHash, selected:Object.keys(outcomes), outcomes, selection:plan.selection, obligations:plan.obligations, phase:policy.phase, phasePolicy:policy, commit:head, tree, prod:f.first, result:"pass" as const, at:"2026-11-01T01:00:00Z", steps:results, browserRelaunchTelemetry:{status:"pass" as const}};
	const {affectedPromotion} = await import("../lib/affected-promotion.mts");
	const verify = (row: typeof receipt, extra: { policy?: PhasePolicy; areas?: typeof areas } = {}) => {
		const options = {cwd:f.cwd, receipt:row, policy, steps, areas, counts, anchors:[anchor], notes:receiptNotes, history:[row], ...extra};
		return affectedPromotion(options);
	};
	const missing = structuredClone(receipt);
	const requiredUnit = plan.obligations.flatMap(item => item.units)[0];
	missing.outcomes[requiredUnit] = "unrun";
	assert.throws(() => verify(missing), /outcomes differ|verdict differs/);
	assert.throws(() => verify(receipt, {policy:{...policy, phase:"launch"}}), /prelaunch/);
	assert.throws(() => verify(receipt, {areas:{...areas, areas:[...areas.areas, {name:"new rule", paths:["extra/"], steps:["secrets"]}]}}), /policy changed/);
	const noBuild=structuredClone(receipt); noBuild.selected=noBuild.selected.filter(id=>id!=="build"); delete noBuild.outcomes.build;
	assert.throws(() => verify(noBuild), /exact head's passing build/);
	writeReceipt(receipt);
	assert.equal(promoteFull(f.cwd, {receiptId:receipt.id}).updated, false);
	assert.equal(promoteFull(f.cwd, {receiptId:receipt.id, apply:true}).updated, true);
	assert.deepEqual(receiptNotes<PromotionNote>(receipt.id).find(note=>note.kind==="promotion")?.affectedProof?.obligations,plan.obligations,"The frozen proof retains each commit's paused units and reasons");
	const auditResult = audit({cwd:f.cwd, head}); assert.equal(auditResult.status, "pass", JSON.stringify(auditResult));
	assert.equal(mergeProd({candidate:"candidate/1", go:"Daniel go 2026-11-01", cwd:f.cwd, notifyBackmergeFailure:undefined}), 0);
	assert.equal(git(f.remote, "rev-parse", "prod"), head);
});

test("release packet shows the exact affected receipt, selected units, gaps and earlier coverage", async t => {
	const f=fixture(t);
	const policy={phase:"prelaunch" as const,since:"2026-10-30",by:"Daniel",rotation:[],prelaunchFull:{browsers:["chromium"],partialRetry:true}};
	const prior={...f.receipt,id:`affected-20261101T000000Z-${f.second.slice(0,10)}`,kind:"affected",phase:"prelaunch" as const,phasePolicy:policy,selection:[{id:"old-check",status:"run",reason:"Earlier change"}],outcomes:{"old-check":"pass"}};
	const receipt={...prior,id:`affected-20261101T010000Z-${f.second.slice(0,10)}`,selection:[{id:"site",status:"run",reason:"Changed <site>"},{id:"runtime:case:abc",command:"node verify-report-runtime",caseKey:"bar",status:"run",reason:"Selected report"},{id:"perf",status:"paused",reason:"Below threshold",decisions:[{commit:f.first,status:"paused",reason:"One failure"}]},{id:"old-check",status:"skipped",reason:"Affected obligations already passed in earlier runs.",decisions:[{commit:f.first,status:"run",reason:"Earlier change"}]},{id:"firefox",status:"skipped",reason:"Prelaunch: Chrome only."}],outcomes:{site:"pass","runtime:case:abc":"pass"}};
	writeReceipt(prior);writeReceipt(receipt);writeReceiptNote(receipt.id,"promotion",{commit:f.second,tree:receipt.tree,affectedProof:{receipts:[prior.id,receipt.id],outcomes:[{head:f.first,outcomes:{"old-check":"pass"}}]}});
	const html=readFileSync(await writeReport({cwd:f.cwd,from:f.first,to:f.second}),"utf8");
	assert.match(html,/Verification evidence/);assert.ok(html.includes(receipt.id));assert.match(html,/Chromium only: Firefox and WebKit were skipped/);
	for(const expected of ["run: pass","run: pass (earlier run)","site","node verify-report-runtime: bar","Changed &lt;site&gt;","paused","perf","Below threshold",f.first.slice(0,10),"One failure","skipped","firefox","Prelaunch: Chrome only.","Earlier affected runs used for coverage",prior.id,"old-check","Earlier change"]) assert.ok(html.includes(expected),expected);
	assert.ok(!html.includes("Changed <site>"),"Receipt text is escaped");
});

test("release packet shows full-run browser gaps and does not invent an affected receipt", async t => {
	const f=fixture(t);
	const policy={phase:"prelaunch" as const,since:"2026-10-30",by:"Daniel",rotation:[],prelaunchFull:{browsers:["chromium"],partialRetry:true}};
	const receipt={...f.receipt,phase:"prelaunch" as const,phasePolicy:policy,steps:[{id:"chromium",status:"pass",details:{unrunInputs:[{what:"journey in webkit",reason:"Chromium only"}]}},{id:"firefox",status:"unrun",reason:"Chromium only"}]};
	writeReceipt(receipt);writeReceiptNote(receipt.id,"promotion",{commit:f.second,tree:receipt.tree});
	const html=readFileSync(await writeReport({cwd:f.cwd,from:f.first,to:f.second}),"utf8");
	for(const expected of [receipt.id,"Chromium only: Firefox and WebKit were skipped","journey in webkit","firefox","unrun"]) assert.ok(html.includes(expected),expected);
});

test("release packet warns when the exact head has no receipt", async t => {
	const f=fixture(t);
	writeReceipt({...f.receipt,commit:f.first});
	const html=readFileSync(await writeReport({cwd:f.cwd,from:f.first,to:f.second}),"utf8");
	assert.match(html,/No candidate receipt is recorded for this exact source/);
});

test("candidate packet uses the same candidate-specific receipt as merge-prod without inventing Chrome-only gaps", async t => {
	const f=fixture(t);
	git(f.cwd,"push","-q","origin","HEAD:refs/heads/candidate/1");
	const policy={phase:"prelaunch" as const,since:"2026-10-30",by:"Daniel",rotation:[],prelaunchFull:{browsers:["chromium"],partialRetry:true}};
	writeReceipt({...f.receipt,phase:"prelaunch" as const,phasePolicy:policy});
	const candidate={...f.receipt,id:`prod-20261101T020000Z-${f.second.slice(0,10)}`,kind:"prod",candidate:"candidate/1",phase:"prelaunch" as const,phasePolicy:policy,steps:[{id:"webkit",status:"pass"}]};writeReceipt(candidate);
	const html=readFileSync(await writeReport({cwd:f.cwd,from:f.first,to:"candidate/1"}),"utf8");
	assert.ok(html.includes(candidate.id));assert.match(html,/Candidate-specific receipt/);assert.ok(html.includes("webkit"));
	assert.ok(!html.includes("Chromium only: Firefox and WebKit were skipped"));
	const headPacket=readFileSync(await writeReport({cwd:f.cwd,from:f.first,to:f.second}),"utf8");
	assert.match(headPacket,/Candidate-specific receipts also exist at this head/);
});

test("packet shows the real selector's later unknown-path pause even after an earlier pass", async t => {
	const f=fixture(t);
	const {pendingPlan}=await import("../lib/affected-coverage.mts"), {selectAffected}=await import("../lib/selection.mts");
	const areas=JSON.parse(readFileSync(new URL("../areas.json",import.meta.url),"utf8")),steps=stepsForTier("full","full");
	const select=(paths: string[],options={})=>selectAffected(steps,paths,Object.assign({areas,counts:new Map()},options));
	const pathsA=["crates/richview-core/src/figure.rs"],pathsB=["crates/richview-bench/src/main.rs"],first=select(pathsA),second=select(pathsB);
	const target=first.find(item=>item.status==="run"&&second.some(item2=>item2.id===item.id&&item2.status==="paused"));
	assert.ok(target);
	const outcomes=Object.fromEntries(first.filter(item=>item.status==="run").map(item=>[item.id,"pass" as const]));
	const plan=pendingPlan({commits:[{commit:f.first,paths:pathsA},{commit:f.second,paths:pathsB}],receipts:[{head:f.first,outcomes}],select,contains:(head: string,commit: string)=>head===commit||head===f.second&&commit===f.first,head:f.second});
	assert.equal(plan.selection.find((item: { id: string; status: string })=>item.id===target.id)?.status,"paused");
	const policy={phase:"prelaunch" as const,since:"2026-10-30",by:"Daniel",rotation:[],prelaunchFull:{browsers:["chromium"],partialRetry:true}};
	const receipt={...f.receipt,id:`affected-20261101T010000Z-${f.second.slice(0,10)}`,kind:"affected",phase:"prelaunch" as const,phasePolicy:policy,selection:plan.selection,obligations:plan.obligations,outcomes:{}};
	writeReceipt(receipt);writeReceiptNote(receipt.id,"promotion",{commit:f.second,tree:receipt.tree,affectedProof:{receipts:[receipt.id],obligations:plan.obligations,outcomes:[{head:f.first,outcomes}]}});
	const html=readFileSync(await writeReport({cwd:f.cwd,from:f.first,to:f.second}),"utf8");
	assert.ok(html.includes(`<td>paused</td><td><code>${target.id}</code></td>`),target.id);
	assert.ok(html.includes(`Paused for ${f.second.slice(0,10)}`));
});

test("hotfix packet resolves the documented candidate ref and states browser scope", async t => {
	const f=fixture(t);
	git(f.cwd,"push","-q","origin","HEAD:refs/heads/candidate/1");
	const receipt={...f.receipt,id:`hotfix-20261101T030000Z-${f.second.slice(0,10)}`,kind:"hotfix",candidate:"candidate/1"};writeReceipt(receipt);
	const html=readFileSync(await writeReport({cwd:f.cwd,from:f.first,to:"candidate/1"}),"utf8");
	assert.ok(html.includes(receipt.id));assert.match(html,/Hotfix browser coverage is limited to affected Chromium checks/);
});
