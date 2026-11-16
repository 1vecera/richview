// Host reason (ARC-03): reserve gate permits before holding the shared Rust debug target.
import {retryCommands} from "./affected-retry.mjs";

/** Prepare fresh in this run and retry only failed stable shards while the same hold is active. */
export async function runAffectedCohort({step, ctx, parallel, schedule, expand, hold, execute, selected, signal}) {
	if (step?.cohort !== "rust-debug" || !Number.isSafeInteger(parallel) || parallel < 1 || [schedule, expand, hold, execute].some(fn => typeof fn !== "function")) throw new Error("Invalid cohort request");
	const cohort = await expand(step, ctx);
	if (!cohort?.prepare || !Array.isArray(cohort.shards) || !cohort.shards.length || !Number.isSafeInteger(cohort.maxConcurrent) || cohort.maxConcurrent < 1) throw new Error("Invalid expanded cohort");
	const ids = cohort.shards.map(shard => shard.id);
	if (ids.some(id => typeof id !== "string" || !id) || new Set(ids).size !== ids.length) throw new Error("Cohort shards need unique stable ids");
	const chosen = selected === undefined ? ids : selected;
	if (!Array.isArray(chosen) || !chosen.length || new Set(chosen).size !== chosen.length || chosen.some(id => !ids.includes(id))) throw new Error("Unknown or duplicate cohort shard selection");
	const permits = Math.min(parallel, cohort.maxConcurrent);
	return schedule(() => hold(step.cohort, ctx, async () => {
		const [prepare] = await retryCommands([cohort.prepare], execute, {signal});
		if (prepare.status !== "pass") return {status: "fail", prepare, shards: cohort.shards.map(shard => ({id: shard.id, status: "unrun", reason: "Cohort preparation did not pass."}))};
		const selectedShards = cohort.shards.filter(shard => chosen.includes(shard.id));
		const results = await retryCommands(selectedShards, execute, {limit: permits, signal, shouldRetry: result => !result.environmentRetry});
		const missing = results.flatMap((result, index) => result.environmentRetry ? [index] : []);
		let environmentPreparation;
		if (missing.length && !signal?.aborted) {
			// The pool has drained before rebuilding. A second disappearance is a hard failure.
			try { environmentPreparation = await execute(cohort.prepare, prepare.attempts.length + 1); }
			catch (error) { environmentPreparation = {status: "fail", reason: String(error?.message ?? error)}; }
			if (environmentPreparation.status === "pass") {
				const retries = await retryCommands(missing.map(index => selectedShards[index]), (shard) => execute(shard, 2), {limit: permits, signal, shouldRetry: () => false});
				for (let i = 0; i < missing.length; i++) {
					const index = missing[i], prior = results[index], retry = retries[i];
					results[index] = {...retry, environmentRetry: true, attempts: [...prior.attempts, {...retry.attempts[0], number: 2}]};
				}
			}
		}
		const byId = new Map(selectedShards.map((shard, index) => [shard.id, {...results[index], id: shard.id}]));
		const shards = cohort.shards.map(shard => byId.get(shard.id) ?? {id: shard.id, status: "unrun", reason: "Shard not selected."});
		return {status: results.every(result => result.status === "pass") && (!environmentPreparation || environmentPreparation.status === "pass") ? "pass" : "fail", prepare, shards,
			...(missing.length && {environmentRetry: true, environmentPreparation})};
	}), permits);
}
