// Host reason (ARC-03): promote only an exact source tree with a complete passing full receipt.
import { execFileSync, spawnSync } from "node:child_process";
import type { ArtifactStep, PhaseRules } from "./phase.mts";
import type { AffectedReceipt } from "./affected-coverage.mts";
import type { GateCommand } from "./steps.mts";
import type { GateChange, StoredReceipt } from "./store.mts";
import { FRESH_AGENT_SCHEDULE, STEPS, stepsForTier } from "./steps.mts";
import {readPhase, validatePhase, requiredArtifacts, prelaunchException, browserGap, CHROMIUM_ONLY_REASON} from "./phase.mts";
import { parsePrBody } from "./pr.mts";
import {rotationFromHistory} from "./value.mts";
import {affectedPromotion} from "./affected-promotion.mts";
import { appendChange, listReceipts, readChanges, readReceipt, receiptNotes, writeBaseline, writeReceiptNote } from "./store.mts";

interface PromotionReceipt extends StoredReceipt {
	candidate?: string;
	prod?: string;
	rotation?: ReturnType<typeof rotationFromHistory>;
	captures?: Record<string, string>;
}
type RegistryStep = ArtifactStep & {run?: GateCommand[]};
interface PromotionOptions { policy?: PhaseRules; history?: StoredReceipt[]; now?: Date | string; stepDependencies?: RegistryStep[] }

