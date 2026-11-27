// Host reason (ARC-03): prove incremental case coverage on a disposable Git history.
import test from "node:test";
import assert from "node:assert/strict";
import {execFileSync} from "node:child_process";
import {mkdirSync,mkdtempSync,rmSync,writeFileSync} from "node:fs";
import {join} from "node:path";
import type {AffectedReceipt} from "../lib/affected-coverage.mts";
import type {SelectionItem} from "../lib/selection.mts";
import type {GateStep} from "../lib/steps.mts";
import type {StoredReceipt} from "../lib/store.mts";
import type {PhasePolicy} from "../lib/phase.mts";
import {affectedContext} from "../lib/affected-context.mts";
import {affectedPromotion,historicalAffectedProblems} from "../lib/affected-promotion.mts";
import {outcomesForSelection,policyFingerprint} from "../lib/affected-coverage.mts";
const git=(cwd: string,...args: string[])=>execFileSync("git",args,{cwd,encoding:"utf8",stdio:["ignore","pipe","pipe"]}).trim();

test("a later case pass cannot stand in for another changed case, but both receipts promote together",t=>{
	mkdirSync(join(process.cwd(),"tmp"),{recursive:true});const cwd=mkdtempSync(join(process.cwd(),"tmp/case-proof-"));t.after(()=>rmSync(cwd,{recursive:true,force:true}));
	git(cwd,"init","-q");git(cwd,"config","user.name","Test");git(cwd,"config","user.email","test@example.invalid");
	const commit=(title: string)=>{git(cwd,"add","-A");git(cwd,"commit","-qm",title);return git(cwd,"rev-parse","HEAD");};
	mkdirSync(join(cwd,"crates/demo/src"),{recursive:true});writeFileSync(join(cwd,"crates/demo/Cargo.toml"),'[package]\nname="demo"\nversion="0.1.0"\n');writeFileSync(join(cwd,"crates/demo/src/lib.rs"),"// demo\n");
	for(const name of ["bar","line"]){mkdirSync(join(cwd,"examples",name),{recursive:true});writeFileSync(join(cwd,"examples",name,"report.rmd"),"base");}const base=commit("base");
	writeFileSync(join(cwd,"examples/bar/report.rmd"),"change bar");const a=commit("bar");writeFileSync(join(cwd,"examples/line/report.rmd"),"change line");const b=commit("line");
	const gateFields = {tier: "full" as const, title: "Fixture", cards: []};
	const steps: GateStep[]=[{...gateFields,id:"build",run:[{cmd:"fake",args:[]}]},{...gateFields,id:"chromium",parallel:2,needs:"build",run:[{cmd:"node",args:["scripts/verify-report-runtime.mjs"],entry:"scripts/verify-report-runtime.mjs"}]}];
	const areas={version:1 as const,areas:[{name:"examples",paths:["examples/"],steps:["chromium"]}]};
	const policy: PhasePolicy={since:"2026-11-01",by:"Fixture",rotation:[],phase:"prelaunch",affected:{failureThreshold:5,fallback:{mode:"failure-threshold",realDefectSteps:[]}}},counts=new Map(),policyHash=policyFingerprint({policy,steps,areas,counts});
	const tree=(head: string)=>git(cwd,"rev-parse",`${head}^{tree}`),at="2026-11-01T00:00:00Z",anchor: StoredReceipt={id:"full-base",at,kind:"full",commit:base,tree:tree(base),result:"pass"},notes=()=>[{kind:"promotion",receipt:anchor.id,at,commit:base,tree:tree(base)}];
	const plan=(head: string,history: AffectedReceipt[])=>affectedContext({cwd,base,head,steps,areas,policy,counts,history,policyHash});
	const make=(head: string,selection: SelectionItem[],id: string): AffectedReceipt=>{
		const chosen=selection.filter((item: { status: string; })=>item.status==="run"),cases=chosen.filter(item=>item.caseKind),keys=cases.map(({caseKey})=>{assert.ok(caseKey);return caseKey;}),units=cases.map(item=>({id:item.id,status:"pass" as const}));
		const results=[{id:"build",status:"pass" as const},{id:"chromium",status:"pass" as const,units,details:{commands:[{status:"pass" as const,caseUnits:units,cases:{cohort:runtime,selected:keys,completed:keys}}]}}];
		const outcomes=outcomesForSelection(selection,results,"pass");
		return {id,at,kind:"affected",base,greenReceipt:anchor.id,commit:head,tree:tree(head),policyHash,phase:"prelaunch",result:"pass",selected:Object.keys(outcomes),selection,outcomes,steps:results,browserRelaunchTelemetry:{status:"pass" as const}};
	};
	const runtime=["bar","line","forms","histogram-legend","wide-chart","legend-cues","numeric-series-names"];
	const pa=plan(a,[]),first=make(a,pa.selection,"affected-a");assert.deepEqual(pa.selection.filter(item=>item.status==="run"&&item.caseKind).map(item=>item.caseKey),["bar"]);
	const pb=plan(b,[first]);assert.deepEqual(pb.selection.filter(item=>item.status==="run"&&item.caseKind).map(item=>item.caseKey),["line"]);
	const second=make(b,pb.selection,"affected-b");
	const promote=(history: AffectedReceipt[])=>affectedPromotion({cwd,receipt:second,policy,steps,areas,counts,history,anchors:[anchor],notes});
	assert.throws(()=>promote([second]),/coverage is incomplete/);
	const proof=promote([first,second]);assert.equal(proof.obligations.length,2);
	assert.deepEqual(historicalAffectedProblems({cwd,receipt:second,proof,read:(id: string)=>{const receipt=[first,second].find(row=>row.id===id);assert.ok(receipt);return receipt;}}),[]);
});
