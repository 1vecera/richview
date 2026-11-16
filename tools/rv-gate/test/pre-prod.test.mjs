// Host reason (ARC-03): test promotion against a disposable Git remote and immutable local receipts.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import {fileURLToPath, pathToFileURL} from "node:url";
import test from "node:test";
import {prelaunchException} from "../lib/phase.mjs";
import {rotationFromHistory} from "../lib/value.mjs";
import { fullPromotionProblems, promoteFull, readDev, sourceChanges } from "../lib/pre-prod.mjs";
import { audit } from "../lib/checks.mjs";
import { FRESH_AGENT_SCHEDULE, STEPS, stepsForTier } from "../lib/steps.mjs";
import { appendChange, listReceipts, readBaseline, readChanges, receiptNotes, writeBaseline, writeReceipt, writeReceiptNote } from "../lib/store.mjs";

const git = (cwd, ...args) => execFileSync("git", args, { cwd, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
const passingSteps = () => stepsForTier("full", "full").map(({id}) => ({id, status: ["voiceover", "hosted-smoke"].includes(id) ? "unrun" : "pass", reason: "Standing gap, Daniel29Oct"}));
test("only a qualified approved prelaunch rotation can authorize a full gap",()=>{
	const policy={phase:"prelaunch",since:"2026-10-30",by:"Daniel",rotation:["slow-a","slow-b","slow-c"]};
	const steps=policy.rotation.map(id=>({id}));
	const history=Array.from({length:20},(_,i)=>({id:`full-${i}`,kind:"full",at:new Date(Date.UTC(2026,9,28,i)).toISOString(),steps:steps.map(({id})=>({id,status:"pass",runMs:300000}))}));
	const rotation=rotationFromHistory(policy,{mode:"full",steps,receipts:history});
	const receipt={kind:"full",result:"pass",phase:"prelaunch",phasePolicy:policy,at:"2026-10-30T12:00:00Z",rotation,
		steps:steps.map(({id})=>({id,status:rotation.skipped.includes(id)?"unrun":"pass",...(rotation.skipped.includes(id)&&{reason:"prelaunch rotation",rotation:{ordinal:rotation.ordinal,cycle:3}})}))};
	const check=(r,extra={})=>fullPromotionProblems(r,steps,{policy,history,...extra});
	assert.deepEqual(check(receipt),[]);
	assert.ok(check(receipt,{policy:{...policy,phase:"launch"}}).length);
	for(const mutate of [
		r=>delete r.phasePolicy,r=>r.rotation.ordinal=-1,r=>r.rotation.selected=[],r=>r.rotation.skipped.push("slow-a"),
		r=>r.rotation.qualifications[1].timingSamples=1,r=>r.rotation.qualifications[1].medianRunMs=18000,
		r=>r.rotation.qualifications[1].receipts.fill("full-0"),r=>r.steps[1].reason="skipped",r=>r.steps[1].rotation.cycle=4,
	]){const bad=structuredClone(receipt);mutate(bad);assert.ok(check(bad).length);}
	const brokenHistory=structuredClone(history);brokenHistory[0].steps[1].flaky=true;assert.ok(check(receipt,{history:brokenHistory}).length);
	const reusedHistory=structuredClone(history);reusedHistory[0].steps[1].reused="earlier";assert.ok(check(receipt,{history:reusedHistory}).length);
	for(const [name,mutate]of [
		["zero cost",h=>h.forEach(r=>r.steps.forEach(s=>s.runMs=0))],
		["queued wall only",h=>h.forEach(r=>{r.parallel=4;r.steps.forEach(s=>{delete s.runMs;s.durationMs=900000;});})],
		["missing observation",h=>h.pop()],
		["unrun is not an execution",h=>h[0].steps[1].status="unrun"],
		["missing step",h=>h[0].steps.splice(1,1)],
		["newer failure",h=>h.push({id:"newer",kind:"full",at:"2026-10-30T11:00:00Z",steps:steps.map(({id})=>({id,status:"fail",runMs:300000}))})],
		["newer execution changes latest twenty",h=>h.push({id:"newer",kind:"full",at:"2026-10-30T11:00:00Z",steps:steps.map(({id})=>({id,status:"pass",runMs:300000}))})],
	]){const changed=structuredClone(history);mutate(changed);assert.ok(check(receipt,{history:changed}).length,`${name} cannot borrow a claimed qualifying summary`);}
	const falseMedian=structuredClone(receipt);falseMedian.rotation.qualifications[1].medianRunMs=300001;assert.ok(check(falseMedian).length,"The claimed median must match actual observation times");
	const falseHash=structuredClone(receipt);falseHash.rotation.qualifications[1].receiptHashes[0]="0".repeat(64);assert.ok(check(falseHash).length,"Qualifications bind immutable receipt contents");
	const laterHistory=[...history,{id:"later-red",kind:"full",at:"2026-10-30T13:00:00Z",steps:steps.map(({id})=>({id,status:"fail",runMs:300000}))}];
	assert.ok(check(receipt,{history:laterHistory,now:"2026-10-30T14:00:00Z"}).length,"New promotion cannot reuse a window superseded after the full");
	assert.deepEqual(check(receipt,{history:laterHistory,now:receipt.at}),[],"Historical audit freezes the observation window at execution");
	const laterUnrun=structuredClone(laterHistory);laterUnrun.at(-1).phase="prelaunch";for(const s of laterUnrun.at(-1).steps)s.status="unrun";
	assert.deepEqual(check(receipt,{history:laterUnrun,now:"2026-10-30T14:00:00Z"}),[],"Later fulls without an execution preserve the admitted due-slot plan and actual execution window");
	const allPass=structuredClone(receipt);delete allPass.rotation;allPass.steps=steps.map(({id})=>({id,status:"pass"}));allPass.steps[0].flaky=true;
	assert.deepEqual(check(allPass),[]);assert.ok(check(allPass,{policy:{...policy,phase:"launch"}}).length);
});
function fixture(t) {
	mkdirSync(join(process.cwd(), "tmp"), { recursive: true });
	const root = mkdtempSync(join(process.cwd(), "tmp/pre-prod-"));
	const previous = process.env.RV_GATE_HOME;
	process.env.RV_GATE_HOME = join(root, "home");
	t.after(() => { if (previous === undefined) delete process.env.RV_GATE_HOME; else process.env.RV_GATE_HOME = previous; rmSync(root, { recursive: true, force: true }); });
	const remote = join(root, "remote.git"), cwd = join(root, "work");
	git(root, "init", "--bare", remote);
	git(root, "init", "-b", "dev", cwd);
	for (const [key, value] of [["user.name", "Fixture"], ["user.email", "fixture@example.test"], ["commit.gpgsign", "false"], ["core.hooksPath", "/dev/null"]]) git(cwd, "config", key, value);
	const commit = text => { writeFileSync(join(cwd, "report.txt"), text); git(cwd, "add", "report.txt"); git(cwd, "commit", "-qm", text); return git(cwd, "rev-parse", "HEAD"); };
	const first = commit("first");
	git(cwd, "remote", "add", "origin", remote);
	git(cwd, "push", "-q", "origin", "HEAD:dev", "HEAD:pre-prod", "HEAD:prod");
	const second = commit("second");
	git(cwd, "push", "-q", "origin", "HEAD:dev");
	const receipt = { id: `full-20261029T180000Z-${second.slice(0,10)}`, kind: "full", commit: second, tree: git(cwd,"rev-parse",`${second}^{tree}`), prod: first, result: "pass", steps: passingSteps() };
	return { cwd, remote, first, second, commit, receipt };
}

test("lead judgment audits pre-prod as distinct provenance, never candidate or prod proof", t => {
	const f=fixture(t), third=f.commit("third");
	git(f.cwd,"push","-q","origin","HEAD:dev","HEAD:pre-prod");git(f.cwd,"fetch","-q","origin");
	const receipt={...f.receipt,id:`affected-20261110T163824Z-${third.slice(0,10)}`,kind:"affected",commit:third,tree:git(f.cwd,"rev-parse",`${third}^{tree}`),result:"fail"};
	writeReceipt(receipt);
	writeReceiptNote(receipt.id,"lead-judgment-promotion",{base:f.first,commit:third,tree:receipt.tree,authority:"Daniel 10 Nov 2026 17:25 CET permits automatic pre-prod judgment; lead explicitly authorizes this source.",resultAtPromotion:"in-progress",steps:[{id:"build",status:"pass"},{id:"site",status:"fail",reason:"stale expectation"},{id:"audit",status:"pending"}],prodEligible:false});
	const preprod=audit({cwd:f.cwd,head:third,mode:"full"});assert.equal(preprod.status,"pass",preprod.reason);assert.equal(preprod.details.judgments[0].commit,third);
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
	]) {
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
	const result=audit({cwd:f.cwd});assert.equal(result.status,"fail");assert.match(result.reason,/tree differs/);
});

test("shipping coverage, promotion and historical audit preserve only justified declared rotation gaps",async t=>{
	const f=fixture(t),home=process.env.RV_GATE_HOME;
	// Give the unchanged shipping modules a private real phase file and evidence store.
	cpSync(fileURLToPath(new URL("../../",import.meta.url)),join(f.cwd,"tools"),{recursive:true});
	const policy={phase:"prelaunch",since:"2026-10-30",by:"Daniel",rotation:["fonts"]};
	writeFileSync(join(f.cwd,"tools/rv-gate/phase.json"),JSON.stringify(policy));
	const module=path=>import(pathToFileURL(join(f.cwd,"tools/rv-gate",path)).href);
	const localChecks=await module("lib/checks.mjs"),localPromotion=await module("lib/pre-prod.mjs"),localValue=await module("lib/value.mjs");
	const history=Array.from({length:20},(_,i)=>({id:`full-20261028T${String(i).padStart(2,"0")}0000Z-abcdef0123`,kind:"full",at:new Date(Date.UTC(2026,9,28,i)).toISOString(),phase:"launch",phasePolicy:{phase:"launch",since:"2026-10-28",by:"Daniel",rotation:[]},steps:[{id:"fonts",status:"pass",runMs:300000}]}));
	history.push({id:"full-20261030T110000Z-abcdef0123",kind:"full",at:"2026-10-30T11:00:00Z",phase:"prelaunch",phasePolicy:policy,steps:[{id:"fonts",status:"unrun",reason:"Build failed"}]});
	for(const prior of history)writeReceipt(prior);
	const rotation=localValue.rotationFromHistory(policy,{mode:"full",steps:STEPS,receipts:listReceipts()});
	assert.deepEqual(rotation.skipped,["fonts"]);
	const receipt={...f.receipt,id:"full-20261030T120000Z-abcdef0123",at:"2026-10-30T12:00:00Z",phase:"prelaunch",phasePolicy:policy,rotation,steps:f.receipt.steps.map(s=>s.id==="fonts"?{...s,status:"unrun",reason:"prelaunch rotation",rotation:{ordinal:rotation.ordinal,cycle:3}}:s)};
	const context={cwd:f.cwd,pr:{items:["DEL-82"]},completed:new Map(receipt.steps.map(s=>[s.id,s.status]))};
	assert.equal(localChecks.coverage(context).status,"pass");assert.equal(context.completed.get("fonts"),"unrun");
	const mapPath=join(f.cwd,"tools/rv-gate/coverage.json"),mapBytes=readFileSync(mapPath),badMap=JSON.parse(mapBytes);
	badMap.items["DEL-82"].checks=["not-a-declared-step"];writeFileSync(mapPath,JSON.stringify(badMap));
	assert.equal(localChecks.coverage(context).status,"fail","Rotation cannot make an unresolved coverage declaration pass");writeFileSync(mapPath,mapBytes);
	writeReceipt(receipt);assert.equal(localPromotion.promoteFull(f.cwd,{receiptId:receipt.id}).updated,false);
	assert.equal(localPromotion.promoteFull(f.cwd,{receiptId:receipt.id,apply:true}).updated,true);assert.equal(git(f.remote,"rev-parse","pre-prod"),f.second);assert.equal(git(f.remote,"rev-parse","prod"),f.first);
	const priors=listReceipts().filter(r=>r.id!==receipt.id);
	const later={id:"full-20261030T130000Z-abcdef0123",kind:"full",at:"2026-10-30T13:00:00Z",phase:"prelaunch",phasePolicy:policy,steps:[{id:"fonts",status:"fail",runMs:300000}]};writeReceipt(later);
	assert.equal(localChecks.audit({cwd:f.cwd,head:f.second}).status,"pass","Later failures do not rewrite a valid historical promotion");
	assert.throws(()=>localPromotion.promoteFull(f.cwd,{receiptId:receipt.id}),/actual latest executed history/);
	const localRegistry=(await module("lib/steps.mjs")).STEPS;
	localRegistry.push({id:"new-artifact-consumer",tier:"integration",after:["fonts"]});
	try {
		const old=localChecks.audit({cwd:f.cwd,head:f.second});assert.equal(old.status,"pass",`Historical dependency protection must use the admitted registry: ${old.reason}`);
		assert.throws(()=>localPromotion.promoteFull(f.cwd,{receiptId:receipt.id}),/Cannot promote pre-prod/,"New promotion retains today's dependency protection");
	} finally {localRegistry.pop();}
	for(const [name,mutate]of [
		["unjustified-reason",r=>r.steps.find(s=>s.id==="fonts").reason="manual skip"],
		["unapproved-step",r=>Object.assign(r.steps.find(s=>s.id==="host"),{status:"unrun",reason:"prelaunch rotation"})],
		["false-cost",r=>r.rotation.qualifications[0].medianRunMs=900000],
	]){
		process.env.RV_GATE_HOME=join(f.cwd,name);for(const prior of priors)writeReceipt(prior);
		const bad=structuredClone(receipt);mutate(bad);writeReceipt(bad);
		assert.throws(()=>localPromotion.promoteFull(f.cwd,{receiptId:bad.id}),/Cannot promote pre-prod/);
		writeReceiptNote(bad.id,"promotion",{commit:f.second,tree:bad.tree,commits:[f.second],required:bad.steps.map(s=>s.id)});
		assert.equal(localChecks.audit({cwd:f.cwd,head:f.second}).status,"fail",name);
	}
	process.env.RV_GATE_HOME=home;
});

test("only the live matching personal-quota exception promotes, and its historical audit survives expiry",t=>{
	const f=fixture(t),policy={phase:"prelaunch",since:"2026-10-30",by:"Daniel",rotation:[],freshAgentPause:{until:"2026-10-31T18:00:00.581308Z",reason:"Daniel: personal quota"}};
	const at="2026-10-30T14:00:00Z",exception=prelaunchException(policy,{mode:"full",id:"fresh-agent",now:at});
	const receipt={...f.receipt,id:"full-20261030T140000Z-abcdef0123",at,phase:"prelaunch",phasePolicy:policy,steps:f.receipt.steps.map(s=>s.id==="fresh-agent"?{...s,status:"unrun",reason:exception.reason,exception}:s)};
	const check=(r,options={})=>fullPromotionProblems(r,undefined,{policy,now:at,...options});
	assert.deepEqual(check(receipt),[]);
	assert.ok(check(receipt,{now:policy.freshAgentPause.until}).length,"New promotion expires even if the run was earlier");
	assert.ok(check(receipt,{policy:{...policy,phase:"launch"}}).length);
	for(const mutate of [r=>r.kind="prod",r=>r.phase="launch",r=>delete r.at,r=>r.at=policy.freshAgentPause.until,
		r=>delete r.phasePolicy.freshAgentPause,r=>r.steps.find(s=>s.id==="fresh-agent").reason="quota",
		r=>r.steps.find(s=>s.id==="fresh-agent").exception.until="2026-11-02T00:00:00Z",
		r=>r.steps.find(s=>s.id==="fresh-agent").exception.type="other",
		r=>{const s=r.steps.find(s=>s.id==="fresh-agent");r.steps.find(s=>s.id==="chromium").exception=s.exception;r.steps.find(s=>s.id==="chromium").status="unrun";},
	]){const bad=structuredClone(receipt);mutate(bad);assert.ok(check(bad).length,JSON.stringify(bad));}
	writeReceipt(receipt);writeReceiptNote(receipt.id,"promotion",{commit:f.second,tree:receipt.tree,commits:[f.second],required:receipt.steps.map(s=>s.id)});
	const OriginalDate=globalThis.Date;
	try {
		globalThis.Date=class extends OriginalDate {constructor(...args){super(...(args.length?args:["2026-11-01T00:00:00Z"]));}static now(){return OriginalDate.parse("2026-11-01T00:00:00Z");}};
		const result=audit({cwd:f.cwd,head:f.second});assert.equal(result.status,"pass",result.reason);
	} finally {globalThis.Date=OriginalDate;}
	assert.ok(check(receipt,{now:"2026-11-01T00:00:00Z"}).length);
});

test("historical prelaunch promotion audits under its frozen policy after today's rotation list changes",t=>{
	const f=fixture(t),policy={phase:"prelaunch",since:"2026-10-30",by:"Daniel",rotation:["fresh-agent"]};
	const history=Array.from({length:20},(_,i)=>({id:`full-20261028T${String(i).padStart(2,"0")}0000Z-abcdef0123`,kind:"full",at:new Date(Date.UTC(2026,9,28,i)).toISOString(),parallel:1,phase:"launch",phasePolicy:{phase:"launch",since:"2026-10-28",by:"Daniel",rotation:[]},steps:[{id:"fresh-agent",status:"pass",runMs:300000,durationMs:300000}]}));
	for(const prior of history)writeReceipt(prior);
	writeReceipt({id:"full-20261030T110000Z-abcdef0123",kind:"full",at:"2026-10-30T11:00:00Z",phase:"prelaunch",phasePolicy:policy,steps:[{id:"fresh-agent",status:"unrun",reason:"The build failed."}]});
	const rotation=rotationFromHistory(policy,{mode:"full",steps:STEPS,receipts:listReceipts()});
	const receipt={...f.receipt,id:"full-20261030T120000Z-abcdef0123",at:"2026-10-30T12:00:00Z",phase:"prelaunch",phasePolicy:policy,rotation,steps:f.receipt.steps.map(s=>s.id==="fresh-agent"?{...s,status:"unrun",reason:"prelaunch rotation",rotation:{ordinal:1,cycle:3}}:s)};
	writeReceipt(receipt);writeReceiptNote(receipt.id,"promotion",{commit:f.second,tree:receipt.tree,commits:[f.second],required:receipt.steps.map(s=>s.id)});
	assert.ok(fullPromotionProblems(receipt).length,"A new promotion cannot borrow a previous approved list");
	const result=audit({cwd:f.cwd,head:f.second});assert.equal(result.status,"pass",result.reason);
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
	assert.deepEqual(receiptNotes(f.receipt.id)[0].commits,[f.second]);
	assert.deepEqual(receiptNotes(f.receipt.id)[0].required, stepsForTier("full", "full").map(step => step.id));
	assert.equal(audit({cwd:f.cwd,head:third,mode:"full"}).status,"pass","unpromoted dev does not need its own prior release receipt");
	assert.equal(audit({cwd:f.cwd}).status,"pass","actual promotion has the matching full tree receipt");
});

test("historical promotions retain their required steps while new promotions require registry additions", t => {
	const f = fixture(t); writeReceipt(f.receipt);
	promoteFull(f.cwd, { receiptId: f.receipt.id, apply: true });
	STEPS.push({ id: "new-required-check", tier: "integration" });
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
	STEPS.push({ id: "new-required-check", tier: "integration" });
	t.after(() => STEPS.pop());
	const check = (suffix, mutate = () => {}, required) => {
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
		["-failed-step", r => r.steps.find(step => step.id === "chromium").status = "fail"],
		["-gap", r => r.steps.find(step => step.id === "vscode").status = "unrun"],
		["-empty", r => r.steps = []],
		["-duplicate", r => r.steps.push(r.steps[0])],
		["-missing", r => r.steps.pop(), cohort],
		["-empty-required", () => {}, []],
		["-duplicate-required", () => {}, [...cohort, cohort[0]]],
		["-invalid-required", () => {}, [null]],
	]) assert.equal(check(suffix, mutate, required).status, "fail", suffix);
});

