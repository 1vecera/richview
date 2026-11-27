// Host reason (ARC-03): concurrent built-in journals and failed commands must survive in full receipts.
import assert from "node:assert/strict";
import { appendFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import test, { type TestContext } from "node:test";
import { withBrowserEvents } from "../lib/browser-receipt.mts";

const event = { version: 1, event: "browser-startup-relaunch", surface: "playwright", engine: "chromium", attempt: 2, maxAttempts: 2, timeoutMs: 15000 };
function fixture(t: TestContext) {
	const scratch = join(process.cwd(), "tmp"); mkdirSync(scratch, { recursive: true });
	const dir = mkdtempSync(join(scratch, "browser-receipt-"));
	t.after(() => rmSync(dir, { recursive: true, force: true }));
	return join(dir, "browser-relaunches.jsonl");
}

test("all concurrent function relaunches and failed-command events survive with their journal provenance", async t => {
	const file = fixture(t), previous = process.env.RICHVIEW_BROWSER_EVENT_LOG;
	writeFileSync(file, "stale event must not be reused\n");
	const commandEvent = { ...event, surface: "pdf" as const, journal: join(file, "..", "command.log.browser-events.jsonl") };
	const steps = [{ id: "exports", status: "fail", reason: "second startup failed", details: { browserRelaunches: [commandEvent] } }];
	const result = await withBrowserEvents(file, async () => {
		await Promise.all([11, 12].map(async pid => {
			await new Promise(resolve => setImmediate(resolve));
			assert.equal(process.env.RICHVIEW_BROWSER_EVENT_LOG, file);
			appendFileSync(file, JSON.stringify({ ...event, pid }) + "\n");
		}));
		return steps;
	});
	assert.equal(result.results, steps);
	assert.equal(result.results[0].status, "fail");
	assert.equal(result.browserRelaunchTelemetry.status, "pass");
	assert.equal(result.browserRelaunches.length, 3);
	assert.deepEqual(result.browserRelaunches[0], { ...commandEvent, step: "exports" });
	assert.deepEqual(result.browserRelaunches.slice(1).map(e => ["pid" in e ? e.pid : undefined, e.journal]), [[11, file], [12, file]]);
	assert.equal(readFileSync(file, "utf8").split("\n").filter(Boolean).length, 2);
	assert.equal(process.env.RICHVIEW_BROWSER_EVENT_LOG, previous);
});

test("a malformed parent journal fails telemetry and retains preceding events and every step result", async t => {
	const file = fixture(t), steps = [{ id: "captures", status: "pass" }];
	const result = await withBrowserEvents(file, async () => {
		appendFileSync(file, JSON.stringify(event) + "\n{incomplete");
		return steps;
	});
	assert.equal(result.results, steps);
	assert.equal(result.browserRelaunchTelemetry.status, "fail");
	const reason = result.browserRelaunchTelemetry.reason;
	assert.ok(typeof reason === "string");
	assert.match(reason, /journal/);
	assert.deepEqual(result.browserRelaunches, [{ ...event, journal: file }]);
});

test("zero relaunches stay zero and the parent environment is restored after a thrown full run", async t => {
	const file = fixture(t), previous = process.env.RICHVIEW_BROWSER_EVENT_LOG;
	t.after(() => {
		if (previous === undefined) delete process.env.RICHVIEW_BROWSER_EVENT_LOG;
		else process.env.RICHVIEW_BROWSER_EVENT_LOG = previous;
	});
	const inherited = join(file, "..", "caller.jsonl");
	process.env.RICHVIEW_BROWSER_EVENT_LOG = inherited;
	const result = await withBrowserEvents(file, async () => [{ id: "captures", status: "pass" }]);
	assert.deepEqual(result.browserRelaunches, []);
	assert.equal(result.browserRelaunchTelemetry.status, "pass");
	await assert.rejects(withBrowserEvents(file, async () => { throw Error("full dispatch failed"); }), /full dispatch failed/);
	assert.equal(process.env.RICHVIEW_BROWSER_EVENT_LOG, inherited);
});
