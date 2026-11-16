// Host reason (ARC-03): verify partial retries without starting host commands.
import test from "node:test";
import assert from "node:assert/strict";
import { retryCommands } from "../lib/affected-retry.mjs";

test("only failed commands retry and their first attempt remains visible", async () => {
	const calls = [];
	const outcomes = await retryCommands(["a", "b", "c"], async (id, attempt) => {
		calls.push(`${id}:${attempt}`);
		return {status: id === "b" && attempt === 1 ? "fail" : "pass"};
	});
	assert.deepEqual(calls, ["a:1", "b:1", "c:1", "b:2"]);
	assert.deepEqual(outcomes.map(item => item.status), ["pass", "pass", "pass"]);
	assert.deepEqual(outcomes[1].attempts.map(item => item.status), ["fail", "pass"]);
	assert.equal(outcomes[1].flaky, true);
});

test("second failures and unrun do not become passes; thrown commands fail independently", async () => {
	const calls = [];
	const results = await retryCommands(["a", "b", "c"], async id => {
		calls.push(id);
		if (id === "a") throw Error("broken");
		return {status: id === "b" ? "unrun" : "pass"};
	});
	assert.deepEqual(calls, ["a", "b", "c", "a"]);
	assert.deepEqual(results.map(item => item.status), ["fail", "unrun", "pass"]);
});
