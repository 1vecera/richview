// Host reason (ARC-03): execute real children to verify worker admission and the held Rust dispatch seam.
import assert from "node:assert/strict";
import test from "node:test";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { runScheduledCommand } from "../lib/command.mts";
import type { GateCommand } from "../lib/steps.mts";
import { gateEnv } from "../lib/proc.mts";
import { cargoHolder, holdCargo } from "../lib/target.mts";

const fixture = (t: test.TestContext) => {
	const dir = realpathSync(mkdtempSync(join(tmpdir(), "rv-admitted-command-")));
	t.after(() => rmSync(dir, { recursive: true, force: true }));
	return dir;
};
const dump = `console.log(JSON.stringify({argv:process.argv.slice(2),cwd:process.cwd(),workers:process.env.RICHVIEW_VERIFY_WORKERS,extra:process.env.COMMAND_EXTRA}));`;
const record = (log: string) => {
	const line = readFileSync(log, "utf8").split("\n").find(line => line.startsWith('{"argv":'));
	assert.ok(line);
	return JSON.parse(line);
};

test("a gate cannot inherit the snippet updater, whose presence rewrites expected output even at zero", t => {
	const before = process.env.UPDATE_SNIPPETS;
	t.after(() => { if (before === undefined) delete process.env.UPDATE_SNIPPETS; else process.env.UPDATE_SNIPPETS = before; });
	for (const value of ["1", "0", ""]) {
		process.env.UPDATE_SNIPPETS = value;
		assert.equal("UPDATE_SNIPPETS" in gateEnv(), false);
	}
});

test("a real command receives only its reserved workers and malformed assignments never spawn", async t => {
	const dir = fixture(t), script = join(dir, "child.cjs"); writeFileSync(script, dump);
	for (const [name, workers, parallel, expectedArgs, expectedWorkers, weight] of [
		["default", undefined, 3, ["original"], "1", 1],
		["env", { env: "RICHVIEW_VERIFY_WORKERS", count: 2 }, 3, ["original"], "2", 2],
		["arg", { arg: "--jobs", count: 4 }, 2, ["original", "--jobs", "2"], "1", 2],
	] satisfies [string, GateCommand["workers"], number, string[], string, number][]) {
		const logPath = join(dir, name + ".log"), admitted: number[] = [];
		const result = await runScheduledCommand({ cmd: process.execPath, args: [], workers }, [script, "original"], {
			cwd: dir, env: gateEnv({ RICHVIEW_VERIFY_WORKERS: "99", COMMAND_EXTRA: "kept" }), logPath, mode: "full", parallel,
			scheduleCommand: (run, permits = 1) => { admitted.push(permits); return run(); },
		});
		assert.equal(result.status, "pass"); assert.deepEqual(admitted, [weight]);
		assert.deepEqual(record(logPath), { argv: expectedArgs, cwd: dir, workers: expectedWorkers, extra: "kept" });
	}
	for (const workers of [{ count: 2 }, { env: "OTHER", count: 2 }, { env: "RICHVIEW_VERIFY_WORKERS", arg: "--jobs", count: 2 }, { arg: "--jobs", count: 0 }, { arg: "--jobs", count: 1.5 }]) {
		const logPath = join(dir, "invalid.log");
		await assert.rejects(runScheduledCommand({ cmd: process.execPath, args: [], workers }, [script], { cwd: dir, env: gateEnv(), logPath, mode: "full", parallel: 2 }), /workers/);
		assert.equal(existsSync(logPath), false);
	}
	const logPath = join(dir, "invalid.log");
	await assert.rejects(runScheduledCommand({ cmd: process.execPath, args: [], workers: { arg: "--jobs", count: 2 } }, [script, "--jobs=4"], { cwd: dir, logPath, mode: "full", parallel: 2 }), /already exists/);
	// The runtime rejects the combination even when the shard options are malformed.
	await assert.rejects(runScheduledCommand({ cmd: process.execPath, args: [], workers: { env: "RICHVIEW_VERIFY_WORKERS", count: 2 }, shard: {} as GateCommand["shard"] }, [script], { cwd: dir, logPath, mode: "full", parallel: 2 }), /cannot combine/);
});

