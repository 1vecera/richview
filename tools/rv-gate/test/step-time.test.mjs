import test from "node:test";
import assert from "node:assert/strict";
import { receiptStepTime, executionMeter } from "../lib/step-time.mjs";

const unknown = { runMs: null, queuedMs: null, source: "unknown" };
const parallel = { kind: "full", parallel: 4 };
const footer = duration => `rv-gate: exit 0 after ${duration} ms\n`;
const deferred = () => { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; };

test("the production default clock measures work with its required Performance receiver", async () => {
	const before = performance.now(), meter = executionMeter();
	assert.equal(await meter.measure(() => 42), 42);
	const elapsed = performance.now() - before;
	const result = meter.finish(elapsed);
	assert.ok(Number.isFinite(result.runMs) && result.runMs >= 0 && result.runMs <= elapsed);
	assert.ok(Number.isFinite(result.queuedMs) && result.queuedMs >= 0);
});

test("explicit execution telemetry wins, including zero, without reading logs", () => {
	const readLog = () => { throw new Error("telemetry must not need logs"); };
	assert.deepEqual(receiptStepTime({ durationMs: 900, runMs: 30, queuedMs: 870, logs: ["gone"] }, parallel, { readLog }),
		{ runMs: 30, queuedMs: 870, source: "telemetry" });
	assert.deepEqual(receiptStepTime({ durationMs: 900, runMs: 0 }, parallel),
		{ runMs: 0, queuedMs: 900, source: "telemetry" });
	assert.deepEqual(receiptStepTime({ runMs: 30 }, parallel),
		{ runMs: 30, queuedMs: null, source: "telemetry" });
	assert.equal(receiptStepTime({ durationMs: 10, runMs: 30 }, parallel).queuedMs, null);
});

test("only explicitly serial full/prod elapsed duration can stand in for execution", () => {
	for (const kind of ["prod", "full"]) assert.deepEqual(receiptStepTime({ durationMs: 80 }, { kind, parallel: 1 }),
		{ runMs: 80, queuedMs: 0, source: "serial-duration" });
	for (const receipt of [parallel, { kind: "prod", parallel: 2 }, { kind: "full" }, { kind: "dev", parallel: 1 }])
		assert.deepEqual(receiptStepTime({ durationMs: 900000 }, receipt), unknown);
});

test("retained command footers give a lower bound, never parallel receipt elapsed time", () => {
	// Actual footers: full-20261030T035828Z-1fb0f71e82 packaging-0.log (2727),
	// rust-tests-0.log (nested 3489, enclosing 99408). No log mtimes or starts are inferred.
	const logs = new Map([["packaging", "rv-gate: exit 0 after 2727 ms\n"],
		["rust", "rv-gate: exit 0 after 3489 ms\n\nrv-gate: exit 0 after 99408 ms\n"]]);
	assert.deepEqual(receiptStepTime({ durationMs: 1155000, logs: ["packaging", "rust", "rust"] }, parallel, { readLog: path => logs.get(path) }),
		{ runMs: 99408, queuedMs: null, source: "log-footer-lower-bound" });
	assert.equal(receiptStepTime({ logs: ["packaging"] }, parallel, { readLog: path => Buffer.from(logs.get(path)) }).runMs, 2727);
});

test("forwarded retained shard footers are not summed or counted twice", () => {
	// Footer/marker excerpts from full-20261030T042439Z-e6c4daedaf/no-js-0.log.
	const text = "$ node scripts/verify-no-js.mjs (2 shards, command budget 4)\n\n"
		+ "--- shard 0/2 ---\nrv-gate: exit 0 after 20231 ms\n\n"
		+ "--- shard 1/2 ---\nrv-gate: exit 0 after 30311 ms\n\nrv-gate: 20 cases completed across 2 shards; 0 failed\n";
	assert.deepEqual(receiptStepTime({ durationMs: 452000, logs: ["parent", "leaf"] }, parallel,
		{ readLog: path => path === "parent" ? text : "rv-gate: exit 0 after 30311 ms\n" }),
		{ runMs: 30311, queuedMs: null, source: "log-footer-lower-bound" });
	for (const broken of [text.replace("--- shard 1/2 ---", "--- shard 0/2 ---"),
		text.replace("rv-gate: exit 0 after 20231 ms", "unfinished"), text.replace("--- shard 1/2 ---", "--- shard 1/3 ---"), text.slice(0, text.indexOf("--- shard 1/2 ---"))])
		assert.deepEqual(receiptStepTime({ logs: ["parent"] }, parallel, { readLog: () => broken }), unknown);
});

