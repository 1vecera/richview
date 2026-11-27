// Host reason (ARC-03): exercise the coverage proof without moving a Git ref.
import test from "node:test";
import assert from "node:assert/strict";
import {appendFileSync, cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync} from "node:fs";
import {join} from "node:path";
import {fileURLToPath, pathToFileURL} from "node:url";
import { coverageProblems, outstandingCoverage, buildObligations, validateAffectedHistory, pendingPlan, policyFingerprint, outcomesForSelection, requireGreenBase, lastGreenBase } from "../lib/affected-coverage.mts";
import type {AffectedReceipt, CoverageReceipt} from "../lib/affected-coverage.mts";
import type {StepOutcome} from "../lib/results.mts";
import type {SelectionItem} from "../lib/selection.mts";
import {caseUnitId} from "../lib/case-selection.mts";

const at = "2026-11-01T00:00:00Z";
const order = ["base", "a", "b", "c"];
const contains = (head: string, commit: string) => order.indexOf(head) >= order.indexOf(commit) && order.includes(head) && order.includes(commit);
const obligation = [{commit: "a", units: ["chromium:0", "site"]}, {commit: "b", units: ["chromium:1"]}];
const receipt = (head: string, outcomes: CoverageReceipt["outcomes"]): CoverageReceipt => ({head, outcomes});

test("judgment advancement keeps the old green base and unmapped failures pending", async () => {
	const green={at,id:"full-base",kind:"full",commit:"base",tree:"tree-base",result:"pass"};
	const red: AffectedReceipt={at,id:"affected-a",kind:"affected",base:"base",commit:"a",tree:"tree-a",policyHash:"policy",selected:["pins"],outcomes:{pins:"fail"},steps:[{id:"pins",status:"fail"}],result:"fail"};
	const unproven={at,id:"affected-b",kind:"affected",commit:"b",tree:"tree-b",result:"pass"};
	const notes=(id: string)=>[{at,receipt:id,kind:id===green.id?"promotion":"lead-judgment-promotion",commit:id===green.id?"base":id===red.id?"a":"b",tree:id===green.id?"tree-base":id===red.id?"tree-a":"tree-b"}];
	const anchor=lastGreenBase({head:"b",receipts:[green,red,unproven],notes,contains,treeOf:(commit: string)=>`tree-${commit}`});
	assert.deepEqual(anchor,{base:"base",tree:"tree-base",receipt:green.id});
	const history=validateAffectedHistory({receipts:[red],base:anchor.base,head:"b",policyHash:"policy",contains,treeOf:(commit: string)=>`tree-${commit}`});
	const {stepsForTier}=await import("../lib/steps.mts"),{selectAffected}=await import("../lib/selection.mts");
	const steps=stepsForTier("full","full"),areas=JSON.parse(readFileSync(new URL("../areas.json",import.meta.url), "utf8"));
	const select=(paths: string[],options={})=>selectAffected(steps,paths,Object.assign({areas,counts:new Map()},options));
	const plan=pendingPlan({commits:[{commit:"a",paths:["assets/landing/homepage.css"]},{commit:"b",paths:["assets/landing/editor.css"]}],receipts:history,select,contains,head:"b"});
	assert.equal(plan.selection.find((item: { id: string; })=>item.id==="pins")?.status,"run");
	assert.deepEqual(plan.obligations.map(item=>item.commit),["a","b"]);
});

