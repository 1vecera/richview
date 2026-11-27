// Host reason (ARC-03): prove incremental gate coverage before a prelaunch promotion.
import {createHash} from "node:crypto";
import {readFileSync, readdirSync} from "node:fs";
import {caseUnitId} from "./case-selection.mts";
import type { SelectionItem, AreaMap, FailureCounts } from "./selection.mts";
import type { GateStatus, StepOutcome } from "./results.mts";
import type { StoredReceipt, ReceiptNote } from "./store.mts";
import type { PhasePolicy } from "./phase.mts";
import type { GateStep } from "./steps.mts";

type Contains = (later: string, earlier: string) => boolean;
type TreeOf = (commit: string) => string;
type Notes = (id: string) => ReceiptNote[];
type Select = (paths: string[], options?: { retry?: string[] }) => SelectionItem[];
type ChangedCommit = { commit: string; paths: string[] };
export interface CoverageObligation { commit: string; units: string[] }
export interface CoverageReceipt { head: string; outcomes: Record<string, GateStatus> }
export interface AffectedReceipt extends StoredReceipt {
	kind: "affected";
	base: string;
	policyHash: string;
	selected: string[];
	outcomes: Record<string, GateStatus>;
	steps: NonNullable<StoredReceipt["steps"]>;
	selection?: SelectionItem[];
	greenReceipt?: string;
	result: GateStatus;
	browserRelaunchTelemetry?: { status: GateStatus };
}
interface CoverageRequest {
	obligations: CoverageObligation[];
	receipts: CoverageReceipt[];
	contains: Contains;
	head?: string;
}

/**
 * Return units lacking a pass after their latest failure at a head containing each obligation.
 * The caller verifies receipt identities, source trees and selection provenance. Receipts must be
 * chronological and, when head is supplied, follow one ancestry ending at that promoted head.
 */
export function outstandingCoverage({obligations, receipts, contains, head}: CoverageRequest) {
	if (!Array.isArray(obligations) || !Array.isArray(receipts) || typeof contains !== "function") throw new Error("Coverage needs obligations, receipts and ancestry");
	const pending = new Map<string, string[]>(), events = new Map<string, {head: string; status: GateStatus}[]>();
	const add = (unit: string, reason: string) => {
		if (!pending.has(unit)) pending.set(unit, []);
		pending.get(unit)!.push(reason);
	};
	let previous;
	for (const receipt of receipts) {
		if (typeof receipt.head !== "string" || !receipt.outcomes || typeof receipt.outcomes !== "object" || Array.isArray(receipt.outcomes)) throw new Error("Invalid coverage receipt");
		if (head && (!contains(head, receipt.head) || previous && !contains(receipt.head, previous))) throw new Error("Coverage receipts do not follow the promoted ancestry");
		previous = receipt.head;
		for (const [unit, status] of Object.entries(receipt.outcomes)) {
			if (!unit || !["pass", "fail", "unrun"].includes(status)) throw new Error("Invalid coverage outcome");
			if (!events.has(unit)) events.set(unit, []);
			events.get(unit)!.push({head: receipt.head, status});
		}
	}
	for (const {commit, units} of obligations) {
		if (typeof commit !== "string" || !Array.isArray(units) || new Set(units).size !== units.length || units.some(unit => typeof unit !== "string" || !unit)) throw new Error("Invalid coverage obligation");
		if (head && !contains(head, commit)) throw new Error("Coverage obligation is outside the promoted ancestry");
		for (const unit of units) {
			const relevant = events.get(unit) ?? [];
			const lastFailure = relevant.findLastIndex((event: { status: string; }) => event.status === "fail");
			if (!relevant.some((event, index: number) => index > lastFailure && event.status === "pass" && contains(event.head, commit))) add(unit, `no pass covering ${commit} after its last failure`);
		}
	}
	for (const [unit, relevant] of events) {
		const lastFailure = relevant.findLastIndex((event: { status: string; }) => event.status === "fail");
		if (lastFailure >= 0 && !relevant.some((event, index: number) => index > lastFailure && event.status === "pass" && contains(event.head, relevant[lastFailure].head))) add(unit, `failure at ${relevant[lastFailure].head} has no later pass`);
	}
	return [...pending].map(([unit, reasons]) => ({unit, reasons: [...new Set(reasons)]}));
}

