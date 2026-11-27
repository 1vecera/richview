// Host reason (ARC-03): deterministic scheduling probes verify the full gate's build and quiet barriers.
import assert from "node:assert/strict";
import test from "node:test";
import { fullStepGroups, runFullSteps } from "../lib/full.mts";
import type { GateStep } from "../lib/steps.mts";
import {withRerun} from "../lib/rerun.mts";
import type {PhasePolicy} from "../lib/phase.mts";
import {outcomesForSelection, validateAffectedHistory, outstandingCoverage} from "../lib/affected-coverage.mts";

const command = (id: string) => ({ id, title: id, cards: ["DEL-33"], needs: "build" as const, run: [{ cmd: "node", args: [] }] });
type FixtureStep = Pick<GateStep, "id"> & Partial<GateStep>;
const fixtureSteps = (input: readonly FixtureStep[]): GateStep[] => input.map(step => ({ title: step.id, cards: [], tier: "full", ...step }));
const ids = (groups: readonly (readonly GateStep[])[]) => groups.map(group => group.map(step => step.id));
const steps = () => fixtureSteps([
	{ id: "pins", fn: "pins" }, { id: "build", run: [{ cmd: "cargo", args: [] }] },
	{ ...command("rust-slow"), run: [{ cmd: "cargo", args: [] }] }, command("chromium"), command("golden"),
	{ ...command("captures"), fn: "captures" }, { ...command("engines"), parallel: 2 },
	{ ...command("perf"), alone: true }, command("exports"), { id: "audit", fn: "audit" },
]);

test("the default full sequence retains every existing step and its order", () => {
	assert.deepEqual(ids(fullStepGroups(steps())), steps().map(step => [step.id]));
});

test("parallel full suites share one post-build cohort and retain exclusive quiet perf", () => {
	assert.deepEqual(ids(fullStepGroups(steps(), 2)), [["pins"], ["build"], ["rust-slow", "chromium", "golden", "captures", "engines", "exports", "audit"], ["perf"]]);
});

test("two full workers overlap actual commands and function suites without crossing build or quiet barriers", async () => {
	const events: string[] = [], active = new Set();
	let peak = 0;
	const result = await runFullSteps(steps(), async (step, scheduleCommand) => {
		const work = async () => {
		if (["pins", "build", "perf"].includes(step.id)) assert.equal(active.size, 0, `${step.id} must start alone`);
		active.add(step.id); peak = Math.max(peak, active.size); events.push(`start ${step.id}`);
		await new Promise(resolve => setTimeout(resolve, 5));
		active.delete(step.id); events.push(`end ${step.id}`);
		return { id: step.id, status: "pass" as const };
		};
		return step.fn ? work() : scheduleCommand(work);
	}, 2);
	assert.equal(peak, 2);
	assert.ok(events.indexOf("start chromium") < events.indexOf("end rust-slow"));
	assert.ok(events.indexOf("end build") < events.indexOf("start chromium"));
	assert.ok(events.indexOf("start engines") < events.indexOf("start captures"), "later engines need not wait behind the function suite");
	assert.deepEqual(events.slice(-2), ["start perf", "end perf"]);
	assert.deepEqual(result.map(step => step.id), ids(fullStepGroups(steps(), 2)).flat());
});