test("missing, partial and invalid evidence stays unknown instead of becoming free execution", () => {
	for (const readLog of [() => undefined, () => null, () => { throw new Error("pruned"); },
		() => "still running", () => footer(20) + "$ a command still running\n", () => "rv-gate: exit 0 after -1 ms\n"])
		assert.deepEqual(receiptStepTime({ durationMs: 900, logs: ["missing"] }, parallel, { readLog }), unknown);
	assert.deepEqual(receiptStepTime({ logs: ["present", "missing"] }, parallel,
		{ readLog: path => path === "present" ? footer(20) : null }), unknown);
	for (const runMs of [null, -1, NaN, Infinity, "20"])
		assert.deepEqual(receiptStepTime({ durationMs: 900, runMs }, parallel), unknown);
	assert.deepEqual(receiptStepTime({ logs: [] }, parallel, { readLog: () => footer(20) }), unknown);
	assert.deepEqual(receiptStepTime({ logs: ["zero"] }, parallel, { readLog: () => footer(0) }),
		{ runMs: 0, queuedMs: null, source: "log-footer-lower-bound" });
});

test("admission timing excludes the queue and preserves permits, callback result and scheduler rejection", async () => {
	let time = 0, admitted;
	const meter = executionMeter({ now: () => time });
	const scheduled = meter.schedule((run, permits) => { assert.equal(permits, 3); admitted = run; return "queued"; });
	assert.equal(scheduled(() => { time = 120; return "completed"; }, 3), "queued");
	time = 100;
	assert.deepEqual(meter.finish(100), { runMs: 0, queuedMs: 100 });
	assert.equal(await admitted(), "completed");
	assert.deepEqual(meter.finish(150), { runMs: 20, queuedMs: 130 });
	const rejection = new Error("never admitted");
	assert.throws(() => meter.schedule(() => { throw rejection; })(() => { throw Error("must not run"); }), error => error === rejection);
	assert.deepEqual(meter.finish(150), { runMs: 20, queuedMs: 130 });
});

test("overlapping admitted commands contribute their interval union", async () => {
	let time = 0;
	const meter = executionMeter({ now: () => time }), a = deferred(), b = deferred();
	const first = meter.measure(async () => { await a.promise; time = 20; });
	time = 10;
	const second = meter.measure(async () => { await b.promise; time = 30; });
	assert.throws(() => meter.finish(40), /execution is active/);
	a.resolve(); await first;
	b.resolve(); await second;
	assert.deepEqual(meter.finish(40), { runMs: 30, queuedMs: 10 });
	assert.deepEqual(meter.finish(40), { runMs: 30, queuedMs: 10 });
});

test("nested built-in work is not charged twice and gaps remain outside execution", async () => {
	let time = 0;
	const meter = executionMeter({ now: () => time });
	await meter.measure(async () => { time = 5; await meter.measure(() => { time = 10; }); time = 20; });
	time = 30;
	await meter.measure(() => { time = 40; });
	assert.deepEqual(meter.finish(50), { runMs: 30, queuedMs: 20 });
});

test("sync throws and asynchronous rejections are measured in finally and keep the original error", async () => {
	let time = 0;
	const meter = executionMeter({ now: () => time }), failure = new Error("actual failure");
	await assert.rejects(meter.measure(() => { time = 10; throw failure; }), error => error === failure);
	time = 20;
	await assert.rejects(meter.measure(async () => { await Promise.resolve(); time = 35; throw failure; }), error => error === failure);
	assert.deepEqual(meter.finish(50), { runMs: 25, queuedMs: 25 });
});

test("empty execution and invalid timing inputs are explicit", async () => {
	const meter = executionMeter();
	assert.deepEqual(meter.finish(60), { runMs: 0, queuedMs: 60 });
	for (const duration of [-1, NaN, Infinity, "60"]) assert.throws(() => meter.finish(duration), /duration/);
	let time = 5;
	const backwards = executionMeter({ now: () => time });
	await backwards.measure(() => { time = 4; });
	assert.throws(() => backwards.finish(10), /clock/);
});