test("failed, partial, duplicate or incomplete full results cannot authorize promotion", t => {
	const f=fixture(t);
	assert.deepEqual(fullPromotionProblems(f.receipt),[]);
	for(const mutate of [r=>r.result="fail",r=>r.kind="release",r=>r.candidate="candidate/1",r=>r.steps.pop(),r=>r.steps.push(r.steps[0]),r=>r.steps.find(s=>s.id==="vscode").status="unrun",r=>r.steps.find(s=>s.id==="chromium").status="fail",r=>r.steps.find(s=>s.id==="voiceover").reason="",r=>r.steps.push({id:"extra",status:"unrun"})]){
		const broken=structuredClone(f.receipt); mutate(broken); assert.ok(fullPromotionProblems(broken).length,JSON.stringify(broken));
	}
	f.receipt.steps.find(s=>s.id==="vscode").status="unrun";writeReceipt(f.receipt);
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
	const third = f.commit('Reader change\n\nRefs S27-787\n\ndecision: design\ncause: request "reader proof"\nitems: FEA-CH-39\nassumed: FEA-SF-24\n');
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
	const third = f.commit("Reader change\n\nRefs S27-787\n\ndecision: design\ncause: issue S27-787\nitems: FEA-CH-39\nassumed: FEA-SF-24\n\n## Retired tests\n- `old-test` — replaced by reader proof\n\nA control separator \x1e remains ordinary commit text.");
	const prior = { commit: f.second, title: "Earlier PR title", receipt: "dev-20261029T170000Z-abcdef0123", class: "feature", items: ["DEL-81"], pr: 123 };
	appendChange(prior);
	const entries = sourceChanges(f.cwd, { base: f.first, head: third, receipt: "prospective-full" });
	assert.deepEqual(entries[0], prior);
	assert.equal(entries[1].class, "design");
	assert.deepEqual(entries[1].items, ["FEA-CH-39"]);
	assert.deepEqual(entries[1].assumed, ["FEA-SF-24"]);
	assert.deepEqual(entries[1].deletedTests, [{ test: "old-test", reason: "replaced by reader proof" }]);
	assert.equal(entries[1].receipt, "prospective-full");
	assert.deepEqual(readChanges(), [prior], "reading prospective changes has no store effects");
	assert.deepEqual(sourceChanges(f.cwd, { base: third, head: third, receipt: "empty" }), []);
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
	assert.deepEqual(new Set(receiptNotes(receipt.id)[0].commits), new Set([f.second, third]));
	assert.equal(audit({ cwd: f.cwd }).status, "pass");
});

