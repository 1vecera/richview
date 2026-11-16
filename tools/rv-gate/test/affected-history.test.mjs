// Host reason (ARC-03): keep failure history bounded and bound to stored receipt identities.
import test from "node:test";
import assert from "node:assert/strict";
import {mkdtempSync, rmSync, writeFileSync} from "node:fs";
import {tmpdir} from "node:os";
import {join} from "node:path";
import {fullReceipts, summarizeHistory} from "../lib/affected-history.mjs";
import {commandId} from "../lib/selection.mjs";

test("stream full receipts once in order, excluding notes and other modes", t => {
	const dir = mkdtempSync(join(tmpdir(), "affected-history-")); t.after(() => rmSync(dir, {recursive: true, force: true}));
	const first = "full-20261101T000000Z-abc123", second = "full-20261102T000000Z-def456";
	for (const id of [second, first]) writeFileSync(join(dir, `${id}.json`), JSON.stringify({id, kind: "full", steps: []}));
	writeFileSync(join(dir, `${first}.promotion.json`), "{}");
	writeFileSync(join(dir, "release-20261102T000000Z-def456.json"), "{}");
	assert.deepEqual([...fullReceipts(dir)].map(receipt => receipt.id), [first, second]);
	writeFileSync(join(dir, `${first}.json`), JSON.stringify({id: second, kind: "full"}));
	assert.throws(() => [...fullReceipts(dir)], /identity/);
});

test("command medians use final passing observations and preserve unknown timings", () => {
	const step={id:"chromium",parallel:2,run:[{cmd:"node",args:["scripts/a.mjs"]},{cmd:"node",args:["scripts/b.mjs"]}]};
	const receipts=[
		{kind:"full",steps:[{id:"chromium",status:"pass",durationMs:100,details:{commands:[{status:"pass",command:"node scripts/a.mjs",durationMs:1000},{status:"fail",command:"node scripts/b.mjs",durationMs:5000}]}}]},
		{kind:"full",steps:[{id:"chromium",status:"pass",durationMs:300,details:{commands:[{status:"pass",command:"node scripts/a.mjs",durationMs:3000},{status:"pass",command:"node scripts/changed.mjs",durationMs:7000}]}}]},
	];
	const {durations} = summarizeHistory(receipts,[step]);
	assert.equal(durations.get(commandId(step,step.run[0])),2000);
	assert.equal(durations.has(commandId(step,step.run[1])),false);
	assert.equal(durations.get("chromium"),200);
});
