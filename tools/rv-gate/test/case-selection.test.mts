// Host reason (ARC-03): prove source-to-case obligations without browsers or builds.
import test from "node:test";
import assert from "node:assert/strict";
import type {GateStep} from "../lib/steps.mts";
import type {CoverageReceipt, AffectedReceipt} from "../lib/affected-coverage.mts";
import type {SelectionItem} from "../lib/selection.mts";
import {caseCatalog,caseUnitId,expandLegacyCaseOutcomes,runtimeInventory} from "../lib/case-selection.mts";
import {commandId,selectAffected} from "../lib/selection.mts";
import {buildObligations,coverageProblems,pendingPlan,validateAffectedHistory} from "../lib/affected-coverage.mts";

const command=(entry: string)=>({cmd:"node",args:[entry],entry});
const gateFields = {tier:"full" as const,title:"Fixture",cards:[]};
const steps=[{...gateFields,id:"build",run:[command("build")]},{...gateFields,id:"chromium",parallel:4,needs:"build" as const,run:[command("scripts/verify-report-runtime.mjs"),command("scripts/verify-browser.mjs"),command("scripts/verify-figure-expand.mjs"),command("scripts/verify-docs.mjs")]}];
const runtime=runtimeInventory(["examples/bar/report.rmd","examples/line/report.rmd","examples/sql-report/report.rmd"]), catalog=caseCatalog(steps,runtime,commandId);
const areas={version:1 as const,areas:[{name:"examples",paths:["examples/"],steps:["chromium"]},{name:"hero",paths:["assets/landing/hero/"],steps:["chromium"],commands:{chromium:["scripts/verify-browser.mjs"]}},{name:"theme",paths:["crates/richview-core/assets/"],steps:["chromium"]}]};
const select=(paths: string[],{retry=[]}: {retry?: string[]}={})=>selectAffected(steps,paths,{areas,catalog,retry,read:()=>null});
const unit=(entry: string,key: string)=>{const picked=steps[1].run.find(item=>item.entry===entry);assert.ok(picked, entry);return caseUnitId(commandId(steps[1],picked),key);};
const get=(plan: readonly SelectionItem[],id: string)=>{const row=plan.find(item=>item.id===id);assert.ok(row,id);return row;};

test("runtime inventory derives report names from source and refuses fixture collisions",()=>{
	assert.deepEqual(runtime,["bar","line","forms","histogram-legend","wide-chart","legend-cues","numeric-series-names"]);
	assert.throws(()=>runtimeInventory(["examples/forms/report.rmd"]),/unique/);
});
test("a report edit selects only its own runtime case; unrelated changes select all cases",()=>{
	const plan=select(["examples/bar/report.rmd"]), id=(entry: string)=>`scripts/${entry}`;
	assert.equal(get(plan,unit(id("verify-report-runtime.mjs"),"bar")).status,"run");
	assert.equal(get(plan,unit(id("verify-report-runtime.mjs"),"line")).status,"skipped");
	assert.equal(get(plan,unit(id("verify-report-runtime.mjs"),"forms")).status,"skipped");
	const broad=select(["examples/bar/report.rmd","crates/richview-core/src/figure.rs"]);
	assert.equal(get(broad,unit(id("verify-report-runtime.mjs"),"line")).status,"run");
});
test("hero and theme assets select real bounded cases and retry keeps its exact case",()=>{
	const hero=select(["assets/landing/hero/pip.avif"]), browser="scripts/verify-browser.mjs";
	assert.equal(get(hero,unit(browser,"landing-hero")).status,"run");assert.equal(get(hero,unit(browser,"landing-deferred")).status,"run");assert.equal(get(hero,unit(browser,"runtime-loader")).status,"skipped");
	const theme=select(["crates/richview-core/assets/scientific.rstyle.yaml"]), figure="scripts/verify-figure-expand.mjs";
	assert.equal(get(theme,unit(figure,"theme/scientific/420")).status,"run");assert.equal(get(theme,unit(figure,"expanded-regression")).status,"run");assert.equal(get(theme,unit(figure,"theme/air/420")).status,"skipped");
	const retry=select([],{retry:[unit(figure,"theme/air/420")]});assert.equal(get(retry,unit(figure,"theme/air/420")).status,"run");assert.equal(get(retry,unit(figure,"theme/air/320")).status,"skipped");
});
test("a partial passing case cannot cover another case or a later change",()=>{
	const commits=[{commit:"a",paths:["examples/bar/report.rmd"]},{commit:"b",paths:["examples/line/report.rmd"]}];
	const obligations=buildObligations(commits,(paths: string[])=>select(paths));
	const bar=unit("scripts/verify-report-runtime.mjs","bar"),line=unit("scripts/verify-report-runtime.mjs","line");
	const contains=(head: string,base: string)=>["a","b"].indexOf(head)>=["a","b"].indexOf(base);
	const receipts: CoverageReceipt[]=[{head:"b",outcomes:{[line]:"pass"}}];
	assert.ok(coverageProblems({obligations,receipts,contains,head:"b"}).some(problem=>problem.includes(bar)));
	assert.ok(!coverageProblems({obligations,receipts,contains,head:"b"}).some(problem=>problem.includes(line)));
});
test("old command failures expand to all case failures and need individual later passes",()=>{
	const id=commandId(steps[1],steps[1].run[0]), spec=catalog.get(id), contains=()=>true;
	assert.ok(spec);const {units}=spec;
	const receipts=expandLegacyCaseOutcomes([{head:"a",outcomes:{[id]:"fail"}}],{[id]:units});
	assert.equal(receipts[0].outcomes[id],undefined);assert.ok(units.every((unit: string|number)=>receipts[0].outcomes[unit]==="fail"));
	const pending=pendingPlan({commits:[],receipts,select,contains,head:"b"});assert.deepEqual(pending.pending.map(item=>item.unit),units);
	const oldPass=expandLegacyCaseOutcomes([{head:"a",outcomes:{[id]:"pass" as const}}],{[id]:units});assert.deepEqual(oldPass[0].outcomes,{});
	const removed=caseUnitId(id,"deleted-example"), retired=expandLegacyCaseOutcomes([{head:"a",outcomes:{[removed]:"fail"}}],{[id]:units});
	assert.ok(units.every((unit: string|number)=>retired[0].outcomes[unit]==="fail"));assert.equal(retired[0].outcomes[removed],undefined);
});

