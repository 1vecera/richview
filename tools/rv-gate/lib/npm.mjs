// Whether a checkout's node_modules matches its package-lock.json. A gate checkout keeps its
// node_modules between runs, so a dependency a batch adds must trigger `npm ci` (S27-759).

import { existsSync, globSync, lstatSync, readFileSync, readlinkSync } from "node:fs";
import { join } from "node:path";
import { sh } from "./proc.mjs";

/** Whether `npm ci` must run: a locked, non-optional package is missing or at another version. */
export function npmStale(cwd) {
	const hidden = join(cwd, "node_modules/.package-lock.json");
	if (!existsSync(hidden)) return true;
	const want = JSON.parse(readFileSync(join(cwd, "package-lock.json"), "utf8")).packages ?? {};
	const have = JSON.parse(readFileSync(hidden, "utf8")).packages ?? {};
	return lockDiffers(want, have);
}

/** Every non-optional `node_modules/...` entry of the lock is installed at its locked version. */
export function lockDiffers(want, have) {
	return Object.entries(want).some(([path, meta]) => path.startsWith("node_modules/") && !meta.optional && have[path]?.version !== meta.version);
}

/** Install locked dependencies only when npm cannot empty a linked installation. */
export function npmCi(cwd, { prefix = ".", env } = {}) {
	const root = join(cwd, prefix);
	const manifest = join(root, "package.json");
	const configured = existsSync(manifest) ? JSON.parse(readFileSync(manifest, "utf8")).workspaces : [];
	const patterns = Array.isArray(configured) ? configured : configured?.packages ?? [];
	const matches = (pattern) => globSync(pattern, { cwd: root, exclude: ["**/node_modules/**"] });
	const excluded = new Set(patterns.filter(pattern => pattern.startsWith("!")).flatMap(pattern => matches(pattern.slice(1))));
	const workspaces = patterns.filter(pattern => !pattern.startsWith("!")).flatMap(matches)
		.filter(path => !excluded.has(path) && existsSync(join(root, path, "package.json")));
	for (const path of ["", ...workspaces]) {
		const modules = join(root, path, "node_modules");
		let entry;
		try { entry = lstatSync(modules); } catch (error) { if (error.code !== "ENOENT") throw error; }
		if (entry?.isSymbolicLink()) {
			const quoted = "'" + modules.replaceAll("'", "'\\''") + "'";
			throw new Error(`${modules} is a symlink to ${readlinkSync(modules)}; remove the link itself (rm -- ${quoted}, no trailing slash), then run npm ci --ignore-scripts in this checkout`);
		}
	}
	sh("npm", ["ci", "--ignore-scripts", "--no-audit", "--no-fund", "--prefix", prefix], { cwd, env });
}
