// Host reason (ARC-03): verify that only a tree-preserving hotfix back-merge bypasses a new fast run.
import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { PRE_PUSH } from "../lib/hooks.mjs";

const git = (cwd, ...args) => execFileSync("git", args, { cwd, encoding: "utf8" }).trim();

test("the hotfix marker accepts only the exact prod second parent and unchanged dev tree", () => {
	const root = mkdtempSync(join(tmpdir(), "rv-hotfix-hook-"));
	try {
		git(root, "init", "-q");
		git(root, "config", "user.email", "test@example.invalid"); git(root, "config", "user.name", "Test");
		writeFileSync(join(root, "a"), "base\n"); git(root, "add", "a"); git(root, "commit", "-qm", "base");
		const base = git(root, "rev-parse", "HEAD");
		writeFileSync(join(root, "a"), "dev\n"); git(root, "commit", "-qam", "dev");
		const dev = git(root, "rev-parse", "HEAD"), devTree = git(root, "rev-parse", "HEAD^{tree}");
		git(root, "checkout", "-q", "--detach", base);
		writeFileSync(join(root, "a"), "prod\n"); git(root, "commit", "-qam", "prod");
		const prod = git(root, "rev-parse", "HEAD"), prodTree = git(root, "rev-parse", "HEAD^{tree}");
		git(root, "update-ref", "refs/remotes/origin/dev", dev); git(root, "update-ref", "refs/remotes/origin/prod", prod);
		const merge = (tree, ...parents) => git(root, "commit-tree", tree, ...parents.flatMap(parent => ["-p", parent]), "-m", "merge");
		const hook = join(root, "pre-push"); writeFileSync(hook, PRE_PUSH);
		const check = (commit, marker) => spawnSync("sh", [hook], { cwd: root, input: `refs/heads/backmerge ${commit} refs/heads/dev ${dev}\n`, env: { ...process.env, RV_GATE_HOME: join(root, "gate"), RV_GATE_PUSH: marker }, encoding: "utf8" });
		const same = merge(devTree, dev, prod);
		assert.equal(check(same, "hotfix-backmerge").status, 0);
		assert.notEqual(check(same, "").status, 0);
		assert.notEqual(check(merge(prodTree, dev, prod), "hotfix-backmerge").status, 0);
		assert.notEqual(check(merge(devTree, prod, dev), "hotfix-backmerge").status, 0);
		const other = git(root, "commit-tree", prodTree, "-p", base, "-m", "another prod");
		assert.notEqual(check(merge(devTree, dev, other), "hotfix-backmerge").status, 0);
		assert.notEqual(check(merge(devTree, dev, prod, base), "hotfix-backmerge").status, 0);
	} finally { rmSync(root, { recursive: true, force: true }); }
});