/** Promotion is allowed only if every obligation and every earlier failure has been re-passed. */
export function coverageProblems(args: CoverageRequest) {
	return outstandingCoverage(args).flatMap(({unit, reasons}) => reasons.map(reason => `${unit}: ${reason}`));
}

/** Recompute what each commit affects; a reverted path still creates its own obligation. */
export function buildObligations(commits: ChangedCommit[], select: Select) {
	if (!Array.isArray(commits) || typeof select !== "function") throw new Error("Coverage needs commits and a selector");
	return commits.map(({commit, paths}) => {
		if (typeof commit !== "string" || !commit || !Array.isArray(paths) || paths.some(path => typeof path !== "string" || !path)) throw new Error("Invalid changed commit");
		const plan = select(paths);
		if (!Array.isArray(plan) || plan.some(item => typeof item?.id !== "string" || !["run", "paused", "skipped"].includes(item.status))) throw new Error("Invalid selection plan");
		return {commit, units: [...new Set(plan.filter(item => item.status === "run").map(item => item.id))],
			paused: plan.filter(item => item.status === "paused").map(item => ({id: item.id, reason: item.reason}))};
	});
}

/** Reject receipts that cannot prove which exact units ran on the promoted ancestry. */
export function validateAffectedHistory({receipts, base, head, policyHash, contains, treeOf, allowPolicyChange = false}: {
	receipts: AffectedReceipt[]; base: string; head: string; policyHash: string; contains: Contains; treeOf: TreeOf; allowPolicyChange?: boolean;
}) {
	if (!Array.isArray(receipts) || typeof base !== "string" || typeof head !== "string" || typeof policyHash !== "string" || !policyHash || typeof contains !== "function" || typeof treeOf !== "function") throw new Error("Invalid affected history request");
	const ids = new Set<string>(), verified: CoverageReceipt[] = [];
	let previous = base;
	for (const receipt of receipts) {
		if (!receipt || receipt.kind !== "affected" || typeof receipt.id !== "string" || !receipt.id || ids.has(receipt.id) || receipt.base !== base || typeof receipt.policyHash !== "string" || !receipt.policyHash || !allowPolicyChange && receipt.policyHash !== policyHash) throw new Error("Affected receipt identity, base or policy differs");
		ids.add(receipt.id);
		if (typeof receipt.commit !== "string" || !contains(receipt.commit, previous) || !contains(head, receipt.commit) || receipt.tree !== treeOf(receipt.commit)) throw new Error("Affected receipt tree or ancestry differs");
		previous = receipt.commit;
		if (!Array.isArray(receipt.selected) || receipt.selected.some(unit => typeof unit !== "string" || !unit) || new Set(receipt.selected).size !== receipt.selected.length || !receipt.outcomes || typeof receipt.outcomes !== "object" || Array.isArray(receipt.outcomes)) throw new Error("Invalid affected receipt selection");
		const units = Object.keys(receipt.outcomes);
		if (units.length !== receipt.selected.length || units.some(unit => !receipt.selected.includes(unit) || !["pass", "fail", "unrun"].includes(receipt.outcomes[unit]))) throw new Error("Affected receipt outcomes differ from selected units");
		if (!Array.isArray(receipt.steps)) throw new Error("Affected receipt has no actual step results");
		const actual = new Map<string, GateStatus>();
		const add = (id: string, status: GateStatus | undefined) => {
			if (!receipt.selected.includes(id)) return;
			if (actual.has(id) || !["pass", "fail", "unrun"].includes(status!)) throw new Error(`Invalid actual outcome for ${id}`);
			actual.set(id, status!);
		};
		for (const step of receipt.steps) {
			if (!step || typeof step.id !== "string") throw new Error("Invalid affected step result");
			add(step.id, step.status);
			if (step.units !== undefined && !Array.isArray(step.units)) throw new Error("Invalid affected command results");
			for (const unit of step.units ?? []) add(unit.id, unit.status);
		}
		const caseIds = receipt.selected.filter((id: string|string[]) => id.includes(":case:"));
		if (caseIds.length) {
			if (!Array.isArray(receipt.selection)) throw new Error("Affected receipt lacks its case selection");
			for (const id of caseIds) {
				const matches = receipt.selection.filter(item => item.id === id && item.status === "run");
				const item = matches[0];
				if (matches.length !== 1 || typeof item.commandId !== "string" || typeof item.caseKey !== "string" || caseUnitId(item.commandId, item.caseKey) !== id) throw new Error(`Invalid affected case identity: ${id}`);
				if (receipt.outcomes[id] !== "pass") continue;
				const step = receipt.steps.find(entry => entry.id === item.step);
				if (item.caseKind === "rust-cohort") {
					const commands = (step?.details?.commands ?? []).filter(command => command.id === item.caseKey);
					if (step?.details?.preparation?.status !== "pass" || commands.length !== 1 || commands[0].status !== "pass") throw new Error(`Missing fresh cohort preparation or passing shard evidence for ${id}`);
					continue;
				}
				const commands = (step?.details?.commands ?? []).filter(command => command.caseUnits?.some(unit => unit.id === id));
				if (commands.length !== 1 || commands[0].status !== "pass" || commands[0].caseUnits!.find(unit => unit.id === id)?.status !== "pass") throw new Error(`Missing passing command evidence for ${id}`);
				const evidence = commands[0].cases;
				const group = receipt.selection.filter(entry => entry.status === "run" && entry.commandId === item.commandId).map(entry => entry.caseKey);
				if (!Array.isArray(evidence?.cohort) || new Set(evidence.cohort).size !== evidence.cohort.length || !Array.isArray(evidence.selected) || !Array.isArray(evidence.completed)
					|| JSON.stringify(evidence.selected) !== JSON.stringify(group) || JSON.stringify(evidence.completed) !== JSON.stringify(group) || group.some(key => !evidence.cohort.includes(key!))) throw new Error(`Incomplete affected case evidence for ${id}`);
			}
		}
		if (receipt.selected.includes("browser-telemetry")) add("browser-telemetry", receipt.browserRelaunchTelemetry?.status);
		if (units.some(unit => (actual.get(unit) ?? "unrun") !== receipt.outcomes[unit])) throw new Error("Affected outcomes differ from actual step results");
		if (!["pass", "fail"].includes(receipt.result) || receipt.result === "pass" && units.some(unit => receipt.outcomes[unit] !== "pass")) throw new Error("Affected receipt verdict differs from its outcomes");
		// A policy change invalidates old passes, but every old failure still has to pass again.
		const outcomes = receipt.policyHash === policyHash ? receipt.outcomes : Object.fromEntries(Object.entries(receipt.outcomes).map(([id, status]) => [id, status === "pass" ? "unrun" : status]));
		verified.push({head: receipt.commit, outcomes});
	}
	return verified;
}

