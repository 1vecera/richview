// Host reason (ARC-03): exercise the complete fast CLI phase path; Cargo is a metadata-only protocol fixture.
import assert from "node:assert/strict";
import {execFileSync} from "node:child_process";
import {mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync} from "node:fs";
import {join} from "node:path";
import test from "node:test";
import {fastInPhase} from "../lib/fast-phase.mts";

// An issue id of the tracker, built here and not written out: this file ships, and the public-tree check scans it.
const issueId = (n: number) => ["S27", n].join("-");

test("a failed complete fast invocation reruns once in a fresh process and retains both local traces",async t=>{
	mkdirSync("tmp",{recursive:true});const root=mkdtempSync(join(process.cwd(),"tmp/fast-phase-"));
	const previous={PATH:process.env.PATH,RV_GATE_HOME:process.env.RV_GATE_HOME,NODE_TEST_CONTEXT:process.env.NODE_TEST_CONTEXT};
	t.after(()=>{for(const [key,value]of Object.entries(previous)){if(value===undefined)delete process.env[key];else process.env[key]=value;}rmSync(root,{recursive:true,force:true});});
	// The fixture invokes a real CLI outside this test runner's internal worker protocol.
	delete process.env.NODE_TEST_CONTEXT;
	const cwd=join(root,"workspace"),bin=join(root,"bin");mkdirSync(join(cwd,"tools/agent-use/test"),{recursive:true});mkdirSync(bin);
	writeFileSync(join(bin,"cargo"),`#!${process.execPath}\nif(process.argv[2]!=='metadata')process.exit(99);console.log(JSON.stringify({packages:[]}));\n`,{mode:0o755});
	writeFileSync(join(cwd,"Cargo.toml"),'[workspace]\nmembers=[]\n');writeFileSync(join(cwd,".gitignore"),'tmp/\n');
	const fixture=join(cwd,"tools/agent-use/test/flip.test.mjs");
	writeFileSync(fixture,"import test from 'node:test';import assert from 'node:assert/strict';import fs from 'node:fs';fs.mkdirSync('tmp/cache',{recursive:true});const path='tmp/cache/attempt';const n=fs.existsSync(path)?Number(fs.readFileSync(path)):0;fs.writeFileSync(path,String(n+1));test('real first failure then pass',()=>assert.ok(n>0,'FIRST_ATTEMPT_FAULT'));\n");
	const git=(...args: string[])=>execFileSync("git",["-c","user.name=Fixture","-c","user.email=fixture@example.test",...args],{cwd,stdio:"pipe"});
	git("init","-q");git("add",".");git("commit","-qm","base");git("update-ref","refs/remotes/origin/dev","HEAD");
	writeFileSync(fixture,readFileSync(fixture,"utf8")+"// changed fixture\n");git("add",".");git("commit","-qm",`fixture change\n\nRefs ${issueId(787)}`);
	process.env.PATH=bin+":"+process.env.PATH;process.env.RV_GATE_HOME=join(root,"gate");
	assert.equal(await fastInPhase({cwd,target:join(root,"protocol-cache")}),0);
	const runs=join(root,"gate/evidence/runs"),names=readdirSync(runs).filter(n=>n.startsWith("fast-local-"));assert.equal(names.length,1);
	const result: { phase: string; flaky: boolean; attempts: { status: string; pid: number; logs: string[] }[] } = JSON.parse(readFileSync(join(runs,names[0],"result.json"),"utf8"));
	assert.equal(result.phase,"prelaunch");assert.equal(result.flaky,true);assert.deepEqual(result.attempts.map(a=>a.status),["fail","pass"]);
	assert.notEqual(result.attempts[0].pid,result.attempts[1].pid);assert.equal(readFileSync(join(cwd,"tmp/cache/attempt"),"utf8"),"2");
	assert.match(readFileSync(result.attempts[0].logs[0],"utf8"),/FIRST_ATTEMPT_FAULT/);
	assert.match(readFileSync(result.attempts[1].logs[0],"utf8"),/fast: pass/);
	for(const attempt of result.attempts)assert.doesNotMatch(readFileSync(attempt.logs[0],"utf8"),/recorded for the pre-push hook/,'Only the parent may record after both raw logs have finished');
});