test("source declarations include a merged branch when the merge body has no repeated fields", t => {
	const f = fixture(t);
	git(f.cwd, "checkout", "-qb", "side", f.first);
	writeFileSync(join(f.cwd, "side.txt"), "reader change");
	git(f.cwd, "add", "side.txt");
	git(f.cwd, "commit", "-qm", "Reader change\n\nRefs S27-787\ndecision: design\ncause: issue S27-787\nitems: FEA-CH-19");
	const side = git(f.cwd, "rev-parse", "HEAD");
	git(f.cwd, "checkout", "-q", "dev");
	git(f.cwd, "merge", "--no-ff", "-qm", "Merge reviewed branch", "side");
	const entries = sourceChanges(f.cwd, { base: f.first, head: git(f.cwd, "rev-parse", "HEAD"), receipt: "full" });
	assert.deepEqual(entries.find(entry => entry.commit === side)?.items, ["FEA-CH-19"]);
	assert.equal(entries.find(entry => entry.commit === side)?.class, "design");
});

test("a full may leave fresh-agent unrun for its DEL-78 schedule, and nothing else may", t => {
	const f = fixture(t);
	const scheduled = { ...f.receipt, steps: f.receipt.steps.map(s => s.id === "fresh-agent" ? { ...s, status: "unrun", reason: FRESH_AGENT_SCHEDULE } : s) };
	assert.deepEqual(fullPromotionProblems(scheduled), []);
	for (const mutate of [r => { r.steps.find(s => s.id === "fresh-agent").reason = "skipped"; }, r => { r.kind = "prod"; },
		r => { const s = r.steps.find(s => s.id === "chromium"); s.status = "unrun"; s.reason = FRESH_AGENT_SCHEDULE; }]) {
		const bad = structuredClone(scheduled);
		mutate(bad);
		assert.ok(fullPromotionProblems(bad).length, JSON.stringify(bad.steps.filter(s => s.status === "unrun")));
	}
});

