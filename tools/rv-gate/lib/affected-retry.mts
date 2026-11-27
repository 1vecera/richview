// Host reason (ARC-03): retain individual command outcomes while retrying only failures.
import { runPool } from "./release.mts";
import type { CommandVerdict } from "./results.mts";

export type RetriedCommand = CommandVerdict & { attempts: (CommandVerdict & { number: number })[] };

/** Execute each command once, then execute only failures again under the same concurrency limit. */
export async function retryCommands<T>(commands: readonly T[], execute: (command: T, attempt: number, index: number) => CommandVerdict | Promise<CommandVerdict>,
	{limit = 1, signal, shouldRetry = result => result.status === "fail"}: {
		limit?: number; signal?: AbortSignal; shouldRetry?: (result: CommandVerdict, command: T) => boolean;
	} = {}): Promise<RetriedCommand[]> {
	if (!Array.isArray(commands) || typeof execute !== "function" || !Number.isSafeInteger(limit) || limit < 1) throw new Error("Invalid command retry request");
	const run = async (indices: number[], attempt: number): Promise<CommandVerdict[]> => runPool(indices.map(index => async () => {
		try {
			const result = await execute(commands[index], attempt, index);
			if (!result || !["pass", "fail", "unrun"].includes(result.status)) throw new Error("Command returned an invalid status");
			return result;
		} catch (error) { return {status: "fail", reason: String((error as Error | null)?.message ?? error)}; }
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