test("green ancestry uses exact trees and real promotion notes rather than receipt order", () => {
	const records=[{at,id:"future",kind:"full",commit:"c",tree:"tree-c",result:"pass"},
		{at,id:"forged",kind:"affected",commit:"b",tree:"forged-tree",result:"pass"},
		{at,id:"old",kind:"full",commit:"base",tree:"tree-base",result:"pass"},
		{at,id:"latest",kind:"affected",commit:"a",tree:"tree-a",result:"pass"}];
	const notes=(id: string)=>{const r=records.find(r=>r.id===id);assert.ok(r);return [{at,receipt:id,kind:"promotion",commit:r.commit,tree:r.tree}];};
	assert.deepEqual(lastGreenBase({head:"b",receipts:records,notes,contains,treeOf:(commit: string)=>`tree-${commit}`}),{base:"a",tree:"tree-a",receipt:"latest"});
	assert.throws(()=>lastGreenBase({head:"b",receipts:records,notes:()=>[],contains,treeOf:(commit: string)=>`tree-${commit}`}),/no exact promoted green ancestor/);
});

test("incomparable promoted green branches cannot silently shorten the coverage range", () => {
	const records=["left","right"].map(commit=>({at,id:commit,kind:"full",commit,tree:`tree-${commit}`,result:"pass"}));
	assert.throws(()=>lastGreenBase({head:"merge",receipts:records,notes:(id: string)=>[{at,receipt:id,kind:"promotion",commit:id,tree:`tree-${id}`}],
		contains:(later: string,earlier: string)=>later===earlier||later==="merge",treeOf:(commit: string)=>`tree-${commit}`}),/one ancestry chain/);
});

test("passes across runs cover commits only at descendants", () => {
	const receipts = [receipt("a", {"chromium:0": "pass", site: "pass"}), receipt("b", {"chromium:1": "pass"})];
	assert.deepEqual(coverageProblems({obligations: obligation, receipts, contains}), []);
	assert.match(coverageProblems({obligations: [{commit: "b", units: ["site"]}], receipts, contains})[0], /site/);
});

test("a later failure needs a later pass, including failure in a red run", () => {
	const receipts = [receipt("a", {"chromium:0": "pass", site: "pass"}), receipt("b", {"chromium:0": "fail", "chromium:1": "pass"})];
	assert.match(coverageProblems({obligations: obligation, receipts, contains})[0], /chromium:0/);
	receipts.push(receipt("c", {"chromium:0": "pass"}));
	assert.deepEqual(coverageProblems({obligations: obligation, receipts, contains}), []);
});

test("unrun is not a pass and failures with no change obligation still need a pass", () => {
	const receipts = [receipt("a", {"chromium:0": "pass", site: "unrun", extra: "fail"}), receipt("b", {"chromium:1": "pass"})];
	const problems = coverageProblems({obligations: obligation, receipts, contains});
	assert.ok(problems.some(problem => problem.includes("site")));
	assert.ok(problems.some(problem => problem.includes("extra")));
});

test("the next run keeps passed commands, adds new obligations and retries failures", () => {
	const commits = [{commit: "a", units: ["site", "chart"]}, {commit: "b", units: ["other"]}];
	const receipts = [receipt("a", {site: "pass", chart: "fail"})];
	const pending = outstandingCoverage({obligations: commits, receipts, contains, head: "b"});
	assert.deepEqual(pending.map(item => item.unit).sort(), ["chart", "other"]);
});

test("coverage cannot borrow outcomes from a divergent or future head", () => {
	assert.throws(() => outstandingCoverage({obligations: obligation, receipts: [receipt("c", {site: "pass"})], contains, head: "b"}), /ancestry/);
	assert.throws(() => outstandingCoverage({obligations: obligation, receipts: [receipt("b", {site: "pass"}), receipt("a", {site: "pass"})], contains, head: "b"}), /ancestry/);
});

test("each commit creates obligations from its own changed paths", () => {
	const commits=[{commit:"a",paths:["chart/a.rs"]},{commit:"b",paths:["site/a.css"]}];
	const obligations=buildObligations(commits,(paths: string[])=>paths[0].startsWith("chart")?[{id:"chart",step:"chart",status:"run"},{id:"site",step:"site",status:"skipped"}]:[{id:"site",step:"site",status:"run"}]);
	assert.deepEqual(obligations,[{commit:"a",units:["chart"],paused:[]},{commit:"b",units:["site"],paused:[]}]);
});

