// Host reason (ARC-03): schedule full-run host suites behind build and quiet-measurement barriers.
import { NO_WEIGHTS, longestFirst } from "./weights.mjs";
const usesCargo = step => step.run?.some(command => command.cmd === "cargo");
const freshAgent = step => step.id === "fresh-agent" && step.needs === "build" && !step.fn && !step.alone && step.parallel !== 1 && !step.after?.length && !usesCargo(step);

/** Group post-build work behind explicit exclusive turns; perf drains the complete run. */
export function fullStepGroups(steps, parallel = 1) {
	if (!Number.isSafeInteger(parallel) || parallel < 1) throw new Error("--parallel must be a positive integer");
	if (parallel === 1) return steps.map(step => [step]);
	const groups = [], quiet = [], pending = [];
	const flush = () => { if (pending.length) groups.push(pending.splice(0)); };
	let built = false;
	const agents = steps.some(step => step.id === "build") ? steps.filter(freshAgent) : [];
	for (const step of steps) {
		if (agents.includes(step)) continue;
		if (step.alone) { quiet.push([step]); continue; }
		if (built && step.parallel !== 1) pending.push(step);
		else { flush(); groups.push([step]); }
		if (step.id === "build") { built = true; pending.push(...agents); }
	}
	flush();
	return [...groups, ...quiet];
}

/** One weighted FIFO budget shared by commands, function suites and nested worker pools. */
function commandScheduler(limit, signal) {
	const queue = [];
	let active = 0;
	const drain = () => {
		if (signal?.aborted) { for (const entry of queue.splice(0)) entry.reject(new Error("Gate cancelled")); return; }
		while (queue.length && active + queue[0].permits <= limit) {
			const { run, permits, accept, reject } = queue.shift();
			active += permits;
			const finish = (settle, value) => { active -= permits; drain(); settle(value); };
			Promise.resolve().then(() => { if (signal?.aborted) throw new Error("Gate cancelled"); return run(); }).then(value => finish(accept, value), error => finish(reject, error));
		}
	};
	signal?.addEventListener("abort", drain, {once: true});
	return (run, permits = 1) => new Promise((accept, reject) => {
		if (!Number.isSafeInteger(permits) || permits < 1 || permits > limit) { reject(new Error(`Command permits must be an integer from 1 to ${limit}`)); return; }
		queue.push({ run, permits, accept, reject }); drain();
	});
}

/** Preserve undeclared function dependencies conservatively; declared artifacts can finish early. */
function dependencies(groups) {
	const result = new Map(), known = new Set(groups.flat().map(step => step.id));
	if (known.size !== groups.flat().length) throw new Error("Full steps need unique ids");
	const previous = [];
	for (const group of groups) {
		for (const step of group) {
			const after = step.after ?? (step.fn ? previous.filter(prior => !freshAgent(prior)).map(prior => prior.id) : []);
			if (!Array.isArray(after) || after.some(id => !known.has(id) || id === step.id)) throw new Error(`Invalid dependencies for ${step.id}`);
			result.set(step.id, after);
			previous.push(step);
		}
	}
	// Reject cycles and dependencies across a future exclusive turn before any check starts.
	const done = new Set();
	for (const group of groups) {
		const waiting = [...group];
		while (waiting.length) {
			const index = waiting.findIndex(step => result.get(step.id).every(id => done.has(id)));
			if (index < 0) throw new Error(`Cyclic or unavailable full-step dependency: ${waiting.map(step => step.id).join(", ")}`);
			done.add(waiting.splice(index, 1)[0].id);
		}
	}
	return result;
}

/**
 * Run every full step; serialize own Cargo writers while independent work shares the budget. With
 * `weights` (recent run times), the steps of a group start longest first, so the longest chains do not
 * start last and leave the budget idle at the end. Dependencies come from the registry order, unchanged.
 */
export async function runFullSteps(steps, run, parallel = 1, weights = NO_WEIGHTS, {signal} = {}) {
	const groups = fullStepGroups(steps, parallel);
	const after = parallel > 1 ? dependencies(groups) : new Map(steps.map(step => [step.id, []]));
	const order = parallel === 1 ? steps : [...steps.filter(step => !step.alone), ...steps.filter(step => step.alone)];
	const results = new Map(), scheduleCommand = commandScheduler(parallel, signal);
	const cancelled = () => ({status: "unrun", reason: "Gate cancelled before completion."});
	const execute = async step => {
		const started = Date.now();
		let outcome;
		try {
			// Command suites acquire individual permits in runCommands. Built-ins reserve one
			// here for their complete process lifecycle, since they have no such callback.
			outcome = step.fn || !step.run?.length ? await scheduleCommand(() => run(step, scheduleCommand)) : await run(step, scheduleCommand);
		} catch (error) { outcome = signal?.aborted ? cancelled() : { status: "fail", reason: String(error?.message ?? error), durationMs: Date.now() - started }; }
		if (signal?.aborted) {
			const completedFailure = outcome.attempts?.some(attempt => attempt.status === "fail" && !attempt.interrupted)
				|| outcome.units?.some(unit => unit.status === "fail")
				|| outcome.details?.commands?.some(command => command.status === "fail");
			outcome = completedFailure ? {...outcome, status: "fail", reason: outcome.reason ?? "A completed failure remains after cancellation."} : {...outcome, ...cancelled()};
		}
		results.set(step.id, { id: step.id, title: step.title, cards: step.cards, ...outcome });
	};
	for (const group of groups) {
		const pending = parallel > 1 ? longestFirst(group, step => weights.steps.get(step.id)) : [...group], running = new Map();
		let cargoActive = false;
		while (pending.length || running.size) {
			if (signal?.aborted) for (const step of pending.splice(0)) results.set(step.id, {id: step.id, title: step.title, cards: step.cards, ...cancelled()});
			for (let index = 0; index < pending.length;) {
				const step = pending[index], cargo = usesCargo(step);
				if (!after.get(step.id).every(id => results.has(id)) || cargo && cargoActive) { index++; continue; }
				pending.splice(index, 1);
				if (cargo) cargoActive = true;
				const promise = execute(step).finally(() => { running.delete(step.id); if (cargo) cargoActive = false; });
				running.set(step.id, promise);
			}
			if (running.size) await Promise.race(running.values());
		}
	}
	return order.map(step => results.get(step.id));
}
