import { readFile } from "node:fs/promises";

export const browserScripts = new Map([
	["app.js", "app.ts"],
	["dashboard.js", "dashboard.ts"],
	["journal.js", "journal.ts"],
]);

// These scripts share the browser's global scope. Compile them with the pinned compiler at
// the serving boundary, so the repository and the app's working tree need no generated files.
export async function compileBrowserScript(file: string): Promise<string> {
	const [source, { default: ts }] = await Promise.all([readFile(file, "utf8"), import("typescript")]);
	return ts.transpileModule(source, {
		fileName: file,
		compilerOptions: {
			target: ts.ScriptTarget.ES2024,
			module: ts.ModuleKind.None,
			moduleDetection: ts.ModuleDetectionKind.Legacy,
		},
	}).outputText;
}