test("incremental receipt history binds base, source trees, selection and outcomes", () => {
	const receipts: AffectedReceipt[]=[{at,id:"affected-1",kind:"affected",base:"base",commit:"a",tree:"tree-a",policyHash:"policy",selected:["chart","site"],outcomes:{chart:"pass",site:"fail"},steps:[{id:"chart",status:"pass"},{id:"site",status:"fail"}],result:"fail"}];
	const input={receipts,base:"base",head:"b",policyHash:"policy",contains,treeOf:(commit: string)=>`tree-${commit}`};
	assert.deepEqual(validateAffectedHistory(input),[{head:"a",outcomes:{chart:"pass",site:"fail"}}]);
	for(const mutate of [(r: { base: string; })=>r.base="wrong",(r: { tree: string; })=>r.tree="wrong",(r: { policyHash: string; })=>r.policyHash="other",r=>delete r.outcomes.site,r=>r.outcomes.extra="pass",(r: { result: string; })=>r.result="pass",(r: { steps: { status: string; }[]; })=>r.steps[0].status="fail",(r: { commit: string; })=>r.commit="c"] satisfies ((receipt: AffectedReceipt) => unknown)[]){
		const broken=structuredClone(receipts);mutate(broken[0]);assert.throws(()=>validateAffectedHistory({...input,receipts:broken}));
	}
});

test("a changed map invalidates old passes but carries failed units into the new run", () => {
	const receipts: AffectedReceipt[]=[{at,id:"affected-old",kind:"affected",base:"base",commit:"a",tree:"tree-a",policyHash:"old",selected:["chart","site"],outcomes:{chart:"pass",site:"fail"},steps:[{id:"chart",status:"pass"},{id:"site",status:"fail"}],result:"fail"}];
	const history=validateAffectedHistory({receipts,base:"base",head:"b",policyHash:"new",contains,treeOf:(commit: string)=>`tree-${commit}`,allowPolicyChange:true});
	assert.deepEqual(history,[{head:"a",outcomes:{chart:"unrun",site:"fail"}}]);
	const pending=outstandingCoverage({obligations:[{commit:"a",units:["chart"]}],receipts:history,contains,head:"b"});
	assert.deepEqual(pending.map(item=>item.unit).sort(),["chart","site"]);
});

test("an incremental plan schedules only outstanding units and their prerequisites", () => {
	const commits=[{commit:"a",paths:["chart"]},{commit:"b",paths:["site"]}];
	const select=(paths: string[],{retry=[]}: {retry?: string[]}={})=>{
		const run=new Set([...paths,...retry]);
		if(run.has("site"))run.add("build");
		return ["chart","site","build"].map((id): SelectionItem=>({id,step:id,status:run.has(id)?"run":"skipped"}));
	};
	const receipts=[receipt("a",{chart:"pass"})];
	const result=pendingPlan({commits,receipts,select,contains,head:"b"});
	assert.deepEqual(result.pending.map(item=>item.unit),["site","build"]);
	assert.deepEqual(result.selection.filter((item: { status: string; })=>item.status==="run").map((item: {id: string})=>item.id),["site","build"]);
});

test("a pending unit missing from the current registry fails closed", () => {
	assert.throws(()=>pendingPlan({commits:[],receipts:[receipt("a",{retired:"fail"})],select:()=>[],contains,head:"b"}),/retired/);
});

