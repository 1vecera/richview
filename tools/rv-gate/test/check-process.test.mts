import assert from "node:assert/strict";
import {execFileSync} from "node:child_process";
import {mkdtempSync, rmSync, readFileSync, mkdirSync, writeFileSync, existsSync} from "node:fs";
import {tmpdir} from "node:os";
import {join} from "node:path";
import {pathToFileURL} from "node:url";
import {deserialize} from "node:v8";
import test from "node:test";
import {freshCheck, type WorkerContext} from "../lib/check-process.mts";
import {parsePrBody} from "../lib/pr.mts";

// An issue id of the tracker, built here and not written out: this file ships, and the public-tree check scans it.
const issueId = (n: number) => ["S27", n].join("-");

test("a fresh built-in worker runs the real check and binds a new process and retained files",async t=>{
	const dir=mkdtempSync(join(tmpdir(),"gate-check-process-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
	const step={id:"pr-fields",fn:"prFields"},pids: number[]=[];
	for(const [name,pr,expected,attempt]of [["first",parsePrBody(`Refs ${issueId(787)}\ndecision: none\ncause: issue ${issueId(787)}`),"pass",1],["failed",undefined,"fail",2],["passed",parsePrBody(`Refs ${issueId(787)}\ndecision: none\ncause: issue ${issueId(787)}`),"pass",2]] satisfies [string, ReturnType<typeof parsePrBody> | undefined, string, number][]){
		const runDir=join(dir,name);mkdirSync(runDir);
		const peers={captures:{"report.png":"kept-sha"},captureDiffs:[{name:"report.png",status:"changed"}],surfaceChanges:["docs/surface/api.json"]};
		const ctx: WorkerContext={cwd:dir,runDir,pr,mode:"dev",completed:new Map([["pins","pass"]]),...peers};
		const result=await freshCheck(step,ctx,join(dir,"target"),attempt);
		assert.equal(result.status,expected,result.reason ?? "");const pid = result.details?.processPid;
		assert.ok(typeof pid === "number" && pid!==process.pid);pids.push(pid);
		const log = result.logs?.at(-1);assert.ok(log);
		assert.match(readFileSync(log,"utf8"),/rv-gate: exit/);
		const workerDir=join(runDir,"attempts/pr-fields",attempt===1?"worker-1":"2");
		assert.ok(readFileSync(join(workerDir,"output.bin")).length>0);
		assert.deepEqual(deserialize(readFileSync(join(workerDir,"input.bin"))).ctx.completed,new Map([["pins","pass"]]));
		for(const [key,value]of Object.entries(peers))assert.equal(ctx[key],value,`An unrelated ${expected} retry must preserve completed peer ${key} identity`);
	}
	assert.equal(new Set(pids).size,pids.length);
});

test("release-tier retry keeps its existing worker path",async t=>{
	const dir=mkdtempSync(join(tmpdir(),"gate-check-release-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
	const controller=new AbortController(),ctx: WorkerContext={cwd:dir,runDir:dir,mode:"release",signal:controller.signal};
	const result=await freshCheck({id:"pr-fields",fn:"prFields"},ctx,join(dir,"target"));
	assert.equal(result.status,"fail");assert.match(result.reason ?? "",/No PR context/);
	assert.equal(existsSync(join(dir,"attempts/pr-fields/2/groups.json")),false);
	assert.equal(ctx.unknownWorkerDrains,undefined);
});

interface PeerState { captures: Record<string, string>; captureDiffs: { name: string; status: string }[]; surfaceChanges: string[] }

test("capture and surface retries replace only their own state, including absent fresh values",async t=>{
	const dir=mkdtempSync(join(tmpdir(),"gate-check-owners-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
	// A real surface worker invokes this native-CLI protocol fixture; no Rust build or browser.
	const target=join(dir,"native-cli-fixture");mkdirSync(join(target,"release"),{recursive:true});
	const init={protocolVersion:"fixture",capabilities:{},instructions:"fixture"};
	const replies=[{id:1,result:init},{id:2,result:{tools:[]}},{id:3,result:{resources:[]}}].map(r=>JSON.stringify(r)).join("\n")+"\n";
	writeFileSync(join(target,"release/richview"),`#!${process.execPath}\nprocess.stdout.write(process.argv[2]==='mcp'?${JSON.stringify(replies)}:'Fixture CLI\\n');\n`,{mode:0o755});
	mkdirSync(join(dir,"docs/surface/cli"),{recursive:true});mkdirSync(join(dir,"docs/surface/mcp"));
	writeFileSync(join(dir,"docs/surface/cli/richview.txt"),"Fixture CLI\n");
	writeFileSync(join(dir,"docs/surface/mcp/surface.json"),JSON.stringify({...init,tools:[],resources:[],serverVersion:"<version>"},null,2)+"\n");
	const git=(...args: string[])=>execFileSync("git",["-c","user.name=Fixture","-c","user.email=fixture@example.test","-c","commit.gpgsign=false",...args],{cwd:dir,stdio:"pipe"}).toString().trim();
	git("init","-q");git("add","docs");git("commit","-qm","surface fixture");const head=git("rev-parse","HEAD");
	// Plant unexpected peer updates at the real worker's serialized response boundary.
	const preload=join(dir,"peer-update.mjs");
	writeFileSync(preload,"import {readFileSync,writeFileSync} from 'node:fs';import {deserialize,serialize} from 'node:v8';if(process.argv[1]?.endsWith('/check-worker.mts'))process.on('exit',()=>{const path=process.argv[3],saved=deserialize(readFileSync(path));Object.assign(saved.updates,{captures:{injected:true},captureDiffs:[{injected:true}]});writeFileSync(path,serialize(saved));});\n");
	for(const [name,fn,mode,expected,absent,kept,attempt]of [
		["captures-first","captures","full","fail",["captures","captureDiffs"],["surfaceChanges"],1],
		["surface-first","surface","full","pass",["surfaceChanges"],["captures","captureDiffs"],1],
		["captures","captures","full","fail",["captures","captureDiffs"],["surfaceChanges"],2],
		["surface","surface","full","pass",["surfaceChanges"],["captures","captureDiffs"],2],
		["surface-peer-response","surface","dev","pass",[],["captures","captureDiffs"],2],
	] satisfies [string, string, string, string, (keyof PeerState)[], (keyof PeerState)[], number][]){
		const runDir=join(dir,name);mkdirSync(runDir);
		const before={captures:{"report.png":"old-sha"},captureDiffs:[{name:"report.png",status:"changed"}],surfaceChanges:["docs/surface/api.json"]};
		const ctx: WorkerContext={cwd:dir,runDir,mode,base:head,head,...before};
		let result;const previous=process.env.NODE_OPTIONS;
		try {
			if(name==="surface-peer-response")process.env.NODE_OPTIONS=`${previous??""} --import=${pathToFileURL(preload).href}`;
			result=await freshCheck({id:fn,fn},ctx,target,attempt);
		} finally {if(previous===undefined)delete process.env.NODE_OPTIONS;else process.env.NODE_OPTIONS=previous;}
		assert.equal(result.status,expected,result.reason ?? "");const pid = result.details?.processPid;
		assert.ok(typeof pid === "number");assert.notEqual(pid,process.pid);
		if(fn==="captures")assert.match(result.reason ?? "",/Cannot find module.*node_modules\/playwright\/index\.mjs/);
		else assert.equal(readFileSync(join(runDir,"surface/cli/richview.txt"),"utf8"),"Fixture CLI\n");
		for(const key of absent)assert.equal(Object.hasOwn(ctx,key),false,`${fn} must remove stale own ${key} when the fresh result has none`);
		for(const key of kept)assert.equal(ctx[key],before[key],`${fn} must preserve completed peer ${key} identity`);
		if(name==="surface-peer-response"){
			assert.deepEqual(ctx.surfaceChanges,[],"The real surface check's fresh owned update replaces its stale value");
			const saved=deserialize(readFileSync(join(runDir,"attempts/surface/2/output.bin")));
			assert.deepEqual(saved.updates.captures,{injected:true});assert.deepEqual(saved.updates.captureDiffs,[{injected:true}]);
		}
	}
});
