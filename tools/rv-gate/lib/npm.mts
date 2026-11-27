// Whether a checkout's node_modules matches its package-lock.json. A gate checkout keeps its
// node_modules between runs, so a dependency a batch adds must trigger `npm ci`.

import { existsSync, globSync, lstatSync, readFileSync, readlinkSync } from "node:fs";
import { join } from "node:path";
import { sh } from "./proc.mts";

export interface NpmLockEntry {
	version?: string;
	optional?: boolean;
	license?: string;
	link?: boolean;
	dev?: boolean;
}

export interface NpmLock { packages?: Record<string, NpmLockEntry> }
interface PackageManifest { workspaces?: string[] | { packages?: string[] } }

/** Whether `npm ci` must run: a locked, non-optional package is missing or at another version. */
export function npmStale(cwd: string) {
	const hidden = join(cwd, "node_modules/.package-lock.json");
	if (!existsSync(hidden)) return true;
	const want: NpmLock = JSON.parse(readFileSync(join(cwd, "package-lock.json"), "utf8"));
	const have: NpmLock = JSON.parse(readFileSync(hidden, "utf8"));
	return lockDiffers(want.packages ?? {}, have.packages ?? {});
}

/** Every non-optional `node_modules/...` entry of the lock is installed at its locked version. */
export function lockDiffers(want: Record<string, NpmLockEntry>, have: Record<string, NpmLockEntry>) {
	return Object.entries(want).some(([path, meta]) => path.startsWith("node_modules/") && !meta.optional && have[path]?.version !== meta.version);
}

/** Install locked dependencies only when npm cannot empty a linked installation. */
export function npmCi(cwd: string, { prefix = ".", env }: { prefix?: string; env?: NodeJS.ProcessEnv } = {}) {
	const root = join(cwd, prefix);
	const manifest = join(root, "package.json");
	const pkg: PackageManifest = existsSync(manifest) ? JSON.parse(readFileSync(manifest, "utf8")) : {};
	const configured = pkg.workspaces;
	const patterns = Array.isArray(configured) ? configured : configured?.packages ?? [];
	const matches = (pattern: string) => globSync(pattern, { cwd: root, exclude: ["**/node_modules/**"] });
	const excluded = new Set(patterns.filter(pattern => pattern.startsWith("!")).flatMap(pattern => matches(pattern.slice(1))));
	const workspaces = patterns.filter(pattern => !pattern.startsWith("!")).flatMap(matches)
		.filter(path => !excluded.has(path) && existsSync(join(root, path, "package.json")));
	for (const path of ["", ...workspaces]) {
		const modules = join(root, path, "node_modules");
		let entry;
		try { entry = lstatSync(modules); } catch (error) { if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error; }
		if (entry?.isSymbolicLink()) {
			const quoted = "'" + modules.replaceAll("'", "'\\''") + "'";
			throw new Error(`${modules} is a symlink to ${readlinkSync(modules)}; remove the link itself (rm -- ${quoted}, no trailing slash), then run npm ci --ignore-scripts in this checkout`);
		}
	}
	sh("npm", ["ci", "--ignore-scripts", "--no-audit", "--no-fund", "--prefix", prefix], { cwd, env });
}