test("incremental receipts retain per-commit pauses and explain earlier coverage while retrying failures", () => {
	const commits=[{commit:"a",paths:["first"]},{commit:"b",paths:["second"]}];
	const select=(paths: string[],{retry=[]}: {retry?: string[]}={})=>["covered","paused","mixed","failed"].map((id): SelectionItem=>{
		if (retry.includes(id)) return {id,step:id,status:"run",reason:"Previous failure requires a pass."};
		if (paths[0]==="first" && id==="covered") return {id,step:id,status:"run",reason:"First area"};
		if (paths.length && id==="paused" || paths[0]==="first" && id==="mixed") return {id,step:id,status:"paused",reason:"Below threshold"};
		if (paths[0]==="second" && id==="mixed") return {id,step:id,status:"run",reason:"Second area"};
		return {id,step:id,status:"skipped",reason:"No affected area"};
	});
	const receipts=[receipt("a",{covered:"pass",failed:"fail"}),receipt("b",{mixed:"pass"})];
	const result=pendingPlan({commits,receipts,select,contains,head:"b"});
	const find=(id: string)=>{const item=result.selection.find(row=>row.id===id);assert.ok(item,id);return item;};
	assert.deepEqual(result.pending.map(item=>item.unit),["failed"]);
	assert.equal(find("failed").status,"run");
	assert.equal(find("covered").status,"skipped");assert.match(find("covered").reason ?? "",/already passed/);
	assert.equal(find("paused").status,"paused");assert.match(find("paused").reason ?? "",/Below threshold/);
	assert.deepEqual(result.obligations[0].paused,[{id:"paused",reason:"Below threshold"},{id:"mixed",reason:"Below threshold"}]);
	assert.equal(find("mixed").status,"skipped");assert.deepEqual(find("mixed").decisions?.map((item: {status: string})=>item.status),["paused","run"]);
});

test("a real mapped pass cannot cover a later unknown-path pause until that later commit is in the passing head", async () => {
	const {readFileSync} = await import("node:fs");
	const {stepsForTier} = await import("../lib/steps.mts");
	const {selectAffected} = await import("../lib/selection.mts");
	const steps=stepsForTier("full","full"), areas=JSON.parse(readFileSync(new URL("../areas.json",import.meta.url), "utf8"));
	const select=(paths: string[],options={})=>selectAffected(steps,paths,Object.assign({areas,counts:new Map()},options));
	const first=select(["crates/richview-core/src/figure.rs"]), second=select(["crates/richview-bench/src/main.rs"]);
	const target=first.find(item=>item.status==="run" && second.some(next=>next.id===item.id&&next.status==="paused"));
	assert.ok(target,"The real selector must expose an affected check paused by the later unknown path");
	const commits=[{commit:"a",paths:["crates/richview-core/src/figure.rs"]},{commit:"b",paths:["crates/richview-bench/src/main.rs"]}];
	const firstPass=receipt("a",Object.fromEntries(first.filter(item=>item.status==="run").map(item=>[item.id,"pass"] as const)));
	const plan=pendingPlan({commits,receipts:[firstPass],select,contains,head:"b"});
	const selected=plan.selection.find((item: {id: string})=>item.id===target.id);
	assert.equal(selected?.status,"paused");assert.match(selected?.reason ?? "",/Paused for b/);
	assert.ok(plan.obligations[1].paused.some(item=>item.id===target.id&&item.reason));
	const laterPass=receipt("b",{[target.id]:"pass"});
	const covered=pendingPlan({commits,receipts:[firstPass,laterPass],select,contains,head:"b"}).selection.find(item=>item.id===target.id);
	assert.equal(covered?.status,"skipped");assert.match(covered?.reason ?? "",/already passed/);
});

test("policy fingerprints change with selection rules and receipt-derived counts", () => {
	const value: Parameters<typeof policyFingerprint>[0]={policy:{phase:"prelaunch",since:"2026-11-01",by:"Fixture",rotation:[]},steps:[{id:"site",title:"Site",tier:"full",cards:[]}],areas:{version:1,areas:[]},counts:new Map([["site",{step:1,commands:[]}]])};
	assert.equal(policyFingerprint(value),policyFingerprint(value));
	assert.notEqual(policyFingerprint(value),policyFingerprint({...value,counts:new Map([["site",{step:2,commands:[]}]])}));
	assert.notEqual(policyFingerprint(value),policyFingerprint({...value,areas:{version:1,areas:[{name:"new",paths:["src/"],steps:["site"]}]}}));
});