test("promotion history requires actual passing command evidence for every selected case",()=>{
	const id=unit("scripts/verify-report-runtime.mjs","bar"),command=commandId(steps[1],steps[1].run[0]);
	const item={id,step:"chromium",commandId:command,caseKey:"bar",caseKind:"runtime" as const,status:"run" as const};
	const outcome={status:"pass" as const,caseUnits:[{id,status:"pass" as const}],cases:{cohort:["bar","line"],selected:["bar"],completed:["bar"]}};
	const entry={id:"affected-1",at:"2026-11-01T00:00:00Z",kind:"affected" as const,base:"base",commit:"a",tree:"tree-a",policyHash:"p",selected:[id],selection:[item],outcomes:{[id]:"pass" as const},steps:[{id:"chromium",status:"pass" as const,units:[{id,status:"pass" as const}],details:{commands:[outcome]}}],result:"pass" as const};
	const validate=(receipt: typeof entry)=>validateAffectedHistory({receipts:[receipt],base:"base",head:"a",policyHash:"p",contains:()=>true,treeOf:()=>"tree-a"});
	assert.equal(validate(entry)[0].outcomes[id],"pass");
	for(const mutate of [r=>delete (r.steps[0] as Partial<typeof r.steps[number]>).details,r=>r.steps[0].details.commands[0].cases.completed=[],(r: { steps: { details: { commands: { cases: { selected: string[]; }; }[]; }; }[]; })=>r.steps[0].details.commands[0].cases.selected.push("line"),(r: { selection: { caseKey: string; }[]; })=>r.selection[0].caseKey="line"] satisfies ((fixture: typeof entry) => unknown)[]){const bad=structuredClone(entry);mutate(bad);assert.throws(()=>validate(bad));}
});

