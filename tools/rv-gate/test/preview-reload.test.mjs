// Host reason (ARC-03): the Firefox preview reload stays bounded, and every reload reaches the receipt (S27-844).
import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { pathToFileURL } from "node:url";
import { withBrowserEvents } from "../lib/browser-receipt.mjs";
import { PREVIEW_RELOAD, previewReloadLine, reloadRefusal } from "../lib/preview-reload.mjs";
import { gateEnv, runLogged } from "../lib/proc.mjs";
import { archiveAttempt, withRerun } from "../lib/rerun.mjs";

const fixture = t => {
	const dir = mkdtempSync(join(tmpdir(), "rv-preview-reload-"));
	t.after(() => rmSync(dir, { recursive: true, force: true }));
	return dir;
};
// What verify-browser's capturePreviewFailure reads when the driver has lost the frame: the parent
// answers with the expected source, and neither frame answers within two seconds.
const silent = { expected: "Identity bar 1-2", error: "TimeoutError: locator.waitFor: Timeout 45000ms exceeded.", parent: { hasExpected: true }, snapshotTimeout: true };

test("only a silent Firefox frame whose parent drew the source reloads, twice per case and five times per run", () => {
	const refusal = (failure, options = {}) => reloadRefusal(failure, { engine: "firefox", reloads: [], title: "Identity bar 1-2", ...options });
	assert.equal(refusal(silent), null);
	assert.match(refusal(silent, { engine: "chromium" }), /chromium never reloads/);
	assert.match(refusal(silent, { engine: "webkit" }), /webkit never reloads/);
	assert.match(refusal({ ...silent, parent: { hasExpected: false } }), /parent has not rendered/);
	assert.match(refusal({ ...silent, parent: undefined }), /parent has not rendered/);
	assert.match(refusal({ ...silent, snapshotTimeout: undefined }), /frame answered/);
	assert.match(refusal({ ...silent, currentFrame: { error: "Frame was detached" } }), /frame answered/, "a frame that fails at once has not been lost");
	assert.match(refusal({ ...silent, retainedFrame: { ready: "complete", titles: [] } }), /frame answered/);
	const reloads = (...titles) => titles.map(title => ({ case: title }));
	assert.equal(refusal(silent, { reloads: reloads("Identity bar 1-2") }), null);
	assert.match(refusal(silent, { reloads: reloads("Identity bar 1-2", "Identity bar 1-2") }), /same case/);
	assert.equal(refusal(silent, { reloads: reloads("A", "B", "C", "D") }), null);
	assert.match(refusal(silent, { reloads: reloads("A", "B", "C", "D", "E") }), /already reloaded the preview 5 times/);
});

// A child that records its reloads as verify-browser does: the screenshot first, then the journal line.
const reloading = String.raw`
const { writeFileSync } = await import('node:fs');
const { recordPreviewReload } = await import(process.argv[1]);
for (const [index, title] of JSON.parse(process.argv[2]).entries()) {
  const screenshot = process.env.SHOTS + '/preview-reload-' + (index + 1) + '.png';
  writeFileSync(screenshot, 'png');
  recordPreviewReload({ title, recovery: index + 1, screenshot });
}`;

test("each reload reaches the receipt with its step, case and screenshot, and the go card counts them", async t => {
	const dir = fixture(t), logPath = join(dir, "verify-browser.log"), titles = ["Identity bar 1-2", "Identity bar 1-2", "Zoom scatter numbers"];
	const module = pathToFileURL(join(import.meta.dirname, "../lib/preview-reload.mjs")).href;
	const outcome = await runLogged(process.execPath, ["--input-type=module", "-e", reloading, module, JSON.stringify(titles)], { cwd: dir, env: gateEnv({ SHOTS: dir }), logPath });
	assert.equal(outcome.status, "pass", readFileSync(logPath, "utf8"));
	assert.deepEqual(outcome.previewReloads.map(reload => [reload.recovery, reload.case, reload.screenshot]), titles.map((title, i) => [i + 1, title, join(dir, `preview-reload-${i + 1}.png`)]));
	assert.ok(outcome.previewReloads.every(reload => reload.issue === PREVIEW_RELOAD.issue && reload.journal === `${logPath}.browser-events.jsonl`));
	assert.match(readFileSync(logPath, "utf8"), /^RICHVIEW_PREVIEW_RELOAD .*"case":"Identity bar 1-2"/m);
	// The step's details carry its commands' reloads; the full's receipt names the step of each.
	const steps = [{ id: "firefox", status: "pass", details: { previewReloads: outcome.previewReloads } }, { id: "chromium", status: "pass" }];
	const full = await withBrowserEvents(join(dir, "browser-relaunches.jsonl"), async () => steps);
	assert.deepEqual(full.previewReloads.map(reload => [reload.step, reload.case]), [["firefox", "Identity bar 1-2"], ["firefox", "Identity bar 1-2"], ["firefox", "Zoom scatter numbers"]]);
	assert.equal(previewReloadLine(steps), `Firefox preview reloads (microsoft/playwright#43124): 3. firefox: Identity bar 1-2 (screenshot \`${join(dir, "preview-reload-1.png")}\`); firefox: Identity bar 1-2 (screenshot \`${join(dir, "preview-reload-2.png")}\`); firefox: Zoom scatter numbers (screenshot \`${join(dir, "preview-reload-3.png")}\`).`);
	assert.equal(previewReloadLine([{ id: "firefox", status: "pass" }]), "Firefox preview reloads (microsoft/playwright#43124): none.");
});

