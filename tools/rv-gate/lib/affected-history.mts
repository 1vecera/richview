// Host reason (ARC-03): read historical full verdicts without loading the evidence store at once.
import {existsSync, readdirSync, readFileSync} from "node:fs";
import {join} from "node:path";
import {evidenceDir} from "./store.mts";
import {commandId, failureCounts} from "./selection.mts";
import type {GateStep} from "./steps.mts";
import type {StoredReceipt} from "./store.mts";
import type {AffectedReceipt} from "./affected-coverage.mts";

/** Iterate immutable full receipts in filename order, excluding their separate notes. */
export function* fullReceipts(dir = join(evidenceDir(), "receipts")) {
	if (!existsSync(dir)) return;
	for (const name of readdirSync(dir).filter(value => /^full-\d{8}T\d{6}Z-[0-9a-f]+\.json$/.test(value)).sort()) {
		const receipt: StoredReceipt = JSON.parse(readFileSync(join(dir, name), "utf8"));
		if (receipt.kind !== "full" || `${receipt.id}.json` !== name) throw new Error(`Invalid full receipt identity: ${name}`);
		yield receipt;
	}
}

/** Summarize full receipts in one pass; unavailable command timings stay unknown. */
export function summarizeHistory(receipts: Iterable<Pick<StoredReceipt, "kind" | "steps">>, steps: readonly GateStep[]) {
	const byId = new Map(steps.map(step => [step.id, step])), samples = new Map<string, number[]>();
	const add = (key: string, ms: number | undefined) => { if (ms !== undefined && Number.isFinite(ms) && ms >= 0) samples.set(key, [...(samples.get(key) ?? []), ms]); };
	function* observed() {
		for (const receipt of receipts) {
			if (receipt.kind === "full") for (const result of receipt.steps ?? []) {
				const step = byId.get(result.id);
				if (!step || result.status !== "pass" || result.reused) continue;
				add(step.id, result.runMs ?? result.durationMs);
				const commands = result.details?.commands ?? [], labels = (step.run ?? []).map(command => [command.cmd, ...(command.args ?? [])].join(" ").split("{run}")[0]);
				for (let index = 0; index < labels.length; index++) {
					const result = commands[index], label = labels[index];
					if (labels.filter(other => other === label).length === 1 && result?.status === "pass" && typeof result.command === "string" && result.command.startsWith(label)) add(commandId(step, step.run![index]), result.durationMs);
				}
			}
			yield receipt;
		}
	}
	const counts = failureCounts(observed(), steps), durations = new Map<string, number>();
	for (const [key, values] of samples) {
		values.sort((a: number,b: number) => a-b); const middle = Math.floor(values.length / 2);
		durations.set(key, values.length % 2 ? values[middle] : (values[middle-1] + values[middle]) / 2);
	}
	return {counts, durations};
}

/** Iterate stored affected receipts without loading the full evidence store. */
export function* affectedReceipts(dir = join(evidenceDir(), "receipts")) {
	if (!existsSync(dir)) return;
	for (const name of readdirSync(dir).filter(value => /^affected-\d{8}T\d{6}Z-[0-9a-f]+\.json$/.test(value)).sort()) {
		const receipt: AffectedReceipt = JSON.parse(readFileSync(join(dir, name), "utf8"));
		if (receipt.kind !== "affected" || `${receipt.id}.json` !== name) throw new Error(`Invalid affected receipt identity: ${name}`);
		yield receipt;
	}
}