/** Plan only unresolved units, then include prerequisites selected by the current registry. */
export function pendingPlan({commits, receipts, select, contains, head, always = []}: {
	commits: ChangedCommit[]; receipts: CoverageReceipt[]; select: Select; contains: Contains; head: string; always?: string[];
}) {
	const perCommit: SelectionItem[][] = [];
	const obligations = buildObligations(commits, paths => {
		const plan = select(paths);
		perCommit.push(plan);
		return plan;
	});
	const pending = outstandingCoverage({obligations, receipts, contains, head});
	const telemetry = pending.some(item => item.unit === "browser-telemetry");
	const selection = select([], {retry: [...new Set([...pending.filter(item => item.unit !== "browser-telemetry").map(item => item.unit), ...always])]});
	for (const item of selection) {
		const decisions = perCommit.flatMap((plan, index) => plan.filter(entry => entry.id === item.id).map(entry => ({commit: commits[index].commit, status: entry.status, reason: entry.reason})));
		if (decisions.length) item.decisions = decisions;
		if (item.status === "run") {
			const unresolved = pending.find(entry => entry.unit === item.id);
			if (unresolved) item.reason = unresolved.reasons.join("; ");
			continue;
		}
		const covered = decisions.some(decision => decision.status === "run");
		const lastPass = receipts.findLast((receipt: { outcomes: { [x: string]: string; }; }) => receipt.outcomes[item.id] === "pass");
		const paused = decisions.find(decision => decision.status === "paused" && (!lastPass || !contains(lastPass.head, decision.commit)));
		if (paused) { item.status = "paused"; item.reason = `Paused for ${paused.commit.slice(0,10)}: ${paused.reason}`; }
		else if (covered) { item.status = "skipped"; item.reason = "Affected obligations already passed in earlier runs."; }
		else if (decisions.length && decisions[0].reason) item.reason = decisions[0].reason;
	}
	if (telemetry) selection.push({id: "browser-telemetry", step: "browser-telemetry", status: "run", reason: "Earlier browser telemetry failed."});
	const running = new Set(selection.filter((item: { status: string; }) => item.status === "run").map(item => item.id));
	for (const unit of [...pending.map(item => item.unit), ...always]) if (!running.has(unit)) throw new Error(`Outstanding unit is unavailable under the current registry: ${unit}`);
	return {obligations, pending, selection};
}


