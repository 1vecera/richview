// Host reason (ARC-03): verify process-level Cargo ownership using disposable target folders.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { setTimeout as sleep } from "node:timers/promises";
import test from "node:test";
import { RELEASE_HOLD, acquireFastCargo, cargoHolder, heldOnTurn, holdCargo, holdRelease } from "../lib/target.mjs";
import { runFullSteps } from "../lib/full.mjs";
import { runPool } from "../lib/release.mjs";

test("a gated build waits for a live fast owner without displacing its hold or profile", async t => {
	const target = mkdtempSync(join(tmpdir(), "rv-gate-hold-"));
	t.after(() => rmSync(target, { recursive: true, force: true }));
	const fast = await acquireFastCargo(target);
	let acquired = false;
	const pending = Promise.resolve(holdCargo(target, { every: 1 })).then(release => { acquired = true; return release; });
	await sleep(20);
	try {
		assert.equal(acquired, false);
		fast.assertOwned();
		assert.equal(readFileSync(join(target, ".rv-gate-cargo.profile"), "utf8").trim(), "debug");
	} finally { fast.release(); }
	const release = await pending;
	assert.equal(cargoHolder(target), process.pid);
	assert.equal(readFileSync(join(target, ".rv-gate-cargo.profile"), "utf8").trim(), "all");
	release();
	assert.equal(existsSync(join(target, ".rv-gate-cargo")), false);
	assert.equal(existsSync(join(target, ".rv-gate-cargo.profile")), false);
});

test("nested gated commands share ownership while separate targets keep independent holds", async t => {
	const root = mkdtempSync(join(tmpdir(), "rv-gate-hold-"));
	t.after(() => rmSync(root, { recursive: true, force: true }));
	const a = join(root, "streams"), b = join(root, "release");
	const [outer, nested, other] = await Promise.all([holdCargo(a), holdCargo(a), holdCargo(b)]);
	assert.equal(cargoHolder(a), process.pid);
	assert.equal(cargoHolder(b), process.pid);
	nested(); outer(); outer();
	assert.equal(cargoHolder(a), 0);
	assert.equal(cargoHolder(b), process.pid);
	other();
	assert.equal(cargoHolder(b), 0);
});

test("a failed sibling aborts a waiting gated build into a result and leaves its foreign owner intact", async t => {
	const target = mkdtempSync(join(tmpdir(), "rv-gate-hold-"));
	t.after(() => rmSync(target, { recursive: true, force: true }));
	const foreign = await acquireFastCargo(target);
	const controller = new AbortController();
	const results = await runPool([
		async signal => { const release = await holdCargo(target, { signal, every: 1 }); try { return { status: "pass" }; } finally { release(); } },
		async () => { await sleep(10); throw new Error("lint failed"); },
	], 2, controller, { steps: [{ id: "build", title: "Build", cards: [] }, { id: "lint", title: "Lint", cards: [] }] });
	assert.equal(results[0].status, "unrun");
	assert.equal(results[0].id, "build");
	assert.equal(results[1].id, "lint");
	assert.match(results[0].reason, /Stopped: lint/);
	foreign.assertOwned();
	assert.equal(readFileSync(join(target, ".rv-gate-cargo.profile"), "utf8").trim(), "debug");
	foreign.release();
	const release = await holdCargo(target);
	release();
	assert.equal(existsSync(join(target, ".rv-gate-cargo")), false);
	assert.equal(existsSync(join(target, ".rv-gate-cargo.profile")), false);
});

test("a thrown pool task records a failure after the other in-flight worker finishes", async () => {
	let finished = false;
	const results = await runPool([
		async () => { throw new Error("snapshot failed"); },
		async () => { await sleep(10); finished = true; return { status: "pass" }; },
	], 2, new AbortController());
	assert.equal(finished, true);
	assert.equal(results[0].status, "fail");
	assert.match(results[0].reason, /snapshot failed/);
});

test("external cancellation cannot turn every task into unrun without a failure", async () => {
	const controller = new AbortController();
	controller.abort();
	const results = await runPool([async signal => { signal.throwIfAborted(); }], 1, controller);
	assert.equal(results[0].status, "fail");
});

test("a full's release hold admits one process and is a file of its own, so a fast run need not wait for it", async t => {
	const target = mkdtempSync(join(tmpdir(), "rv-gate-hold-"));
	t.after(() => rmSync(target, { recursive: true, force: true }));
	const first = await holdRelease(target);
	assert.equal(cargoHolder(target, RELEASE_HOLD), process.pid);
	assert.equal(cargoHolder(target), 0);
	(await acquireFastCargo(target)).release();
	let acquired = false;
	const pending = holdRelease(target, { every: 1 }).then(release => { acquired = true; return release; });
	await sleep(20);
	assert.equal(acquired, false);
	first();
	const second = await pending;
	first();
	assert.equal(cargoHolder(target, RELEASE_HOLD), process.pid, "a stale release cannot remove its successor's hold");
	second();
	assert.equal(existsSync(join(target, RELEASE_HOLD)), false);
	// A hold left by a process that has exited is reclaimed.
	const gone = execFileSync(process.execPath, ["-e", "process.stdout.write(String(process.pid))"], { encoding: "utf8" });
	writeFileSync(join(target, RELEASE_HOLD), `${gone}\n`);
	assert.equal(cargoHolder(target, RELEASE_HOLD), 0);
	(await holdRelease(target, { every: 1 }))();
	assert.equal(existsSync(join(target, RELEASE_HOLD)), false);
});

test("a full's Cargo command queued behind its browser suites holds nothing until its turn, then holds every profile", async t => {
	const target = mkdtempSync(join(tmpdir(), "rv-gate-hold-"));
	t.after(() => rmSync(target, { recursive: true, force: true }));
	const releaseHold = await holdRelease(target);
	const started = Promise.withResolvers(), browser = Promise.withResolvers(), seen = [];
	const steps = [{ id: "build" }, { id: "browser", needs: "build", run: [{ cmd: "node" }] }, { id: "rust-slow", needs: "build", run: [{ cmd: "cargo" }] }];
	const execution = runFullSteps(steps, async (step, schedule) => {
		if (step.id === "build") return { status: "pass" };
		if (step.id === "browser") return schedule(async () => { started.resolve(); await browser.promise; return { status: "pass" }; }, 2);
		return heldOnTurn(schedule, () => holdCargo(target, { every: 1 }))(async () => {
			seen.push([cargoHolder(target), readFileSync(join(target, ".rv-gate-cargo.profile"), "utf8").trim()]);
			return { status: "pass" };
		});
	}, 2);
	await started.promise;
	// Queued, the command holds nothing, so a fast run's debug Cargo goes ahead; release builds still wait.
	assert.equal(cargoHolder(target), 0);
	assert.equal(cargoHolder(target, RELEASE_HOLD), process.pid);
	const fast = await acquireFastCargo(target);
	browser.resolve();
	await sleep(20);
	assert.deepEqual(seen, [], "on its turn the command waits for the fast run's Cargo to end");
	fast.release();
	const results = await execution;
	assert.deepEqual(results.map(result => result.status), ["pass", "pass", "pass"]);
	assert.deepEqual(seen, [[process.pid, "all"]]);
	assert.equal(cargoHolder(target), 0);
	assert.equal(cargoHolder(target, RELEASE_HOLD), process.pid, "the release hold lasts the whole run");
	releaseHold();
	assert.equal(existsSync(join(target, RELEASE_HOLD)), false);
});