test("post-build dependencies wait only for their artifacts while own complete Cargo cohorts serialize", async () => {
	const cargo = (id: string) => ({ ...command(id), run: [{ cmd: "cargo", args: [] }, { cmd: "npm", args: [] }] });
	const input = [{ id: "build" }, cargo("slow"), command("browser"), { ...command("captures"), fn: "captures", after: ["browser"] }, cargo("docs"), command("engines"), { ...command("perf"), alone: true }];
	const events: string[] = [];
	const slow = Promise.withResolvers<void>(), browser = Promise.withResolvers<void>();
	let writers = 0;
	const execution = runFullSteps(fixtureSteps(input), async (step, schedule) => {
		if (step.id === "build") return { status: "pass" };
		events.push(`start ${step.id}`);
		const work = async () => {
			if (["slow", "docs"].includes(step.id)) {
				assert.equal(writers++, 0, "the complete mixed Cargo/npm step must keep its own-writer lease");
				try { if (step.id === "slow") await slow.promise; }
				finally { writers--; }
			} else if (step.id === "browser") await browser.promise;
			if (step.id === "slow") throw new Error("native test failed");
		};
		try { await (step.fn ? work() : schedule(work)); }
		finally { events.push(`end ${step.id}`); }
		return { status: "pass" };
	}, 3);
	let result;
	try {
		// Each turn drains the scheduler's promise continuations; host scheduling cannot
		// finish either explicit gate. The browser alone releases the capture dependency.
		await new Promise(setImmediate);
		assert.equal(writers, 1, "the slow mixed writer must be active");
		assert.ok(events.includes("start browser"));
		browser.resolve();
		await new Promise(setImmediate);
		assert.ok(events.includes("end browser"));
		assert.ok(events.includes("start captures"), "a declared artifact dependency does not inherit unrelated barriers");
		assert.equal(writers, 1, "captures must start while the unrelated writer is held");
	} finally {
		browser.resolve(); slow.resolve();
		result = await execution;
	}
	assert.ok(events.indexOf("start engines") < events.indexOf("end slow"));
	assert.ok(events.indexOf("end browser") < events.indexOf("start captures"));
	assert.ok(events.indexOf("start captures") < events.indexOf("end slow"), "a declared artifact dependency does not inherit unrelated barriers");
	assert.ok(events.indexOf("end slow") < events.indexOf("start docs"), "a failed writer must still drain before the next writer");
	assert.deepEqual(result.map(r => r.status), ["pass", "fail", "pass", "pass", "pass", "pass", "pass"]);
});

test("weighted commands reserve every worker, remain FIFO and release all permits on rejection", async () => {
	const input = [{ id: "build" }, command("pool"), { ...command("perf"), alone: true }];
	const events: string[] = [];
	let active = 0, peak = 0;
	const result = await runFullSteps(fixtureSteps(input), async (step, schedule) => {
		if (step.id !== "pool") { assert.equal(active, 0); return { status: "pass" }; }
		const work = (id: string, weight: number, fail = false) => schedule(async () => {
			active += weight; peak = Math.max(peak, active); events.push(`start ${id}`);
			try { await new Promise(resolve => setTimeout(resolve, 2)); if (fail) throw new Error("pooled child failed"); }
			finally { active -= weight; events.push(`end ${id}`); }
		}, weight);
		const outcomes = await Promise.allSettled([work("first", 1), work("wide", 3, true), work("last", 1)]);
		for (const weight of [0, -1, 1.5, 4, Infinity]) await assert.rejects(schedule(() => assert.fail("invalid weight started"), weight), /permits/);
		await work("after-error", 2);
		return { status: outcomes.some(r => r.status === "rejected") ? "fail" : "pass" };
	}, 3);
	assert.equal(peak, 3);
	assert.deepEqual(events, ["start first", "end first", "start wide", "end wide", "start last", "end last", "start after-error", "end after-error"]);
	assert.deepEqual(result.map(r => r.status), ["pass", "fail", "pass"]);
});

test("invalid artifact graphs fail before any command and no selected step disappears", async () => {
	for (const input of [
		[{ id: "build" }, { ...command("a"), after: ["missing"] }],
		[{ id: "build" }, { ...command("a"), after: ["b"] }, { ...command("b"), after: ["a"] }],
		[{ id: "build" }, { ...command("a"), after: ["later"] }, { ...command("later"), parallel: 1 }],
		[{ id: "build" }, command("same"), command("same")],
	]) {
		let ran = false;
		await assert.rejects(runFullSteps(fixtureSteps(input), () => { ran = true; return {status:"pass"}; }, 2), /dependenc|unique/);
		assert.equal(ran, false);
	}
});