/** Bind reuse to the actual selector, registry, policy, map and frozen failure counts. */
export function policyFingerprint({policy, steps, areas, counts}: { policy: PhasePolicy; steps: readonly GateStep[]; areas: AreaMap; counts: FailureCounts }) {
	if (!(counts instanceof Map)) throw new Error("Policy fingerprint needs failure counts");
	const hash = createHash("sha256").update(JSON.stringify({policy, steps, areas, counts:[...counts].sort(([a],[b])=>a.localeCompare(b))}));
	// Bind every gate runtime module, including new helpers, not only the imports known today.
	const modules = (dir: URL, prefix: string): string[] => readdirSync(dir, {withFileTypes: true}).flatMap(entry => {
		const path = `${prefix}${entry.name}`;
		if (entry.isDirectory()) return modules(new URL(`${entry.name}/`, dir), `${path}/`);
		return entry.isFile() && /\.[cm]?[jt]s$/.test(entry.name) ? [path] : [];
	});
	const runtime = modules(new URL("./", import.meta.url), "./");
	const root = readdirSync(new URL("../", import.meta.url), {withFileTypes: true})
		.filter(entry => entry.isFile() && /\.(?:[cm]?[jt]s|json)$/.test(entry.name)).map(entry => `../${entry.name}`);
	for (const path of [...runtime, ...root,
		"../../../scripts/report-runtime.ts", "../../../scripts/browser-harness.ts", "../../../scripts/verify-report-runtime.ts", "../../../scripts/verify-browser.ts", "../../../scripts/verify-figure-expand.ts", "../../../scripts/verify-docs.ts",
		"../../../scripts/verify-chart-interaction.ts", "../../../scripts/chart-interaction-checks.ts", "../../../scripts/docs-chart-checks.ts", "../../../scripts/verify-no-js.ts", "../../../scripts/preview-recovery.ts", "../../../scripts/runtime-access.ts",
	].sort())
		hash.update("\0").update(path).update("\0").update(readFileSync(new URL(path, import.meta.url)));
	return hash.digest("hex");
}

