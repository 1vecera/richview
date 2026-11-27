import { readdir } from "node:fs/promises";
import { join } from "node:path";

/** Miniflare module list for a `wrangler deploy --dry-run` bundle: the entry, fonts and compiled Wasm. */
export async function bundleModules(directory: string) {
	const files = await readdir(directory);
	const fonts = files.filter((name) => name.endsWith(".woff2"));
	const wasm = files.filter((name) => name.endsWith(".wasm"));
	return {
		modulesRoot: directory,
		modules: [
			{ type: "ESModule" as const, path: join(directory, "entry.js") },
			...fonts.map((name) => ({ type: "Data" as const, path: join(directory, name) })),
			...wasm.map((name) => ({ type: "CompiledWasm" as const, path: join(directory, name) })),
		],
	};
}
