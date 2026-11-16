// Host reason (ARC-03): exercise the gate's real npm installer with an isolated executable.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";
import { npmCi } from "../lib/npm.mjs";
import { prepareHost } from "../journeys-publish.mjs";
import { STEPS } from "../lib/steps.mjs";

test("npm installation refuses linked node_modules before starting npm", (t) => {
	mkdirSync("tmp", { recursive: true });
	const root = mkdtempSync(join(process.cwd(), "tmp/npm-install-"));
	t.after(() => rmSync(root, { recursive: true, force: true }));
	const cwd = join(root, "checkout"), shared = join(root, "shared"), bin = join(root, "bin");
	mkdirSync(cwd); mkdirSync(shared); mkdirSync(bin);
	writeFileSync(join(shared, "keep"), "other worktree's installation");
	writeFileSync(join(bin, "npm"), '#!/bin/sh\nprintf called > "$NPM_CALLED"\n');
	chmodSync(join(bin, "npm"), 0o755);
	const called = join(root, "called");
	const env = { ...process.env, PATH: `${bin}:${process.env.PATH}`, NPM_CALLED: called };
	symlinkSync(shared, join(cwd, "node_modules"));
	assert.throws(() => npmCi(cwd, { env }), /node_modules.*symlink|symlink.*node_modules/i);
	assert.equal(existsSync(called), false, "npm must not be started");
	assert.equal(readFileSync(join(shared, "keep"), "utf8"), "other worktree's installation");

	rmSync(join(cwd, "node_modules"));
	symlinkSync(join(root, "missing"), join(cwd, "node_modules"));
	assert.throws(() => npmCi(cwd, { env }), /node_modules.*symlink|symlink.*node_modules/i, "dangling links are still unsafe");
	assert.equal(existsSync(called), false);

	rmSync(join(cwd, "node_modules"));
	mkdirSync(join(cwd, "node_modules"));
	npmCi(cwd, { env });
	assert.equal(readFileSync(called, "utf8"), "called", "an independent install is allowed");
	rmSync(called); rmSync(join(cwd, "node_modules"), { recursive: true });
	npmCi(cwd, { env });
	assert.equal(readFileSync(called, "utf8"), "called", "a new checkout can install its own dependencies");
});

test("npm installation checks the selected package prefix", (t) => {
	mkdirSync("tmp", { recursive: true });
	const root = mkdtempSync(join(process.cwd(), "tmp/npm-prefix-"));
	t.after(() => rmSync(root, { recursive: true, force: true }));
	mkdirSync(join(root, "package")); mkdirSync(join(root, "shared")); mkdirSync(join(root, "bin"));
	const fake = join(root, "bin/npm"); writeFileSync(fake, "#!/bin/sh\nexit 0\n"); chmodSync(fake, 0o755);
	symlinkSync(join(root, "shared"), join(root, "package/node_modules"));
	const env = { ...process.env, PATH: `${join(root, "bin")}:${process.env.PATH}` };
	assert.throws(() => npmCi(root, { prefix: "package", env }), /node_modules.*symlink|symlink.*node_modules/i);
});

test("npm installation refuses linked workspace dependencies before npm starts", (t) => {
	mkdirSync("tmp", { recursive: true });
	const root = mkdtempSync(join(process.cwd(), "tmp/npm-workspace-"));
	t.after(() => rmSync(root, { recursive: true, force: true }));
	mkdirSync(join(root, "packages/editor"), { recursive: true }); mkdirSync(join(root, "shared")); mkdirSync(join(root, "bin"));
	writeFileSync(join(root, "package.json"), JSON.stringify({ workspaces: ["packages/*"] }));
	writeFileSync(join(root, "packages/editor/package.json"), JSON.stringify({ name: "editor" }));
	writeFileSync(join(root, "shared/keep"), "other worktree's installation");
	const called = join(root, "called"), fake = join(root, "bin/npm");
	writeFileSync(fake, '#!/bin/sh\nprintf called > "$NPM_CALLED"\n'); chmodSync(fake, 0o755);
	symlinkSync(join(root, "shared"), join(root, "packages/editor/node_modules"));
	const env = { ...process.env, PATH: `${join(root, "bin")}:${process.env.PATH}`, NPM_CALLED: called };
	assert.throws(() => npmCi(root, { env }), /packages\/editor\/node_modules.*symlink.*rm -- .*no trailing slash/i);
	assert.equal(existsSync(called), false);
	assert.equal(readFileSync(join(root, "shared/keep"), "utf8"), "other worktree's installation");
});