test("full Rust dispatch reserves two workers under the actual hold and other modes retain Cargo", async t => {
	const dir = fixture(t), target = join(dir, "target"), cargo = join(dir, "cargo"); mkdirSync(target);
	writeFileSync(cargo, `#!${process.execPath}\nrequire('node:fs').appendFileSync(process.env.RICHVIEW_BROWSER_EVENT_LOG, JSON.stringify({version:1,event:'browser-startup-relaunch',surface:'pdf',engine:'chromium',attempt:2,maxAttempts:2,timeoutMs:15000})+'\\n');\n${dump}`); chmodSync(cargo, 0o755);
	const env = gateEnv({ PATH: `${dir}:${process.env.PATH}`, CARGO_TARGET_DIR: target, CARGO_BUILD_TARGET: "configured-target", RICHVIEW_VERIFY_WORKERS: "99", COMMAND_EXTRA: "kept" });
	const args = ["test", "--workspace", "--locked", "--features", "richview-app/embedded-runtime,richview-editor/embedded-runtime", "--no-fail-fast", "--", "--ignored", "--skip", "metricflow_fixture", "--exact"];
	const release = await holdCargo(target);
	try {
		for (const [name, mode, parallel, sourceHold, pooled] of [
			["full", "full", 3, true, true], ["serial", "full", 1, true, false], ["release", "release", 3, true, false], ["unheld", "full", 3, false, false],
		] satisfies [string, string, number, boolean, boolean][]) {
			const logPath = join(dir, name + ".log"), admitted: number[] = [];
			const result = await runScheduledCommand({ cmd: "cargo", args }, args, {
				cwd: dir, env, logPath, mode, parallel, sourceHold,
				scheduleCommand: (run, permits = 1) => { assert.equal(cargoHolder(target), process.pid); admitted.push(permits); return run(); },
			});
			assert.equal(result.status, "pass", readFileSync(logPath, "utf8"));
			assert.equal(result.browserRelaunches?.length, 1, "Held Rust and original Cargo both retain native browser events");
			assert.deepEqual(admitted, [pooled ? 2 : 1]);
			assert.equal(existsSync(`${logPath}.rust/cargo-original.log`), pooled, "configured target must use the helper's complete original-Cargo fallback");
			assert.deepEqual(record(logPath), { argv: args, cwd: dir, workers: mode === "full" && parallel > 1 ? "1" : "99", extra: "kept" });
			assert.equal(cargoHolder(target), process.pid, "dispatch must not release the caller's source hold");
		}
	} finally { release(); }
	assert.equal(cargoHolder(target), 0);
});

test("process shards each reserve one worker and keep complete canonical evidence", async t => {
	const dir = fixture(t), script = join(dir, "shard.cjs"), out = join(dir, "out");
	writeFileSync(script, `const fs=require('node:fs'),path=require('node:path');const[i,n]=process.env.RICHVIEW_VERIFY_SHARD.split('/').map(Number);if(process.env.RICHVIEW_VERIFY_WORKERS!=='1')throw Error('unreserved workers');const cohort=['a','b'],selected=cohort.filter((_,p)=>p%n===i);fs.writeFileSync(path.join(process.argv[2],'types-'+i+'-of-'+n+'.json'),JSON.stringify({browser:'fixture',style:'sleek',width:1280,model_count:2,errors:[],completed:selected,results:selected.map(caseKey=>({caseKey,type:caseKey,ok:true})),shard:{index:i,count:n,cohort,selected}}));`);
	const weights: number[] = [];
	const result = await runScheduledCommand({ cmd: process.execPath, args: [script, out], shard: { kind: "interaction", count: 2, out } }, [script, out], {
		cwd: dir, env: gateEnv({ RICHVIEW_VERIFY_WORKERS: "99" }), logPath: join(dir, "shards.log"), mode: "full", parallel: 2,
		scheduleCommand: (run, permits = 1) => { weights.push(permits); return run(); },
	});
	assert.equal(result.status, "pass"); assert.deepEqual(weights, [1, 1]);
	assert.deepEqual(JSON.parse(readFileSync(join(out, "types.json"), "utf8")).completed, ["a", "b"]);
});
