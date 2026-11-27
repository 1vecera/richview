// Host reason (ARC-03): exercise Git's remote URL rewriting before gate PR operations.
import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";
import { gateRepository } from "../lib/repository.mts";

test("PR repository follows origin, respects Git rewrites and refuses unknown destinations", t => {
	mkdirSync("tmp", { recursive: true });
	const cwd = mkdtempSync(join(process.cwd(), "tmp/gate-repository-"));
	t.after(() => rmSync(cwd, { recursive: true, force: true }));
	const env: NodeJS.ProcessEnv = { ...process.env, GIT_CONFIG_GLOBAL: join(cwd, "global"), GIT_CONFIG_NOSYSTEM: "1" };
	delete env.RV_GATE_REPO;
	const git = (...args: string[]) => execFileSync("git", args, { cwd, env, encoding: "utf8", stdio: "pipe" });
	git("init", "-q");
	git("remote", "add", "origin", "git@github.com:1vecera/richview.git");
	assert.equal(gateRepository({ cwd, env }), "1vecera/richview");
	git("remote", "set-url", "origin", "https://github.com/1vecera/richview.git");
	assert.equal(gateRepository({ cwd, env }), "1vecera/richview");
	assert.equal(gateRepository({ cwd, env: { ...env, RV_GATE_REPO: "other/private" } }), "other/private");
	git("config", "--global", "url.git@github.com:1vecera/richview-private.git.insteadOf", "https://github.com/1vecera/richview.git");
	assert.equal(gateRepository({ cwd, env }), "1vecera/richview-private");
	git("remote", "set-url", "origin", "/tmp/unrelated.git");
	assert.throws(() => gateRepository({ cwd, env }), /Cannot determine.*origin.*RV_GATE_REPO/);
	assert.throws(() => gateRepository({ cwd, env: { ...env, RV_GATE_REPO: "invalid" } }), /RV_GATE_REPO/);
});


test("dev PR operation fails before invoking gh when the repository override is invalid", t => {
	mkdirSync("tmp", { recursive: true });
	const cwd = mkdtempSync(join(process.cwd(), "tmp/gate-repository-cli-"));
	t.after(() => rmSync(cwd, { recursive: true, force: true }));
	const marker = join(cwd, "gh-called");
	writeFileSync(join(cwd, "gh"), `#!/bin/sh\ntouch "${marker}"\nexit 1\n`, { mode: 0o755 });
	const env = { ...process.env, PATH: `${cwd}:${process.env.PATH}`, RV_GATE_REPO: "bad", RV_GATE_HOME: join(cwd, "gate") };
	const result = spawnSync(process.execPath, [join(process.cwd(), "tools/rv-gate/rv-gate.mjs"), "dev", "123", "--bootstrap"], { cwd, env, encoding: "utf8" });
	assert.equal(result.status, 1);
	assert.match(result.stderr, /Cannot determine.*origin.*RV_GATE_REPO/);
	assert.equal(existsSync(marker), false);
});
