// Host reason (ARC-03): select prelaunch verification from changed paths and immutable receipts.
import { createHash } from "node:crypto";
import { stepSources } from "./release.mts";
import { affected } from "./impact.mts";
import {affectedCaseKeys} from "./case-selection.mts";
import { receiptSource } from "./source-paths.mts";
import type { GateCommand, GateStep } from "./steps.mts";
import type { StoredReceipt } from "./store.mts";
import type { Phase, PhasePolicy } from "./phase.mts";
import type { WorkspaceCrate } from "./impact.mts";
import type { CaseKind, CaseSpec } from "./case-selection.mts";

export type SelectionStatus = "run" | "paused" | "skipped";
export interface SelectionItem {
	id: string;
	step: string;
	index?: number | null;
	command?: string;
	engine?: string;
	status: SelectionStatus;
	reason?: string;
	failures?: number;
	estimated?: boolean;
	commandId?: string;
	caseKind?: CaseKind;
	caseKey?: string;
	decisions?: { commit: string; status: SelectionStatus; reason?: string }[];
}
export interface Area { name: string; paths: string[]; except?: string[]; steps: string[]; commands?: Record<string, string[]> }
export interface AreaMap { version: 1; areas: Area[] }
export type FailureCounts = ReadonlyMap<string, { step: number; commands: number[]; estimated?: boolean[] }>;
interface SelectionOptions {
	areas?: AreaMap;
	counts?: FailureCounts;
	threshold?: number;
	phase?: Phase;
	read?: (path: string) => string | null;
	retry?: readonly string[];
	graph?: readonly WorkspaceCrate[];
	inputs?: ReadonlyMap<string, ReadonlySet<string>>;
	catalog?: ReadonlyMap<string, CaseSpec>;
	fallback?: NonNullable<PhasePolicy["affected"]>["fallback"];
}
type SelectionUnit = { command: GateCommand; index: number } | { command?: undefined; index: null };

