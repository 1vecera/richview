// Host reason (ARC-03): plan an incremental prelaunch run from immutable commits and receipts.
import {execFileSync, spawnSync} from "node:child_process";
import {changedCommits, sourceInputs, treeReader} from "./affected-git.mjs";
import {validateAffectedHistory, pendingPlan, invalidateTelemetryFailures} from "./affected-coverage.mjs";
import {commandId, selectAffected, validateAreas} from "./selection.mjs";
import {caseCatalog, expandLegacyCaseOutcomes} from "./case-selection.mjs";

/** Recompute each commit's obligations; old passes expire on policy changes, failures do not. */
export function affectedContext({cwd, base, head, steps, areas, policy, counts, history, policyHash}) {
	if (policy?.phase !== "prelaunch" || !policy.affected) throw new Error("Affected checks require the prelaunch affected policy");
	validateAreas(areas, steps);
	const contains = (later, earlier) => {
		const result = spawnSync("git", ["merge-base", "--is-ancestor", earlier, later], {cwd, encoding: "utf8"});
		if (![0,1].includes(result.status)) throw new Error("Cannot verify affected receipt ancestry");
		return result.status === 0;
	};
	const treeOf = commit => execFileSync("git", ["rev-parse", `${commit}^{tree}`], {cwd, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"]}).trim();
	const commits = changedCommits(cwd, base, head), read = treeReader(cwd, head), {graph, inputs, runtimeCases} = sourceInputs(cwd, head, read);
	const catalog = caseCatalog(steps, runtimeCases, commandId, {cohorts: policy.prelaunchFull?.partialRetry === true}), aliases = Object.fromEntries([...catalog].map(([id, spec]) => [id, spec.units]));
	const fallbacks = Object.fromEntries(steps.filter(step => step.cohort === "rust-debug" && !catalog.has(step.id)).map(step => [step.id, step.id]));
	const applicable = history.filter(receipt => receipt.kind === "affected" && receipt.base === base && contains(head, receipt.commit));
	const verified = validateAffectedHistory({receipts: applicable, base, head, policyHash, contains, treeOf, allowPolicyChange: true});
	const receipts = expandLegacyCaseOutcomes(invalidateTelemetryFailures(applicable, verified), aliases, fallbacks);
	const select = (paths, {retry = []} = {}) => selectAffected(steps, paths, {areas, counts, threshold: policy.affected.failureThreshold, phase: policy.phase, fallback: policy.affected.fallback, graph, inputs, read, retry, catalog});
	const plan = pendingPlan({commits, receipts, select, contains, head, always: ["build"]});
	plan.selection.find(item => item.id === "build").reason = "Build the exact head for the retained pre-prod artifact.";
	return {...plan, commits, policyHash, receipts, aliases, fallbacks};
}
