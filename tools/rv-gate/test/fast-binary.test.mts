// Host reason (ARC-03): run the real fast orchestrator against a binary-only workspace.
// Cargo is a protocol double; it never compiles or touches the shared host target.
import assert from "node:assert/strict";
import childProcess, { execFileSync } from "node:child_process";
import { EventEmitter } from "node:events";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { syncBuiltinESMExports } from "node:module";
import { join } from "node:path";
import { PassThrough } from "node:stream";
import test from "node:test";
import { fastGate } from "../lib/fast.mts";

test("fast lints a binary-only crate and its tests without requesting a library", async (t) => {
	mkdirSync("tmp", { recursive: true });
	const root = mkdtempSync(join(process.cwd(), "tmp/fast-binary-"));
	t.after(() => rmSync(root, { recursive: true, force: true }));
	const cwd = join(root, "workspace");
	mkdirSync(join(cwd, "member/src"), { recursive: true });
	writeFileSync(join(cwd, "Cargo.toml"), '[workspace]\nmembers=["member"]\n');
	const manifest = join(cwd, "member/Cargo.toml");
	writeFileSync(manifest, '[package]\nname="binary-fixture"\nversion="0.1.0"\nedition="2024"\n');
	writeFileSync(join(cwd, "member/src/main.rs"), "fn main() {}\n#[test]\nfn binary_contract() {}\n");
	// Manifest edits also select gate tooling (ARC-01). Keep this fixture installed and offline.
	mkdirSync(join(cwd, "node_modules"));
	mkdirSync(join(cwd, "tools/rv-gate/test"), { recursive: true });
	writeFileSync(join(cwd, ".gitignore"), "node_modules/\n");
	writeFileSync(join(cwd, "package.json"), JSON.stringify({ name: "binary-fixture", private: true }));
	const lock = JSON.stringify({ lockfileVersion: 3, packages: {} });
	writeFileSync(join(cwd, "package-lock.json"), lock);
	writeFileSync(join(cwd, "node_modules/.package-lock.json"), lock);
	writeFileSync(join(cwd, "tools/rv-gate/test/fixture.test.mjs"), "import test from 'node:test'; test('fixture tool', () => {});\n");
	const git = (...args: string[]) => execFileSync("git", ["-c", "user.name=Fixture", "-c", "user.email=fixture@example.test", ...args], { cwd, stdio: "pipe" });
	git("init", "-q"); git("add", "."); git("commit", "-qm", "base"); git("update-ref", "refs/remotes/origin/dev", "HEAD");
	writeFileSync(manifest, '[package]\nname="binary-fixture"\nversion="0.1.1"\nedition="2024"\n');
	git("add", "."); git("commit", "-qm", "change");

	const spawnSync = childProcess.spawnSync;
	t.mock.method(childProcess, "spawnSync", (command: string, args: readonly string[], options: childProcess.SpawnSyncOptionsWithStringEncoding) => {
		if (command === "df") return { status: 0, stdout: "Filesystem blocks used available capacity mounted\nfixture 200 1 100 1% /\n" };
		if (command !== "cargo") return spawnSync(command, args, options);
		assert.equal(args[0], "metadata");
		return { status: 0, stdout: JSON.stringify({ packages: [{ name: "binary-fixture", manifest_path: manifest, dependencies: [], targets: [{ name: "binary-fixture", kind: ["bin"], src_path: join(cwd, "member/src/main.rs") }] }] }) };
	});
	const clippy: { args?: readonly string[] } = {};
	const spawn = childProcess.spawn;
	t.mock.method(childProcess, "spawn", (command: string, args: readonly string[] | undefined, options: childProcess.SpawnOptionsWithoutStdio|undefined) => {
		if (command !== "cargo") return spawn(command, args, options);
		assert.ok(args);
		const child = Object.assign(new EventEmitter(), { stdout: new PassThrough(), stderr: new PassThrough() });
		setImmediate(() => {
			if (args[0] === "clippy") {
				clippy.args = args;
				// Cargo rejects an explicitly requested library when the selected package has none.
				if (args.includes("--lib")) {
					child.stderr.end("error: no library targets found in package binary-fixture\n");
					child.emit("close", 101);
					return;
				}
			}
			child.stdout.end("test result: ok. 1 passed; 0 failed\n");
			child.emit("close", 0);
		});
		return child;
	});
	syncBuiltinESMExports();
	t.after(() => { t.mock.restoreAll(); syncBuiltinESMExports(); });
	assert.equal(await fastGate({ cwd, target: join(root, "protocol-target") }), 0);
	assert.ok(clippy.args, "the changed binary crate must be linted");
	assert.ok(clippy.args.includes("--all-targets") || clippy.args.includes("--tests"), "unit-test code must be linted too");
	assert.deepEqual(clippy.args.slice(-3), ["--", "-D", "warnings"]);
});
