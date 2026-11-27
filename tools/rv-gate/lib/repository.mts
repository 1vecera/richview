// Host reason (ARC-03): resolve the GitHub repository used by gate PR operations from Git.
import { spawnSync } from "node:child_process";

/** Resolve the gate's GitHub repository from its override or rewritten origin URL. */
export function gateRepository({ cwd, env = process.env }: { cwd: string; env?: NodeJS.ProcessEnv }) {
	const override = env.RV_GATE_REPO;
	let repository;
	if (override !== undefined) {
		repository = override;
	} else {
		const result = spawnSync("git", ["remote", "get-url", "origin"], { cwd, env, encoding: "utf8" });
		const remote = result.status === 0 ? result.stdout.trim() : "";
		const match = /^(?:git@github\.com:|ssh:\/\/git@github\.com\/|https:\/\/(?:[^/@\s]+@)?github\.com\/)([^/\s]+\/[^/\s]+?)\/?$/i.exec(remote);
		repository = match?.[1].replace(/\.git$/i, "");
	}
	if (repository === undefined || !/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repository)) {
		throw new Error("Cannot determine the GitHub repository from origin; set RV_GATE_REPO to owner/name before running PR operations.");
	}
	return repository;
}
