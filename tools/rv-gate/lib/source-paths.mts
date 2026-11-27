// Host reason (ARC-03): preserve check identity when its source moves to TypeScript.

export const REGISTRY_SOURCES = ["tools/rv-gate/lib/steps.mts", "tools/rv-gate/lib/steps.mjs"] as const;

/** TypeScript successors of a JavaScript source, preserving its module kind. */
export function typeScriptPaths(path: string): string[] {
	if (path.endsWith(".mjs")) return [path.slice(0, -4) + ".mts", path.slice(0, -4) + ".ts"];
	if (path.endsWith(".cjs")) return [path.slice(0, -4) + ".cts"];
	if (path.endsWith(".js")) return [path.slice(0, -3) + ".ts"];
	return [];
}

/** Keep existing command receipt IDs when a tool or verification script changes extension. */
export function receiptSource(path: string): string {
	if (path.endsWith(".mts")) return path.slice(0, -4) + ".mjs";
	if (path.endsWith(".cts")) return path.slice(0, -4) + ".cjs";
	if (path.startsWith("scripts/") && path.endsWith(".ts")) return path.slice(0, -3) + ".mjs";
	return path;
}