test("only the frozen prelaunch Chromium policy admits engine gaps", () => {
	const full = {browsers: ["chromium"], partialRetry: true};
	const policy = {phase: "prelaunch", since: "2026-11-09", by: "Daniel", rotation: [], prelaunchFull: full};
	const reason = "Prelaunch policy: Chromium only until launch.";
	const steps = ["firefox", "webkit", "engines-smoke", "parallel-shaders"].map(id => ({id}));
	const gap = {status: "unrun", reason};
	const receipt = {kind:"full", result:"pass", phase:"prelaunch", phasePolicy:policy, steps:[
		{id:"firefox", ...gap}, {id:"webkit", ...gap},
		{id:"engines-smoke", ...gap, details:{commands:[gap,gap]}},
		{id:"parallel-shaders", status:"pass", details:{commands:[{status:"pass"},gap,gap]}},
	]};
	assert.deepEqual(fullPromotionProblems(receipt, steps, {policy}), []);
	assert.ok(fullPromotionProblems(receipt, steps, {policy:{...policy,phase:"launch"}}).length);
	for (const mutate of [r=>r.steps[2].details.commands.pop(), r=>r.steps[3].details.commands[0].status="unrun", r=>r.steps[3].details.commands[1].reason="other", r=>{r.steps[3].details.commands[1].reason="other";r.steps[3].details.commands[2].reason="other";}, r=>delete r.phasePolicy.prelaunchFull]) {
		const bad=structuredClone(receipt); mutate(bad); assert.ok(fullPromotionProblems(bad, steps, {policy}).length);
	}
});

