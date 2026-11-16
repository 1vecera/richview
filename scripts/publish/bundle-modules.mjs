import { readdir } from "node:fs/promises";
import { join } from "node:path";

/** Miniflare module list for a `wrangler deploy --dry-run` bundle: the entry and its font Data modules. */
export async function bundleModules(directory) {
	const fonts = (await readdir(directory)).filter((name) => name.endsWith(".woff2"));
	return {
		modulesRoot: directory,
		modules: [
			{ type: "ESModule", path: join(directory, "entry.js") },
			...fonts.map((name) => ({ type: "Data", path: join(directory, name) })),
		],
	};
}
