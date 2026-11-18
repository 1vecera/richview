// Dependency gate, npm advisories (ARC-68): `npm audit` of each package-lock.json must report no
// security advisory, except one listed in ACCEPTED below with the reason it cannot reach a RichView
// user. Crates get the same check from `cargo deny` (deny.toml). The audit asks the npm registry, so
// a failed audit fails the step rather than passing it unchecked.
//
// Usage: node tools/rv-gate/npm-advisories.mjs [package-lock.json ...]   (default: the repo's two locks)

import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

/**
 * Advisories that stay, each with the package it is filed against, why it does not apply and when it
 * goes. An earlier entry, GHSA-xcx6-jx8p-x3h5 (MSAL Node, reached through @vscode/vsce's
 * @azure/identity), went when @vscode/vsce 4.1.0 pulled @azure/identity 4.14.1, which accepts
 * @azure/msal-node 7.x, and the override and direct devDependency that forced 7.1.2 were dropped
 * (S27-807).
 *
 * The root package.json also overrides minimatch to ^11.0.3: GHSA-g3hv-75wg-m249 is fixed only from
 * 11.0.1, and @vscode/vsce (^10) and vscode-languageclient (^5) ask for older ranges. Both use the
 * named `minimatch`/`Minimatch` exports that every version has. `npm ls` therefore reports those
 * edges as invalid (ELSPROBLEMS); `npm ci`, the licence check, this audit and `vsce package` pass, and
 * nothing in the gate runs `npm ls`. Drop the override once both ask for 11 or later.
 */
export const ACCEPTED = [
	{
		ghsa: "GHSA-fc5q-39xc-vjpf",
		package: "picomatch",
		reason:
			"ReDoS on crafted extglobs, in every picomatch release (<=4.0.7, no patched version). It is a dev " +
			"dependency reached only through @vscode/vsce -> tinyglobby, which matches the extension's own " +
			"file list with the repository's own patterns when `vsce package` builds the VSIX on a maintainer's " +
			"machine. No user input reaches it, and it is in neither the extension bundle nor any other RichView " +
			"artifact. Every @vscode/vsce 4.x uses tinyglobby, and 3.9.2 reaches picomatch too (secretlint -> " +
			"globby -> fast-glob -> micromatch) and brings GHSA-3g3m-2fjr-6pw4 back.",
		drop: "when picomatch ships a fix (upstream PRs #238 and #254 were open on 17 Nov 2026)",
	},
];

const ghsaOf = (url) => String(url ?? "").match(/GHSA-[0-9a-z]{4}-[0-9a-z]{4}-[0-9a-z]{4}/)?.[0] ?? null;

/** Every advisory a parsed `npm audit --json` report names, once each. */
export function advisoriesIn(report) {
	const seen = new Map();
	for (const vulnerability of Object.values(report.vulnerabilities ?? {})) {
		for (const via of vulnerability.via ?? []) {
			if (typeof via !== "object" || via === null) continue; // a package name: the advisory is listed under it
			const ghsa = ghsaOf(via.url);
			const key = ghsa ?? `${via.name}:${via.title}`;
			if (!seen.has(key)) seen.set(key, { ghsa, package: via.name, severity: via.severity, title: via.title, range: via.range });
		}
	}
	return [...seen.values()];
}

/** Advisories in a report that ACCEPTED does not cover, as readable strings. */
export function checkReport(report, accepted = ACCEPTED) {
	if (report.error) return [`npm audit failed: ${report.error.summary ?? report.error.code ?? JSON.stringify(report.error)}`];
	return advisoriesIn(report)
		.filter((advisory) => !accepted.some((rule) => rule.ghsa === advisory.ghsa && rule.package === advisory.package))
		.map((advisory) => `${advisory.package} ${advisory.range}: ${advisory.severity} ${advisory.ghsa ?? "(no GHSA id)"} ${advisory.title}`);
}

function main(argv) {
	const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
	const files = argv.length ? argv : [join(root, "package-lock.json"), join(root, "cloudflare/publish/package-lock.json")];
	let failed = false;
	for (const file of files) {
		if (!existsSync(file)) {
			console.error(`${file}: not found`);
			failed = true;
			continue;
		}
		const run = spawnSync("npm", ["audit", "--json", "--package-lock-only"], { cwd: dirname(file), encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
		let report;
		try {
			report = JSON.parse(run.stdout);
		} catch {
			report = { error: { summary: (run.stderr || run.error?.message || "no JSON from npm audit").trim().split("\n")[0] } };
		}
		const problems = checkReport(report);
		if (problems.length) failed = true;
		console.log(`${file}: ${problems.length} advisory problem(s)`);
		for (const problem of problems) console.log(`  ${problem}`);
	}
	return failed ? 1 : 0;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) process.exit(main(process.argv.slice(2)));