test("internal journey and perf browser gaps require the frozen prelaunch policy", () => {
	const policy={phase:"prelaunch",since:"2026-11-09",by:"Daniel",rotation:[],prelaunchFull:{browsers:["chromium"],partialRetry:true}};
	const reason="Prelaunch policy: Chromium only until launch.";
	const steps=[{id:"journeys"},{id:"perf"}];
	const receipt={kind:"full",result:"pass",phase:"prelaunch",phasePolicy:policy,steps:[
		{id:"journeys",status:"pass",details:{unrunInputs:["offline author journey in firefox","offline author journey in webkit"].map(what=>({what,reason}))}},
		{id:"perf",status:"pass",details:{unrunInputs:["webkit.frame.p95","webkit.frame.max"].map(what=>({what,reason}))}},
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
	const {affectedContext} = await import("../lib/affected-context.mjs");
	const {policyFingerprint, outcomesForSelection} = await import("../lib/affected-coverage.mjs");
	const {failureCounts} = await import("../lib/selection.mjs");
	const {readPhase} = await import("../lib/phase.mjs");
	const {mergeProd} = await import("../lib/report.mjs");
	const policy = readPhase(), steps = stepsForTier("full", "full"), areas = JSON.parse(readFileSync(new URL("../areas.json", import.meta.url)));
	const anchor = {...f.receipt, id:`full-20261101T000000Z-${f.first.slice(0,10)}`, commit:f.first, tree:git(f.cwd,"rev-parse",`${f.first}^{tree}`), steps:passingSteps(), phase:policy.phase, phasePolicy:policy};
	writeReceipt(anchor); writeReceiptNote(anchor.id, "promotion", {commit:f.first, tree:anchor.tree, commits:[]});
	const counts = failureCounts([anchor], steps), policyHash = policyFingerprint({policy, steps, areas, counts});
	const plan = affectedContext({cwd:f.cwd, base:f.first, head, steps, areas, policy, counts, history:[], policyHash});
	const results = steps.map(step => {
		const selected = plan.selection.filter(item => item.status === "run" && item.step === step.id);
		const groups = new Map();
		for (const item of selected.filter(item=>item.caseKind)) groups.set(item.commandId,[...(groups.get(item.commandId)??[]),item]);
		const commands = [...groups.values()].map(items=>({status:"pass",caseUnits:items.map(item=>({id:item.id,status:"pass"})),cases:{cohort:items.map(item=>item.caseKey),selected:items.map(item=>item.caseKey),completed:items.map(item=>item.caseKey)}}));
		return {id:step.id, status:selected.length ? "pass" : "unrun", ...(commands.length&&{details:{commands}}), ...(!selected.some(item => item.id === step.id) && selected.length ? {units:selected.map(item => ({id:item.id,status:"pass"}))} : {})};
	});
	const outcomes = outcomesForSelection(plan.selection, results, "pass");
	const receipt = {id:`affected-20261101T010000Z-${head.slice(0,10)}`, kind:"affected", base:f.first, greenReceipt:anchor.id, policyHash, selected:Object.keys(outcomes), outcomes, selection:plan.selection, obligations:plan.obligations, phase:policy.phase, phasePolicy:policy, commit:head, tree, prod:f.first, result:"pass", at:"2026-11-01T01:00:00Z", steps:results, browserRelaunchTelemetry:{status:"pass"}};
	const {affectedPromotion} = await import("../lib/affected-promotion.mjs");
	const verify = (row, extra = {}) => affectedPromotion({cwd:f.cwd, receipt:row, policy, steps, areas, counts, anchors:[anchor], notes:receiptNotes, history:[row], ...extra});
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
	assert.deepEqual(receiptNotes(receipt.id).find(note=>note.kind==="promotion").affectedProof.obligations,plan.obligations,"The frozen proof retains each commit's paused units and reasons");
	const auditResult = audit({cwd:f.cwd, head}); assert.equal(auditResult.status, "pass", JSON.stringify(auditResult));
	assert.equal(mergeProd({candidate:"candidate/1", go:"Daniel go 2026-11-01", cwd:f.cwd}), 0);
	assert.equal(git(f.remote, "rev-parse", "prod"), head);
});

test("release packet shows the exact affected receipt, selected units, gaps and earlier coverage", async t => {
	const f=fixture(t), {writeReport}=await import("../lib/report.mjs");
	const policy={phase:"prelaunch",since:"2026-10-30",by:"Daniel",rotation:[],prelaunchFull:{browsers:["chromium"],partialRetry:true}};
	const prior={...f.receipt,id:`affected-20261101T000000Z-${f.second.slice(0,10)}`,kind:"affected",phase:"prelaunch",phasePolicy:policy,selection:[{id:"old-check",status:"run",reason:"Earlier change"}],outcomes:{"old-check":"pass"}};
	const receipt={...prior,id:`affected-20261101T010000Z-${f.second.slice(0,10)}`,selection:[{id:"site",status:"run",reason:"Changed <site>"},{id:"runtime:case:abc",command:"node verify-report-runtime",caseKey:"bar",status:"run",reason:"Selected report"},{id:"perf",status:"paused",reason:"Below threshold",decisions:[{commit:f.first,status:"paused",reason:"One failure"}]},{id:"old-check",status:"skipped",reason:"Affected obligations already passed in earlier runs.",decisions:[{commit:f.first,status:"run",reason:"Earlier change"}]},{id:"firefox",status:"skipped",reason:"Prelaunch: Chrome only."}],outcomes:{site:"pass","runtime:case:abc":"pass"}};
	writeReceipt(prior);writeReceipt(receipt);writeReceiptNote(receipt.id,"promotion",{commit:f.second,tree:receipt.tree,affectedProof:{receipts:[prior.id,receipt.id],outcomes:[{head:f.first,outcomes:{"old-check":"pass"}}]}});
	const html=readFileSync(await writeReport({cwd:f.cwd,from:f.first,to:f.second}),"utf8");
	assert.match(html,/Verification evidence/);assert.ok(html.includes(receipt.id));assert.match(html,/Chromium only: Firefox and WebKit were skipped/);
	for(const expected of ["run: pass","run: pass (earlier run)","site","node verify-report-runtime: bar","Changed &lt;site&gt;","paused","perf","Below threshold",f.first.slice(0,10),"One failure","skipped","firefox","Prelaunch: Chrome only.","Earlier affected runs used for coverage",prior.id,"old-check","Earlier change"]) assert.ok(html.includes(expected),expected);
	assert.ok(!html.includes("Changed <site>"),"Receipt text is escaped");
});

test("release packet shows full-run browser gaps and does not invent an affected receipt", async t => {
	const f=fixture(t), {writeReport}=await import("../lib/report.mjs");
	const policy={phase:"prelaunch",since:"2026-10-30",by:"Daniel",rotation:[],prelaunchFull:{browsers:["chromium"],partialRetry:true}};
	const receipt={...f.receipt,phase:"prelaunch",phasePolicy:policy,steps:[{id:"chromium",status:"pass",details:{unrunInputs:[{what:"journey in webkit",reason:"Chromium only"}]}},{id:"firefox",status:"unrun",reason:"Chromium only"}]};
	writeReceipt(receipt);writeReceiptNote(receipt.id,"promotion",{commit:f.second,tree:receipt.tree});
	const html=readFileSync(await writeReport({cwd:f.cwd,from:f.first,to:f.second}),"utf8");
	for(const expected of [receipt.id,"Chromium only: Firefox and WebKit were skipped","journey in webkit","firefox","unrun"]) assert.ok(html.includes(expected),expected);
});

test("release packet warns when the exact head has no receipt", async t => {
	const f=fixture(t), {writeReport}=await import("../lib/report.mjs");
	writeReceipt({...f.receipt,commit:f.first});
	const html=readFileSync(await writeReport({cwd:f.cwd,from:f.first,to:f.second}),"utf8");
	assert.match(html,/No candidate receipt is recorded for this exact source/);
});

test("candidate packet uses the same candidate-specific receipt as merge-prod without inventing Chrome-only gaps", async t => {
	const f=fixture(t), {writeReport}=await import("../lib/report.mjs");
	git(f.cwd,"push","-q","origin","HEAD:refs/heads/candidate/1");
	const policy={phase:"prelaunch",since:"2026-10-30",by:"Daniel",rotation:[],prelaunchFull:{browsers:["chromium"],partialRetry:true}};
	writeReceipt({...f.receipt,phase:"prelaunch",phasePolicy:policy});
	const candidate={...f.receipt,id:`prod-20261101T020000Z-${f.second.slice(0,10)}`,kind:"prod",candidate:"candidate/1",phase:"prelaunch",phasePolicy:policy,steps:[{id:"webkit",status:"pass"}]};writeReceipt(candidate);
	const html=readFileSync(await writeReport({cwd:f.cwd,from:f.first,to:"candidate/1"}),"utf8");
	assert.ok(html.includes(candidate.id));assert.match(html,/Candidate-specific receipt/);assert.ok(html.includes("webkit"));
	assert.ok(!html.includes("Chromium only: Firefox and WebKit were skipped"));
	const headPacket=readFileSync(await writeReport({cwd:f.cwd,from:f.first,to:f.second}),"utf8");
	assert.match(headPacket,/Candidate-specific receipts also exist at this head/);
});

test("packet shows the real selector's later unknown-path pause even after an earlier pass", async t => {
	const f=fixture(t), {writeReport}=await import("../lib/report.mjs");
	const {pendingPlan}=await import("../lib/affected-coverage.mjs"), {selectAffected}=await import("../lib/selection.mjs");
	const areas=JSON.parse(readFileSync(new URL("../areas.json",import.meta.url))),steps=stepsForTier("full","full");
	const select=(paths,options={})=>selectAffected(steps,paths,{areas,counts:new Map(),...options});
	const pathsA=["crates/richview-core/src/figure.rs"],pathsB=["crates/richview-bench/src/main.rs"],first=select(pathsA),second=select(pathsB);
	const target=first.find(item=>item.status==="run"&&second.some(item2=>item2.id===item.id&&item2.status==="paused"));
	assert.ok(target);
	const outcomes=Object.fromEntries(first.filter(item=>item.status==="run").map(item=>[item.id,"pass"]));
	const plan=pendingPlan({commits:[{commit:f.first,paths:pathsA},{commit:f.second,paths:pathsB}],receipts:[{head:f.first,outcomes}],select,contains:(head,commit)=>head===commit||head===f.second&&commit===f.first,head:f.second});
	assert.equal(plan.selection.find(item=>item.id===target.id).status,"paused");
	const policy={phase:"prelaunch",since:"2026-10-30",by:"Daniel",rotation:[],prelaunchFull:{browsers:["chromium"],partialRetry:true}};
	const receipt={...f.receipt,id:`affected-20261101T010000Z-${f.second.slice(0,10)}`,kind:"affected",phase:"prelaunch",phasePolicy:policy,selection:plan.selection,obligations:plan.obligations,outcomes:{}};
	writeReceipt(receipt);writeReceiptNote(receipt.id,"promotion",{commit:f.second,tree:receipt.tree,affectedProof:{receipts:[receipt.id],obligations:plan.obligations,outcomes:[{head:f.first,outcomes}]}});
	const html=readFileSync(await writeReport({cwd:f.cwd,from:f.first,to:f.second}),"utf8");
	assert.ok(html.includes(`<td>paused</td><td><code>${target.id}</code></td>`),target.id);
	assert.ok(html.includes(`Paused for ${f.second.slice(0,10)}`));
});

test("hotfix packet resolves the documented candidate ref and states browser scope", async t => {
	const f=fixture(t),{writeReport}=await import("../lib/report.mjs");
	git(f.cwd,"push","-q","origin","HEAD:refs/heads/candidate/1");
	const receipt={...f.receipt,id:`hotfix-20261101T030000Z-${f.second.slice(0,10)}`,kind:"hotfix",candidate:"candidate/1"};writeReceipt(receipt);
	const html=readFileSync(await writeReport({cwd:f.cwd,from:f.first,to:"candidate/1"}),"utf8");
	assert.ok(html.includes(receipt.id));assert.match(html,/Hotfix browser coverage is limited to affected Chromium checks/);
});
