// Dependency gate, npm side (ARC-68): every package in each package-lock.json must carry a licence
// on the allowlist that deny.toml uses for crates. Built-ins only. An SPDX `OR` passes when any
// alternative is allowed, `AND` when all are. Packages outside the allowlist pass only through
// DEV_ONLY below, which covers tooling that never ships in a RichView artifact and names why.
//
// Usage: node tools/rv-gate/npm-licences.mts [package-lock.json ...]   (default: the repo's two locks)

import { existsSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import type { NpmLock } from "./lib/npm.mts";

// BlueOak-1.0.0 joined on 1 Nov 2026 (lead ruling on ARC-68): permissive and OSI-approved, which is
// the card's test. The VS Code extension bundles minimatch, so its third-party notices carry the text
// (scripts/vscode-notices.ts).
export const ALLOWED = ["MIT", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Zlib", "Unicode-3.0", "BlueOak-1.0.0"];

/** Licences outside the allowlist, per package (a trailing `*` matches a name prefix). Dev-only. */
export const DEV_ONLY = [
	{
		license: "Artistic-2.0",
		packages: ["binaryextensions", "editions", "istextorbinary", "textextensions", "version-range"],
		reason: "Permissive OSI licence; the file-type helpers of the secretlint scan inside vsce, which packages the extension and never ships in it.",
	},
	{ license: "CC0-1.0", packages: ["@speed-highlight/core"], reason: "Public-domain dedication; syntax highlighter of a dev tool." },
	{ license: "Python-2.0", packages: ["argparse"], reason: "Permissive PSF licence; command-line parsing in dev tooling." },
	{ license: "0BSD", packages: ["tslib"], reason: "Permissive zero-clause BSD; TypeScript helper of dev tooling." },
	{
		license: "SEE LICENSE IN LICENSE.txt",
		packages: ["@vscode/vsce-sign", "@vscode/vsce-sign-*"],
		reason: "Microsoft's proprietary signing binary used only by vsce to package the VS Code extension; never bundled into it.",
	},
	{
		license: "LGPL-3.0-or-later",
		packages: ["@img/sharp-libvips-*"],
		reason: "Prebuilt libvips of sharp, an optional image dependency of wrangler; dev machines only, never distributed.",
	},
	{
		license: "Apache-2.0 AND LGPL-3.0-or-later AND MIT",
		packages: ["@img/sharp-wasm32"],
		reason: "Same sharp build as above (WASM flavour); dev-only, never distributed.",
	},
	{
		license: "MPL-2.0",
		packages: ["axe-core"],
		reason: "Accessibility audit engine of the a11y gate step; test tooling only, never bundled or distributed.",
	},
	{
		license: "Apache-2.0 AND LGPL-3.0-or-later",
		packages: ["@img/sharp-win32-*"],
		reason: "Same sharp build as above (Windows flavour); dev-only, never distributed.",
	},
];

/** Whether an SPDX expression is satisfied by the allowlist. Unparseable input is not. */
export function satisfies(expression: unknown, allowed: readonly string[] = ALLOWED) {
	const tokens = String(expression ?? "").match(/\(|\)|[^\s()]+/g) ?? [];
	let at = 0;
	const parseOr = (): boolean => {
		let ok = parseAnd();
		while (tokens[at] === "OR") {
			at += 1;
			const next = parseAnd();
			ok = ok || next;
		}
		return ok;
	};
	const parseAnd = () => {
		let ok = parseTerm();
		while (tokens[at] === "AND") {
			at += 1;
			const next = parseTerm();
			ok = ok && next;
		}
		return ok;
	};
	const parseTerm = () => {
		const token = tokens[at++];
		if (token === "(") {
			const ok = parseOr();
			if (tokens[at++] !== ")") return false;
			return ok;
		}
		return token !== undefined && token !== ")" && allowed.includes(token);
	};
	if (!tokens.length) return false;
	const ok = parseOr();
	return ok && at === tokens.length;
}

const nameOf = (path: string) => path.slice(path.lastIndexOf("node_modules/") + "node_modules/".length);

function matches(pattern: string, name: string) {
	return pattern.endsWith("*") ? name.startsWith(pattern.slice(0, -1)) : pattern === name;
}

/** Problems found in one parsed package-lock.json, as readable strings. */
export function checkLock(lock: NpmLock, { allowed = ALLOWED, devOnly = DEV_ONLY } = {}) {
	const problems = [];
	for (const [path, entry] of Object.entries(lock.packages ?? {})) {
		if (!path || entry.link) continue; // the root and workspace links; the linked package has its own entry
		const name = path.includes("node_modules/") ? nameOf(path) : path;
		if (satisfies(entry.license, allowed)) continue;
		const exception = devOnly.find((rule) => rule.license === entry.license && rule.packages.some((pattern) => matches(pattern, name)));
		if (exception && entry.dev) continue;
		const why = exception ? "is allowed only as a dev dependency" : "is not allowed";
		problems.push(`${name}@${entry.version ?? "?"}: licence ${JSON.stringify(entry.license ?? null)} ${why}`);
	}
	return problems;
}

function main(argv: string[]) {
	const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
	const files = argv.length ? argv : [join(root, "package-lock.json"), join(root, "cloudflare/publish/package-lock.json")];
	let failed = false;
	for (const file of files) {
		if (!existsSync(file)) {
			console.error(`${file}: not found`);
			failed = true;
			continue;
		}
		const lock: NpmLock = JSON.parse(readFileSync(file, "utf8"));
		const problems = checkLock(lock);
		const count = Object.keys(lock.packages ?? {}).length - 1;
		if (problems.length) failed = true;
		console.log(`${file}: ${count} packages, ${problems.length} problem(s)`);
		for (const problem of problems) console.log(`  ${problem}`);
	}
	return failed ? 1 : 0;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) process.exit(main(process.argv.slice(2)));
