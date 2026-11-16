// Host reason (ARC-03): retain individual command outcomes while retrying only failures.
import { runPool } from "./release.mjs";

/** Execute each command once, then execute only failures again under the same concurrency limit. */
export async function retryCommands(commands, execute, {limit = 1, signal, shouldRetry = result => result.status === "fail"} = {}) {
	if (!Array.isArray(commands) || typeof execute !== "function" || !Number.isSafeInteger(limit) || limit < 1) throw new Error("Invalid command retry request");
	const run = async (indices, attempt) => runPool(indices.map(index => async () => {
		try {
			const result = await execute(commands[index], attempt, index);
			if (!result || !["pass", "fail", "unrun"].includes(result.status)) throw new Error("Command returned an invalid status");
			return result;
		} catch (error) { return {status: "fail", reason: String(error?.message ?? error)}; }
	}), limit, new AbortController(), {keepGoing: true});
	const first = await run(commands.map((_, index) => index), 1);
	const failed = first.flatMap((result, index) => result.status === "fail" && shouldRetry(result, commands[index]) && !signal?.aborted ? [index] : []);
	const second = await run(failed, 2);
	return first.map((result, index) => {
		const position = failed.indexOf(index);
		if (position < 0) return {...result, attempts: [{...result, number: 1}]};
		const retry = second[position];
		return {...retry, status: retry.status === "pass" ? "pass" : "fail", flaky: retry.status === "pass", attempts: [{...result, number: 1}, {...retry, number: 2}]};
	});
}
