// Host reason (ARC-03): retain one prelaunch rerun and both attempts' raw files (DEL-82).
import {existsSync, mkdirSync, renameSync} from "node:fs";
import {dirname, isAbsolute, join, relative, resolve, sep} from "node:path";

/** One rerun only; a second failure or unrun never becomes a pass. */
export async function withRerun(policy, run, {signal, beforeRerun} = {}) {
	const started = performance.now(), attempts = [];
	const attempt = async number => {
		let result;
		try { result = await run(number); } catch (error) { result = {status: "fail", reason: String(error?.message ?? error)}; }
		attempts.push({...result, number, ...(signal?.aborted && {interrupted: true})}); return result;
	};
	let result = await attempt(1);
	if (policy.phase === "prelaunch" && result.status === "fail" && !signal?.aborted) {
		try {
			if (beforeRerun) attempts[0] = {...await beforeRerun(attempts[0]), number: 1};
			result = await attempt(2);
			if (result.status !== "pass") result = {...result, status: "fail", reason: result.reason ?? "The prelaunch rerun did not pass"};
		} catch (error) { result = {...result, status: "fail", reason: `Cannot prepare a fresh rerun: ${error.message}`}; }
	}
	const flaky = attempts.length === 2 && attempts[0].status === "fail" && attempts[1].status === "pass";
	const browserRelaunches = attempts.flatMap(attempt => attempt.details?.browserRelaunches ?? []);
	const previewReloads = attempts.flatMap(attempt => attempt.details?.previewReloads ?? []);
	return {...result, durationMs: performance.now() - started, flaky, attempts,
		...((browserRelaunches.length || previewReloads.length) && {details: {...result.details,
			...(browserRelaunches.length && {browserRelaunches}), ...(previewReloads.length && {previewReloads})}})};
}

function ownedRelative(root, path) {
	const rel = relative(resolve(root), resolve(path));
	if (!rel || rel === ".." || rel.startsWith(`..${sep}`) || isAbsolute(rel)) throw new Error(`Attempt artifact leaves its run: ${path}`);
	return rel;
}

/** Move only this step's outputs, never the complete shared run or another step's files. */
export function archiveAttempt(step, ctx, result) {
	if (!/^[a-z][a-z0-9-]*$/.test(step.id)) throw new Error("Invalid rerun step id");
	const root = resolve(ctx.runDir), paths = new Set(step.retryArtifacts ?? []);
	for (const command of step.run ?? []) for (const value of [...command.args, ...Object.values(command.env ?? {}), command.shard?.out]) {
		if (typeof value !== "string" || !value.startsWith("{run}")) continue;
		const rel = value.slice(5).replace(/^\//, "");
		if (!rel) { if (!step.retryArtifacts?.length) throw new Error(`${step.id} needs explicit retryArtifacts for a run-root output`); continue; }
		paths.add(rel);
	}
	for (const command of step.run ?? []) {
		const protocol = command.env?.RV_GATE_FIREFOX_PROTOCOL_LOG;
		if (typeof protocol === "string" && protocol.startsWith("{run}/")) paths.add(protocol.slice(6) + ".meta.json");
	}
	if (step.id === "build") paths.add("build-manifest.json");
	for (const log of result.logs ?? []) {
		paths.add(ownedRelative(root, log));
		if (existsSync(`${log}.browser-events.jsonl`)) paths.add(ownedRelative(root, `${log}.browser-events.jsonl`));
	}
	const relativePaths = [...paths].map(path => ownedRelative(root, join(root, path))).sort();
	const roots = relativePaths.filter(path => !relativePaths.some(parent => parent !== path && path.startsWith(`${parent}${sep}`)));
	const archive = join(root, "attempts", step.id, "1"), moved = {};
	for (const rel of roots) {
		const source = join(root, rel); if (!existsSync(source)) continue;
		const target = join(archive, rel); if (existsSync(target)) throw new Error(`Previous attempt archive exists: ${target}`);
		mkdirSync(dirname(target), {recursive: true}); renameSync(source, target); moved[source] = target;
	}
	const relocated = path => {
		if (typeof path !== "string") return path;
		for (const [from, to] of Object.entries(moved)) if (path === from || path.startsWith(from + sep)) return to + path.slice(from.length);
		return path;
	};
	const events = values => values.map(event => ({...event, journal: relocated(event.journal), ...(event.screenshot && {screenshot: relocated(event.screenshot)})}));
	const protocol = value => ({...value, metadataPath: relocated(value.metadataPath)});
	return {...result, logs: result.logs?.map(relocated), artifactArchive: moved,
		...(step.id === "build" && {snapshotFiles: ctx.snapshotFiles ?? []}),
		...(step.fn === "captures" && {captures: ctx.captures, captureDiffs: ctx.captureDiffs}),
		...(result.details && {details: {...result.details,
			...(result.details.browserRelaunches && {browserRelaunches: events(result.details.browserRelaunches)}),
			...(result.details.previewReloads && {previewReloads: events(result.details.previewReloads)}),
			...(result.details.commands && {commands: result.details.commands.map(command => ({...command,
				...(command.firefoxProtocol && {firefoxProtocol: protocol(command.firefoxProtocol)}),
				...(command.browserRelaunches && {browserRelaunches: events(command.browserRelaunches)}),
				...(command.previewReloads && {previewReloads: events(command.previewReloads)})}))})}})};
}

/** Preserve a failed build's private copy before relinking the checkout to shared Cargo outputs. */
export function archiveBuildSnapshot(ctx) {
	if (!ctx.tempDir) return;
	const source = join(ctx.tempDir, "target");
	if (ctx.snapshot && resolve(ctx.snapshot) !== resolve(source)) throw new Error("Build snapshot is outside its owned temporary folder");
	let archive;
	if (existsSync(source)) {
		archive = join(ctx.runDir, "attempts/build/1/private-target");
		if (existsSync(archive)) throw new Error("A previous build attempt is already archived");
		mkdirSync(dirname(archive), {recursive: true}); renameSync(source, archive);
	}
	ctx.snapshot = undefined; ctx.snapshotFiles = [];
	return archive;
}