test("Publish journey refuses a linked adapter install before invoking npm", (t) => {
	mkdirSync("tmp", { recursive: true });
	const root = mkdtempSync(join(process.cwd(), "tmp/npm-journey-"));
	const oldPath = process.env.PATH, oldCalled = process.env.NPM_CALLED;
	t.after(() => {
		process.env.PATH = oldPath;
		if (oldCalled === undefined) delete process.env.NPM_CALLED; else process.env.NPM_CALLED = oldCalled;
		rmSync(root, { recursive: true, force: true });
	});
	mkdirSync(join(root, "cloudflare/publish"), { recursive: true }); mkdirSync(join(root, "shared")); mkdirSync(join(root, "bin"));
	writeFileSync(join(root, "shared/keep"), "shared");
	symlinkSync(join(root, "shared"), join(root, "cloudflare/publish/node_modules"));
	const called = join(root, "called"), fake = join(root, "bin/npm");
	writeFileSync(fake, '#!/bin/sh\nprintf called > "$NPM_CALLED"\n'); chmodSync(fake, 0o755);
	process.env.PATH = `${join(root, "bin")}:${oldPath}`; process.env.NPM_CALLED = called;
	const result = prepareHost({ cwd: root, out: root, wasm: "unused-fixture.wasm" });
	assert.match(result.error, /node_modules.*symlink/i);
	assert.equal(existsSync(called), false);
	assert.equal(readFileSync(join(root, "shared/keep"), "utf8"), "shared");
});

test("publish-tests refuses a linked adapter install before invoking npm", (t) => {
	mkdirSync("tmp", { recursive: true });
	const root = mkdtempSync(join(process.cwd(), "tmp/npm-publish-step-"));
	t.after(() => rmSync(root, { recursive: true, force: true }));
	mkdirSync(join(root, "cloudflare/publish"), { recursive: true }); mkdirSync(join(root, "shared")); mkdirSync(join(root, "bin"));
	mkdirSync(join(root, "tools/rv-gate"), { recursive: true });
	symlinkSync(join(process.cwd(), "tools/rv-gate/lib"), join(root, "tools/rv-gate/lib"));
	writeFileSync(join(root, "shared/keep"), "shared");
	symlinkSync(join(root, "shared"), join(root, "cloudflare/publish/node_modules"));
	const called = join(root, "called"), fake = join(root, "bin/npm");
	writeFileSync(fake, '#!/bin/sh\nprintf "%s\\n" "$@" > "$NPM_CALLED"\n'); chmodSync(fake, 0o755);
	const env = { ...process.env, PATH: `${join(root, "bin")}:${process.env.PATH}`, NPM_CALLED: called };
	const command = STEPS.find(step => step.id === "publish-tests").run[0];
	const result = spawnSync(command.cmd, command.args, { cwd: root, env, encoding: "utf8" });
	assert.notEqual(result.status, 0);
	assert.match(result.stderr, /node_modules.*symlink/i);
	assert.equal(existsSync(called), false);
	assert.equal(readFileSync(join(root, "shared/keep"), "utf8"), "shared");
	rmSync(join(root, "cloudflare/publish/node_modules"));
	mkdirSync(join(root, "cloudflare/publish/node_modules"));
	const allowed = spawnSync(command.cmd, command.args, { cwd: root, env, encoding: "utf8" });
	assert.equal(allowed.status, 0, allowed.stderr);
	assert.equal(readFileSync(called, "utf8"), "ci\n--ignore-scripts\n--no-audit\n--no-fund\n--prefix\ncloudflare/publish\n");
});
