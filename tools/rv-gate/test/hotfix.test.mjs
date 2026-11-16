// Host reason (ARC-03): exercise hotfix selection and fast-pass admission against real Git trees.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { hotfixSteps } from "../lib/hotfix.mjs";
import { requireSourcePass } from "../lib/hotfix-run.mjs";
import { recordFastPass } from "../lib/fastpass.mjs";

const git = (cwd, ...args) => execFileSync("git", args, { cwd, encoding: "utf8" }).trim();

test("hotfix selects only affected browser groups, and unknown changes fail safe", () => {
	assert.deepEqual(hotfixSteps(["tools/rv-gate/lib/x.mjs"]), ["fast", "build"]);
	assert.deepEqual(hotfixSteps(["assets/site.css"]), ["fast", "build", "verify-site", "site-browser"]);
	assert.deepEqual(hotfixSteps(["crates/richview-site/src/lib.rs"]), ["fast", "build", "report-runtime", "report-visual", "verify-site", "site-browser"]);
	assert.deepEqual(hotfixSteps(["new/path"]), ["fast", "build", "report-runtime", "report-visual", "verify-site", "site-browser"]);
	const toolbar = hotfixSteps(["crates/richview-core/src/figure.rs", "scripts/verify-figure-expand.mjs"]);
	assert.ok(toolbar.includes("script:scripts/verify-figure-expand.mjs"));
	assert.ok(toolbar.includes("script:scripts/verify-chart-keyboard.mjs"));
	assert.equal(toolbar.filter(id => id === "script:scripts/verify-figure-expand.mjs").length, 1);
	assert.ok(hotfixSteps(["scripts/verify-geometry.mjs"]).includes("script:scripts/verify-geometry.mjs"));
	assert.equal(hotfixSteps(["scripts/verify-report-runtime.mjs"]).filter(id => id === "report-runtime").length, 1);
});

test("hotfix source needs both its patch and exact tree to have a fast pass", () => {
	const root = mkdtempSync(join(tmpdir(), "rv-hotfix-test-"));
	const old = process.env.RV_GATE_HOME;
	try {
		process.env.RV_GATE_HOME = join(root, "gate");
		git(root, "init", "-q");
		git(root, "config", "user.email", "test@example.invalid");
		git(root, "config", "user.name", "Test");
		writeFileSync(join(root, "a"), "before\n");
		git(root, "add", "a"); git(root, "commit", "-qm", "base");
		git(root, "update-ref", "refs/remotes/origin/dev", "HEAD");
		writeFileSync(join(root, "a"), "after\n");
		git(root, "commit", "-qam", "change");
		const head = git(root, "rev-parse", "HEAD");
		assert.throws(() => requireSourcePass(root, head), /No fast pass/);
		assert.equal(recordFastPass(root).head, head);
		assert.doesNotThrow(() => requireSourcePass(root, head));
		writeFileSync(join(root, "b"), "another change\n");
		git(root, "add", "b"); git(root, "commit", "-qm", "another");
		assert.throws(() => requireSourcePass(root, "HEAD"), /No fast pass/);
	} finally {
		if (old === undefined) delete process.env.RV_GATE_HOME; else process.env.RV_GATE_HOME = old;
		rmSync(root, { recursive: true, force: true });
	}
});

