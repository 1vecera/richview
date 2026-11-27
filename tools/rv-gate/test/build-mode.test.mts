// Host reason (ARC-03): verify gate build mode against the checkout being tested.
import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";
import { STEPS } from "../lib/steps.mts";
import { runScheduledCommand } from "../lib/command.mts";

test("the real gate executor passes the selected mode to the build command", async t => {
	const parent = join(process.cwd(), "tmp");
	mkdirSync(parent, {recursive: true});
	const root = mkdtempSync(join(parent, "rv-build-executor-"));
	t.after(() => rmSync(root, {recursive: true, force: true}));
	mkdirSync(join(root, "assets/install"), {recursive: true});
	const script = join(root, "capture.cjs");
	writeFileSync(script, "console.log(JSON.stringify(process.argv.slice(2)))");
	const build = STEPS.find(step => step.id === "build")?.run?.[0];
	assert.ok(build);
	for (const [mode, parallel, enabled] of [["full", 2, false], ["full", 2, true], ["release", 1, false], ["release", 1, true]] as const) {
		writeFileSync(join(root, "assets/install/release.json"), JSON.stringify({enabled}));
		const log = join(root, `${mode}-${enabled}.log`);
		const options = {
			cwd: root, env: process.env, logPath: log, mode, parallel,
		};
		const result = await runScheduledCommand({...build, cmd: process.execPath}, [script, ...build.args], options);
		assert.equal(result.status, "pass");
		const output = readFileSync(log, "utf8").split("\n").find(line => line.startsWith("["));
		assert.ok(output);
		assert.deepEqual(JSON.parse(output), [...build.args, ...(enabled ? ["--", "--repository-public"] : [])]);
	}
	writeFileSync(join(root, "assets/install/release.json"), '{"enabled":"true"}');
	const invalidOptions = {
		cwd: root, env: process.env, logPath: join(root, "invalid.log"), mode: "full", parallel: 2,
	};
	await assert.rejects(runScheduledCommand({...build, cmd: process.execPath}, [script, ...build.args], invalidOptions), /install release must say whether it is enabled/);
});
