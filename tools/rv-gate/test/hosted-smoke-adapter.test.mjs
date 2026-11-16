// Host reason (ARC-03): verify frozen-child invocation and private-artifact/receipt boundaries.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, statSync, symlinkSync, utimesSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import test from "node:test";
import { runHostedSmoke } from "../lib/hosted-smoke.mjs";
import { notRunParts } from "../lib/proc.mjs";
import { STEPS } from "../lib/steps.mjs";

const child = `import {readFileSync,writeFileSync,appendFileSync}from'node:fs';
import{join}from'node:path';import{parseArgs}from'node:util';import{createHash}from'node:crypto';
const{values:v}=parseArgs({options:{cwd:{type:'string'},bin:{type:'string'},'publish-wasm':{type:'string'},out:{type:'string'}}});
const hash=p=>createHash('sha256').update(readFileSync(p)).digest('hex');
const evidence={status:'pass',scope:'local-loopback',native:{closed:true},worker:{closed:true},artifacts:{native:hash(v.bin),publishWasm:hash(v['publish-wasm'])},unrun:[{name:'Deployed service',status:'unrun',reason:'No deployed fixture'}]};
writeFileSync(join(v.out,'invocation.json'),JSON.stringify({cwd:process.cwd(),args:v,target:process.env.CARGO_TARGET_DIR}));
`;

function fixture(t, finish = "writeFileSync(join(v.out,'hosted-smoke.json'),JSON.stringify(evidence));") {
	mkdirSync("tmp", { recursive: true });
	const root = realpathSync(mkdtempSync(resolve("tmp/hosted-adapter-"))), cwd = join(root, "frozen"), tempDir = join(root, "run"), snapshot = join(tempDir, "target"), runDir = join(root, "evidence");
	t.after(() => rmSync(root, { recursive: true, force: true }));
	mkdirSync(join(cwd, "tools/rv-gate"), { recursive: true });
	writeFileSync(join(cwd, "tools/rv-gate/hosted-smoke.mjs"), child + finish);
	const snapshotFiles = [];
	for (const [path, bytes] of [["release/richview", "private native bytes"], ["wasm32-unknown-unknown/release/richview_publish_server.wasm", "private wasm bytes"]]) {
		const file = join(snapshot, path); mkdirSync(join(file, ".."), { recursive: true }); writeFileSync(file, bytes);
		const stat = statSync(file); snapshotFiles.push({ path, size: stat.size, copiedSize: stat.size, mtimeMs: Math.round(stat.mtimeMs) });
	}
	const ctx = { cwd, tempDir, snapshot, snapshotFiles, runDir, completed: new Map([["build", "pass"], ["publish-tests", "pass"]]), ended: new Map([["build", Date.now()], ["publish-tests", Date.now()]]) };
	return { root, ctx, invocation: join(runDir, "hosted-smoke/invocation.json") };
}

test("the registered full function reads the frozen producer and private copies, retaining deployed gaps in the receipt", async t => {
	const step = STEPS.find(step => step.id === "hosted-smoke");
	assert.equal(step.fn, "hostedSmoke"); assert.equal(step.tier, "full"); assert.equal(step.needs, "build");
	assert.deepEqual(step.after, ["build", "publish-tests"]); assert.ok(step.entries.includes("tools/rv-gate/hosted-smoke.mjs"));
	const { ctx, invocation } = fixture(t), result = await runHostedSmoke(ctx);
	assert.equal(result.status, "pass", result.reason);
	const called = JSON.parse(readFileSync(invocation));
	assert.equal(called.cwd, ctx.cwd); assert.equal(called.args.cwd, ctx.cwd);
	assert.equal(called.args.bin, join(ctx.snapshot, "release/richview"));
	assert.equal(called.args["publish-wasm"], join(ctx.snapshot, "wasm32-unknown-unknown/release/richview_publish_server.wasm"));
	assert.equal(called.target, ctx.snapshot);
	assert.equal(result.details.artifacts.native, createHash("sha256").update("private native bytes").digest("hex"));
	assert.deepEqual(notRunParts(result), [{ what: "Deployed service", reason: "No deployed fixture" }]);
	assert.match(readFileSync(result.logs[0], "utf8"), /rv-gate: exit 0 after \d+ ms\n$/);
});

test("both prerequisite steps must pass before any hosted child starts", async t => {
	for (const id of ["build", "publish-tests"]) for (const status of [undefined, "fail", "unrun"]) {
		const { ctx, invocation } = fixture(t); ctx.completed.set(id, status);
		const result = await runHostedSmoke(ctx);
		assert.equal(result.status, "unrun"); assert.match(result.reason, new RegExp(id));
		assert.throws(() => readFileSync(invocation), { code: "ENOENT" });
	}
});

test("missing, stale, modified or external artifact copies cannot start a hosted child", async t => {
	for (const mutate of [
		ctx => delete ctx.snapshot,
		ctx => ctx.ended.delete("publish-tests"),
		ctx => ctx.snapshotFiles.pop(),
		ctx => ctx.snapshotFiles[1].mtimeMs = Date.now() + 10000,
		ctx => writeFileSync(join(ctx.snapshot, "release/richview"), "other bytes"),
		ctx => rmSync(join(ctx.snapshot, "wasm32-unknown-unknown/release/richview_publish_server.wasm")),
		ctx => { const file = join(ctx.snapshot, "release/richview"), outside = join(ctx.cwd, "external"), stat = statSync(file); writeFileSync(outside, readFileSync(file)); utimesSync(outside, stat.atime, stat.mtime); rmSync(file); symlinkSync(outside, file); },
	]) {
		const { ctx, invocation } = fixture(t); mutate(ctx);
		assert.equal((await runHostedSmoke(ctx)).status, "fail");
		assert.throws(() => readFileSync(invocation), { code: "ENOENT" });
	}
});

test("a child failure, missing fresh result, lost gaps, wrong hashes or changed inputs cannot produce a pass", async t => {
	for (const finish of [
		"writeFileSync(join(v.out,'hosted-smoke.json'),JSON.stringify(evidence));process.exitCode=7;",
		"", // A stale successful file must not substitute for this child's missing result.
		"writeFileSync(join(v.out,'hosted-smoke.json'),'{broken');",
		"evidence.native.closed=false;writeFileSync(join(v.out,'hosted-smoke.json'),JSON.stringify(evidence));",
		"evidence.unrun=[];writeFileSync(join(v.out,'hosted-smoke.json'),JSON.stringify(evidence));",
		"evidence.artifacts.native='wrong';writeFileSync(join(v.out,'hosted-smoke.json'),JSON.stringify(evidence));",
		"writeFileSync(join(v.out,'hosted-smoke.json'),JSON.stringify(evidence));appendFileSync(v.bin,'changed');",
	]) {
		const { ctx } = fixture(t);
		assert.equal((await runHostedSmoke(ctx)).status, "pass", "Establish an actual prior successful transport result");
		writeFileSync(join(ctx.cwd, "tools/rv-gate/hosted-smoke.mjs"), child + finish);
		const result = await runHostedSmoke(ctx); assert.equal(result.status, "fail", finish); assert.ok(result.logs.length);
	}
});