test("internal command pools cannot multiply full-run concurrency and a failed command releases its slot", async () => {
	const input = [
		{ id: "build" },
		{ ...command("chromium"), parallel: 4, run: Array.from({ length: 4 }, (_, id) => ({ id: String(id), cmd: "node", args: [] })) },
		{ ...command("firefox"), parallel: 3, run: Array.from({ length: 3 }, (_, id) => ({ id: String(id), cmd: "node", args: [] })) },
		{ ...command("perf"), alone: true },
	];
	let active = 0, peak = 0;
	const completed: string[] = [];
	const result = await runFullSteps(fixtureSteps(input), async (step, scheduleCommand) => {
		if (["build", "perf"].includes(step.id)) {
			assert.equal(active, 0, `${step.id} crossed the command barrier`);
			return { id: step.id, status: "pass" as const };
		}
		assert.ok(step.run);
		const outcomes = await Promise.allSettled(step.run.map(command => scheduleCommand(async () => {
			active++; peak = Math.max(peak, active);
			try {
				await new Promise(resolve => setTimeout(resolve, 5));
				completed.push(`${step.id}-${command.id}`);
				if (step.id === "chromium" && command.id === "0") throw new Error("browser crashed");
			} finally { active--; }
		})));
		return { id: step.id, status: outcomes.some(outcome => outcome.status === "rejected") ? "fail" : "pass" };
	}, 2);
	assert.equal(peak, 2, "--parallel bounds all commands, including nested pools");
	assert.equal(completed.length, 7, "a rejected command must not lose queued checks");
	assert.equal(new Set(completed).size, 7);
	assert.deepEqual(result.map(step => [step.id, step.status]), [["build", "pass"], ["chromium", "fail"], ["firefox", "pass"], ["perf", "pass"]]);
});

test("dependent suites wait for packaging and an explicit single worker drains its peers", async () => {
	for (const exclusive of [false, true]) {
	const input = [{ id: "build" }, command("packaging"), command("other"), { ...command("vscode"), after: ["packaging"], ...(exclusive && { parallel: 1 }) }, command("exports")];
	const active = new Set();
	let packaged = false;
	const result = await runFullSteps(fixtureSteps(input), async (step, scheduleCommand) => {
		if (step.id === "build") return { id: step.id, status: "pass" as const };
		await scheduleCommand(async () => {
			if (step.id === "vscode") {
				assert.equal(packaged, true, "the VSIX must exist before the extension host starts");
				if (exclusive) assert.equal(active.size, 0, "parallel: 1 needs the host to itself");
			}
			active.add(step.id);
			try {
				if (step.id === "packaging") {
					await new Promise(resolve => setTimeout(resolve, 10));
					packaged = true;
				}
			} finally { active.delete(step.id); }
		});
		return { id: step.id, status: "pass" as const };
	}, 2);
	assert.deepEqual(result.map(step => step.status), ["pass", "pass", "pass", "pass", "pass"]);
	}
});

test("fresh-agent overlaps post-build work but drains before explicit dependents and quiet perf", async () => {
	for (const [dependent, fails] of [[false, false], [true, false], [false, true]]) {
		const input = [{ id: "build" }, command("browser"), { id: "capture", fn: "capture" }, command("fresh-agent"), { id: "summary", fn: "summary", ...(dependent && { after: ["fresh-agent"] }) }, { ...command("perf"), alone: true }];
		const events: string[] = [];
		let active = false;
		const { promise: done, resolve: finish } = Promise.withResolvers<void>();
		const result = await runFullSteps(fixtureSteps(input), async (step, scheduleCommand) => {
			events.push(`start ${step.id}`);
			if (step.id === "fresh-agent") {
				assert.ok(events.includes("end build"));
				return scheduleCommand(async () => { active = true; await done; active = false; events.push("end fresh-agent"); if (fails) throw new Error("agent run failed"); return { id: step.id, status: "pass" as const }; });
			}
			if (step.id === "capture") {
				try { assert.equal(active, true, "network work must overlap the capture barrier"); }
				finally { finish(); }
			}
			if (step.id === "perf" || step.id === "summary" && dependent) assert.equal(active, false, `${step.id} must await fresh-agent`);
			events.push(`end ${step.id}`);
			return { id: step.id, status: "pass" as const };
		}, 2);
		assert.ok(events.indexOf("start fresh-agent") < events.indexOf("start browser"));
		assert.ok(result.every(step => step.status === (step.id === "fresh-agent" && fails ? "fail" : "pass")));
		assert.deepEqual(result.map(step => step.id), input.map(step => step.id), "receipt order stays stable");
	}
});

