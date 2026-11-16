// Host reason (ARC-03): run artifact-dependent gate checks in the parent or a fresh retry worker.
import {join} from "node:path";
import * as checks from "./checks.mjs";
import {captureAll, diffManifests, noBaselineResult} from "./captures.mjs";
import {readBaseline, receiptNotes} from "./store.mjs";
import {releaseBinDir} from "./target.mjs";
import {diffSurface, generateSurface, readCommitted, writeSurface} from "./surface.mjs";
import {sh} from "./proc.mjs";
import {runHostedSmoke} from "./hosted-smoke.mjs";

export const specialChecks = target => ({
	hostedSmoke: runHostedSmoke,
	async captures(ctx) {
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
			const notProd = changed.filter((d) => !prod || prod[d.name] !== manifest[d.name]);
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

	async surface(ctx) {
		const generated = generateSurface(releaseBinDir({ snapshot: ctx.snapshot, cwd: ctx.cwd, target: target }));
		writeSurface(join(ctx.runDir, "surface"), generated);
		const committed = readCommitted(join(ctx.cwd, "docs/surface"));
		const stale = diffSurface(generated, committed);
		if (stale.length) {
			return { status: "fail", reason: `docs/surface is out of date (run rv-gate surface --write): ${stale.slice(0, 10).join(", ")}` };
		}
		if (ctx.mode === "dev") {
			const changed = sh("git", ["diff", "--name-only", ctx.base, ctx.head, "--", "docs/surface"], { cwd: ctx.cwd }).split("\n").filter(Boolean);
			ctx.surfaceChanges = changed;
			if (changed.length && ctx.pr?.decision !== "feature" && !ctx.pr?.items?.length) {
				return { status: "fail", reason: `Public surface changed (${changed.join(", ")}): declare feature or name the accepted items (DEL-53).` };
			}
			return { status: "pass", details: { changed } };
		}
		return { status: "pass" };
	},

	reviews(ctx) {
		const changes = ctx.rangeChanges ?? [];
		const records = changes.flatMap((entry) => receiptNotes(entry.receipt));
		const batch = ctx.candidate ? receiptNotes(`batch-${ctx.candidate}`) : [];
		return checks.reviews({ changes, reviewRecords: [...records, ...batch] });
	},
});
