// Host reason (ARC-03): exercise the real registry's artifact dependencies and worker admission.
import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";
import { runScheduledCommand } from "../lib/command.mts";
import { runFullSteps } from "../lib/full.mts";
import { CODEX_UNRUN, ENGINE_LOOP_SUITES, FRESH_AGENT_SCHEDULE, STEPS } from "../lib/steps.mts";

test("only the original canonical Firefox child receives bounded protocol diagnostics and a folder for its preview reloads", () => {
	const marked = STEPS.flatMap(step => (step.run ?? []).filter(command => command.env?.RV_GATE_FIREFOX_PROTOCOL_LOG).map(command => ({ step: step.id, command })));
	assert.equal(marked.length, 1);
	assert.equal(marked[0].step, "firefox");
	assert.deepEqual(marked[0].command.args, ["scripts/verify-browser.ts"]);
	assert.equal(marked[0].command.cmd, "node");
	assert.deepEqual(marked[0].command.env, { RICHVIEW_VERIFY_BROWSER: "firefox", DEBUG: "pw:protocol", DEBUG_COLORS: "0", RV_GATE_FIREFOX_PROTOCOL_LOG: "{run}/firefox-protocol.jsonl", RV_GATE_PREVIEW_RELOAD_DIR: "{run}/firefox-preview-reloads" });
	assert.equal(STEPS.flatMap(step => (step.run ?? []).filter(command => command.env?.RV_GATE_PREVIEW_RELOAD_DIR)).length, 1);
	const firefox = STEPS.find(step => step.id === "firefox");
	assert.ok(firefox?.run);
	assert.deepEqual(firefox.run.map(command => command.args[0]), ENGINE_LOOP_SUITES.map(name => `scripts/verify-${name}.ts`));
});

test("actual capture and surface dependencies wait for their producers, not an unrelated browser", async () => {
	const names = ["build", "chromium", "visual-matrix", "captures", "surface"];
	const steps = names.map(id => {
		const step = STEPS.find(step => step.id === id);
		assert.ok(step, id);
		return step;
	});
	const events: string[] = [];
	const unrelated = Promise.withResolvers<void>();
	const running = runFullSteps(steps, async (step, schedule) => {
		const work = async () => {
			events.push(step.id);
			if (step.id === "chromium") await unrelated.promise;
			return { status: "pass" as const };
		};
		return step.fn ? work() : schedule(work);
	}, 2);
	await new Promise(resolve => setImmediate(resolve));
	const beforeRelease = [...events];
	unrelated.resolve();
	const results = await running;
	assert.ok(beforeRelease.includes("captures"), "matrix completion must unlock captures while the unrelated suite still runs");
	assert.ok(beforeRelease.includes("surface"), "build completion must unlock surface while the unrelated suite still runs");
	assert.ok(events.indexOf("visual-matrix") < events.indexOf("captures"));
	assert.ok(events.indexOf("build") < events.indexOf("surface"));
	assert.deepEqual(results.map(result => result.id), names);
	assert.ok(results.every(result => result.status === "pass"));
});

test("the registry's internal browser pools reserve their real worker counts before launch", async t => {
	const parent = join(process.cwd(), "tmp"); mkdirSync(parent, { recursive: true });
	const dir = mkdtempSync(join(parent, "registry-workers-"));
	t.after(() => rmSync(dir, { recursive: true, force: true }));
	const script = join(dir, "child.cjs");
	writeFileSync(script, 'console.log(JSON.stringify({argv:process.argv.slice(2),workers:process.env.RICHVIEW_VERIFY_WORKERS}));');
	for (const [id, args, workers] of [["visual-matrix", [], "2"], ["cross-style", ["--jobs", "2"], "1"], ["label-truth", ["--jobs", "2"], "1"], ["fresh-agent", ["--jobs", "2"], "1"]] as const) {
		const entry = STEPS.find(step => step.id === id)?.run?.[0];
		assert.ok(entry, id);
		const logPath = join(dir, `${id}.log`), admitted: number[] = [];
		const result = await runScheduledCommand({ ...entry, cmd: process.execPath }, [script], {
			cwd: dir, env: { ...process.env, RICHVIEW_VERIFY_WORKERS: "99" }, logPath, mode: "full", parallel: 4,
			scheduleCommand: (run, permits = 1) => { admitted.push(permits); return run(); },
		});
		assert.equal(result.status, "pass");
		assert.deepEqual(admitted, [2], `${id} must reserve both workers`);
		const line = readFileSync(logPath, "utf8").split("\n").find(line => line.startsWith('{"argv":'));
		assert.ok(line);
		assert.deepEqual(JSON.parse(line), { argv: args, workers });
	}
});

test("fresh-agent follows DEL-78: unrun in a full, Claude only when it runs, Codex reported unrun", () => {
	const step = STEPS.find(candidate => candidate.id === "fresh-agent");
	assert.ok(step?.run);
	assert.deepEqual(step.unrunIn, { full: FRESH_AGENT_SCHEDULE });
	const args = step.run[0].args;
	assert.equal(args[args.indexOf("--products") + 1], "claude");
	assert.equal(args[args.indexOf("--unrun") + 1], `codex:${CODEX_UNRUN}`);
});
