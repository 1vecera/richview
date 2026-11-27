// Impact tracing for the fast gate and the release run (DEL-81): which unit tests a change can
// affect. A Rust crate is affected when a file it compiles changed, or when a workspace crate it
// depends on is affected; a JavaScript tool when a file under its folder changed. Tests a change
// cannot affect do not run.
//
// Pure functions over the workspace graph, the include macros and the changed paths, so the
// tests can drive them without Cargo or Git.

import { posix } from "node:path";
import type { GateCommand } from "./steps.mts";

interface CargoTarget { name: string; kind: string[]; src_path: string; test?: boolean }
interface CargoPackage {
	name: string;
	manifest_path: string;
	dependencies: { name: string }[];
	targets: CargoTarget[];
}

export interface CargoMetadata { packages: CargoPackage[] }
export interface WorkspaceCrate {
	name: string;
	dir: string;
	deps: string[];
	targets: { name: string; kind: "bin" | "lib"; src: string }[];
}

export type IncludeInput = [file: string, literal: string];
interface ToolSuite { id: string; paths: string[]; needs?: string; run: GateCommand }
interface IntegrationTarget { crate: string; test: string; filters?: string[] }

/** Files whose change can alter every crate's build. */
export const WORKSPACE_FILES = ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo/"];

/** Check the whole TypeScript project: an edit can change types in distant consumers. */
export function affectsTypeScript(changed: readonly string[]) {
	return changed.some((path) => /\.[cm]?[jt]sx?$/.test(path) || /(?:^|\/)(?:tsconfig(?:\.[^/]+)?|package(?:-lock)?)\.json$/.test(path));
}

/**
 * JavaScript tools with unit tests, by the folders they own. `scripts/verify-*` are browser
 * suites and run at release, not here.
 */
export const JS_SUITES: ToolSuite[] = [
	// The golden and a11y tests drive Playwright, and a11y reads axe-core: the root packages.
	// The golden tests read the spec's chart types and fence tags against examples/every-block.
	{ id: "rv-gate", paths: ["tools/rv-gate/", "scripts/preview-recovery.ts", "docs/RMD-SPEC.md", "examples/every-block/", "docs/public/", "packages/vscode/snippets/", ".github/"], needs: "node_modules", run: { cmd: "node", args: ["--test", "tools/rv-gate/test/*.test.m?s"] } },
	{ id: "agent-use", paths: ["tools/agent-use/"], run: { cmd: "node", args: ["--test", "tools/agent-use/test/*.test.m?s"] } },
	{ id: "rv-release", paths: ["tools/rv-release/"], run: { cmd: "node", args: ["--test", "tools/rv-release/test/*.test.m?s"] } },
	{ id: "lockdown-app", paths: ["tools/lockdown-app/"], run: { cmd: "node", args: ["--test", "tools/lockdown-app/test/*.test.m?s"] } },
	{ id: "dependency-rules", paths: ["tools/dependency-rules/", "tools/rv-gate/lib/layers.mts"], run: { cmd: "node", args: ["--test", "tools/dependency-rules/test/*.test.m?s"] } },
	{ id: "vscode", paths: ["packages/vscode/", "scripts/vscode-notices.ts"], run: { cmd: "node", args: ["--test", "packages/vscode/test/*.test.cts"] } },
	// Keep current dependent Rust checks; these registered producers/consumers also verify the
	// accepted source interfaces. The extra gate test uses a fresh report, never a saved pass.
	{ id: "api-contracts", paths: ["tools/api-contracts/", "tools/dependency-rules/", "tools/rv-gate/lib/layers.mts", "richview-chart/", "crates/richview-core/", "crates/richview-runtime/", "crates/richview-app/", "crates/richview-build/"], needs: "node_modules", run: { cmd: "node", args: ["--test", "tools/api-contracts/test/*.test.m?s", "tools/rv-gate/test/interfaces.test.mts"] } },
	{
		id: "publish-worker",
		paths: ["cloudflare/publish/"],
		needs: "cloudflare/publish/node_modules",
		run: { cmd: "node", args: ["--test", "cloudflare/publish/test/*.test.ts"] },
	},
];

/**
 * The workspace from `cargo metadata --no-deps`: each member's folder relative to `root`, its
 * targets and the members it depends on (normal, dev and build).
 */