test("a built-in predicate change immediately after a green full creates its own obligations", async () => {
	const {stepsForTier} = await import("../lib/steps.mts"), {selectAffected} = await import("../lib/selection.mts");
	const steps = stepsForTier("full", "full"), areas = JSON.parse(readFileSync(new URL("../areas.json", import.meta.url), "utf8"));
	const select = (paths: string[], options = {}) => selectAffected(steps, paths, Object.assign({areas, counts: new Map()}, options));
	const changed = ["tools/rv-gate/lib/checks.mts"];
	const plan = pendingPlan({commits: [{commit: "a", paths: changed}], receipts: [], select, contains, head: "a"});
	for (const id of ["pins", "entrypoints", "arch", "secrets", "coverage", "audit"]) {
		assert.equal(plan.selection.find((item: { id: string; }) => item.id === id)?.status, "run", `${id} must run when its implementation changes`);
		assert.ok(plan.obligations[0].units.includes(id), `${id} must be a commit obligation`);
	}
	assert.equal(plan.selection.find((item: { id: string; }) => item.id === "captures")?.status, "skipped", "An unrelated capture suite stays narrow");
});

test("changes to shard membership and case evidence invalidate earlier shard passes", async () => {
	const root = fileURLToPath(new URL("../../..", import.meta.url));
	const scratch = join(process.cwd(), "tmp");
	mkdirSync(scratch, {recursive: true});
	const copy = mkdtempSync(join(scratch, "affected-fingerprint-"));
	try {
		mkdirSync(join(copy, "tools"));
		cpSync(join(root, "tools/rv-gate"), join(copy, "tools/rv-gate"), {recursive: true});
		cpSync(join(root, "scripts"), join(copy, "scripts"), {recursive: true});
		const coverage = await import(pathToFileURL(join(copy, "tools/rv-gate/lib/affected-coverage.mts")).href);
		const value = {policy: {phase: "prelaunch"}, steps: [], areas: {}, counts: new Map()};
		const before = coverage.policyFingerprint(value);
		const checks = join(copy, "tools/rv-gate/lib/checks.mts"), originalChecks = readFileSync(checks, "utf8");
		const changedChecks = originalChecks.replace(/(rust:\s*["'])[^"']+/, "$1different-rust-version");
		assert.notEqual(changedChecks, originalChecks, "The fixture must change the real Rust pin");
		writeFileSync(checks, changedChecks);
		const changedPinHash = coverage.policyFingerprint(value);
		const {stepsForTier} = await import("../lib/steps.mts");
		const {selectAffected} = await import("../lib/selection.mts");
		const steps = stepsForTier("full", "full"), areas = JSON.parse(readFileSync(join(root, "tools/rv-gate/areas.json"), "utf8"));
		const select = (paths: string[], options = {}) => selectAffected(steps, paths, Object.assign({areas, counts: new Map()}, options));
		assert.equal(select([".node-version"]).find(item => item.id === "pins")?.status, "run");
		assert.equal(select(["assets/landing/hero.css"]).find(item => item.id === "pins")?.status, "skipped");
		const oldPinPass = {id: "affected-pins", kind: "affected", base: "base", commit: "a", tree: "tree-a", policyHash: before,
			selected: ["pins"], outcomes: {pins: "pass"}, steps: [{id: "pins", status: "pass"}], result: "pass"};
		const prior = coverage.validateAffectedHistory({receipts: [oldPinPass], base: "base", head: "b", policyHash: changedPinHash,
			contains, treeOf: () => "tree-a", allowPolicyChange: true});
		const replay = coverage.pendingPlan({commits: [{commit: "a", paths: [".node-version"]}, {commit: "b", paths: ["assets/landing/hero.css"]}], receipts: prior, select, contains, head: "b"});
		assert.equal(replay.selection.find((item: { id: string; }) => item.id === "pins")?.status, "run", "A changed Rust pin invalidates the old pins pass");
		writeFileSync(checks, originalChecks);
		const added = [
			"tools/rv-gate/lib/affected-history.mts", "tools/rv-gate/lib/affected-promotion.mts", "tools/rv-gate/lib/affected-retry.mts",
			"tools/rv-gate/lib/no-js-shards.mts", "tools/rv-gate/lib/rust-cohort-worker.mts", "tools/rv-gate/lib/browser-events.mts",
			"tools/rv-gate/lib/browser-launch.mts", "tools/rv-gate/lib/preview-reload.mts", "tools/rv-gate/lib/command.mts", "tools/rv-gate/lib/connectors.mts", "tools/rv-gate/lib/proc.mts",
			"tools/rv-gate/lib/target.mts", "tools/rv-gate/lib/steps.mts", "tools/rv-gate/lib/checks.mts", "tools/rv-gate/lib/full.mts",
			"tools/rv-gate/lib/browser-receipt.mts", "tools/rv-gate/lib/step-functions.mts", "tools/rv-gate/lib/build-manifest.mts", "scripts/verify-chart-interaction.ts", "scripts/chart-interaction-checks.ts",
			"scripts/docs-chart-checks.ts", "scripts/verify-no-js.ts", "scripts/preview-recovery.ts", "scripts/runtime-access.ts",
		];
		for (const name of added) {
			const file = join(copy, name), original = readFileSync(file);
			appendFileSync(file, "\n// changed policy\n");
			assert.notEqual(coverage.policyFingerprint(value), before, `${name} must change the policy fingerprint`);
			writeFileSync(file, original);
		}
		const newHelper = join(copy, "tools/rv-gate/lib/new-policy-helper.mjs");
		writeFileSync(newHelper, "export const policy = true;\n");
		assert.notEqual(coverage.policyFingerprint(value), before, "New runtime helpers must change the fingerprint");
		rmSync(newHelper);
		appendFileSync(join(copy, "tools/rv-gate/lib/rust-cohort-worker.mts"), "\n// changed shard membership\n");
		const after = coverage.policyFingerprint(value), key = "shard-1-of-4";
		const id = `rust-tests:case:${Buffer.from(key).toString("base64url")}`;
		const selection = {id, step: "rust-tests", commandId: "rust-tests", caseKind: "rust-cohort", caseKey: key, status: "run"};
		const receipt = {id: "affected-rust", kind: "affected", base: "base", commit: "a", tree: "tree-a", policyHash: before,
			selected: [id], selection: [selection], outcomes: {[id]: "pass"}, result: "pass",
			steps: [{id: "rust-tests", status: "pass", units: [{id, status: "pass"}], details: {preparation: {status: "pass"}, commands: [{id: key, status: "pass"}]}}]};
		const history = {receipts: [receipt], base: "base", head: "a", contains, treeOf: () => "tree-a", allowPolicyChange: true};
		assert.equal(coverage.validateAffectedHistory({...history, policyHash: before})[0].outcomes[id], "pass");
		assert.equal(coverage.validateAffectedHistory({...history, policyHash: after})[0].outcomes[id], "unrun");
		assert.match(coverage.coverageProblems({obligations: [{commit: "a", units: [id]}], receipts: coverage.validateAffectedHistory({...history, policyHash: after}), contains})[0], /no pass/);
	} finally {
		rmSync(copy, {recursive: true, force: true});
	}
});

test("receipt outcomes are taken from actual steps and missing commands are unrun", () => {
	const selected: SelectionItem[]=[{id:"build",step:"build",status:"run"},{id:"chromium:a",step:"chromium",status:"run"},{id:"chromium:b",step:"chromium",status:"run"},{id:"unaffected",step:"unaffected",status:"skipped"}];
	const steps: (StepOutcome & {id: string})[]=[{id:"build",status:"pass"},{id:"chromium",status:"fail",units:[{id:"chromium:a",status:"pass"}]}];
	assert.deepEqual(outcomesForSelection(selected,steps,"pass"),{build:"pass","chromium:a":"pass","chromium:b":"unrun","browser-telemetry":"pass"});
	assert.throws(()=>outcomesForSelection(selected,[...steps,steps[0]],"pass"),/duplicate/i);
});

test("a cancelled receipt round-trips missing selected commands and cases without hiding failures", () => {
	const commandId = "chromium:command", missingCase = caseUnitId(commandId, "landing"), failed = "chromium:failed", missingCommand = "host:unstarted";
	const selection: SelectionItem[] = [{id:"pins",step:"pins",status:"run"},{id:failed,step:"chromium",status:"run"},{id:missingCommand,step:"host",status:"run"},
		{id:missingCase,step:"chromium",commandId,caseKey:"landing",caseKind:"browser",status:"run"}];
	const steps: (StepOutcome & {id: string})[] = [{id:"pins",status:"pass"},{id:"chromium",status:"unrun",units:[{id:failed,status:"fail"}]},{id:"host",status:"unrun"}];
	const outcomes = outcomesForSelection(selection, steps, "unrun");
	const receipt: AffectedReceipt & {cancelled: boolean} = {at,id:"affected-cancelled",kind:"affected",base:"base",commit:"a",tree:"tree-a",policyHash:"policy",
		selected:Object.keys(outcomes),selection,outcomes,steps,browserRelaunchTelemetry:{status:"unrun"},result:"fail",cancelled:true};
	const input = {receipts:[receipt],base:"base",head:"b",policyHash:"policy",contains,treeOf:(commit: string)=>`tree-${commit}`};
	const history = validateAffectedHistory(input);
	assert.deepEqual(history[0].outcomes,{pins:"pass",[failed]:"fail",[missingCommand]:"unrun",[missingCase]:"unrun","browser-telemetry":"unrun"});
	const pending = outstandingCoverage({obligations:[{commit:"a",units:[missingCommand,missingCase]}],receipts:history,contains,head:"b"});
	assert.deepEqual(pending.map(item=>item.unit).sort(),[failed,missingCommand,missingCase].sort());
	const changedPolicy = validateAffectedHistory({...input,policyHash:"new-policy",allowPolicyChange:true});
	assert.equal(changedPolicy[0].outcomes.pins,"unrun");
	assert.equal(changedPolicy[0].outcomes[failed],"fail");
	assert.ok(outstandingCoverage({obligations:[],receipts:changedPolicy,contains,head:"b"}).some(item=>item.unit===failed));
	for (const id of [missingCommand,missingCase]) for (const forged of ["pass","fail"] as const) {
		const bad=structuredClone(receipt);bad.outcomes[id]=forged;
		assert.throws(()=>validateAffectedHistory({...input,receipts:[bad]}),/Affected outcomes differ|Missing passing command evidence/);
	}
	for (const forged of ["pass","unrun"] as const) {
		const bad=structuredClone(receipt);bad.outcomes[failed]=forged;
		assert.throws(()=>validateAffectedHistory({...input,receipts:[bad]}),/Affected outcomes differ/);
	}
});

test("an affected base must be the exact tree of a previously promoted passing receipt", () => {
	const receipt={at,id:"full-1",kind:"full",commit:"base",tree:"tree-base",result:"pass"};
	const notes=(id: string)=>id==="full-1"?[{at,receipt:id,kind:"promotion",commit:"base",tree:"tree-base"}]:[];
	assert.equal(requireGreenBase({base:"base",tree:"tree-base",receipts:[receipt],notes}),"full-1");
	for(const changed of [{...receipt,result:"fail"},{...receipt,tree:"other"},{...receipt,commit:"other"}])assert.throws(()=>requireGreenBase({base:"base",tree:"tree-base",receipts:[changed],notes}),/green/);
	assert.throws(()=>requireGreenBase({base:"base",tree:"tree-base",receipts:[receipt],notes:()=>[]}),/green/);
});