const git = (cwd: string, ...args: string[]) => execFileSync("git", args, { cwd, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
const ancestor = (cwd: string, from: string, to: string) => {
	const result = spawnSync("git", ["merge-base", "--is-ancestor", from, to], { cwd, encoding: "utf8" });
	if (result.status === 0 || result.status === 1) return result.status === 0;
	throw new Error(result.stderr || "Cannot verify Git ancestry");
};

/** Read dev and pre-prod without changing either ref. */
export function readDev(cwd: string, { at = "origin/dev" } = {}) {
	git(cwd, "fetch", "-q", "origin", "dev", "pre-prod");
	const base = git(cwd, "rev-parse", "origin/pre-prod");
	const dev = git(cwd, "rev-parse", "origin/dev");
	const head = git(cwd, "rev-parse", at);
	if (!ancestor(cwd, head, dev)) throw new Error("The full source must be on origin/dev.");
	return { base, head, updated: false };
}

/** Read declarations from the actual source range, retaining richer records from earlier PR gates. */
export function sourceChanges(cwd: string, { base, head, receipt, at }: {base: string; head: string; receipt: string; at?: string}): GateChange[] {
	const recorded = new Map(readChanges().map(entry => [entry.commit, entry]));
	const records = git(cwd, "log", "--topo-order", "--reverse", "--format=%H%x00%B%x00", `${base}..${head}`).split("\0");
	const changes: GateChange[] = [];
	for (let i = 0; i + 1 < records.length; i += 2) {
		const commit = records[i].trim(), message = records[i + 1];
		if (!/^[0-9a-f]{40}$/.test(commit)) throw new Error("Cannot read source change declarations");
		const fields = parsePrBody(message);
		changes.push(recorded.get(commit) ?? {
			at, commit, title: message.split("\n")[0], receipt,
			class: fields.decision ?? "none", cause: fields.cause, issue: fields.issue,
			items: fields.items, assumed: fields.assumed, deletedTests: fields.retired,
			revert: `git revert ${commit}`,
		});
	}
	return changes;
}

/** Require the full cohort; only standing gaps and justified frozen prelaunch exceptions may remain unrun. */
export function fullPromotionProblems(receipt: PromotionReceipt, steps: RegistryStep[] = stepsForTier("full", "full"), {policy = readPhase(), history, now = new Date(), stepDependencies = STEPS}: PromotionOptions = {}) {
	const problems = [];
	if (receipt.kind !== "full" || receipt.candidate || receipt.result !== "pass") problems.push("A passing full receipt is required");
	if (receipt.phase !== undefined) {
		try {
			const recorded = validatePhase(receipt.phasePolicy);
			if (recorded.phase !== receipt.phase) throw new Error("Receipt phase differs from its frozen policy");
		} catch (error) { problems.push(`Invalid recorded phase: ${(error as Error).message}`); }
	}
	const actual = Array.isArray(receipt.steps) ? receipt.steps : [];
	if (!Array.isArray(steps) || !steps.length || steps.some(step => typeof step?.id !== "string" || !step.id) || new Set(steps.map(step => step.id)).size !== steps.length)
		return [...problems, "Required full steps must be a nonempty set of ids"];
	const required = new Set(steps.map(step => step.id));
	const rotated = new Set();
	if (receipt.rotation?.eligible?.length || receipt.rotation?.skipped?.length || receipt.rotation?.qualifications?.length) {
		try {
			const recorded = validatePhase(receipt.phasePolicy);
			if (receipt.phase !== "prelaunch" || recorded.phase !== "prelaunch" || policy.phase !== "prelaunch") throw new Error("Rotation is allowed only in prelaunch");
			const plan = receipt.rotation, same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);
			if (!Number.isSafeInteger(plan.ordinal) || plan.ordinal < 0 || plan.cycle !== 3 || !same(plan.approved, recorded.rotation) || !same(plan.approved, policy.rotation)) throw new Error("Rotation does not match the approved policy");
			if (!Array.isArray(stepDependencies) || !stepDependencies.length || new Set(stepDependencies.map(step => step?.id)).size !== stepDependencies.length
				|| stepDependencies.some(step => typeof step?.id !== "string" || !step.id || step.needs !== undefined && typeof step.needs !== "string"
					|| step.after !== undefined && (!Array.isArray(step.after) || step.after.some(id => typeof id !== "string")))) throw new Error("Invalid frozen step dependencies");
			const registry = [...stepDependencies, ...steps], protectedIds = requiredArtifacts(registry);
			for (const key of ["eligible", "selected", "skipped", "forced"] as const) if (!Array.isArray(plan[key]) || new Set(plan[key]).size !== plan[key].length || plan[key].some(id => !required.has(id) || !plan.approved.includes(id) || protectedIds.has(id))) throw new Error(`Invalid rotation ${key}`);
			if (plan.forced.some(id => !plan.eligible.includes(id))) throw new Error("Forced rotation steps are not eligible");
			const selected = plan.eligible.filter(id => plan.approved.indexOf(id) % 3 === plan.ordinal % 3 || plan.forced.includes(id));
			if (!same(plan.selected, selected) || !same(plan.skipped, plan.eligible.filter(id => !selected.includes(id)))) throw new Error("Rotation skipped a due step");
			if (!Array.isArray(plan.qualifications) || plan.qualifications.length !== plan.eligible.length || new Set(plan.qualifications.map(q => q.id)).size !== plan.eligible.length) throw new Error("Rotation qualifications differ from eligible steps");
			const cutoff = Number(new Date(now));
			if (!Number.isFinite(Date.parse(receipt.at)) || !Number.isFinite(cutoff) || Date.parse(receipt.at) > cutoff) throw new Error("Rotation needs an execution timestamp no later than validation");
			const prior = (history ?? listReceipts()).filter(earlier => {
				if (!["full", "prod"].includes(earlier.kind)) return false;
				if (!Number.isFinite(Date.parse(earlier.at))) throw new Error("Rotation history has an invalid timestamp");
				return earlier.id !== receipt.id && Date.parse(earlier.at) < cutoff;
			});
			const expected = rotationFromHistory(recorded, {mode: "full", steps: registry, receipts: prior.filter((earlier: { at: string; }) => Date.parse(earlier.at) < Date.parse(receipt.at))});
			for (const key of ["ordinal", "cycle", "approved", "eligible", "forced", "selected", "skipped", "qualifications"] as const)
				if (!same(plan[key], expected[key])) throw new Error(`Rotation ${key} differs from actual latest executed history`);
			// Keep the execution's due-slot plan, but refuse new promotion after a later execution
			// supersedes its qualifying window. Later unrun/reused observations add no execution.
			const latest = rotationFromHistory(recorded, {mode: "full", steps: registry, receipts: prior});
			for (const qualification of plan.qualifications)
				if (!same(qualification, latest.qualifications.find(actual => actual.id === qualification.id))) throw new Error(`Rotation qualifications differ from actual latest executed history: ${qualification.id}`);
			for (const id of plan.skipped) rotated.add(id);
		} catch (error) { problems.push(`Invalid prelaunch rotation: ${(error as Error).message}`); }
	}
	for (const step of steps) {
		const found = actual.filter((result: { id: string; }) => result.id === step.id);
		if (found.length !== 1) { problems.push(`${step.id}: expected exactly one result`); continue; }
		const result = found[0];
		const registryStep = stepDependencies.find(entry => entry.id === step.id) ?? step;
		const browserPolicy = receipt.phase === "prelaunch" && policy.phase === "prelaunch"
			&& JSON.stringify(receipt.phasePolicy?.prelaunchFull?.browsers) === JSON.stringify(["chromium"])
			&& JSON.stringify(policy.prelaunchFull?.browsers) === JSON.stringify(["chromium"]);
		const wholeBrowserGap = browserPolicy && browserGap(policy, {mode: "full", step: registryStep}) && result.status === "unrun" && result.reason === CHROMIUM_ONLY_REASON;
		const commands = registryStep.run ?? [];
		const expectedGaps = commands.map(command => browserPolicy && browserGap(policy, {mode: "full", step: registryStep, command}));
		const actualCommands = result.details?.commands;
		const hasCommandGap = actualCommands?.some((command: { status: string; }) => command.status === "unrun");
		let commandBrowserGap = false;
		if (hasCommandGap || result.status === "unrun" && result.reason === CHROMIUM_ONLY_REASON && !wholeBrowserGap) {
			if (!browserPolicy || !Array.isArray(actualCommands) || actualCommands.length !== commands.length || !expectedGaps.some(Boolean)
				|| actualCommands.some((command, index) => expectedGaps[index] ? command.status !== "unrun" || command.reason !== CHROMIUM_ONLY_REASON : command.status !== "pass")) {
				problems.push(`${step.id}: invalid prelaunch browser command gaps`);
			} else commandBrowserGap = expectedGaps.every(Boolean) && result.status === "unrun" && result.reason === CHROMIUM_ONLY_REASON;
		}
		const innerBrowserGaps = (result.details?.unrunInputs ?? []).filter((input: { reason: string; }) => input.reason === CHROMIUM_ONLY_REASON);
		if (innerBrowserGaps.length) {
			const requiredInner = ["journeys", "journeys-full"].includes(step.id) ? ["offline author journey in firefox", "offline author journey in webkit"]
				: step.id === "perf" ? ["webkit.frame.p95", "webkit.frame.max"] : [];
			// Command-level gaps have already been checked against the actual registry above.
			if (!hasCommandGap && (!browserPolicy || !requiredInner.length || innerBrowserGaps.length !== requiredInner.length
				|| requiredInner.some(name => !innerBrowserGaps.some((input: { what: string; }) => input.what === name)))) problems.push(`${step.id}: invalid internal prelaunch browser gaps`);
		}
		const standingGap = ["voiceover", "hosted-smoke", "windows"].includes(step.id) && result.status === "unrun" && result.reason?.trim();
		const scheduledGap = step.id === "fresh-agent" && receipt.kind === "full" && result.status === "unrun" && result.reason === FRESH_AGENT_SCHEDULE;
		const prelaunchGap = rotated.has(step.id) && result.status === "unrun" && result.reason === "prelaunch rotation" && result.rotation?.ordinal === receipt.rotation!.ordinal && result.rotation?.cycle === 3;
		const exception = prelaunchException(policy, {mode: receipt.kind, id: step.id, now});
		const atExecution = prelaunchException(receipt.phasePolicy ?? {}, {mode: receipt.kind, id: step.id, now: receipt.at});
		const datedGap = receipt.phase === "prelaunch" && Number.isFinite(Date.parse(receipt.at)) && Date.parse(receipt.at) <= Number(new Date(now)) && result.status === "unrun" && exception && atExecution
			&& JSON.stringify(exception) === JSON.stringify(atExecution) && JSON.stringify(result.exception) === JSON.stringify(exception) && result.reason === exception.reason;
		if (result.exception && !datedGap) problems.push(`${step.id}: invalid or expired prelaunch exception`);
		if (result.status !== "pass" && !standingGap && !scheduledGap && !prelaunchGap && !datedGap && !wholeBrowserGap && !commandBrowserGap) problems.push(`${step.id}: ${result.status}`);
		if (rotated.has(step.id) && !prelaunchGap) problems.push(`${step.id}: rotated result differs from the recorded plan`);
		if (policy.phase === "launch" && (result.flaky || result.attempts?.some((attempt: { status: string; }) => attempt.status === "fail"))) problems.push(`${step.id}: launch requires a strict first-attempt pass`);
	}
	for (const step of actual) if (!required.has(step.id)) problems.push(`Unexpected full step: ${step.id}`);
	return problems;
}

/** Fast-forward pre-prod to the stored passing full's exact commit; read-only unless apply is explicit. */
export function promoteFull(cwd: string, { receiptId, apply = false }: {receiptId?: string; apply?: boolean} = {}) {
	const receipt = readReceipt<PromotionReceipt & Partial<AffectedReceipt>>(receiptId!);
	const required = stepsForTier("full", "full").map(({ id }) => ({ id }));
	const isAffected = receipt.kind === "affected";
	const problems = isAffected ? [] : fullPromotionProblems(receipt, required);
	if (receipt.id !== receiptId) problems.push("Receipt identity differs from its stored id");
	if (!/^[0-9a-f]{40}$/.test(receipt.commit ?? "")) problems.push("Receipt has no exact source commit");
	if (!/^[0-9a-f]{40}$/.test(receipt.prod ?? "")) problems.push("Receipt has no exact prod base");
	if (problems.length) throw new Error(`Cannot promote pre-prod: ${problems.join("; ")}`);
	const { base, head } = readDev(cwd, { at: receipt.commit });
	if (isAffected && base !== receipt.base && base !== head) throw new Error("Affected receipt does not cover the current pre-prod base");
	const affectedProof = isAffected ? affectedPromotion({cwd, receipt: receipt as AffectedReceipt}) : undefined;
	const tree = git(cwd, "rev-parse", `${head}^{tree}`);
	if (tree !== receipt.tree) throw new Error("Full receipt tree differs from its source commit");
	if (base !== head && !ancestor(cwd, base, head)) throw new Error("pre-prod only fast-forwards to the passing full head");
	if (!ancestor(cwd, receipt.prod!, head)) throw new Error("Full receipt prod base is outside its source history");
	const changes = sourceChanges(cwd, { base: receipt.prod!, head, receipt: receiptId!, at: receipt.at });
	const prod = git(cwd, "ls-remote", "origin", "refs/heads/prod");
	const updated = apply && base !== head;
	if (updated) execFileSync("git", ["push", "-q", `--force-with-lease=refs/heads/pre-prod:${base}`, "origin", `${head}:refs/heads/pre-prod`], {
		cwd, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"], env: { ...process.env, RV_GATE_PUSH: "pre-prod" },
	});
	if (git(cwd, "ls-remote", "origin", "refs/heads/prod") !== prod) throw new Error("prod changed externally during pre-prod promotion");
	if (apply) {
		const recorded = new Set(readChanges().map(entry => entry.commit));
		for (const change of changes) if (!recorded.has(change.commit)) appendChange(change);
	}
	if (apply && !receiptNotes(receiptId!).some(note => note.kind === "promotion")) {
		// Cover the full unreleased range even after a retry observes pre-prod already at head.
		const commits = git(cwd, "rev-list", `${receipt.prod}..${head}`).split("\n").filter(Boolean);
		const capturesBaseline = receipt.captures ? writeBaseline("pre-prod", receipt.captures) : undefined;
		const stepDependencies = receipt.rotation?.eligible?.length ? STEPS.map(({id, needs, after}) => ({id, ...(needs && {needs}), ...(after && {after})})) : undefined;
		writeReceiptNote(receiptId!, "promotion", { base, coveredBase: receipt.prod, commit: head, tree, commits, capturesBaseline, required: isAffected ? receipt.selected : required.map(step => step.id), ...(stepDependencies && {stepDependencies}), ...(affectedProof && {affectedProof}) });
	}
	return { base, head, updated, receipt: receiptId };
}
