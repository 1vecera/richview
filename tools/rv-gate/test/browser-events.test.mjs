// Host reason (ARC-03): relaunch evidence must survive command failures, buffering and concurrent execution.
import assert from "node:assert/strict";
import test from "node:test";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { gateEnv, runLogged } from "../lib/proc.mjs";
import { collectBrowserEvents, readBrowserRelaunches } from "../lib/browser-events.mjs";

const event = { version: 1, event: "browser-startup-relaunch", surface: "playwright", engine: "chromium", attempt: 2, maxAttempts: 2, timeoutMs: 15000 };
const fixture = t => {
	const dir = mkdtempSync(join(tmpdir(), "rv-browser-events-"));
	t.after(() => rmSync(dir, { recursive: true, force: true }));
	return dir;
};

test("an ambient browser journal cannot redirect gate evidence", t => {
	const original = process.env.RICHVIEW_BROWSER_EVENT_LOG;
	t.after(() => { if (original === undefined) delete process.env.RICHVIEW_BROWSER_EVENT_LOG; else process.env.RICHVIEW_BROWSER_EVENT_LOG = original; });
	process.env.RICHVIEW_BROWSER_EVENT_LOG = "/unrelated/old.jsonl";
	assert.equal("RICHVIEW_BROWSER_EVENT_LOG" in gateEnv(), false);
});

test("the parent collector reads the exact journal without clearing events or hiding an existing failure", t => {
	const dir = fixture(t), file = join(dir, "parent.jsonl"), bytes = JSON.stringify(event) + "\n";
	writeFileSync(file, bytes);
	for (const outcome of [{ status: "pass", exitCode: 0 }, { status: "fail", exitCode: 7, reason: "original failure" }]) {
		const result = collectBrowserEvents(file, outcome);
		assert.equal(result.status, outcome.status); assert.equal(result.exitCode, outcome.exitCode); assert.equal(result.reason, outcome.reason);
		assert.deepEqual(result.browserRelaunches, [{ ...event, journal: file }]);
		assert.equal(readFileSync(file, "utf8"), bytes);
	}
	assert.deepEqual(readBrowserRelaunches(file), [{ ...event, journal: file }]);
	assert.deepEqual(readBrowserRelaunches(join(dir, "missing.jsonl")), []);
	writeFileSync(file, bytes + "{broken");
	assert.throws(() => readBrowserRelaunches(file), error => error.browserRelaunches.length === 1 && /journal/.test(error.message));
	assert.equal(collectBrowserEvents("relative.jsonl", { status: "pass", exitCode: 0 }).status, "fail");
});

const emit = String.raw`const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const [encoded,mode]=process.argv.slice(1), event=JSON.parse(encoded);
assert.ok(path.isAbsolute(process.env.RICHVIEW_BROWSER_EVENT_LOG));
if(mode!=='none'){
 fs.appendFileSync(process.env.RICHVIEW_BROWSER_EVENT_LOG,JSON.stringify({...event,pid:process.pid})+'\n');
 process.stderr.write('RICHVIEW_BROWSER_RELAUNCH '+JSON.stringify(event)+'\n');
}
if(mode==='fail')process.exitCode=7;
if(mode==='stop')process.kill(process.pid,'SIGTERM');
`;

test("real concurrent children retain separate journals on pass, failure and termination without stale counts", async t => {
	const dir = fixture(t);
	const results = await Promise.all(["pass", "fail", "stop", "pass"].map((mode, index) => runLogged(process.execPath, ["-e", emit, JSON.stringify({ ...event, surface: index % 2 ? "pdf" : "playwright" }), mode], {
		cwd: tmpdir(), env: gateEnv({ RICHVIEW_BROWSER_EVENT_LOG: "/unrelated/inherited.jsonl" }), logPath: join(dir, `${index}.log`),
	})));
	assert.deepEqual(results.map(result => result.status), ["pass", "fail", "fail", "pass"]);
	assert.equal(results[1].exitCode, 7, "Telemetry does not hide the child's real failure");
	assert.equal(new Set(results.map(result => result.browserRelaunches[0].journal)).size, 4);
	for (let i = 0; i < results.length; i++) {
		const [record] = results[i].browserRelaunches;
		assert.equal(results[i].browserRelaunches.length, 1);
		assert.equal(record.journal, resolve(join(dir, `${i}.log.browser-events.jsonl`)));
		assert.equal(record.surface, i % 2 ? "pdf" : "playwright");
		assert.ok(record.pid > 0);
		assert.match(readFileSync(join(dir, `${i}.log`), "utf8"), /^RICHVIEW_BROWSER_RELAUNCH /m);
		const next = await runLogged(process.execPath, ["-e", emit, JSON.stringify(event), "none"], { logPath: join(dir, `${i}.log`) });
		assert.equal(next.status, "pass"); assert.deepEqual(next.browserRelaunches, [], "A new invocation never reuses a previous relaunch");
	}
});

test("malformed or incomplete browser journals fail evidence collection and keep preceding valid events", async t => {
	const dir = fixture(t), logPath = join(dir, "run.log");
	const child = `require('node:fs').writeFileSync(process.env.RICHVIEW_BROWSER_EVENT_LOG,process.argv[1]);`;
	for (const bad of [
		"not-json\n", JSON.stringify(event),
		...[{ version: 2 }, { surface: "other" }, { engine: "firefox" }, { attempt: 3 }, { maxAttempts: 3 }, { timeoutMs: 0 }, { timeoutMs: 15001 }, { pid: -1 }].map(change => JSON.stringify({ ...event, ...change }) + "\n"),
	]) {
		const result = await runLogged(process.execPath, ["-e", child, bad], { logPath });
		assert.equal(result.status, "fail", bad);
		assert.match(result.reason, /Could not read browser relaunch journal/);
	}
	const partial = await runLogged(process.execPath, ["-e", child, JSON.stringify(event) + "\n{broken"], { logPath });
	assert.equal(partial.status, "fail"); assert.equal(partial.browserRelaunches.length, 1);
	const unreadable = await runLogged(process.execPath, ["-e", "require('node:fs').mkdirSync(process.env.RICHVIEW_BROWSER_EVENT_LOG)"], { logPath });
	assert.equal(unreadable.status, "fail"); assert.match(unreadable.reason, /journal/);
});