export function workspaceGraph(metadata: CargoMetadata, root: string): WorkspaceCrate[] {
	const prefix = root.endsWith("/") ? root : `${root}/`;
	const members = new Set(metadata.packages.map((pkg) => pkg.name));
	return metadata.packages.map((pkg) => ({
		name: pkg.name,
		dir: pkg.manifest_path.slice(prefix.length).replace(/\/?Cargo\.toml$/, ""),
		deps: [...new Set(pkg.dependencies.filter((dep) => members.has(dep.name)).map((dep) => dep.name))],
		targets: pkg.targets
			.filter((target) => target.kind.some((kind) => ["lib", "rlib", "cdylib", "bin"].includes(kind)))
			.map((target) => ({
				name: target.name,
				kind: target.kind.includes("bin") ? "bin" : "lib",
				src: target.src_path.slice(prefix.length),
			})),
	}));
}

/** The member whose folder holds `file`, the deepest when folders nest. */
export function crateOf(file: string, graph: readonly WorkspaceCrate[]) {
	let best: WorkspaceCrate | undefined;
	let depth = -1;
	for (const crate of graph) {
		const dir = crate.dir ? `${crate.dir}/` : "";
		if (file.startsWith(dir) && dir.length > depth) {
			best = crate;
			depth = dir.length;
		}
	}
	return best;
}

/**
 * Files outside each member's folder that it compiles in with `include_str!` or `include_bytes!`.
 * `includes` holds `[file, literal]` pairs: the Rust source and the path written in the macro,
 * which Rust resolves against that source's folder.
 */
export function includeInputs(includes: readonly IncludeInput[], graph: readonly WorkspaceCrate[]) {
	const inputs = new Map(graph.map((crate) => [crate.name, new Set<string>()]));
	for (const [file, literal] of includes) {
		const crate = crateOf(file, graph);
		if (!crate) continue;
		const path = posix.normalize(posix.join(posix.dirname(file), literal));
		if (!path.startsWith("../") && !path.startsWith(`${crate.dir}/`)) inputs.get(crate.name)!.add(path);
	}
	return inputs;
}

/** Members that depend on any of `names`, directly or through other members, plus `names`. */
export function withDependents(names: Iterable<string>, graph: readonly WorkspaceCrate[]) {
	const result = new Set(names);
	let grew = true;
	while (grew) {
		grew = false;
		for (const crate of graph) {
			if (!result.has(crate.name) && crate.deps.some((dep) => result.has(dep))) {
				result.add(crate.name);
				grew = true;
			}
		}
	}
	return result;
}

/** Members that `names` build against, directly or through other members, plus `names`. */
export function withDependencies(names: Iterable<string>, graph: readonly WorkspaceCrate[]) {
	const byName = new Map(graph.map((crate) => [crate.name, crate]));
	const result = new Set<string>();
	const visit = (name: string) => {
		if (result.has(name)) return;
		result.add(name);
		for (const dep of byName.get(name)?.deps ?? []) visit(dep);
	};
	for (const name of names) visit(name);
	return result;
}

/**
 * What a change affects.
 *
 * - `changed`: repository-relative paths that differ from the base, including deleted ones.
 * - `inputs`: `includeInputs` for the members.
 *
 * Returns the members whose own files changed (`direct`), those plus their dependents (`crates`),
 * the JavaScript suites to run, and the changed files no test traces (`untraced`), which the
 * release run covers.
 */
export function affected(changed: readonly string[], graph: readonly WorkspaceCrate[], inputs: ReadonlyMap<string, ReadonlySet<string>> = new Map()) {
	const direct = new Set<string>();
	const suites = new Set<string>();
	const untraced = [];
	let everything = false;
	for (const file of changed) {
		let traced = false;
		// ARC-01: dependency direction is checked whenever a workspace manifest changes.
		if (file === "Cargo.toml" || file.endsWith("/Cargo.toml")) {
			suites.add("rv-gate");
			traced = true;
		}
		if (WORKSPACE_FILES.some((path) => (path.endsWith("/") ? file.startsWith(path) : file === path))) {
			everything = true;
			traced = true;
		}
		const crate = crateOf(file, graph);
		if (crate) {
			direct.add(crate.name);
			traced = true;
		}
		for (const [name, files] of inputs) {
			if (files.has(file)) {
				direct.add(name);
				traced = true;
			}
		}
		for (const suite of JS_SUITES) {
			if (suite.paths.some((path) => file.startsWith(path))) {
				suites.add(suite.id);
				traced = true;
			}
		}
		if (!traced) untraced.push(file);
	}
	if (everything) for (const crate of graph) direct.add(crate.name);
	return { direct, crates: withDependents(direct, graph), suites, untraced };
}

