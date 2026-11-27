// Host reason (ARC-03): run the frozen local services smoke against this run's private artifacts.
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, realpathSync, rmSync, statSync } from "node:fs";
import { join } from "node:path";
import { gateEnv, runLogged } from "./proc.mts";
import { provenanceProblems } from "./target.mts";

import type { PathOrFileDescriptor } from "node:fs";
import type { CopiedOutput } from "./target.mts";
import type { GateStatus, StepOutcome } from "./results.mts";
import type { HostedSmokeEvidence } from "../hosted-smoke.mts";

export type HostedSmokeContext = {cwd: string; runDir: string; completed?: ReadonlyMap<string, GateStatus>; ended?: ReadonlyMap<string, number>; tempDir?: string; snapshot?: string; snapshotFiles?: CopiedOutput[]; signal?: AbortSignal};

const digest = (file: PathOrFileDescriptor) => createHash("sha256").update(readFileSync(file)).digest("hex");

/** Preserve prerequisite, private-copy and deployed-gap evidence for the local hosted step. */
export async function runHostedSmoke(ctx: HostedSmokeContext): Promise<StepOutcome> {
	const missing = ["build", "publish-tests"].filter(id => ctx.completed?.get(id) !== "pass");
	if (missing.length) return { status: "unrun", reason: `Needs passing prerequisite steps: ${missing.join(", ")}.` };
	const logs: string[] = [];
	const inputs: Partial<Record<"native" | "publishWasm", {file: string; sha256: string}>> = {};
	try {
		const privateRoot = ctx.tempDir && join(realpathSync(ctx.tempDir), "target");
		if (!ctx.snapshot || !privateRoot || realpathSync(ctx.snapshot) !== privateRoot)
			throw new Error("Local hosted smoke requires the run's private artifact snapshot");
		for (const [key, path, step] of [
			["native", "release/richview", "build"],
			["publishWasm", "wasm32-unknown-unknown/release/richview_publish_server.wasm", "publish-tests"],
		] as const) {
			const file = join(privateRoot, path), record = ctx.snapshotFiles?.find((entry: { path: string; }) => entry.path === path), ended = ctx.ended?.get(step);
			if (!record || !Number.isFinite(ended) || !Number.isFinite(record.mtimeMs)) throw new Error(`Missing ${step} copy provenance for ${path}`);
			const problems = provenanceProblems([record], ended!), stat = statSync(file);
			if (realpathSync(file) !== file || stat.size !== record.copiedSize || Math.abs(stat.mtimeMs - record.mtimeMs) > 1 || problems.length)
				throw new Error(`Invalid private artifact copy: ${path}${problems.length ? ` (${problems.join("; ")})` : ""}`);
			inputs[key] = { file, sha256: digest(file) };
		}
		const out = join(ctx.runDir, "hosted-smoke"), evidencePath = join(out, "hosted-smoke.json"), logPath = join(ctx.runDir, "logs/hosted-smoke.log");
		mkdirSync(out, { recursive: true }); mkdirSync(join(ctx.runDir, "logs"), { recursive: true });
		rmSync(evidencePath, { force: true });
		logs.push(logPath);
		// The full scheduler already reserves one permit for this function's entire child lifecycle.
		const outcome = await runLogged(process.execPath, [join(ctx.cwd, "tools/rv-gate/hosted-smoke.mts"), "--cwd", ctx.cwd, "--bin", inputs.native!.file, "--publish-wasm", inputs.publishWasm!.file, "--out", out], {
			cwd: ctx.cwd, env: gateEnv({ CARGO_TARGET_DIR: ctx.snapshot, TMPDIR: ctx.tempDir }), logPath, signal: ctx.signal, timeoutMs: 60_000,
		});
		if (outcome.status !== "pass") throw new Error(`Local hosted smoke failed (exit ${outcome.exitCode}; ${logPath})`);
		const evidence = JSON.parse(readFileSync(evidencePath, "utf8")) as HostedSmokeEvidence;
		if (evidence.status !== "pass" || evidence.scope !== "local-loopback" || !evidence.native?.closed || !evidence.worker?.closed)
			throw new Error("Local hosted smoke did not prove successful local services and cleanup");
		for (const [key, input] of Object.entries(inputs))
			if (evidence.artifacts?.[key as keyof HostedSmokeEvidence["artifacts"]] !== input.sha256 || digest(input.file) !== input.sha256) throw new Error(`Local hosted smoke artifact changed or differs: ${key}`);
		if (!Array.isArray(evidence.unrun) || !evidence.unrun.length || evidence.unrun.some((check: { status: string; name: string; reason: string; }) => check.status !== "unrun" || !check.name?.trim() || !check.reason?.trim()))
			throw new Error("Local hosted smoke must retain its reasoned deployed-check gaps");
		return { status: "pass", logs, details: { scope: evidence.scope, artifacts: evidence.artifacts, evidence: evidencePath, unrunInputs: evidence.unrun.map(({ name, reason }) => ({ what: name, reason })) } };
	} catch (error) { return { status: "fail", reason: (error as Error).message, logs }; }
}
