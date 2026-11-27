// Host reason (ARC-03): exercise Cargo source ownership against real Git inputs and fingerprint files.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, utimesSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { claimSources, holdCargo, recordSourceOwner } from "../lib/target.mts";

test("warm source claims retain timestamps, and foreign checkout/profile writes invalidate them", async t => {
	const dir = mkdtempSync(join(tmpdir(), "rv-source-owner-"));
	t.after(() => rmSync(dir, { recursive: true, force: true }));
	const target = join(dir, "outputs"), cwd = join(dir, "checkout");
	mkdirSync(cwd); mkdirSync(target);
	execFileSync("git", ["init", "-q", cwd]);
	for (const name of ["lib.rs", "Cargo.toml", "shader.wgsl"]) writeFileSync(join(cwd, name), "original\n");
	execFileSync("git", ["add", "."], { cwd });
	const source = join(cwd, "lib.rs"), old = new Date("2001-01-01T00:00:00Z");
	const fingerprint = (profile: string, name: string) => {
		const path = join(target, profile, ".fingerprint", "richview-core-0123456789abcdef", name);
		mkdirSync(join(path, ".."), { recursive: true }); writeFileSync(path, "build\n");
		return path;
	};
	const native = fingerprint("debug", "lib-richview_core");
	const wasm = fingerprint("wasm32-unknown-unknown/wasm-release", "lib-richview_runtime");
	utimesSync(source, old, old);
	assert.equal(claimSources(target, cwd), true);
	assert.ok(statSync(source).mtimeMs > old.getTime());
	utimesSync(source, old, old);
	assert.equal(claimSources(target, cwd), false);
	assert.equal(statSync(source).mtimeMs, old.getTime(), "the same generation must not force recompilation");
	const release = await holdCargo(target);
	try {
		writeFileSync(native, "our completed build\n");
		recordSourceOwner(target, cwd);
		assert.equal(claimSources(target, cwd), false, "our own completed build stays warm");
		assert.equal(statSync(source).mtimeMs, old.getTime());
	} finally { release(); }
	assert.throws(() => recordSourceOwner(target, cwd), /requires the Cargo hold/);
	// Keep this older than the latest native fingerprint: a max-mtime check would miss it.
	utimesSync(wasm, new Date("2002-01-01T00:00:00Z"), new Date("2002-01-01T00:00:00Z"));
	assert.equal(claimSources(target, cwd), true, "a foreign custom Wasm profile must invalidate ownership");
	assert.ok(statSync(source).mtimeMs > old.getTime());
	const other = join(dir, "other");
	mkdirSync(other); execFileSync("git", ["init", "-q", other]);
	writeFileSync(join(other, "lib.rs"), "other\n"); execFileSync("git", ["add", "."], { cwd: other });
	assert.equal(claimSources(target, other), true);
	utimesSync(source, old, old);
	assert.equal(claimSources(target, cwd), true, "returning from another checkout must stamp again");
	assert.equal(readFileSync(source, "utf8"), "original\n", "claiming changes timestamps only");
	writeFileSync(join(target, ".rv-gate-sources.json"), "{");
	assert.equal(claimSources(target, cwd), true, "malformed ownership must fail closed");
});