const NON_PRODUCT = [/^tools\//, /^\.github\//, /^\.claude\//, /^docs\/lockdown\//, /^[^/]+\.md$/, /^LICENSE/];
const INDEPENDENT = new Set(["host", "hostile", "label-truth", "deps"]);
const independent = (step: GateStep) => Boolean(step.parallel && step.parallel > 1 || INDEPENDENT.has(step.id));
const pathMatches = (path: string, prefix: string) => prefix === "*" || (prefix.endsWith("/") ? path.startsWith(prefix) : path === prefix);
/** Stable across reordering; cohorts may supply their own stable id. */
export function commandId(step: GateStep, command: GateCommand) {
	const script = command.cmd === "node" ? command.args?.[0] : undefined;
	const previous = script && receiptSource(script);
	const identity = previous && previous !== script
		? { ...command, args: command.args.with(0, previous), ...(command.entry === script && { entry: previous }) }
		: command;
	const key = command.id ?? createHash("sha256").update(JSON.stringify(identity)).digest("hex").slice(0, 16);
	if (typeof key !== "string" || !/^[a-zA-Z0-9_-]+$/.test(key)) throw new Error("Invalid command id");
	return `${step.id}:${key}`;
}
const commandLabel = (command: GateCommand) => [command.cmd, ...(command.args ?? [])].join(" ");

/** Count final full verdicts. Older command-less receipts conservatively count against every command. */
export function failureCounts(receipts: Iterable<Pick<StoredReceipt, "kind" | "steps">>, steps: readonly GateStep[]) {
	const counts = new Map(steps.map(step => [step.id, {step: 0, commands: (step.run ?? []).map(() => 0), estimated: (step.run ?? []).map(() => false)}]));
	const byId = new Map(steps.map(step => [step.id, step]));
	for (const receipt of receipts) {
		if (receipt.kind !== "full") continue;
		for (const result of receipt.steps ?? []) {
			const step = byId.get(result.id), count = counts.get(result.id);
			if (!step || !count || result.reused || result.status !== "fail") continue;
			if (result.status === "fail") count.step++;
			const labels = (step.run ?? []).map(commandLabel), commands = result.details?.commands;
			for (let index = 0; index < labels.length; index++) {
				const outcome = commands?.[index], label = labels[index];
				// Older receipts cannot distinguish identical commands with different engine
				// environments. Use the whole step's failure as an upper bound in that case.
				if (!outcome || typeof outcome.command !== "string" || !outcome.command.startsWith(label) || labels.filter(other => other === label).length > 1) {
					if (result.status === "fail") { count.commands[index]++; count.estimated[index] = true; }
				} else if (outcome.status === "fail") count.commands[index]++;
			}
		}
	}
	return counts;
}

/** Refuse an incomplete or mistyped checked-in map before it can authorize omissions. */
export function validateAreas(data: AreaMap | undefined, steps: readonly GateStep[]) {
	if (data?.version !== 1 || !Array.isArray(data.areas) || !data.areas.length || !Array.isArray(steps)) throw new Error("Invalid area map");
	const byStep = new Map<string, GateStep>(steps.map(step => [step.id, step]));
	const names = new Set();
	const validPath = (path: unknown) => typeof path === "string" && path.length > 0 && !path.startsWith("/") && !path.split("/").includes("..") && !path.includes("\\");
	for (const area of data.areas) {
		if (typeof area.name !== "string" || !area.name.trim() || names.has(area.name)) throw new Error("Area needs a unique name");
		names.add(area.name);
		if (!Array.isArray(area.paths) || !area.paths.length || area.paths.some(path => !validPath(path)) || !Array.isArray(area.except ?? []) || (area.except ?? []).some(path => !validPath(path))) throw new Error(`Invalid paths for ${area.name}`);
		if (!Array.isArray(area.steps) || !area.steps.length || new Set(area.steps).size !== area.steps.length || area.steps.some(id => !byStep.has(id))) throw new Error(`Invalid steps for ${area.name}`);
		for (const [id, commands] of Object.entries(area.commands ?? {})) {
			const step = byStep.get(id);
			if (!area.steps.includes(id) || !step || !independent(step) || !Array.isArray(commands) || !commands.length || new Set(commands).size !== commands.length
				|| commands.some(command => !(step.run ?? []).some(value => (value.entry ?? value.args?.[0]) === command))) throw new Error(`Invalid command mapping for ${area.name}: ${id}`);
		}
	}
	return data;
}

/** Select steps and independent commands, retaining explicit reasons for every omission. */
export function selectAffected(steps: readonly GateStep[], changed: readonly string[], { areas, counts = new Map(), threshold = 5, phase = "prelaunch", read, retry = [], graph = [], inputs = new Map(), catalog = new Map(), fallback = {mode: "failure-threshold", realDefectSteps: []} }: SelectionOptions = {}): SelectionItem[] {
	if (!Array.isArray(areas?.areas) || !Number.isSafeInteger(threshold) || threshold < 0 || !["failure-threshold", "real-defects"].includes(fallback?.mode) || !Array.isArray(fallback.realDefectSteps)) throw new Error("Invalid affected selection policy");
	const applies = (area: Area, path: string) => area.paths.some(prefix => pathMatches(path, prefix)) && !(area.except ?? []).some(prefix => pathMatches(path, prefix));
	const unknownFallback = fallback.realDefectSteps.filter(id => !steps.some(step => step.id === id));
	if (fallback.mode === "real-defects" && (unknownFallback.length || !fallback.realDefectSteps.length)) throw new Error(`Invalid real-defect steps: ${unknownFallback.join(", ")}`);
	const matches = areas.areas.filter(area => changed.some(path => applies(area, path)));
	const sources = new Map(steps.map(step => [step.id, stepSources(step, read)]));
	const reads = (source: ReturnType<typeof stepSources>, path: string) => source.files.includes(path) || source.folders.some(folder => path.startsWith(folder));
	const unknownProduct = changed.some((path: string) => !NON_PRODUCT.some(pattern => pattern.test(path)) && !areas.areas.some(area => area.paths.some((prefix: string) => prefix !== "*" && pathMatches(path, prefix)) && applies(area, path)));
	const impact = affected(changed, graph, inputs);
	const retrySet = new Set(retry);
	const result: SelectionItem[] = [];
	for (const step of steps) {
		const areaFor = (unit: SelectionUnit) => matches.filter(item => item.steps.includes(step.id) && (!unit.command || !item.commands?.[step.id] || item.commands[step.id].includes(unit.command.entry ?? unit.command.args?.[0]))).map(item => item.name);
		const source = sources.get(step.id)!;
		const traced = changed.some(path => reads(source, path));
		const fullRetry = retrySet.has(step.id);
		const count = counts.get(step.id) ?? {step: 0, commands: []};
		const units: SelectionUnit[] = independent(step) ? step.run!.map((command, index) => ({command, index})) : [{index: null}];
		for (const unit of units) {
			const id = unit.index === null ? step.id : commandId(step, unit.command);
			if (result.some(item => item.id === id)) throw new Error(`Duplicate selection id: ${id}`);
			const area = areaFor(unit);
			const commandSource = unit.command && stepSources({run: [unit.command]}, read);
			const commandTraced = commandSource && changed.some((path: string) => commandSource.files.includes(path) || commandSource.folders.some(folder => path.startsWith(folder)));
			const engine = unit.command?.env?.RICHVIEW_VERIFY_BROWSER;
			const failures = unit.index === null ? count.step : count.commands[unit.index] ?? 0;
			const estimated = unit.index !== null && Boolean(count.estimated?.[unit.index]);
			const observation = `${estimated ? "up to " : ""}${failures} recorded failures`;
			let status: SelectionStatus, reason: string | undefined;
			if (phase === "prelaunch" && (["firefox", "webkit"].includes(step.id) || ["firefox", "webkit"].includes(engine!))) {
				status = "skipped"; reason = "Prelaunch: Chrome only.";
			} else if (step.unrun || step.unrunIn?.full) {
				status = "skipped"; reason = step.unrun ?? step.unrunIn!.full;
			} else if (fullRetry || retrySet.has(id)) {
				status = "run"; reason = "Previous failure requires a pass.";
			} else if (impact.crates.size && ["fmt", "clippy", "clippy-wasm", "rust-tests", "rust-slow"].includes(step.id)) {
				status = "run"; reason = `Affected Rust crates: ${[...impact.crates].sort().join(", ")}.`;
			} else if (impact.suites.size && step.id === "tool-tests") {
				status = "run"; reason = `Affected tool suites: ${[...impact.suites].sort().join(", ")}.`;
			} else if (area.length || (unit.index === null ? traced : commandTraced)) {
				status = "run"; reason = area.length ? `Affected area: ${area.join(", ")}.` : "Changed script or fixture read by this command.";
			} else if (unknownProduct && (fallback.mode === "real-defects" ? fallback.realDefectSteps.includes(step.id) : failures >= threshold)) {
				status = "run"; reason = fallback.mode === "real-defects" ? `Unknown product path; ${step.id} caught a real defect.` : `Unknown product path; ${observation} ${estimated ? "could meet" : "meets"} threshold ${threshold}.`;
			} else if (unknownProduct) {
				status = "paused"; reason = fallback.mode === "real-defects" ? `Unknown product path; ${step.id} is not in the real-defect list (${observation}).` : `Unknown product path; ${observation} below threshold ${threshold}.`;
			} else {
				status = "skipped"; reason = "No affected area or traced input.";
			}
			const base = {id, step: step.id, index: unit.index, ...(unit.command && {command: commandLabel(unit.command), ...(engine && {engine})}), status, reason, failures, estimated};
			const spec = catalog.get(id);
			if (!spec) { result.push(base); continue; }
			const fullCommand = fullRetry || retrySet.has(id);
			const impacted = new Set(fullCommand ? spec.keys : affectedCaseKeys(spec.kind, spec.keys, changed));
			for (const [caseIndex, key] of spec.keys.entries()) {
				const caseId = spec.units[caseIndex], retried = retrySet.has(caseId);
				let caseStatus = status, caseReason = reason;
				if (retried) { caseStatus = "run"; caseReason = "Previous case failure requires a pass."; }
				else if (status === "run" && !impacted.has(key)) { caseStatus = "skipped"; caseReason = "This case is outside the changed source's mapped cases."; }
				result.push({...base, id: caseId, commandId: id, caseKind: spec.kind, caseKey: key, status: caseStatus, reason: caseReason});
			}
		}
	}
	// An affected check cannot run without the artifacts its registry declares. Include every
	// command of a prerequisite, and repeat until transitive prerequisites are present.
	const byStep = new Map(steps.map(step => [step.id, step]));
	let grew = true;
	while (grew) {
		grew = false;
		for (const step of steps) {
			if (!result.some(item => item.step === step.id && item.status === "run")) continue;
			for (const dependency of [...(step.needs ? [step.needs] : []), ...(step.after ?? [])]) {
				if (!byStep.has(dependency)) throw new Error(`Missing prerequisite ${dependency} for ${step.id}`);
				for (const item of result.filter(entry => entry.step === dependency)) {
					if (item.status === "run") continue;
					if (byStep.get(dependency)!.unrun) throw new Error(`Prerequisite ${dependency} is unavailable`);
					item.status = "run"; item.reason = `Prerequisite of ${step.id}.`; grew = true;
				}
			}
		}
	}
	return result;
}
