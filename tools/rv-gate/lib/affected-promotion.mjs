// Host reason (ARC-03): validate prelaunch incremental coverage before a ref can move.
import {execFileSync, spawnSync} from "node:child_process";
import {readFileSync} from "node:fs";
import {affectedContext} from "./affected-context.mjs";
import {coverageProblems, policyFingerprint, requireGreenBase, validateAffectedHistory, invalidateTelemetryFailures} from "./affected-coverage.mjs";
import {expandLegacyCaseOutcomes} from "./case-selection.mjs";
import {affectedReceipts, fullReceipts} from "./affected-history.mjs";
import {failureCounts} from "./selection.mjs";
import {readPhase} from "./phase.mjs";
import {stepsForTier} from "./steps.mjs";
import {receiptNotes} from "./store.mjs";

const git = (cwd, ...args) => execFileSync("git", args, {cwd, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"]}).trim();
const contains = (cwd, later, earlier) => {
	const result = spawnSync("git", ["merge-base", "--is-ancestor", earlier, later], {cwd, encoding: "utf8"});
	if (![0,1].includes(result.status)) throw new Error("Cannot verify affected ancestry");
	return result.status === 0;
};

/** Recompute current-policy coverage; do not trust the receipt's declared obligations. */
export function affectedPromotion({cwd, receipt, policy = readPhase(), steps = stepsForTier("full", "full"),
	areas = JSON.parse(readFileSync(new URL("../areas.json", import.meta.url), "utf8")), counts, history, anchors, notes = receiptNotes} = {}) {
	if (policy.phase !== "prelaunch" || receipt?.kind !== "affected" || receipt.phase !== "prelaunch" || receipt.result !== "pass") throw new Error("A passing prelaunch affected receipt is required");
	if (!receipt.selected?.includes("build") || receipt.outcomes?.build !== "pass" || receipt.steps?.filter(step => step.id === "build" && step.status === "pass").length !== 1) throw new Error("Affected promotion requires the exact head's passing build");
	counts ??= failureCounts(fullReceipts(), steps);
	history ??= [...affectedReceipts()];
	if (!anchors) anchors = (function* () { yield* fullReceipts(); yield* history; })();
	const currentHash = policyFingerprint({policy, steps, areas, counts});
	if (receipt.policyHash !== currentHash) throw new Error("Affected policy changed; run affected checks again");
	if (receipt.tree !== git(cwd, "rev-parse", `${receipt.commit}^{tree}`)) throw new Error("Affected receipt tree differs from its source");
	const baseTree = git(cwd, "rev-parse", `${receipt.base}^{tree}`);
	const green = requireGreenBase({base: receipt.base, tree: baseTree, receipts: anchors, notes});
	if (receipt.greenReceipt !== green) throw new Error("Affected receipt green anchor differs");
	if (!history.some(row => row.id === receipt.id && JSON.stringify(row) === JSON.stringify(receipt))) throw new Error("Affected receipt is missing from immutable history");
	const context = affectedContext({cwd, base: receipt.base, head: receipt.commit, steps, areas, policy, counts, history, policyHash: currentHash});
	if (context.pending.length) throw new Error(`Affected coverage is incomplete: ${context.pending.map(item => item.unit).join(", ")}`);
	const onPath = history.filter(row => row.kind === "affected" && row.base === receipt.base && contains(cwd, receipt.commit, row.commit));
	return {base: receipt.base, policyHash: currentHash, obligations: context.obligations, receipts: onPath.map(row => row.id), outcomes: context.receipts, aliases: context.aliases, fallbacks: context.fallbacks};
}

/** Historical audit checks the frozen promotion proof and its underlying immutable receipts. */
export function historicalAffectedProblems({cwd, receipt, proof, read}) {
	try {
		if (!proof || proof.base !== receipt.base || proof.policyHash !== receipt.policyHash || !Array.isArray(proof.receipts) || !Array.isArray(proof.obligations) || typeof read !== "function") throw new Error("Missing affected promotion proof");
		const commits = git(cwd, "rev-list", "--reverse", "--topo-order", `${proof.base}..${receipt.commit}`).split("\n").filter(Boolean);
		if (JSON.stringify(commits) !== JSON.stringify(proof.obligations.map(item => item.commit))) throw new Error("Affected proof omits commits");
		const history = proof.receipts.map(read);
		const verified = validateAffectedHistory({receipts: history, base: proof.base, head: receipt.commit, policyHash: proof.policyHash,
			contains: (later, earlier) => contains(cwd, later, earlier), treeOf: commit => git(cwd, "rev-parse", `${commit}^{tree}`), allowPolicyChange: true});
		return coverageProblems({obligations: proof.obligations, receipts: expandLegacyCaseOutcomes(invalidateTelemetryFailures(history, verified), proof.aliases ?? {}, proof.fallbacks ?? {}), contains: (later, earlier) => contains(cwd, later, earlier), head: receipt.commit});
	} catch (error) { return [String(error.message)]; }
}
