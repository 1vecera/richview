// Host reason (ARC-03): exercise permit and hold ordering for partial cohort retries.
import test from "node:test";
import assert from "node:assert/strict";
import {runAffectedCohort} from "../lib/affected-cohort.mjs";

test("reserve permits before hold; prepare once; rerun only a failed shard", async () => {
	const calls = [];
	const result = await runAffectedCohort({step: {cohort: "rust-debug"}, ctx: {target: "/target"}, parallel: 5,
		expand: () => ({prepare: {id: "prepare"}, shards: [{id: "a"}, {id: "b"}, {id: "c"}], maxConcurrent: 2}),
		schedule: async (fn, permits) => { calls.push(`reserve:${permits}`); const out = await fn(); calls.push("release"); return out; },
		hold: async (name, ctx, fn) => { calls.push(`hold:${name}:${ctx.target}`); const out = await fn(); calls.push("unhold"); return out; },
		execute: async (command, attempt) => { calls.push(`${command.id}:${attempt}`); return {status: command.id === "b" && attempt === 1 ? "fail" : "pass"}; },
	});
	assert.deepEqual(calls, ["reserve:2", "hold:rust-debug:/target", "prepare:1", "a:1", "b:1", "c:1", "b:2", "unhold", "release"]);
	assert.equal(result.status, "pass");
	assert.deepEqual(result.shards[1].attempts.map(a => a.status), ["fail", "pass"]);
});

test("a failed prepare never starts shards; retries can select stable ids", async () => {
	const calls = [];
	const base = {step: {cohort: "rust-debug"}, ctx: {}, parallel: 5,
		expand: () => ({prepare: {id: "prepare"}, shards: [{id: "a"}, {id: "b"}], maxConcurrent: 2}),
		schedule: async fn => fn(), hold: async (_name, _ctx, fn) => fn(),
	};
	const failed = await runAffectedCohort({...base, execute: async (command, attempt) => {calls.push(`${command.id}:${attempt}`); return {status: "fail"};}});
	assert.deepEqual(calls, ["prepare:1", "prepare:2"]);
	assert.deepEqual(failed.shards.map(shard => shard.status), ["unrun", "unrun"]);
	calls.length = 0;
	await runAffectedCohort({...base, selected: ["b"], execute: async command => {calls.push(command.id); return {status: "pass"};}});
	assert.deepEqual(calls, ["prepare", "b"]);
	await assert.rejects(() => runAffectedCohort({...base, selected: ["missing"], execute: async () => ({status: "pass"})}), /Unknown/);
});

test("a vanished binary drains peers, reprepares once and retries only its shard", async () => {
	const calls=[];let prepared=0;
	const result=await runAffectedCohort({step:{cohort:"rust-debug"},ctx:{},parallel:5,
		expand:()=>({prepare:{id:"prepare"},shards:[{id:"a"},{id:"b"}],maxConcurrent:2}),
		schedule:fn=>fn(),hold:(_name,_ctx,fn)=>fn(),
		execute:async(command,attempt)=>{calls.push(`${command.id}:${attempt}`);if(command.id==="prepare")prepared++;if(command.id==="a"&&prepared===1)return {status:"fail",exitCode:75,environmentRetry:true};return {status:"pass"};},
	});
	assert.equal(result.status,"pass");assert.equal(prepared,2);
	assert.deepEqual(calls,["prepare:1","a:1","b:1","prepare:2","a:2"]);
	assert.equal(result.environmentRetry,true);
});

test("a second missing-binary event fails without a third preparation", async () => {
	let prepares=0;
	const result=await runAffectedCohort({step:{cohort:"rust-debug"},ctx:{},parallel:5,
		expand:()=>({prepare:{id:"prepare"},shards:[{id:"a"}],maxConcurrent:2}),schedule:fn=>fn(),hold:(_name,_ctx,fn)=>fn(),
		execute:async command=>command.id==="prepare"?(prepares++,{status:"pass"}):{status:"fail",exitCode:75,environmentRetry:true},
	});
	assert.equal(result.status,"fail");assert.equal(prepares,2);
});
