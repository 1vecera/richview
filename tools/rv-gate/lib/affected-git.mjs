// Host reason (ARC-03): derive affected coverage inputs from immutable Git trees.
import {execFileSync, spawnSync} from "node:child_process";
import {includeInputs} from "./impact.mjs";
import {runtimeInventory} from "./case-selection.mjs";

const git = (cwd, ...args) => execFileSync("git", args, {cwd, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"], maxBuffer: 64 * 1024 * 1024}).trim();
const lines0 = value => value.split("\0").filter(Boolean);

/** Keep each commit's paths, including changes later reverted and both sides of merges. */
export function changedCommits(cwd, base, head) {
	const ancestor = spawnSync("git", ["merge-base", "--is-ancestor", base, head], {cwd, encoding: "utf8"});
	if (ancestor.status !== 0) throw new Error("Affected base is not an ancestor of the source head");
	const commits = git(cwd, "rev-list", "--reverse", "--topo-order", `${base}..${head}`).split("\n").filter(Boolean);
	return commits.map(commit => ({commit, paths: [...new Set(lines0(git(cwd, "diff-tree", "--root", "-m", "--no-commit-id", "--name-only", "--no-renames", "-r", "-z", commit)))]}));
}

/** Read a repository-relative file at one commit, without checking out or editing another tree. */
export function treeReader(cwd, head) {
	const cache = new Map();
	return path => {
		if (typeof path !== "string" || !path || path.startsWith("/") || path.includes("\0") || path.split("/").includes("..")) throw new Error("Invalid tree input path");
		if (cache.has(path)) return cache.get(path);
		const result = spawnSync("git", ["show", `${head}:${path}`], {cwd, encoding: "utf8", maxBuffer: 64 * 1024 * 1024});
		if (result.error) throw result.error;
		const value = result.status === 0 ? result.stdout : null;
		cache.set(path, value); return value;
	};
}

/** The crate folders and literal include inputs in a head tree; Rust checks stay whole-workspace. */
export function sourceInputs(cwd, head, read = treeReader(cwd, head)) {
	const files = lines0(git(cwd, "ls-tree", "-r", "--name-only", "-z", head));
	const manifests = files.filter(path => path.endsWith("/Cargo.toml"));
	const graph = manifests.map(path => {
		const manifest = read(path), packageText = /^\[package\]\s*([\s\S]*?)(?=^\[|$(?![\s\S]))/m.exec(manifest ?? "")?.[1];
		const name = /^name\s*=\s*"([^"]+)"/m.exec(packageText ?? "")?.[1];
		if (!name) throw new Error(`Cannot identify workspace package ${path}`);
		return {name, dir: path.slice(0, -"/Cargo.toml".length), deps: [], targets: []};
	});
	if (!graph.length || new Set(graph.map(crate => crate.name)).size !== graph.length) throw new Error("Cannot find a unique set of workspace crate manifests in the source tree");
	const search = spawnSync("git", ["grep", "-l", "-z", "-E", "include_(str|bytes)!\\(", head, "--", "*.rs"], {cwd, encoding: "utf8", maxBuffer: 64 * 1024 * 1024});
	if (search.error || ![0, 1].includes(search.status)) throw new Error(`Cannot trace Rust includes: ${search.error?.message ?? search.stderr}`);
	const includes = [];
	for (const entry of lines0(search.stdout)) {
		const prefix = `${head}:`;
		if (!entry.startsWith(prefix)) throw new Error("Unexpected Git include search output");
		const path = entry.slice(prefix.length), text = read(path);
		if (text === null) throw new Error(`Cannot read Rust include source ${path}`);
		for (const match of text.matchAll(/include_(?:str|bytes)!\(\s*"([^"]+)"/g)) includes.push([path, match[1]]);
	}
	return {graph, inputs: includeInputs(includes, graph), runtimeCases: runtimeInventory(files)};
}
