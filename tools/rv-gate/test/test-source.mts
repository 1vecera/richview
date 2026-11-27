import { readFileSync } from "node:fs";
import { stripTypeScriptTypes } from "node:module";

/** Read an implementation or a fixture with its TypeScript syntax stripped for VM tests. */
export function readTestSource(file: string | URL): string {
	const source = readFileSync(file, "utf8");
	const path = typeof file === "string" ? file : file.pathname;
	return /\.[cm]?ts$/.test(path) ? stripTypeScriptTypes(source) : source;
}
