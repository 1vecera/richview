// Host reason (ARC-03): schedule full-run host suites behind build and quiet-measurement barriers.
import { NO_WEIGHTS, longestFirst } from "./weights.mts";
import type { ScheduleCommand, StepOutcome } from "./results.mts";
import type { GateStep } from "./steps.mts";
const usesCargo = (step: GateStep) => step.run?.some(command => command.cmd === "cargo");
const freshAgent = (step: GateStep) => step.id === "fresh-agent" && step.needs === "build" && !step.fn && !step.alone && step.parallel !== 1 && !step.after?.length && !usesCargo(step);

/** Group post-build work behind explicit exclusive turns; perf drains the complete run. */
export function fullStepGroups(steps: readonly GateStep[], parallel = 1): GateStep[][] {
	if (!Number.isSafeInteger(parallel) || parallel < 1) throw new Error("--parallel must be a positive integer");
	if (parallel === 1) return steps.map(step => [step]);
	const groups: GateStep[][] = [], quiet: GateStep[][] = [], pending: GateStep[] = [];
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
function commandScheduler(limit: number, signal?: AbortSignal): ScheduleCommand {
	const queue: { permits: number; start: () => void; reject: (reason: Error) => void }[] = [];
	let active = 0;
	const drain = () => {
		if (signal?.aborted) { for (const entry of queue.splice(0)) entry.reject(new Error("Gate cancelled")); return; }
		while (queue.length && active + queue[0].permits <= limit) {
			const { start, permits } = queue.shift()!;
			active += permits;
			start();
		}
	};
	signal?.addEventListener("abort", drain, {once: true});
	return <T,>(run: () => T, permits = 1) => new Promise<Awaited<T>>((accept, reject) => {
		if (!Number.isSafeInteger(permits) || permits < 1 || permits > limit) { reject(new Error(`Command permits must be an integer from 1 to ${limit}`)); return; }
		const finish = (settle: () => void) => { active -= permits; drain(); settle(); };
		const start = () => {
			Promise.resolve().then<Awaited<T> | T>(() => { if (signal?.aborted) throw new Error("Gate cancelled"); return run(); })
				// This fulfillment handler receives the value after promise resolution has unwrapped it.
				.then(value => finish(() => accept(value as Awaited<T>)), error => finish(() => reject(error)));
		};
		queue.push({ permits, start, reject }); drain();
	});
}

/** Preserve undeclared function dependencies conservatively; declared artifacts can finish early. */
function dependencies(groups: GateStep[][]) {
	const result = new Map<string, string[]>(), known = new Set(groups.flat().map(step => step.id));
	if (known.size !== groups.flat().length) throw new Error("Full steps need unique ids");
	const previous: GateStep[] = [];
	for (const group of groups) {
		for (const step of group) {
			const after = step.after ?? (step.fn ? previous.filter(prior => !freshAgent(prior)).map(prior => prior.id) : []);
			if (!Array.isArray(after) || after.some(id => !known.has(id) || id === step.id)) throw new Error(`Invalid dependencies for ${step.id}`);
			result.set(step.id, after);
			previous.push(step);
		}
	}
	// Reject cycles and dependencies across a future exclusive turn before any check starts.
	const done = new Set<string>();
	for (const group of groups) {
		const waiting = [...group];
		while (waiting.length) {
			const index = waiting.findIndex(step => result.get(step.id)!.every(id => done.has(id)));
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
export async function runFullSteps(steps: readonly GateStep[], run: (step: GateStep, schedule: ScheduleCommand) => StepOutcome | Promise<StepOutcome>,
	parallel = 1, weights = NO_WEIGHTS, {signal}: {signal?: AbortSignal} = {}) {
	const groups = fullStepGroups(steps, parallel);
	const after = parallel > 1 ? dependencies(groups) : new Map<string, string[]>(steps.map(step => [step.id, []]));
	const order = parallel === 1 ? steps : [...steps.filter(step => !step.alone), ...steps.filter(step => step.alone)];
	const results = new Map<string, Pick<GateStep, "id" | "title" | "cards"> & StepOutcome>(), scheduleCommand = commandScheduler(parallel, signal);
	const cancelled = () => ({status: "unrun" as const, reason: "Gate cancelled before completion."});
	const execute = async (step: GateStep) => {
		const started = Date.now();
		let outcome: StepOutcome;
		try {
			// Command suites acquire individual permits in runCommands. Built-ins reserve one
			// here for their complete process lifecycle, since they have no such callback.
			outcome = step.fn || !step.run?.length ? await scheduleCommand(() => run(step, scheduleCommand)) : await run(step, scheduleCommand);
		} catch (error) { outcome = signal?.aborted ? cancelled() : { status: "fail", reason: String((error as Error | null)?.message ?? error), durationMs: Date.now() - started }; }
		if (signal?.aborted) {
			const completedFailure = outcome.attempts?.some(attempt => attempt.status === "fail" && !attempt.interrupted)
				|| outcome.units?.some(unit => unit.status === "fail")
				|| outcome.details?.commands?.some(command => command.status === "fail");
			outcome = completedFailure ? {...outcome, status: "fail", reason: outcome.reason ?? "A completed failure remains after cancellation."} : {...outcome, ...cancelled()};
		}
		results.set(step.id, { id: step.id, title: step.title, cards: step.cards, ...outcome });
	};
	for (const group of groups) {
		const pending = parallel > 1 ? longestFirst(group, step => weights.steps.get(step.id)) : [...group], running = new Map<string, Promise<void>>();
		let cargoActive = false;
		while (pending.length || running.size) {
			if (signal?.aborted) for (const step of pending.splice(0)) results.set(step.id, {id: step.id, title: step.title, cards: step.cards, ...cancelled()});
			for (let index = 0; index < pending.length;) {
				const step = pending[index], cargo = usesCargo(step);
				if (!after.get(step.id)!.every(id => results.has(id)) || cargo && cargoActive) { index++; continue; }
				pending.splice(index, 1);
				if (cargo) cargoActive = true;
				const promise = execute(step).finally(() => { running.delete(step.id); if (cargo) cargoActive = false; });
				running.set(step.id, promise);
			}
			if (running.size) await Promise.race(running.values());
		}
	}
	return order.map(step => results.get(step.id)!);
}
