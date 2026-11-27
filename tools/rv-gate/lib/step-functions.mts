// Host reason (ARC-03): run artifact-dependent gate checks in the parent or a fresh retry worker.
import {join} from "node:path";
import * as checks from "./checks.mts";
import {captureAll, diffManifests, noBaselineResult} from "./captures.mts";
import {readBaseline, receiptNotes} from "./store.mts";
import {releaseBinDir} from "./target.mts";
import {diffSurface, generateSurface, readCommitted, writeSurface} from "./surface.mts";
import {sh} from "./proc.mts";
import {runHostedSmoke} from "./hosted-smoke.mts";
import type {GateChange} from "./store.mts";
import type {StepOutcome} from "./results.mts";

interface ArtifactContext {
	cwd: string;
	runDir: string;
	mode?: string;
	base?: string;
	head?: string;
	snapshot?: string;
	baseline?: string;
	candidate?: string;
	pr?: {decision?: string; items?: string[]};
	rangeChanges?: GateChange[];
	captures?: Record<string, string>;
	captureDiffs?: Awaited<ReturnType<typeof diffManifests>>;
	surfaceChanges?: string[];
}

export const specialChecks = (target: string) => ({
	hostedSmoke: runHostedSmoke,
	async captures(ctx: ArtifactContext): Promise<StepOutcome> {
		const matrixDir = join(ctx.runDir, "chart-review");
		const { manifest, problems } = await captureAll({ cwd: ctx.cwd, runDir: ctx.runDir, binDir: releaseBinDir({ snapshot: ctx.snapshot, cwd: ctx.cwd, target: target }), matrixDir });
		ctx.captures = manifest;
		if (problems.length) return { status: "fail", reason: problems.join("; ") };
		// A release run compares with the last released captures, a dev run with dev's.
		const dev = readBaseline(ctx.baseline ?? "dev");
		const prod = readBaseline("prod");
		if (ctx.mode === "prod") {
			const diffs = prod ? await diffManifests({ cwd: ctx.cwd, runDir: ctx.runDir, before: prod, after: manifest }) : [];
			ctx.captureDiffs = diffs;
			return { status: "pass", details: { captures: Object.keys(manifest).length, changedAgainstProd: diffs.filter((d) => d.status !== "same").length } };
		}
		if (!dev) return noBaselineResult(ctx.baseline ?? "dev", manifest);
		const diffs = await diffManifests({ cwd: ctx.cwd, runDir: ctx.runDir, before: dev, after: manifest });
		ctx.captureDiffs = diffs;
		const changed = diffs.filter((d) => d.status === "changed" || d.status === "added" || d.status === "removed");
		if (!changed.length) return { status: "pass", details: { captures: Object.keys(manifest).length, changed: 0 } };
		const decision = ctx.pr?.decision;
		if (ctx.mode === "dev" && decision === "none") {
			// A `none` PR may only change captures back to the prod look (DEL-32).
			const notProd = changed.filter((d) => !prod || prod[d.name] !== (manifest as Record<string, string>)[d.name]);
			if (notProd.length) {
				return {
					status: "fail",
					reason: `decision: none, but ${notProd.length} capture(s) differ from dev and from prod: ${notProd
						.slice(0, 8)
						.map((d) => d.name)
						.join(", ")}`,
					details: { changed },
				};
			}
		}
		return { status: "pass", details: { captures: Object.keys(manifest).length, changed: changed.length, names: changed.map((d) => d.name) } };
	},

	async surface(ctx: ArtifactContext): Promise<StepOutcome> {
		const generated = generateSurface(releaseBinDir({ snapshot: ctx.snapshot, cwd: ctx.cwd, target: target }));
		writeSurface(join(ctx.runDir, "surface"), generated);
		const committed = readCommitted(join(ctx.cwd, "docs/surface"));
		const stale = diffSurface(generated, committed);
		if (stale.length) {
			return { status: "fail", reason: `docs/surface is out of date (run rv-gate surface --write): ${stale.slice(0, 10).join(", ")}` };
		}
		if (ctx.mode === "dev") {
			const changed = sh("git", ["diff", "--name-only", ctx.base!, ctx.head!, "--", "docs/surface"], { cwd: ctx.cwd }).split("\n").filter(Boolean);
			ctx.surfaceChanges = changed;
			if (changed.length && ctx.pr?.decision !== "feature" && !ctx.pr?.items?.length) {
				return { status: "fail", reason: `Public surface changed (${changed.join(", ")}): declare feature or name the accepted items (DEL-53).` };
			}
			return { status: "pass", details: { changed } };
		}
		return { status: "pass" };
	},

	reviews(ctx: ArtifactContext) {
		const changes = ctx.rangeChanges ?? [];
		const records = changes.flatMap((entry: { receipt: string; }) => receiptNotes(entry.receipt));
		const batch = ctx.candidate ? receiptNotes(`batch-${ctx.candidate}`) : [];
		return checks.reviews({ changes, reviewRecords: [...records, ...batch] });
	},
});
