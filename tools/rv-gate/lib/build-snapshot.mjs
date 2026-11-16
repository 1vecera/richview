// Host reason (ARC-03): keep full/release build failures inside their ordinary result and receipt path.
import { join } from "node:path";
import { linkTarget, provenanceProblems, snapshotRelease } from "./target.mjs";

/** Copy a passing build or return a failed build before any dependent can use shared artifacts. */
export function captureBuildSnapshot({ ctx, target, result, notAfter }) {
	ctx.snapshotFiles = [];
	try {
		ctx.snapshot = snapshotRelease({ target, cwd: ctx.cwd, dir: join(ctx.tempDir, "target"), files: ctx.snapshotFiles });
		const problems = provenanceProblems(ctx.snapshotFiles, notAfter);
		if (problems.length) throw new Error(`The copy is not this run's build: ${problems.slice(0, 3).join("; ")}`);
		return result;
	} catch (error) {
		ctx.snapshot = undefined;
		ctx.snapshotFiles = [];
		linkTarget(ctx.cwd, target);
		return { ...result, status: "fail", reason: `Cannot preserve the release build: ${error.message}` };
	}
}