test("a journal outside the reload's bounds fails the command and keeps the reloads before it", async t => {
	const dir = fixture(t), logPath = join(dir, "run.log"), screenshot = join(dir, "shot.png");
	writeFileSync(screenshot, "png");
	const line = (recovery, title, change = {}) => JSON.stringify({ version: 1, event: "firefox-preview-reload", surface: "playwright", engine: "firefox", issue: PREVIEW_RELOAD.issue, recovery, maxRecoveries: 5, case: title, screenshot, ...change }) + "\n";
	const run = text => runLogged(process.execPath, ["-e", "require('node:fs').writeFileSync(process.env.RICHVIEW_BROWSER_EVENT_LOG,process.argv[1])", text], { logPath });
	for (const bad of [
		line(1, "A", { engine: "chromium" }), line(1, "A", { issue: "microsoft/playwright#1" }), line(1, "A", { maxRecoveries: 4 }), line(1, " "),
		line(1, "A", { screenshot: "shot.png" }), line(1, "A", { screenshot: join(dir, "missing.png") }),
		line(2, "A"), line(1, "A") + line(1, "B"), line(1, "A") + line(2, "A") + line(3, "A"),
		line(1, "A") + line(2, "B") + line(3, "C") + line(4, "D") + line(5, "E") + line(6, "F"),
		line(1, "A", { maxRecoveries: 3 }) + line(2, "A", { maxRecoveries: 3 }),
		line(1, "A", { maxRecoveries: 3 }) + line(2, "B"),
	]) {
		const result = await run(bad);
		assert.equal(result.status, "fail", bad);
		assert.match(result.reason, /Could not read browser relaunch journal/);
	}
	assert.deepEqual((await run(line(1, "A") + line(1, "B"))).previewReloads.map(reload => reload.case), ["A"]);
	assert.equal((await run(line(1, "A") + line(2, "A") + line(3, "B") + line(4, "C") + line(5, "D"))).status, "pass");
	assert.equal((await run(line(1, "A", { maxRecoveries: 3 }) + line(2, "B", { maxRecoveries: 3 }) + line(3, "C", { maxRecoveries: 3 }))).status, "pass", "earlier journals retain their original bounds");
});

test("a rerun keeps the first attempt's reloads, with their screenshots moved into its archive", async t => {
	const runDir = fixture(t), shots = join(runDir, "firefox-preview-reloads"), shot = join(shots, "preview-reload-1.png");
	const step = { id: "firefox", run: [{ cmd: "node", args: ["scripts/verify-browser.mjs"], env: { RV_GATE_PREVIEW_RELOAD_DIR: "{run}/firefox-preview-reloads" } }] };
	const result = await withRerun({ phase: "prelaunch" }, async n => {
		mkdirSync(shots); writeFileSync(shot, `attempt ${n}`);
		const reloads = [{ case: `Case ${n}`, recovery: 1, screenshot: shot, journal: join(runDir, "journal.jsonl") }];
		return { status: n === 1 ? "fail" : "pass", details: { previewReloads: reloads, commands: [{ previewReloads: reloads }] } };
	}, { beforeRerun: first => archiveAttempt(step, { runDir }, first) });
	assert.equal(result.status, "pass");
	assert.deepEqual(result.details.previewReloads.map(reload => [reload.case, readFileSync(reload.screenshot, "utf8")]), [["Case 1", "attempt 1"], ["Case 2", "attempt 2"]]);
	assert.equal(result.details.previewReloads[0].screenshot, join(runDir, "attempts/firefox/1/firefox-preview-reloads/preview-reload-1.png"));
	assert.equal(result.attempts[0].details.commands[0].previewReloads[0].screenshot, result.details.previewReloads[0].screenshot);
});
