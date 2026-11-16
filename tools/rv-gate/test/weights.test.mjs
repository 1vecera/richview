// Host reason (ARC-03): the longest-first ordering of full-run work, from recorded run times.
import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { runFullSteps } from "../lib/full.mjs";
import { NO_WEIGHTS, commandOrder, longestFirst, readRecentWeights, weightsFrom } from "../lib/weights.mjs";

const receipt = (steps) => ({ kind: "full", steps });
const step = (id, runMs, commands, status = "pass") => ({ id, status, runMs, details: { commands: commands?.map((durationMs) => ({ status: "pass", durationMs })) } });

test("weights are medians of passing observations, by step and by command position", () => {
	const weights = weightsFrom([
		receipt([step("a", 100, [10, 90]), step("b", 50), step("c", 999, undefined, "fail")]),
		receipt([step("a", 300, [30, 70]), step("b", 70)]),
		receipt([step("a", 200, [20, 80])]),
	]);
	assert.equal(weights.steps.get("a"), 200);
	assert.equal(weights.steps.get("b"), 60);
	assert.equal(weights.steps.has("c"), false);
	assert.deepEqual(weights.commands.get("a"), [20, 80]);
});

test("longest first keeps ties and unknown weights in place, and unknown counts as the median", () => {
	const weight = { a: 5, b: 50, c: 5, d: 20 };
	assert.deepEqual(longestFirst(["a", "b", "c", "d"], (id) => weight[id]), ["b", "d", "a", "c"]);
	assert.deepEqual(longestFirst(["a", "x", "b"], (id) => weight[id]), ["b", "x", "a"]);
	assert.deepEqual(longestFirst(["a", "b"], () => undefined), ["a", "b"]);
});

test("reading recent weights skips a malformed receipt and takes the newest full receipts", (t) => {
	const dir = mkdtempSync(join(tmpdir(), "rv-weights-"));
	t.after(() => rmSync(dir, { recursive: true, force: true }));
	assert.equal(readRecentWeights(join(dir, "missing")), NO_WEIGHTS);
	writeFileSync(join(dir, "full-20261101T100000Z-aaaaaaaaaa.json"), JSON.stringify(receipt([step("a", 10)])));
	writeFileSync(join(dir, "full-20261101T110000Z-bbbbbbbbbb.json"), "{ half written");
	writeFileSync(join(dir, "full-20261101T120000Z-cccccccccc.json"), JSON.stringify(receipt([step("a", 30)])));
	writeFileSync(join(dir, "release-20261101T130000Z-dddddddddd.json"), JSON.stringify(receipt([step("a", 9999)])));
	assert.equal(readRecentWeights(dir).steps.get("a"), 20);
	assert.equal(readRecentWeights(dir, 1).steps.get("a"), 30);
});

test("with weights the steps of a group start longest first, and a declared dependency still holds", async () => {
	const steps = [
		{ id: "build", run: [{ cmd: "cargo" }] },
		{ id: "short", needs: "build", run: [{ cmd: "node" }] },
		{ id: "long", needs: "build", run: [{ cmd: "node" }] },
		{ id: "mid", needs: "build", run: [{ cmd: "node" }] },
		{ id: "late", needs: "build", after: ["long"], run: [{ cmd: "node" }] },
	];
	const order = [];
	const run = async (candidate) => { order.push(candidate.id); return { status: "pass" }; };
	const weights = { steps: new Map([["short", 1], ["long", 100], ["mid", 10], ["late", 500]]), commands: new Map() };
	await runFullSteps(steps, run, 2, weights);
	assert.deepEqual(order, ["build", "long", "mid", "short", "late"]);
	const plain = [];
	await runFullSteps(steps, async (candidate) => { plain.push(candidate.id); return { status: "pass" }; }, 2);
	assert.deepEqual(plain.slice(0, 4), ["build", "short", "long", "mid"]);
});

test("a sequential step keeps its command order; only a step that runs several at once is reordered", () => {
	const commands = [0, 1, 2, 3, 4].map((index) => ({ command: { cmd: "node" }, index }));
	const weights = { steps: new Map(), commands: new Map([["steps", [5, 1, 3, 2, 9]]]) };
	const indexes = (list) => list.map(({ index }) => index);
	assert.deepEqual(indexes(commandOrder({ id: "steps" }, commands, weights)), [0, 1, 2, 3, 4], "no step.parallel: sequential");
	assert.deepEqual(indexes(commandOrder({ id: "steps", parallel: 1 }, commands, weights)), [0, 1, 2, 3, 4]);
	assert.deepEqual(indexes(commandOrder({ id: "steps", parallel: 3 }, commands, weights)), [4, 0, 2, 3, 1]);
	assert.deepEqual(indexes(commandOrder({ id: "steps", parallel: 3 }, commands, undefined)), [0, 1, 2, 3, 4]);
});

test("every sequential step of the registry keeps its order under any weights", async () => {
	const { STEPS } = await import("../lib/steps.mjs");
	for (const step of STEPS.filter((candidate) => candidate.run?.length > 1 && !(candidate.parallel > 1))) {
		const commands = step.run.map((command, index) => ({ command, index }));
		const weights = { steps: new Map(), commands: new Map([[step.id, commands.map((_, index) => commands.length - index === 1 ? 1e9 : index)]]) };
		assert.deepEqual(commandOrder(step, commands, weights), commands, step.id);
	}
});