/**
 * The Cargo test invocations for `crates`: unit tests only, and only targets whose sources hold
 * a test. `testFiles` is the set of `.rs` files containing `#[test]`.
 *
 * A library's unit tests live anywhere in its folder outside `src/main.rs` and `src/bin/`; a
 * binary's in its own source file or its folder under `src/bin/`.
 */
export function unitTargets(crates: ReadonlySet<string>, graph: readonly WorkspaceCrate[], testFiles: ReadonlySet<string>) {
	const libs = [];
	const bins = [];
	for (const crate of graph) {
		if (!crates.has(crate.name)) continue;
		const own = [...testFiles].filter((file) => file.startsWith(`${crate.dir}/`));
		const binSources = crate.targets.filter((target) => target.kind === "bin").map((target) => target.src);
		const binOwned = (file: string) =>
			binSources.some((src) => file === src || (src.includes("/src/bin/") && file.startsWith(src.replace(/\/main\.rs$|\.rs$/, "/"))));
		const lib = crate.targets.find((target) => target.kind === "lib");
		if (lib && own.some((file) => !binOwned(file) && !file.includes("/tests/") && !file.includes("/benches/") && !file.includes("/examples/")))
			libs.push(crate.name);
		for (const target of crate.targets.filter((t) => t.kind === "bin")) {
			if (own.some((file) => file === target.src || (target.src.includes("/src/bin/") && file.startsWith(target.src.replace(/\/main\.rs$|\.rs$/, "/")))))
				bins.push({ crate: crate.name, name: target.name });
		}
	}
	return { libs, bins };
}

/**
 * Literals that name an untraced file in a test's source: the file's path, and its folder when
 * the folder is specific (two segments or more). Integration tests read assets, fixtures and docs
 * at run time by path, which `include_str!` tracing cannot see (palettes.rs reads assets/showcase).
 */
export function readerLiterals(untraced: readonly string[]) {
	const literals = new Set<string>();
	for (const file of untraced) {
		literals.add(file);
		const dir = posix.dirname(file);
		if (dir.split("/").length >= 2) literals.add(dir);
	}
	return [...literals];
}

/** The one integration test binary of a crate whose `tests/<name>.rs` files are modules of it. */
export const MERGED_TESTS = "integration";

/**
 * Integration test targets `{crate, test}` for test files (`<crate dir>/tests/<name>.rs`). A file
 * under `tests/` that is not a top-level test file (`tests/common/mod.rs`, a fixture) may be used
 * by any of them, so every integration test of that crate runs; `listTests(crate)` names them.
 * A crate named in `merged` has one test binary, `integration`, with each file a module of it: a
 * top-level file then selects that binary with `filters: [<name>]` (its tests are `<name>::...`),
 * and any other file under `tests/`, or a change to the binary's own root, selects all of it.
 */
export function testTargets(files: readonly string[], graph: readonly WorkspaceCrate[], { listTests = () => [], merged = new Set() }: {
	listTests?: (crate: WorkspaceCrate) => readonly string[];
	merged?: ReadonlySet<string>;
} = {}) {
	const targets: IntegrationTarget[] = [];
	const add = (crate: string, test: string, filter?: string) => {
		const found = targets.find((target) => target.crate === crate && target.test === test);
		if (!found) targets.push(filter === undefined ? { crate, test } : { crate, test, filters: [filter] });
		else if (filter === undefined) delete found.filters;
		else if (found.filters && !found.filters.includes(filter)) found.filters.push(filter);
	};
	for (const file of files) {
		const crate = crateOf(file, graph);
		if (!crate) continue;
		const rel = file.slice(crate.dir.length + 1);
		if (!rel.startsWith("tests/")) continue;
		const match = rel.match(/^tests\/([^/]+)\.rs$/);
		if (merged.has(crate.name)) add(crate.name, MERGED_TESTS, match && match[1] !== MERGED_TESTS ? match[1] : undefined);
		else if (match) add(crate.name, match[1]);
		else for (const test of listTests(crate)) add(crate.name, test);
	}
	return targets;
}
