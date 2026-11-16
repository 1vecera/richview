// Minimal glob matching for repository paths: `*` matches within one segment, `**/` any depth.

import { readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";

export function globToRegExp(glob) {
	let source = "";
	for (let i = 0; i < glob.length; i++) {
		const char = glob[i];
		if (char === "*" && glob[i + 1] === "*") {
			source += glob[i + 2] === "/" ? "(?:.*/)?" : ".*";
			i += glob[i + 2] === "/" ? 2 : 1;
		} else if (char === "*") source += "[^/]*";
		else if (char === "?") source += "[^/]";
		else source += char.replace(/[.+^${}()|[\]\\]/g, "\\$&");
	}
	return new RegExp(`^${source}$`);
}

export function matchesAny(path, globs) {
	return globs.some((glob) => globToRegExp(glob).test(path));
}

/** List files under `root` (relative paths), skipping VCS, build and dependency folders. */
export function listFiles(root, dir = "", skip = new Set([".git", "node_modules", "target", "dist", "tmp", ".claude"])) {
	const out = [];
	for (const name of readdirSync(join(root, dir))) {
		if (skip.has(name)) continue;
		const rel = dir ? `${dir}/${name}` : name;
		const stat = statSync(join(root, rel), { throwIfNoEntry: false });
		if (!stat) continue;
		if (stat.isDirectory()) out.push(...listFiles(root, rel, skip));
		else out.push(rel);
	}
	return out;
}

export function rel(root, path) {
	return relative(root, path).split("\\").join("/");
}
