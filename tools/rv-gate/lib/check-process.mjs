// Host reason (ARC-03): bind a fresh built-in check worker to its retained attempt files.
import {mkdirSync, readFileSync, writeFileSync, existsSync} from "node:fs";
import {join} from "node:path";
import {fileURLToPath} from "node:url";
import {serialize, deserialize} from "node:v8";
import {createHash} from "node:crypto";
import {gateEnv, runLogged, snapshotOwnedGroup, classifyOwnedGroup, signalOwnedGroup} from "./proc.mjs";

/** Drain persisted, identity-verified worker groups; unknown is never treated as empty. */
export async function drainWorkerGroups(file, {notStarted = false, timeoutMs = 4000, ownershipHash} = {}) {
	if (!existsSync(file)) return notStarted ? {status: "empty"} : {status: "unknown", reason: "Worker did not persist group ownership.", ownershipHash: null};
	let groups, hash;
	try {
		const contents = readFileSync(file); hash = createHash("sha256").update(contents).digest("hex");
		if (ownershipHash === null || ownershipHash !== undefined && ownershipHash !== hash) return {status: "unknown", reason: "Worker group ownership changed or was not authenticated.", ownershipHash: ownershipHash ?? null};
		groups = JSON.parse(contents);
	} catch { return {status: "unknown", reason: "Cannot read worker group ownership.", ownershipHash: null}; }
	if (!Array.isArray(groups) || groups.some(group => !Number.isSafeInteger(group.pgid) || group.pgid <= 0 || !Array.isArray(group.identities)
		|| group.identities.some(identity => !Number.isSafeInteger(identity.pid) || identity.pid <= 0 || typeof identity.start !== "string" || !identity.start)))
		return {status: "unknown", reason: "Invalid worker group ownership.", ownershipHash: hash};
	const deadline = Date.now() + timeoutMs;
	for (const {pgid, identities} of groups) {
		for (;;) {
			const state = classifyOwnedGroup(snapshotOwnedGroup(pgid), identities);
			if (state === "empty") break;
			if (state === "owned" && signalOwnedGroup(pgid, identities) === "empty") break;
			if (Date.now() >= deadline) return {status: "unknown", reason: `Worker group ${pgid} could not be verified drained.`, ownershipHash: hash};
			await new Promise(resolve => setTimeout(resolve, 25));
		}
	}
	return {status: "empty"};
}

/** Retain existing holds until every unresolved group is freshly verified empty. */
export async function waitForWorkerGroups(records, {alert, persist, intervalMs = 10000} = {}) {
	if (!Array.isArray(records) || !records.length || typeof alert !== "function" || typeof persist !== "function") throw new Error("Unresolved worker groups need a diagnostic and operator alert");
	let durable = false, warned = false, alerted = false;
	for (;;) {
		if (!durable) try { await persist(records); durable = true; }
		catch (error) { if (!warned) console.error(`Cannot persist blocked-worker diagnostic: ${error.message}`); warned = true; }
		if (!alerted) {
			alerted = true;
			try { await alert(records); } catch (error) { console.error(`Could not alert the gate lead: ${error.message}`); }
		}
		let unresolved = false;
		for (const record of records) {
			try { if ((await drainWorkerGroups(record.file, {ownershipHash: record.ownershipHash ?? null})).status !== "empty") unresolved = true; }
			catch { unresolved = true; }
		}
		if (durable && !unresolved) return;
		await new Promise(resolve => setTimeout(resolve, intervalMs));
	}
}

export async function freshCheck(step, ctx, target, attempt = 2) {
	if (ctx.signal?.aborted) return {status: "unrun", reason: "Gate cancelled before check start."};
	const dir = join(ctx.runDir, "attempts", step.id, attempt === 1 ? "worker-1" : "2"); mkdirSync(dir, {recursive: true});
	const input = join(dir, "input.bin"), output = join(dir, "output.bin"), logPath = join(dir, "worker.log"), groups = join(dir, "groups.json");
	const fields = ["cwd", "base", "head", "mode", "pr", "runDir", "tempDir", "steps", "registry", "completed", "ended", "cargoMark", "snapshot", "snapshotFiles", "baseline", "rangeChanges", "candidate", "phasePolicy"];
	writeFileSync(input, serialize({step, target, ctx: Object.fromEntries(fields.filter(key => Object.hasOwn(ctx, key)).map(key => [key, ctx[key]]))}), {flag: "wx"});
	// Only fullTier installs this capability; release has an AbortSignal but no blocked-hold lifecycle.
	const managed = typeof ctx.cancel === "function";
	const outcome = await runLogged(process.execPath, [fileURLToPath(new URL("./check-worker.mjs", import.meta.url)), input, output, ...(managed ? [groups] : [])], {cwd: ctx.cwd, env: gateEnv(), logPath, signal: ctx.signal});
	if (managed) {
		const drain = await drainWorkerGroups(groups, {notStarted: outcome.status === "unrun"});
		if (drain.status === "unknown") {
			(ctx.unknownWorkerDrains ??= []).push({file: groups, reason: drain.reason, ownershipHash: drain.ownershipHash});
			ctx.cancel?.();
			return {status: ctx.signal?.aborted ? "unrun" : "fail", reason: drain.reason, logs: [logPath], drain};
		}
		if (ctx.signal?.aborted) return {status: "unrun", reason: "Gate cancelled during built-in check.", logs: [logPath], drain};
	}
	let saved;
	try { saved = deserialize(readFileSync(output)); }
	catch (error) { return {...outcome, status: "fail", reason: `Fresh check did not retain a result: ${error.message}`, logs: [logPath]}; }
	if (!["pass", "fail", "unrun"].includes(saved.result?.status) || !Number.isSafeInteger(saved.pid) || saved.pid === process.pid)
		return {status: "fail", reason: "Invalid fresh-check result or process identity", logs: [logPath]};
	const owned = step.fn === "captures" ? ["captures", "captureDiffs"] : step.fn === "surface" ? ["surfaceChanges"] : [];
	for (const key of owned) {
		delete ctx[key];
		if (Object.hasOwn(saved.updates ?? {}, key)) ctx[key] = saved.updates[key];
	}
	const result = outcome.status === "pass" ? saved.result : {...saved.result, status: "fail", reason: [saved.result.reason, outcome.reason].filter(Boolean).join("; ") || "Fresh check process failed"};
	return {...result, logs: [...(result.logs ?? []), logPath], details: {...result.details, processPid: saved.pid, browserRelaunches: outcome.browserRelaunches ?? []}};
}
