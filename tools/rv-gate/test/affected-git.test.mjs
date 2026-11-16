// Host reason (ARC-03): derive changed commits and Rust inputs from immutable Git trees.
import test from "node:test";
import assert from "node:assert/strict";
import {execFileSync} from "node:child_process";
import {mkdirSync, mkdtempSync, rmSync, writeFileSync} from "node:fs";
import {join} from "node:path";
import {affectedContext} from "../lib/affected-context.mjs";
import {changedCommits, sourceInputs, treeReader} from "../lib/affected-git.mjs";
const git=(cwd,...args)=>execFileSync("git",args,{cwd,encoding:"utf8",stdio:["ignore","pipe","pipe"]}).trim();

test("each commit keeps its paths even when a later commit reverts them", t=>{
	mkdirSync(join(process.cwd(),"tmp"),{recursive:true});const dir=mkdtempSync(join(process.cwd(),"tmp/affected-git-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
	git(dir,"init","-q");git(dir,"config","user.name","Test");git(dir,"config","user.email","test@example.invalid");
	const commit=(name)=>{git(dir,"add","-A");git(dir,"commit","-qm",name);return git(dir,"rev-parse","HEAD");};
	mkdirSync(join(dir,"crates/core/src"),{recursive:true});mkdirSync(join(dir,"assets"));
	writeFileSync(join(dir,"crates/core/Cargo.toml"),'[package]\nname="core"\nversion="0.1.0"\n');
	writeFileSync(join(dir,"crates/core/src/lib.rs"),'const TEXT: &str = include_str!("../../../assets/a.txt");\n');
	writeFileSync(join(dir,"assets/a.txt"),"base");const base=commit("base");
	writeFileSync(join(dir,"assets/a.txt"),"changed");const first=commit("change");writeFileSync(join(dir,"assets/a.txt"),"base");const head=commit("revert");
	assert.deepEqual(changedCommits(dir,base,head),[{commit:first,paths:["assets/a.txt"]},{commit:head,paths:["assets/a.txt"]}]);
	const read=treeReader(dir,head),{graph,inputs}=sourceInputs(dir,head,read);
	assert.equal(read("assets/a.txt"),"base");assert.equal(read("missing"),null);
	assert.ok(graph.some(crate=>crate.dir==="crates/core"));assert.ok(inputs.get("core").has("assets/a.txt"));
});

test("a red run carries its failure while a new commit adds only its affected checks", t=>{
	mkdirSync(join(process.cwd(),"tmp"),{recursive:true});const dir=mkdtempSync(join(process.cwd(),"tmp/affected-context-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
	git(dir,"init","-q");git(dir,"config","user.name","Test");git(dir,"config","user.email","test@example.invalid");
	const commit=name=>{git(dir,"add","-A");git(dir,"commit","-qm",name);return git(dir,"rev-parse","HEAD");};
	mkdirSync(join(dir,"crates/core/src"),{recursive:true});mkdirSync(join(dir,"assets"));
	writeFileSync(join(dir,"crates/core/Cargo.toml"),'[package]\nname="core"\nversion="0.1.0"\n');writeFileSync(join(dir,"crates/core/src/lib.rs"),"// base\n");writeFileSync(join(dir,"assets/a.css"),"base");const base=commit("base");
	writeFileSync(join(dir,"assets/a.css"),"changed");const first=commit("site");writeFileSync(join(dir,"crates/core/src/lib.rs"),"// changed\n");const head=commit("rust");
	const steps=[{id:"build",run:[{cmd:"fake",args:[]}]},{id:"site",needs:"build",run:[{cmd:"fake",args:[]}]},{id:"rust-tests",run:[{cmd:"cargo",args:["test"]}]}];
	const areas={version:1,areas:[{name:"site",paths:["assets/"],steps:["site"]}]};
	const policy={phase:"prelaunch",affected:{failureThreshold:5,fallback:{mode:"failure-threshold",realDefectSteps:[]}}};
	const history=[{id:"affected-1",kind:"affected",base,commit:first,tree:git(dir,"rev-parse",`${first}^{tree}`),policyHash:"p",selected:["build","site"],outcomes:{build:"pass",site:"fail"},steps:[{id:"build",status:"pass"},{id:"site",status:"fail"}],result:"fail"}];
	const plan=affectedContext({cwd:dir,base,head,steps,areas,policy,counts:new Map(),history,policyHash:"p"});
	assert.deepEqual(plan.pending.map(row=>row.unit).sort(),["rust-tests","site"]);
	assert.deepEqual(plan.selection.filter(row=>row.status==="run").map(row=>row.id).sort(),["build","rust-tests","site"]);
	const passing={id:"affected-2",kind:"affected",base,commit:head,tree:git(dir,"rev-parse",`${head}^{tree}`),policyHash:"p",selected:["build","site","rust-tests"],outcomes:{build:"pass",site:"pass","rust-tests":"pass"},steps:[{id:"build",status:"pass"},{id:"site",status:"pass"},{id:"rust-tests",status:"pass"}],result:"pass"};
	const again=affectedContext({cwd:dir,base,head,steps,areas,policy,counts:new Map(),history:[passing],policyHash:"p"});
	assert.equal(again.pending.length,0);
	assert.deepEqual(again.selection.filter(row=>row.status==="run").map(row=>row.id),["build"],"Even an otherwise covered head gets a fresh retained build");
	mkdirSync(join(dir,"tools"));writeFileSync(join(dir,"tools/note.txt"),"tool-only change");const toolHead=commit("tool only");
	const toolPlan=affectedContext({cwd:dir,base,head:toolHead,steps,areas,policy,counts:new Map(),history:[passing],policyHash:"p"});
	assert.deepEqual(toolPlan.selection.filter(row=>row.status==="run").map(row=>row.id),["build"],"A tool-only commit still builds its exact source head");
});