test("the native exclusive turn drains early fresh-agent work as well as browser commands", async () => {
	const input = [{ id: "build" }, command("packaging"), { ...command("vscode"), after: ["packaging"], parallel: 1 }, command("fresh-agent"), { ...command("perf"), alone: true }];
	const active = new Set(), events: string[] = [];
	const result = await runFullSteps(fixtureSteps(input), async (step, schedule) => {
		if (step.id === "build") return { status: "pass" };
		return schedule(async () => {
			if (["vscode", "perf"].includes(step.id)) assert.equal(active.size, 0, `${step.id} overlapped early agent work`);
			active.add(step.id); events.push(`start ${step.id}`);
			try { await new Promise(resolve => setTimeout(resolve, step.id === "fresh-agent" ? 10 : 1)); }
			finally { active.delete(step.id); events.push(`end ${step.id}`); }
			return { status: "pass" };
		});
	}, 3);
	assert.ok(events.indexOf("start fresh-agent") < events.indexOf("start packaging"));
	assert.ok(events.indexOf("end fresh-agent") < events.indexOf("start vscode"));
	assert.deepEqual(result.map(r => r.id), input.map(step => step.id));
	assert.ok(result.every(r => r.status === "pass"));
});

test("serial candidate execution retains the original step order, including perf and late agents", async () => {
	const input = [{ id: "build" }, command("browser"), { ...command("perf"), alone: true }, command("fresh-agent")], ran: string[] = [];
	const results = await runFullSteps(fixtureSteps(input), async step => { ran.push(step.id); return { status: "pass" }; });
	assert.deepEqual(ran, input.map(step => step.id));
	assert.deepEqual(results.map(result => result.id), ran);
});

test("a thrown suite is a failed step and does not omit its peer, later checks or perf", async () => {
	const ran: string[] = [];
	const result = await runFullSteps(steps(), async step => {
		ran.push(step.id);
		if (step.id === "chromium") throw new Error("browser crashed");
		return { id: step.id, status: "pass" as const };
	}, 2);
	assert.equal(result.length, steps().length);
	assert.equal(new Set(ran).size, steps().length);
	assert.equal(result.find(step => step.id === "chromium")?.status, "fail");
	assert.equal(result.find(step => step.id === "golden")?.status, "pass");
	assert.equal(result.at(-1)?.id, "perf");
});

test("invalid parallel limits fail before any step executes", async () => {
	for (const parallel of [0, -1, NaN, Infinity, 1.5]) {
		let ran = false;
		await assert.rejects(runFullSteps(steps(), async () => { ran = true; return {status:"pass"}; }, parallel), /positive integer/);
		assert.equal(ran, false);
	}
});

test("a cancelled full scheduler drains active work and never starts queued or later suites", async () => {
	const controller = new AbortController(), started: string[] = [];
	const input = [{id: "build"}, {...command("active"), fn: "active"}, {...command("queued"), fn: "queued"}, {...command("quiet"), fn: "quiet", alone: true}];
	const results = await runFullSteps(fixtureSteps(input), async step => {
		started.push(step.id);
		if (step.id === "active") { controller.abort(); await new Promise(resolve => setTimeout(resolve, 5)); return {status: "unrun", reason: "Cancelled"}; }
		return {status: "pass"};
	}, 1, undefined, {signal: controller.signal});
	assert.deepEqual(started, ["build", "active"]);
	assert.deepEqual(results.map(result => result.status), ["pass", "unrun", "unrun", "unrun"]);
});

