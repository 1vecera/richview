// Host reason (ARC-03): prove incremental case coverage on a disposable Git history.
import test from "node:test";
import assert from "node:assert/strict";
import {execFileSync} from "node:child_process";
import {mkdirSync,mkdtempSync,rmSync,writeFileSync} from "node:fs";
import {join} from "node:path";
import {affectedContext} from "../lib/affected-context.mjs";
import {affectedPromotion,historicalAffectedProblems} from "../lib/affected-promotion.mjs";
import {outcomesForSelection,policyFingerprint} from "../lib/affected-coverage.mjs";
const git=(cwd,...args)=>execFileSync("git",args,{cwd,encoding:"utf8",stdio:["ignore","pipe","pipe"]}).trim();

test("a later case pass cannot stand in for another changed case, but both receipts promote together",t=>{
	mkdirSync(join(process.cwd(),"tmp"),{recursive:true});const cwd=mkdtempSync(join(process.cwd(),"tmp/case-proof-"));t.after(()=>rmSync(cwd,{recursive:true,force:true}));
	git(cwd,"init","-q");git(cwd,"config","user.name","Test");git(cwd,"config","user.email","test@example.invalid");
	const commit=title=>{git(cwd,"add","-A");git(cwd,"commit","-qm",title);return git(cwd,"rev-parse","HEAD");};
	mkdirSync(join(cwd,"crates/demo/src"),{recursive:true});writeFileSync(join(cwd,"crates/demo/Cargo.toml"),'[package]\nname="demo"\nversion="0.1.0"\n');writeFileSync(join(cwd,"crates/demo/src/lib.rs"),"// demo\n");
	for(const name of ["bar","line"]){mkdirSync(join(cwd,"examples",name),{recursive:true});writeFileSync(join(cwd,"examples",name,"report.rmd"),"base");}const base=commit("base");
	writeFileSync(join(cwd,"examples/bar/report.rmd"),"change bar");const a=commit("bar");writeFileSync(join(cwd,"examples/line/report.rmd"),"change line");const b=commit("line");
	const steps=[{id:"build",run:[{cmd:"fake",args:[]}]},{id:"chromium",parallel:2,needs:"build",run:[{cmd:"node",args:["scripts/verify-report-runtime.mjs"],entry:"scripts/verify-report-runtime.mjs"}]}];
	const areas={version:1,areas:[{name:"examples",paths:["examples/"],steps:["chromium"]}]};
	const policy={phase:"prelaunch",affected:{failureThreshold:5,fallback:{mode:"failure-threshold",realDefectSteps:[]}}},counts=new Map(),policyHash=policyFingerprint({policy,steps,areas,counts});
	const tree=head=>git(cwd,"rev-parse",`${head}^{tree}`),anchor={id:"full-base",kind:"full",commit:base,tree:tree(base),result:"pass"},notes=()=>[{kind:"promotion",commit:base,tree:tree(base)}];
	const plan=(head,history)=>affectedContext({cwd,base,head,steps,areas,policy,counts,history,policyHash});
	const make=(head,selection,id)=>{
		const chosen=selection.filter(item=>item.status==="run"),cases=chosen.filter(item=>item.caseKind),keys=cases.map(item=>item.caseKey),units=cases.map(item=>({id:item.id,status:"pass"}));
		const results=[{id:"build",status:"pass"},{id:"chromium",status:"pass",units,details:{commands:[{status:"pass",caseUnits:units,cases:{cohort:runtime,selected:keys,completed:keys}}]}}];
		const outcomes=outcomesForSelection(selection,results,"pass");
		return {id,kind:"affected",base,greenReceipt:anchor.id,commit:head,tree:tree(head),policyHash,phase:"prelaunch",result:"pass",selected:Object.keys(outcomes),selection,outcomes,steps:results,browserRelaunchTelemetry:{status:"pass"}};
	};
	const runtime=["bar","line","forms","histogram-legend","wide-chart","legend-cues","numeric-series-names"];
	const pa=plan(a,[]),first=make(a,pa.selection,"affected-a");assert.deepEqual(pa.selection.filter(item=>item.status==="run"&&item.caseKind).map(item=>item.caseKey),["bar"]);
	const pb=plan(b,[first]);assert.deepEqual(pb.selection.filter(item=>item.status==="run"&&item.caseKind).map(item=>item.caseKey),["line"]);
	const second=make(b,pb.selection,"affected-b");
	const promote=history=>affectedPromotion({cwd,receipt:second,policy,steps,areas,counts,history,anchors:[anchor],notes});
	assert.throws(()=>promote([second]),/coverage is incomplete/);
	const proof=promote([first,second]);assert.equal(proof.obligations.length,2);
	assert.deepEqual(historicalAffectedProblems({cwd,receipt:second,proof,read:id=>[first,second].find(row=>row.id===id)}),[]);
});
