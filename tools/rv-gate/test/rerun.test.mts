import assert from "node:assert/strict";
import {mkdtempSync, readFileSync, rmSync, writeFileSync, existsSync, mkdirSync} from "node:fs";
import {tmpdir} from "node:os";
import {join} from "node:path";
import test from "node:test";
import type {GateStep} from "../lib/steps.mts";
import type {Phase, PhasePolicy} from "../lib/phase.mts";
import type {CommandVerdict} from "../lib/results.mts";
import {archiveAttempt, archiveBuildSnapshot, withRerun} from "../lib/rerun.mts";
import {runLogged} from "../lib/proc.mts";

const policy = (phase: Phase = "prelaunch"): PhasePolicy => ({phase, since:"2026-11-01", by:"Fixture", rotation:[]});
const step = (id: string, options: Partial<GateStep> = {}): GateStep => ({id, title:id, tier:"full", cards:[], ...options});

test("prelaunch retains both fresh child processes and marks a recovered failure flaky",async t=>{
	const dir=mkdtempSync(join(tmpdir(),"gate-rerun-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
	const result=await withRerun(policy(),(n: number)=>runLogged(process.execPath,["-e",`console.log(JSON.stringify({pid:process.pid}));process.exit(${n===1?1:0})`],{cwd:dir,logPath:join(dir,`attempt${n}.log`)}));
	assert.equal(result.status,"pass");assert.equal(result.flaky,true);assert.deepEqual(result.attempts.map((attempt: {status: string})=>attempt.status),["fail","pass"]);
	const pids=[1,2].map(n=>{
		const line = readFileSync(join(dir,`attempt${n}.log`),"utf8").split("\n").find(line=>line.startsWith("{"));
		assert.ok(line);
		return JSON.parse(line).pid;
	});
	assert.notEqual(pids[0],pids[1]);
});
test("deterministic failure runs twice, launch once, and cancellation never retries",async()=>{
	for(const [phase,count]of [["prelaunch",2],["launch",1]] as const){let calls=0;const r=await withRerun(policy(phase),async()=>{calls++;return{status:"fail",reason:"deterministic"};});assert.equal(calls,count);assert.equal(r.status,"fail");assert.equal(r.flaky,false);}
	const c=new AbortController();let calls=0;const r=await withRerun(policy(),async()=>{calls++;c.abort();return{status:"fail"};},{signal:c.signal});assert.equal(calls,1);assert.equal(r.status,"fail");
	const unrun=await withRerun(policy(),async (n: number)=>({status:n===1?"fail":"unrun"}));assert.equal(unrun.status,"fail");assert.equal(unrun.flaky,false);
});
test("reruns preserve first raw artifacts and leave unrelated concurrent outputs in place",async t=>{
	const dir=mkdtempSync(join(tmpdir(),"gate-attempt-files-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
	mkdirSync(join(dir,"checks"));mkdirSync(join(dir,"logs"));writeFileSync(join(dir,"unrelated.json"),"peer");
	writeFileSync(join(dir,"checks/result.json"),"first");writeFileSync(join(dir,"logs/check-0.log"),"first log");
	const journal=join(dir,"logs/check-0.log.browser-events.jsonl"),raw='{"pid":123}\n';writeFileSync(journal,raw);
	const event={pid:123,journal,surface:"playwright" as const};
	const attempt={status:"fail" as const,logs:[join(dir,"logs/check-0.log")],details:{browserRelaunches:[event],commands:[{status:"fail" as const,pid:321,command:"original command",browserRelaunches:[event]}]}};
	const first=archiveAttempt(step("check",{run:[{cmd:"node",args:["check.mjs","--out","{run}/checks"]}]}),{runDir:dir},attempt);
	assert.ok(first.logs && first.details?.browserRelaunches && first.details.commands);
	const [command] = first.details.commands as typeof attempt.details.commands;
	assert.ok(command.browserRelaunches);
	assert.equal(readFileSync(join(dir,"attempts/check/1/checks/result.json"),"utf8"),"first");
	assert.equal(readFileSync(first.logs[0],"utf8"),"first log");assert.equal(existsSync(join(dir,"checks")),false);
	assert.equal(readFileSync(first.details.browserRelaunches[0].journal,"utf8"),raw);assert.equal(readFileSync(command.browserRelaunches[0].journal,"utf8"),raw);
	assert.equal(command.pid,321);assert.equal(command.command,"original command");assert.equal(first.details.browserRelaunches[0].pid,123);
	assert.equal(readFileSync(join(dir,"unrelated.json"),"utf8"),"peer");
	assert.throws(()=>archiveAttempt(step("bad",{run:[{cmd:"node",args:["{run}"]}]}),{runDir:dir},{logs:[]}),/explicit retryArtifacts/);
	assert.throws(()=>archiveAttempt(step("bad",{retryArtifacts:["../outside"]}),{runDir:dir},{logs:[]}),/leaves its run/);
});
test("a failed build's private snapshot is retained before its fresh attempt resets ownership",t=>{
	const dir=mkdtempSync(join(tmpdir(),"gate-build-attempt-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
	const runDir=join(dir,"run"),tempDir=join(dir,"temp");mkdirSync(join(tempDir,"target"),{recursive:true});writeFileSync(join(tempDir,"target/native"),"first binary");
	const files=[{path:"native",sha256:"first"}],ctx={runDir,tempDir,snapshot:join(tempDir,"target"),snapshotFiles:files};
	const result=archiveAttempt(step("build",{run:[]}),ctx,{logs:[]});assert.deepEqual(result.snapshotFiles,files);
	const archived=archiveBuildSnapshot(ctx);assert.ok(archived);assert.equal(readFileSync(join(archived,"native"),"utf8"),"first binary");
	assert.equal(ctx.snapshot,undefined);assert.deepEqual(ctx.snapshotFiles,[]);assert.deepEqual(result.snapshotFiles,files);
	assert.throws(()=>archiveBuildSnapshot({...ctx,snapshot:join(dir,"foreign")}),/outside/);
});
test("a Firefox rerun preserves its protocol metadata and frees both exclusive-create paths",t=>{
	const dir=mkdtempSync(join(tmpdir(),"gate-protocol-attempt-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));
	const path=join(dir,"firefox-protocol.jsonl"),metadataPath=path+".meta.json";
	writeFileSync(path,'{"first":true}\n');writeFileSync(metadataPath,'{"ended":true}\n');
	// The retained metadata fixture supplies the two fields used by archiving and its assertions.
	const firefoxProtocol = {metadataPath,critical_complete:true} as NonNullable<CommandVerdict["firefoxProtocol"]>;
	const attempt={status:"fail" as const,details:{commands:[{status:"fail" as const,pid:123,firefoxProtocol}]}};
	const first=archiveAttempt(step("firefox",{run:[{cmd:"node",args:[],env:{RV_GATE_FIREFOX_PROTOCOL_LOG:"{run}/firefox-protocol.jsonl"}}]}),{runDir:dir},attempt);
	assert.ok(first.details?.commands);
	const [command] = first.details.commands as typeof attempt.details.commands;
	assert.ok(command.firefoxProtocol);
	assert.equal(readFileSync(command.firefoxProtocol.metadataPath,"utf8"),'{"ended":true}\n');
	assert.equal(readFileSync(join(dir,"attempts/firefox/1/firefox-protocol.jsonl"),"utf8"),'{"first":true}\n');
	writeFileSync(path,"second",{flag:"wx"});writeFileSync(metadataPath,"second metadata",{flag:"wx"});
	assert.equal(command.pid,123);assert.equal(command.firefoxProtocol.critical_complete,true);
});
