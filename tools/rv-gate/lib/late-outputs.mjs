// Host reason (ARC-03): freeze late Publish artifacts with their build's copy provenance.
import { rmSync } from "node:fs";
import { join } from "node:path";
import { copyOutputs, LATE_OUTPUTS, provenanceProblems } from "./target.mjs";

/** Add the Publish build's late outputs to the run's private artifact copy. */
export function addLateOutputs(ctx, target) {
	if (!ctx.ended?.has("publish-tests")) return;
	const known = new Set(ctx.snapshotFiles.map(file => file.path));
	const outputs = LATE_OUTPUTS.filter(path => !known.has(path));
	if (!outputs.length) return;
	ctx.snapshotFiles.push(...lateOutputs({ target, dir: ctx.snapshot, notAfter: ctx.ended.get("publish-tests"), outputs }));
}

/** Reject outputs another checkout wrote after this run's Publish build ended. */
export function lateOutputs({ target, dir, notAfter, outputs = LATE_OUTPUTS }) {
	const copied = copyOutputs({ target, dir, outputs });
	for (const file of copied) {
		const problems = provenanceProblems([file], notAfter);
		if (!problems.length) continue;
		rmSync(join(dir, file.path), { force: true });
		console.log(`  not testing ${file.path}: ${problems[0]}; the steps that read it fail on its absence`);
	}
	return copied.filter(file => provenanceProblems([file], notAfter).length === 0);
}