test("abort rejects queued permit requests and preserves outcomes completed before it", async () => {
	const controller=new AbortController(), started: string[]=[];let active=0;
	const { promise: ready, resolve: release } = Promise.withResolvers<void>();
	const input=[{id:"build"},...['a','b','c','queued'].map(id=>({...command(id),fn:id,after:[]})),{...command("quiet"),alone:true}];
	const running=runFullSteps(fixtureSteps(input),async step=>{
		started.push(step.id);if(step.id==="build")return {status:"pass"};
		if(step.id==="a")return {status:"fail",reason:"real failure"};
		if(step.id==="b"||step.id==="c"){if(++active===2)release();await new Promise(resolve=>controller.signal.addEventListener("abort",resolve,{once:true}));return {status:"fail"};}
		return {status:"pass"};
	},2,undefined,{signal:controller.signal});
	await ready;await new Promise(resolve=>setTimeout(resolve,5));controller.abort();
	const results=await running;
	assert.deepEqual(started,["build","a","b","c"]);
	assert.equal(results.find(row=>row.id==="a")?.status,"fail");
	assert.equal(results.find(row=>row.id==="b")?.status,"unrun");
	assert.equal(results.find(row=>row.id==="c")?.status,"unrun");
	assert.equal(results.find(row=>row.id==="queued")?.status,"unrun");
	assert.equal(results.find(row=>row.id==="quiet")?.status,"unrun");
});

const prelaunch: PhasePolicy = { phase: "prelaunch", since: "2026-10-30", by: "Fixture", rotation: [] };

test("a completed failed attempt stays pending when its retry is cancelled", async () => {
	const controller=new AbortController(), steps=[{id:"pins",fn:"pins"},{id:"never-failed",fn:"other"}];
	const results=await runFullSteps(fixtureSteps(steps),step=>withRerun(prelaunch,async (attempt: number)=>{
		if(step.id==="pins" && attempt===1)return {status:"fail",reason:"Completed predicate failure"};
		controller.abort();return {status:"unrun",reason:"Interrupted"};
	},{signal:controller.signal}),1,undefined,{signal:controller.signal});
	assert.equal(results[0].status,"fail");assert.equal(results[1].status,"unrun");
	assert.deepEqual(results[0].attempts?.map(attempt=>attempt.status),["fail","unrun"]);
	const selection=steps.map(step=>({id:step.id,step:step.id,status:"run" as const})),outcomes=outcomesForSelection(selection,results,"unrun");
	const receipt: import("../lib/affected-coverage.mts").AffectedReceipt={id:"cancelled",kind:"affected",at:"2026-10-30T12:00:00Z",base:"base",commit:"a",tree:"tree-a",policyHash:"old",selected:Object.keys(outcomes),outcomes,steps:results,browserRelaunchTelemetry:{status:"unrun"},result:"fail"};
	const history=validateAffectedHistory({receipts:[receipt],base:"base",head:"a",policyHash:"new",contains:()=>true,treeOf:()=>"tree-a",allowPolicyChange:true});
	const pending=outstandingCoverage({obligations:[],receipts:history,contains:()=>true,head:undefined});
	assert.ok(pending.some(item=>item.unit==="pins"));assert.ok(!pending.some(item=>item.unit==="never-failed"));
	assert.deepEqual(outstandingCoverage({obligations:[],receipts:[...history,{head:"b",outcomes:{pins:"pass"}}],contains:()=>true,head:undefined}),[]);
});

test("a failure first observed only after cancellation is not retained as a completed failure", async () => {
	const controller=new AbortController();
	const [result]=await runFullSteps(fixtureSteps([{id:"pins",fn:"pins"}]),()=>withRerun(prelaunch,async()=>{
		controller.abort();return {status:"fail",reason:"Stopped command"};
	},{signal:controller.signal}),1,undefined,{signal:controller.signal});
	assert.equal(result.status,"unrun");
});