/** Build the receipt's selected outcomes from the runner's actual step and command results. */
export function outcomesForSelection(selection: SelectionItem[], steps: (StepOutcome & { id: string })[], telemetry: GateStatus) {
	const selected = selection.filter((item: { status: string; }) => item.status === "run").map(item => item.id);
	if (new Set(selected).size !== selected.length || !["pass", "fail", "unrun"].includes(telemetry)) throw new Error("Invalid affected execution selection");
	const actual = new Map<string, GateStatus>();
	const add = (id: string, status: GateStatus) => {
		if (actual.has(id)) throw new Error(`Duplicate actual unit ${id}`);
		if (!["pass", "fail", "unrun"].includes(status)) throw new Error(`Invalid actual unit ${id}`);
		actual.set(id, status);
	};
	for (const step of steps) {
		add(step.id, step.status);
		for (const unit of step.units ?? []) add(unit.id, unit.status);
	}
	const outcomes = Object.fromEntries(selected.filter((id: string) => id !== "browser-telemetry").map(id => [id, actual.get(id) ?? "unrun"]));
	outcomes["browser-telemetry"] = telemetry;
	return outcomes;
}

/** The pre-prod anchor must have an exact, previously applied passing promotion. */
export function requireGreenBase({base, tree, receipts, notes}: { base: string; tree: string; receipts: Iterable<StoredReceipt>; notes: Notes }) {
	if (typeof base !== "string" || typeof tree !== "string" || typeof notes !== "function") throw new Error("Invalid green base request");
	for (const receipt of receipts) {
		if (!["full", "affected"].includes(receipt.kind) || receipt.commit !== base || receipt.tree !== tree || receipt.result !== "pass") continue;
		if (notes(receipt.id).some(note => note.kind === "promotion" && note.commit === base && note.tree === tree)) return receipt.id;
	}
	throw new Error("pre-prod has no exact promoted green receipt");
}

/** Keep incremental coverage on the latest proven ancestor even when pre-prod advances on judgment. */
export function lastGreenBase({head, receipts, notes, contains, treeOf}: { head: string; receipts: Iterable<StoredReceipt>; notes: Notes; contains: Contains; treeOf: TreeOf }) {
	if (typeof head !== "string" || typeof receipts?.[Symbol.iterator] !== "function" || typeof notes !== "function" || typeof contains !== "function" || typeof treeOf !== "function") throw new Error("Invalid green ancestor request");
	let best;
	for (const receipt of receipts) {
		if (!["full", "affected"].includes(receipt.kind) || receipt.result !== "pass") continue;
		if (!notes(receipt.id).some(note => note.kind === "promotion" && note.commit === receipt.commit && note.tree === receipt.tree)) continue;
		if (!contains(head, receipt.commit)) continue;
		const tree = treeOf(receipt.commit);
		if (receipt.tree !== tree) continue;
		const green = requireGreenBase({base: receipt.commit, tree, receipts: [receipt], notes});
		if (best && !contains(receipt.commit, best.base)) {
			if (!contains(best.base, receipt.commit)) throw new Error("Promoted green ancestors do not form one ancestry chain");
			continue;
		}
		best = {base: receipt.commit, tree, receipt: green};
	}
	if (!best) throw new Error("Head has no exact promoted green ancestor");
	return best;
}


/** Browser-journal failures invalidate the executed observations from that receipt. */
export function invalidateTelemetryFailures(raw: readonly AffectedReceipt[], verified: readonly CoverageReceipt[]) {
	return verified.map((row, index) => {
		if (raw[index].outcomes["browser-telemetry"] !== "fail") return row;
		const outcomes = {...row.outcomes};
		for (const unit of raw[index].selected) if (unit !== "browser-telemetry" && raw[index].outcomes[unit] === "pass") outcomes[unit] = "fail";
		return {...row, outcomes};
	});
}