test("candidate is one tested dev commit on prod and an untested commit cannot be cut", async () => {
	const { mkdirSync } = await import("node:fs");
	const { prepareHotfixCandidate } = await import("../lib/hotfix-run.mjs");
	const root = mkdtempSync(join(tmpdir(), "rv-hotfix-candidate-"));
	const old = process.env.RV_GATE_HOME;
	try {
		process.env.RV_GATE_HOME = join(root, "gate");
		const remote = join(root, "origin.git"), repo = join(root, "repo");
		git(root, "init", "-q", "--bare", remote); mkdirSync(repo);
		git(repo, "init", "-q");
		git(repo, "config", "user.email", "test@example.invalid"); git(repo, "config", "user.name", "Test");
		git(repo, "remote", "add", "origin", remote);
		writeFileSync(join(repo, "a"), "base\n"); git(repo, "add", "a"); git(repo, "commit", "-qm", "base");
		const prod = git(repo, "rev-parse", "HEAD");
		git(repo, "push", "-q", "origin", `${prod}:refs/heads/prod`, `${prod}:refs/heads/dev`);
		git(repo, "fetch", "-q", "origin");
		writeFileSync(join(repo, "a"), "fixed\n"); git(repo, "commit", "-qam", "fix");
		const source = git(repo, "rev-parse", "HEAD");
		assert.equal(recordFastPass(repo).head, source);
		git(repo, "push", "-q", "origin", `${source}:refs/heads/dev`);
		const prepared = prepareHotfixCandidate({ source, cwd: repo });
		assert.equal(prepared.prod, prod); assert.equal(prepared.sourceCommit, source);
		assert.equal(git(repo, "rev-parse", `${prepared.commit}^`), prod);
		assert.equal(git(repo, "ls-remote", "origin", `refs/heads/${prepared.candidate}`).split(/\s+/)[0], prepared.commit);
		git(repo, "checkout", "-q", "--detach", source);
		writeFileSync(join(repo, "a"), "untested\n"); git(repo, "commit", "-qam", "untested");
		const untested = git(repo, "rev-parse", "HEAD");
		git(repo, "push", "-q", "origin", `${untested}:refs/heads/dev`);
		assert.throws(() => prepareHotfixCandidate({ source: untested, cwd: repo }), /No fast pass/);
		assert.equal(git(repo, "ls-remote", "--heads", "origin", "candidate/*").split("\n").length, 1);
	} finally {
		if (old === undefined) delete process.env.RV_GATE_HOME; else process.env.RV_GATE_HOME = old;
		rmSync(root, { recursive: true, force: true });
	}
});

test("a conflicting cherry-pick leaves prod and candidates untouched", async () => {
	const { mkdirSync } = await import("node:fs");
	const { prepareHotfixCandidate } = await import("../lib/hotfix-run.mjs");
	const root = mkdtempSync(join(tmpdir(), "rv-hotfix-conflict-"));
	const old = process.env.RV_GATE_HOME;
	try {
		process.env.RV_GATE_HOME = join(root, "gate");
		const remote = join(root, "origin.git"), repo = join(root, "repo");
		git(root, "init", "-q", "--bare", remote); mkdirSync(repo);
		git(repo, "init", "-q"); git(repo, "config", "user.email", "test@example.invalid"); git(repo, "config", "user.name", "Test");
		git(repo, "remote", "add", "origin", remote);
		writeFileSync(join(repo, "a"), "base\n"); git(repo, "add", "a"); git(repo, "commit", "-qm", "base");
		const base = git(repo, "rev-parse", "HEAD");
		git(repo, "push", "-q", "origin", `${base}:refs/heads/prod`, `${base}:refs/heads/dev`); git(repo, "fetch", "-q", "origin");
		writeFileSync(join(repo, "a"), "dev change\n"); git(repo, "commit", "-qam", "dev");
		const source = git(repo, "rev-parse", "HEAD"); assert.equal(recordFastPass(repo).head, source);
		git(repo, "push", "-q", "origin", `${source}:refs/heads/dev`);
		git(repo, "checkout", "-q", "--detach", base);
		writeFileSync(join(repo, "a"), "prod change\n"); git(repo, "commit", "-qam", "prod");
		const prod = git(repo, "rev-parse", "HEAD"); git(repo, "push", "-q", "origin", `${prod}:refs/heads/prod`);
		assert.throws(() => prepareHotfixCandidate({ source, cwd: repo }), /does not cherry-pick cleanly/);
		assert.equal(git(repo, "rev-parse", "HEAD"), prod);
		assert.equal(git(repo, "status", "--porcelain"), "");
		assert.equal(git(repo, "ls-remote", "--heads", "origin", "candidate/*"), "");
	} finally {
		if (old === undefined) delete process.env.RV_GATE_HOME; else process.env.RV_GATE_HOME = old;
		rmSync(root, { recursive: true, force: true });
	}
});
