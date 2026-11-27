// Host reason (ARC-03): exercise permit and hold ordering for partial cohort retries.
import test from "node:test";
import assert from "node:assert/strict";
import {runAffectedCohort as runAffectedCohortImplementation} from "../lib/affected-cohort.mts";

type Outcome = {status: "pass" | "fail" | "unrun"; exitCode?: number; environmentRetry?: boolean};
type TestCohortResult = {status: "pass" | "fail"; shards: (Outcome & {id: string; attempts?: Outcome[]})[]; environmentRetry?: boolean};
type ShardCommand = {id: string; cmd: string; args: string[]};
type CohortRun<T> = {
	step: {id: string; cohort: "rust-debug"; tier: "full"; title: string; cards: string[]};
	ctx: T;
	parallel: number;
	expand: () => {prepare: ShardCommand; shards: ShardCommand[]; maxConcurrent: number};
	schedule: (work: () => Promise<TestCohortResult>, permits?: number) => Promise<TestCohortResult>;
	hold: (name: "rust-debug", ctx: T, work: () => Promise<TestCohortResult>) => Promise<TestCohortResult>;
	execute: (command: ShardCommand, attempt: number) => Promise<Outcome>;
	selected?: string[];
};
// The unchecked cohort inputs include deliberately invalid scheduler and expansion fixtures.
const runAffectedCohort = runAffectedCohortImplementation as unknown as <T>(options: CohortRun<T>) => Promise<TestCohortResult>;
const step: CohortRun<object>["step"] = {id: "rust-tests", cohort: "rust-debug", tier: "full", title: "Rust tests", cards: []};
const shard = (id: string): ShardCommand => ({id, cmd: "fixture", args: []});
const inline = <T,>(work: () => T): T => work();
const insideHold = <T,>(_name: string, _ctx: unknown, work: () => T): T => work();

test("reserve permits before hold; prepare once; rerun only a failed shard", async () => {
	const calls: string[] = [];
	const result = await runAffectedCohort({step, ctx: {target: "/target"}, parallel: 5,
		expand: () => ({prepare: shard("prepare"), shards: [shard("a"), shard("b"), shard("c")], maxConcurrent: 2}),
		schedule: async (fn, permits) => { calls.push(`reserve:${permits}`); const out = await fn(); calls.push("release"); return out; },
		hold: async (name, ctx, fn) => { calls.push(`hold:${name}:${ctx.target}`); const out = await fn(); calls.push("unhold"); return out; },
		execute: async (command: { id: string; }, attempt: number) => { calls.push(`${command.id}:${attempt}`); return {status: command.id === "b" && attempt === 1 ? "fail" : "pass"}; },
	});
	assert.deepEqual(calls, ["reserve:2", "hold:rust-debug:/target", "prepare:1", "a:1", "b:1", "c:1", "b:2", "unhold", "release"]);
	assert.equal(result.status, "pass");
	assert.deepEqual(result.shards[1].attempts?.map(a => a.status), ["fail", "pass"]);
});

test("a failed prepare never starts shards; retries can select stable ids", async () => {
	const calls: string[] = [];
	const base = {step, ctx: {}, parallel: 5,
		expand: () => ({prepare: shard("prepare"), shards: [shard("a"), shard("b")], maxConcurrent: 2}),
		schedule: inline, hold: insideHold,
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
	const calls: string[]=[];let prepared=0;
	const result=await runAffectedCohort({step,ctx:{},parallel:5,
		expand:()=>({prepare:shard("prepare"),shards:[shard("a"),shard("b")],maxConcurrent:2}),
		schedule:inline,hold:insideHold,
		execute:async(command: { id: string; },attempt)=>{calls.push(`${command.id}:${attempt}`);if(command.id==="prepare")prepared++;if(command.id==="a"&&prepared===1)return {status:"fail",exitCode:75,environmentRetry:true};return {status:"pass"};},
	});
	assert.equal(result.status,"pass");assert.equal(prepared,2);
	assert.deepEqual(calls,["prepare:1","a:1","b:1","prepare:2","a:2"]);
	assert.equal(result.environmentRetry,true);
});

test("a second missing-binary event fails without a third preparation", async () => {
	let prepares=0;
	const result=await runAffectedCohort({step,ctx:{},parallel:5,
		expand:()=>({prepare:shard("prepare"),shards:[shard("a")],maxConcurrent:2}),schedule:inline,hold:insideHold,
		execute:async (command: { id: string; })=>command.id==="prepare"?(prepares++,{status:"pass"}):{status:"fail",exitCode:75,environmentRetry:true},
	});
	assert.equal(result.status,"fail");assert.equal(prepares,2);
});
