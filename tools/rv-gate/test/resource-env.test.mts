// Host reason (ARC-03): keep dedicated Cargo builds within the host's default worker budget.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { dirname } from "node:path";
import test from "node:test";
import { gateEnv } from "../lib/proc.mts";

test("dedicated gate targets get the four-job default even when the shim bypasses shared locking", t => {
	const original = process.env.CARGO_BUILD_JOBS;
	t.after(() => { if (original === undefined) delete process.env.CARGO_BUILD_JOBS; else process.env.CARGO_BUILD_JOBS = original; });
	delete process.env.CARGO_BUILD_JOBS;
	assert.equal(gateEnv({ CARGO_TARGET_DIR: "/work/target-release" }).CARGO_BUILD_JOBS, "4");
	process.env.CARGO_BUILD_JOBS = "2";
	assert.equal(gateEnv().CARGO_BUILD_JOBS, "2");
	assert.equal(gateEnv({ CARGO_BUILD_JOBS: "1" }).CARGO_BUILD_JOBS, "1");
});

test("every command a gate run starts finds the gate's own node first", () => {
	const env = gateEnv();
	const dirs = env.PATH.split(":");
	assert.equal(dirs.filter(dir => !dir.endsWith("/tmp/richview-rewrite/bin"))[0], dirname(process.execPath));
	assert.equal(execFileSync("node", ["-p", "process.version"], { env, encoding: "utf8" }).trim(), process.version);
});
