// ARC-67 as proposed: Windows ships only with a passing end-to-end walk on a fresh AWS Windows
// machine for the candidate's exact commit. tools/windows-e2e runs the walk outside the full;
// `rv-gate windows <candidate> --run <dir>` checks its record and notes it on the candidate's
// receipt, and merge-prod refuses a candidate that ships Windows without such a note.

import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

/** The full reports the Windows step unrun with this reason; pre-prod accepts it as a standing gap. */
export const WINDOWS_STANDING = "ARC-67: the Windows walk runs on a fresh AWS Windows machine for each candidate, outside the full (tools/windows-e2e); merge-prod requires its passing record.";

/** Receipt notes of recorded walks have kinds that start with this. */
export const WINDOWS_NOTE = "windows-e2e";

/** The programs and files a walk's build lists, by the manifest field that records each. */
const WALKED = [
	["richviewSha256", "richview.exe"],
	["workerSha256", "richview-sql-worker.exe"],
	["editorSha256", "richview-editor.exe"],
	["databaseSha256", "warehouse.duckdb"],
];

/**
 * The walk recorded in `dir`, checked against `commit`. `dir` holds the run's `manifest.json`
 * (commit, walk, instance, image, started and terminated times, cost, and the sha256 of the
 * archive, its two programs, the editor and the SQL example database), the build's own sha256
 * list `build.sha256`, and the walk's `e2e.log`. Every recorded hash must be the one the build
 * lists: the archive's programs as packed, and the editor and database as they arrived on the
 * machine. The richview.exe whose --version the log shows naming `commit` is thereby the same
 * build as the editor it walked.
 */
export function readWindowsRun(dir, commit) {
	const problems = [];
	const read = (name) => (existsSync(join(dir, name)) ? readFileSync(join(dir, name), "utf8") : null);
	let manifest = null;
	try {
		manifest = JSON.parse(read("manifest.json") ?? "null");
	} catch {
		problems.push("manifest.json is not JSON");
	}
	if (!manifest) problems.push("the run has no manifest.json");
	else {
		if (manifest.commit !== commit) problems.push(`the walk tested ${manifest.commit ?? "no commit"}, not ${commit}`);
		if (manifest.walk !== "commit") problems.push(`the walk ran ${manifest.walk ?? "an unnamed script"}, not the commit's own e2e.ps1`);
		if (!/^i-[0-9a-f]{8,}$/.test(manifest.instance ?? "")) problems.push("the manifest names no instance");
		if (!manifest.terminated) problems.push("the instance's termination is not recorded");
		const build = read("build.sha256");
		const listed = build === null ? null : Object.fromEntries(build.split("\n").map((line) => line.trim().match(/^([0-9a-f]{64}) [ *]?(\S+)$/i)).filter(Boolean).map(([, sha, name]) => [name, sha.toLowerCase()]));
		if (!listed) problems.push("the run has no build.sha256, the build's own sha256 list");
		for (const [field, name] of WALKED) {
			const recorded = manifest[field];
			if (!/^[0-9a-f]{64}$/.test(recorded ?? "")) problems.push(`the manifest records no sha256 of ${name}`);
			else if (listed && listed[name] !== recorded) problems.push(`the walk used a ${name} with sha256 ${recorded}; the build lists ${listed[name] ?? "none"}`);
		}
		if (!/^[0-9a-f]{64}$/.test(manifest.archiveSha256 ?? "")) problems.push("the manifest records no sha256 of the archive");
	}
	const log = read("e2e.log");
	let result = null;
	if (log === null) problems.push("the run has no e2e.log");
	else {
		const text = log.replace(/\r/g, "");
		result = text.match(/^E2E (PASS|FAIL)\b.*$/gm)?.at(-1) ?? null;
		if (result !== "E2E PASS") problems.push(`the walk ended ${result ? `"${result.slice(0, 200)}"` : "without a result"}`);
		if (!text.includes(`(commit ${commit.slice(0, 10)}`)) problems.push("the installed richview --version does not name the commit");
	}
	const summary = manifest && { instance: manifest.instance, image: manifest.image, started: manifest.started, terminated: manifest.terminated, costUsd: manifest.costUsd, archiveSha256: manifest.archiveSha256, ...Object.fromEntries(WALKED.map(([field]) => [field, manifest[field]])) };
	return { ok: problems.length === 0, problems, result, summary };
}

/**
 * Whether a tree ships Windows, from the text of its `assets/install/release.json` (null when the
 * tree has none). It reads the file as the build's distribution.rs does: a missing `windows` is
 * false, and a missing file, text that is not a JSON object or a `windows` that is not a boolean
 * is an error, so merge-prod refuses instead of guessing.
 */
export function shipsWindows(releaseJson) {
	if (releaseJson === null) throw new Error("The candidate has no assets/install/release.json, so whether it ships Windows is unknown (ARC-67).");
	let release;
	try {
		release = JSON.parse(releaseJson);
	} catch {
		throw new Error("The candidate's assets/install/release.json is not JSON, so whether it ships Windows is unknown (ARC-67).");
	}
	if (release === null || typeof release !== "object" || Array.isArray(release)) throw new Error("The candidate's assets/install/release.json is not a JSON object (ARC-67).");
	if (release.windows === undefined) return false;
	if (typeof release.windows !== "boolean") throw new Error("The candidate's assets/install/release.json must say `windows` as true or false (ARC-67).");
	return release.windows;
}

/** Why merge-prod must refuse `head`, or null. */
export function windowsMergeProblem({ windows, notes, head }) {
	if (!windows) return null;
	const passed = notes.some((note) => note.kind?.startsWith(WINDOWS_NOTE) && note.commit === head && note.result === "pass");
	if (passed) return null;
	return `This candidate ships Windows (assets/install/release.json), but no passing Windows walk is recorded for ${head.slice(0, 10)}. Run tools/windows-e2e on it and record it with rv-gate windows <candidate> --run <dir> (ARC-67).`;
}
