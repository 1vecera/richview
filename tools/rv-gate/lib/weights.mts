// Host reason (ARC-03): order full-run work longest first, from the run times that recent receipts recorded.
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

const FULL_RECEIPT = /^full-\d{8}T\d{6}Z-[0-9a-f]+\.json$/;

interface Weights {
	steps: Map<string, number | undefined>;
	commands: Map<string, (number | undefined)[]>;
}
interface Timing { status: string; durationMs?: number }
interface TimingReceipt { steps?: (Timing & { id: string; runMs?: number; details?: { commands?: Timing[] } })[] }
const measured = (value: number | undefined): value is number => Number.isFinite(value);

const median = (values: readonly number[]) => {
	const sorted = [...values].sort((a, b) => a - b);
	return sorted.length ? (sorted.length % 2 ? sorted[(sorted.length - 1) / 2] : (sorted[sorted.length / 2 - 1] + sorted[sorted.length / 2]) / 2) : undefined;
};

/** No history: every weight is unknown, so nothing is reordered. */
export const NO_WEIGHTS: Weights = Object.freeze({ steps: new Map(), commands: new Map() });

/**
 * Median run time of each passing step and of each of its commands, by position, over the given
 * full receipts. A step or command a receipt did not run, or did not pass, adds no observation.
 */
export function weightsFrom(receipts: Iterable<TimingReceipt>): Weights {
	const stepTimes = new Map<string, number[]>(), commandTimes = new Map<string, number[][]>();
	for (const receipt of receipts) {
		for (const step of Array.isArray(receipt?.steps) ? receipt.steps : []) {
			if (step?.status !== "pass" || typeof step.id !== "string") continue;
			const run = step.runMs ?? step.durationMs;
			if (measured(run) && run >= 0) stepTimes.set(step.id, [...(stepTimes.get(step.id) ?? []), run]);
			const commands = step.details?.commands;
			if (!Array.isArray(commands)) continue;
			const columns = commandTimes.get(step.id) ?? [];
			commands.forEach((command: Timing, index) => {
				if (command?.status === "pass" && measured(command.durationMs)) (columns[index] ??= []).push(command.durationMs);
			});
			commandTimes.set(step.id, columns);
		}
	}
	return {
		steps: new Map([...stepTimes].map(([id, times]) => [id, median(times)])),
		commands: new Map([...commandTimes].map(([id, columns]) => [id, Array.from(columns, (times) => (times ? median(times) : undefined))])),
	};
}

/**
 * The weights of the newest `count` full receipts in a receipts folder. A receipt that cannot be read
 * or parsed is skipped, so a half-written or malformed one never stops a run; with none, nothing is reordered.
 */
export function readRecentWeights(dir: string, count = 3) {
	if (!existsSync(dir)) return NO_WEIGHTS;
	const receipts: TimingReceipt[] = [];
	for (const name of readdirSync(dir).filter((file) => FULL_RECEIPT.test(file)).sort().reverse()) {
		if (receipts.length >= count) break;
		try { receipts.push(JSON.parse(readFileSync(join(dir, name), "utf8"))); } catch { /* skipped on purpose */ }
	}
	return receipts.length ? weightsFrom(receipts) : NO_WEIGHTS;
}

/**
 * A step's commands in start order. Longest first only when the step runs several commands at once
 * (`step.parallel` above 1), where they are independent by construction; a sequential step keeps its
 * registry order, because its commands may depend on each other (publish-tests builds into a folder an
 * earlier command creates).
 */
export function commandOrder<T extends { index: number }>(step: { id: string; parallel?: number }, commands: T[], weights?: Weights) {
	if (!((step.parallel ?? 0) > 1) || !weights) return commands;
	return longestFirst(commands, ({ index }) => weights.commands.get(step.id)?.[index]);
}

/**
 * The items sorted by descending weight, ties and unknown weights in their given order. An item with no
 * weight counts as the median of the known ones, so a new step lands mid-queue rather than first or last.
 */
export function longestFirst<T>(items: readonly T[], weightOf: (item: T) => number | undefined) {
	const known = items.map(weightOf).filter(measured);
	if (!known.length) return [...items];
	const fallback = median(known)!;
	return items
		.map((item, index) => ({ item, index, weight: measured(weightOf(item)) ? weightOf(item)! : fallback }))
		.sort((a, b) => b.weight - a.weight || a.index - b.index)
		.map(({ item }) => item);
}
