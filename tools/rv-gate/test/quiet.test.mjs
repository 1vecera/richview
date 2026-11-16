// Host reason (ARC-03): exercise browser admission against real process-owned benchmark holds.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { setTimeout as sleep } from "node:timers/promises";
import { waitForBenchmark } from "../lib/quiet.mjs";

test("browser admission waits for a live owner and never changes its hold", async t => {
	const dir = mkdtempSync(join(tmpdir(), "rv-quiet-")), file = join(dir, "hold");
	t.after(() => rmSync(dir, { recursive: true, force: true }));
	const owner = spawn(process.execPath, ["-e", "process.stdin.resume()"], { stdio: ["pipe", "ignore", "ignore"] });
	await once(owner, "spawn");
	t.after(async () => { const exited = once(owner, "exit"); owner.stdin.end(); if (owner.exitCode === null && owner.signalCode === null) await exited; });
	writeFileSync(file, `${process.ppid}\n`);
	let admitted = false;
	const waiting = waitForBenchmark({ file }).then(() => { admitted = true; });
	await sleep(20);
	assert.equal(admitted, false);
	assert.equal(readFileSync(file, "utf8"), `${process.ppid}\n`);
	writeFileSync(file, `${owner.pid}\n`);
	await sleep(20);
	assert.equal(admitted, false, "a replacement live owner keeps the window closed");
	assert.equal(readFileSync(file, "utf8"), `${owner.pid}\n`);
	rmSync(file);
	await waiting;
	assert.equal(admitted, true);
	assert.equal(existsSync(file), false);
});

test("the benchmark owner can launch while keeping its own hold intact", async t => {
	const dir = mkdtempSync(join(tmpdir(), "rv-own-quiet-")), file = join(dir, "hold");
	t.after(() => rmSync(dir, { recursive: true, force: true }));
	writeFileSync(file, `${process.pid}\n`);
	const controller = new AbortController(), admitted = waitForBenchmark({ file, signal: controller.signal });
	controller.abort();
	await admitted;
	assert.equal(readFileSync(file, "utf8"), `${process.pid}\n`);
});

test("stale and malformed holds do not block, while cancellation preserves a live hold", async t => {
	const dir = mkdtempSync(join(tmpdir(), "rv-quiet-")), file = join(dir, "hold");
	t.after(() => rmSync(dir, { recursive: true, force: true }));
	await waitForBenchmark({ file });
	const owner = spawn(process.execPath, ["-e", "process.exit(0)"], { stdio: "ignore" });
	await once(owner, "exit");
	for (const value of [`${owner.pid}\n`, "", "broken\n", "-1\n", "999999999999999999999\n"]) {
		writeFileSync(file, value);
		await waitForBenchmark({ file });
		assert.equal(readFileSync(file, "utf8"), value, "admission must not clean another owner's marker");
	}
	writeFileSync(file, `${process.ppid}\n`);
	const controller = new AbortController();
	const waiting = waitForBenchmark({ file, signal: controller.signal });
	controller.abort();
	await assert.rejects(waiting, { name: "AbortError" });
	assert.equal(readFileSync(file, "utf8"), `${process.ppid}\n`);
});
