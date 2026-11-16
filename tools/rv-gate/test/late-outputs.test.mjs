// Host reason (ARC-03): verify immutable private artifact copies against real shared-file writes.
import assert from "node:assert/strict";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, utimesSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import test from "node:test";
import { addLateOutputs } from "../lib/late-outputs.mjs";

const path = "wasm32-unknown-unknown/release/richview_publish_server.wasm";
function fixture(t) {
	mkdirSync("tmp", { recursive: true });
	const root = mkdtempSync(resolve("tmp/late-outputs-")), target = join(root, "shared"), snapshot = join(root, "private"), source = join(target, path), copy = join(snapshot, path);
	t.after(() => rmSync(root, { recursive: true, force: true }));
	mkdirSync(join(source, ".."), { recursive: true }); mkdirSync(snapshot);
	writeFileSync(source, "this run's Wasm");
	return { target, source, copy, ctx: { snapshot, snapshotFiles: [], ended: new Map() } };
}

test("late outputs wait for publish-tests completion instead of adopting an earlier shared artifact", t => {
	const f = fixture(t); addLateOutputs(f.ctx, f.target);
	assert.equal(existsSync(f.copy), false); assert.deepEqual(f.ctx.snapshotFiles, []);
	f.ctx.ended.set("publish-tests", Date.now()); addLateOutputs(f.ctx, f.target);
	assert.equal(readFileSync(f.copy, "utf8"), "this run's Wasm"); assert.equal(f.ctx.snapshotFiles.length, 1);
});

test("later function steps retain the original private Wasm and provenance after another checkout rebuilds", t => {
	const f = fixture(t), ended = Date.now(); f.ctx.ended.set("publish-tests", ended);
	addLateOutputs(f.ctx, f.target); const records = structuredClone(f.ctx.snapshotFiles);
	writeFileSync(f.source, "another checkout's Wasm");
	const later = new Date(ended + 10000); utimesSync(f.source, later, later);
	addLateOutputs(f.ctx, f.target);
	assert.equal(readFileSync(f.copy, "utf8"), "this run's Wasm"); assert.deepEqual(f.ctx.snapshotFiles, records);
	rmSync(f.source); addLateOutputs(f.ctx, f.target);
	assert.equal(readFileSync(f.copy, "utf8"), "this run's Wasm"); assert.deepEqual(f.ctx.snapshotFiles, records);
});

test("a first copy from after publish-tests is rejected rather than recorded as this run's artifact", t => {
	const f = fixture(t), ended = Date.now(); f.ctx.ended.set("publish-tests", ended);
	const later = new Date(ended + 10000); utimesSync(f.source, later, later);
	addLateOutputs(f.ctx, f.target);
	assert.equal(existsSync(f.copy), false); assert.deepEqual(f.ctx.snapshotFiles, []);
});
