// Host reason (ARC-03): keep full/release build failures inside their ordinary result and receipt path.
import { join } from "node:path";
import { linkTarget, provenanceProblems, snapshotRelease } from "./target.mts";
import type { CopiedOutput } from "./target.mts";

interface SnapshotState {
	cwd: string;
	tempDir?: string;
	snapshot?: string;
	snapshotFiles?: CopiedOutput[];
}

/** Copy a passing build or return a failed build before any dependent can use shared artifacts. */
export function captureBuildSnapshot<T extends { status: string }>({ ctx, target, result, notAfter }: {
	ctx: SnapshotState; target: string; result: T; notAfter: number;
}) {
	ctx.snapshotFiles = [];
	try {
		ctx.snapshot = snapshotRelease({ target, cwd: ctx.cwd, dir: join(ctx.tempDir!, "target"), files: ctx.snapshotFiles });
		const problems = provenanceProblems(ctx.snapshotFiles, notAfter);
		if (problems.length) throw new Error(`The copy is not this run's build: ${problems.slice(0, 3).join("; ")}`);
		return result;
	} catch (error) {
		ctx.snapshot = undefined;
		ctx.snapshotFiles = [];
		linkTarget(ctx.cwd, target);
		return { ...result, status: "fail" as const, reason: `Cannot preserve the release build: ${(error as Error).message}` };
	}
}