test("declared workspace Rust cohorts expose stable retry units; scoped Cargo falls back safely",()=>{
	const rust: GateStep={...gateFields,id:"rust-tests",cohort:"rust-debug",run:[{cmd:"cargo",args:["test","--workspace","--locked","--no-fail-fast"]}]};
	const all=[rust], mapping=caseCatalog(all,runtime,commandId), spec=mapping.get("rust-tests");
	assert.ok(spec);
	assert.deepEqual(spec.keys,["shard-0-of-4","shard-1-of-4","shard-2-of-4","shard-3-of-4"]);
	const rules={version:1 as const,areas:[{name:"code",paths:["src/"],steps:["rust-tests"]}]};
	const selection=selectAffected(all,[],{areas:rules,catalog:mapping,retry:[spec.units[2]],read:()=>null});
	assert.deepEqual(selection.filter(item=>item.status==="run").map(item=>item.caseKey),["shard-2-of-4"]);
	const scoped={...rust,run:[{cmd:"cargo",args:["test","-p","demo","--no-fail-fast"]}]};
	assert.equal(caseCatalog([scoped],runtime,commandId).has("rust-tests"),false);
	assert.equal(caseCatalog(all,runtime,commandId,{cohorts:false}).has("rust-tests"),false);
	const recovered=expandLegacyCaseOutcomes([{head:"a",outcomes:{[spec.units[2]]:"fail"}}],{}, {"rust-tests":"rust-tests"});
	assert.deepEqual(recovered[0].outcomes,{"rust-tests":"fail"});
});

test("a cohort shard pass requires fresh preparation and the exact passing shard",()=>{
	const step: GateStep={...gateFields,id:"rust-tests",cohort:"rust-debug",run:[{cmd:"cargo",args:["test","--workspace","--no-fail-fast"]}]},spec=caseCatalog([step],runtime,commandId).get("rust-tests");
	assert.ok(spec);const id=spec.units[1];
	const item={id,step:"rust-tests",commandId:"rust-tests",caseKind:"rust-cohort" as const,caseKey:spec.keys[1],status:"run" as const};
	const receipt={id:"affected-rust",at:"2026-11-01T00:00:00Z",kind:"affected" as const,base:"base",commit:"a",tree:"tree-a",policyHash:"p",selected:[id],selection:[item],outcomes:{[id]:"pass" as const},steps:[{id:"rust-tests",status:"pass" as const,units:[{id,status:"pass" as const}],details:{preparation:{status:"pass" as "pass" | "fail"},commands:[{id:spec.keys[1],status:"pass" as "pass" | "fail"}]}}],result:"pass" as const};
	const check=(row: AffectedReceipt)=>validateAffectedHistory({receipts:[row],base:"base",head:"a",policyHash:"p",contains:()=>true,treeOf:()=>"tree-a"});assert.equal(check(receipt)[0].outcomes[id],"pass");
	for(const mutate of [(r: { steps: { details: { preparation: { status: string; }; }; }[]; })=>r.steps[0].details.preparation.status="fail",r=>r.steps[0].details.commands[0].id=spec.keys[2],(r: { steps: { details: { commands: { status: string; }[]; }; }[]; })=>r.steps[0].details.commands[0].status="fail"] satisfies ((fixture: typeof receipt) => unknown)[]){const bad=structuredClone(receipt);mutate(bad);assert.throws(()=>check(bad));}
});

test("Rust cohort retries only failed shards plus shards affected by new commits",()=>{
	const rust: GateStep={...gateFields,id:"rust-tests",cohort:"rust-debug",run:[{cmd:"cargo",args:["test","--workspace","--no-fail-fast"]}]},all=[rust],mapping=caseCatalog(all,runtime,commandId),spec=mapping.get("rust-tests");
	assert.ok(spec);
	const rules={version:1 as const,areas:[{name:"code",paths:["src/"],steps:["rust-tests"]}]};
	const choose=(paths: string[],{retry=[]}: {retry?: string[]}={})=>selectAffected(all,paths,{areas:rules,catalog:mapping,retry,read:()=>null});
	const previous: CoverageReceipt={head:"a",outcomes:Object.fromEntries(spec.units.map((id,index: number)=>[id,index===2?"fail":"pass"]))};
	const contains=(head: string,base: string)=>["a","b"].indexOf(head)>=["a","b"].indexOf(base);
	const first=pendingPlan({commits:[{commit:"a",paths:["src/one.rs"]},{commit:"b",paths:["tools/note.txt"]}],receipts:[previous],select:choose,contains,head:"b"});
	assert.deepEqual(first.selection.filter((item: { status: string; })=>item.status==="run").map(item=>item.caseKey),["shard-2-of-4"]);
	const changed=pendingPlan({commits:[{commit:"a",paths:["src/one.rs"]},{commit:"b",paths:["src/two.rs"]}],receipts:[previous],select:choose,contains,head:"b"});
	assert.deepEqual(changed.selection.filter((item: { status: string; })=>item.status==="run").map(item=>item.caseKey),spec.keys);
});
