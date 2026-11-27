// The combined check uses the Cargo workspace identities and fails closed at its CLI.
import assert from "node:assert/strict";
import test from "node:test";
import {readFileSync,mkdirSync,mkdtempSync,rmSync,writeFileSync} from "node:fs";
import {resolve} from "node:path";
import {fileURLToPath} from "node:url";
import {spawnSync} from "node:child_process";
import {checkDependencies} from "../check.mts";
const root=fileURLToPath(new URL("../../../",import.meta.url));
const policy=()=>JSON.parse(readFileSync(new URL("../rules.json",import.meta.url),"utf8"));
const api=()=>({schema:"interface-v1",status:"pass",findings:[],indirect:[]});
const member=(name: string)=>({id:`path+file:///${name}#0.1.0`,name,dependencies: [] as {name: string; kind: string}[],targets:[{kind:["lib"]}]});
const metadata=(packages=[member("richview-chart"),member("richview-core")])=>({version:1,packages,workspace_members:packages.map(p=>p.id)});
const check=(data: unknown=metadata(),report: unknown=api(),rules: unknown=policy())=>checkDependencies({metadata:data,apiReport:report,rules});

test("the combined interface-v1 report counts verified workspace crates and indirect bindings",()=>{
	assert.deepEqual(check(),{schema:"interface-v1",status:"pass",findings:[],checked:{crates:2,indirect:0}});
});

test("missing, unknown, duplicate and incomplete Cargo metadata cannot produce a pass",()=>{
	const valid=metadata();
	for(const data of [undefined,null,{}, {...valid,version:2}, {...valid,workspace_members:[]}, {...valid,workspace_members:[...valid.workspace_members,valid.workspace_members[0]]}, {...valid,workspace_members:["missing"]}, {...valid,packages:[valid.packages[0],valid.packages[0]]}]) {
		assert.equal(checkDependencies({metadata:data,apiReport:api(),rules:policy()}).status,"fail",String(JSON.stringify(data)));
	}
});

test("unrelated registry packages in full metadata cannot masquerade as workspace members",()=>{
	const data=metadata();data.packages.push(member("third-party"));
	assert.equal(check(data).status,"pass");
});

test("combined checks fail for forbidden crate edges or failed API verification",()=>{
	const data=metadata();data.packages[0].dependencies.push({name:"richview-core",kind:"build"});
	assert.equal(check(data).status,"fail");
	assert.equal(check(metadata(),{...api(),status:"fail"}).status,"fail");
	const invalid=policy();invalid.allowed["richview-chart"]="*";
	assert.equal(check(metadata(),api(),invalid).status,"fail");
});

test("CLI argument and missing-report errors return one failing JSON document without Cargo",()=>{
	mkdirSync(resolve(root,"tmp"),{recursive:true});
	const scratch=mkdtempSync(resolve(root,"tmp/dependency-cli-"));
	try {
		const rules=resolve(scratch,"rules.json");writeFileSync(rules,JSON.stringify(policy()));
		const report=resolve(scratch,"api.json");writeFileSync(report,JSON.stringify({...api(),status:"fail"}));
		for(const args of [[],["check","--unknown"],["check","--api-report",resolve(scratch,"missing")],["check","--rules",rules,"--api-report",report]]) {
			const result=spawnSync(process.execPath,[resolve(root,"tools/dependency-rules/check.mjs"),...args],{encoding:"utf8",cwd:root,env:{...process.env,PATH:"/nonexistent"}});
			assert.notEqual(result.status,0,result.stdout+result.stderr);
			const parsed=JSON.parse(result.stdout);
			assert.equal(parsed.schema,"interface-v1");assert.equal(parsed.status,"fail");
			assert.ok(parsed.findings.length);assert.equal(result.stderr,"");
		}
	} finally {rmSync(scratch,{recursive:true,force:true});}
});

test("CLI reads real Cargo aliases and rejects a target-specific reverse build edge without compiling",()=>{
	mkdirSync(resolve(root,"tmp"),{recursive:true});
	const scratch=mkdtempSync(resolve(root,"tmp/dependency-cargo-"));
	try {
		writeFileSync(resolve(scratch,"Cargo.toml"),'[workspace]\nmembers = ["chart", "core"]\nresolver = "3"\n');
		const manifest=(name: string,dependencies="")=>`[package]\nname = "richview-${name}"\nversion = "0.1.0"\nedition = "2024"\n${dependencies}`;
		for(const name of ["chart","core"]) {
			mkdirSync(resolve(scratch,name,"src"),{recursive:true});
			writeFileSync(resolve(scratch,name,"src/lib.rs"),"");
			writeFileSync(resolve(scratch,name,"Cargo.toml"),manifest(name));
		}
		writeFileSync(resolve(scratch,"core/Cargo.toml"),manifest("core",'[dependencies]\nengine = { package = "richview-chart", path = "../chart", optional = true }\n'));
		writeFileSync(resolve(scratch,"api.json"),JSON.stringify(api()));
		const invoke=()=>{
			const run=spawnSync(process.execPath,[resolve(root,"tools/dependency-rules/check.mjs"),"check","--root",scratch,"--rules",resolve(root,"tools/dependency-rules/rules.json"),"--api-report","api.json"],{encoding:"utf8",cwd:root});
			return {...run,report:JSON.parse(run.stdout) as ReturnType<typeof checkDependencies>};
		};
		let run=invoke();assert.equal(run.status,0,run.stdout+run.stderr);assert.equal(run.report.checked.crates,2);
		writeFileSync(resolve(scratch,"core/Cargo.toml"),manifest("core"));
		writeFileSync(resolve(scratch,"chart/Cargo.toml"),manifest("chart",'[target.\'cfg(windows)\'.build-dependencies]\ninnocent = { package = "richview-core", path = "../core" }\n'));
		run=invoke();assert.equal(run.status,1,run.stdout+run.stderr);
		assert.ok(run.report.findings.some(f=>f.code === "dependency.edge" && f.message.includes("richview-chart -> richview-core")),run.stdout);
	} finally {rmSync(scratch,{recursive:true,force:true});}
});
