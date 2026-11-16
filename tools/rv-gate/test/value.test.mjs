import test from "node:test";
import assert from "node:assert/strict";
import {mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync, linkSync} from "node:fs";
import {join} from "node:path";
import {valueLedger, valueMarkdown, writeValue, rotationFromHistory} from "../lib/value.mjs";

const now = new Date("2026-10-30T12:00:00Z");
const result = (id, status = "pass", runMs = 250_000) => ({id, status, runMs, durationMs: 900_000});
const receipt = (number, steps, extra = {}) => ({id: `full-${number}`, kind: "full", parallel: 4,
	at: new Date(Date.UTC(2026, 9, 1, number)).toISOString(), steps, ...extra});
const ledger = (receipts, options = {}) => valueLedger(receipts, {now, ...options});
const row = (output, id) => output.steps.find(step => step.id === id);
const greenHistory = (id = "slow", runMs = 250_000, count = 20) => Array.from({length: count}, (_, index) => receipt(index, [result(id, "pass", runMs)]));

test("publishing a value ledger replaces the file without truncating an existing reader's inode", t => {
	mkdirSync("tmp", {recursive: true}); const dir = mkdtempSync(join(process.cwd(), "tmp/value-atomic-"));
	t.after(() => rmSync(dir, {recursive: true, force: true}));
	const target = join(dir, "value.json"), previous = join(dir, "previous.json"), old = '{"previous":"complete"}\n';
	writeFileSync(target, old); linkSync(target, previous);
	writeValue({receipts: greenHistory(), dir});
	assert.equal(readFileSync(previous, "utf8"), old, "A reader of the previous inode must retain the complete old ledger");
	assert.equal(JSON.parse(readFileSync(target, "utf8")).receipts.length, 20);
});

function freeze(value) {
	if (value && typeof value === "object") { Object.values(value).forEach(freeze); Object.freeze(value); }
	return value;
}

test("final verdicts, flaky recoveries, sole failures and unrun observations have independent counts", () => {
	const input = [receipt(1, [result("chromium", "pass", 10), result("webkit", "fail", 20), {id: "fonts", status: "unrun"}]),
		receipt(2, [result("chromium", "fail", 20), {...result("webkit", "fail", 30), attempts: [{status: "fail"}, {status: "fail"}]}]),
		receipt(3, [result("chromium", "pass", 10), {...result("webkit", "pass", 15), attempts: [{status: "fail"}, {status: "pass"}]}]),
		receipt(4, [{id: "chromium", status: "unrun"}, result("webkit", "pass", 16)])];
	const before = JSON.stringify(input), output = ledger(freeze(input)), webkit = row(output, "webkit");
	assert.deepEqual({runs: webkit.runs, pass: webkit.pass, fail: webkit.fail, flaky: webkit.flaky, soleFailures: webkit.soleFailures},
		{runs: 4, pass: 2, fail: 2, flaky: 1, soleFailures: 1});
	assert.equal(webkit.medianMs, 18);
	assert.equal(row(output, "fonts").runs, 0);
	assert.equal(row(output, "fonts").unrun, 1);
	assert.equal(row(output, "fonts").medianMs, null);
	assert.deepEqual(output.conditionalFailures["webkit|chromium-pass"],
		{step: "webkit", given: "chromium", runs: 2, fail: 1, flaky: 1, receipts: ["full-1", "full-3"]});
	assert.equal(JSON.stringify(input), before);
});

test("Firefox and export conditional failures require their actual partner pass", () => {
	const output = ledger([receipt(1, [result("chromium"), result("firefox", "fail"), result("contract"), result("exports", "fail")]),
		receipt(2, [result("chromium", "fail"), result("firefox", "fail"), {id: "contract", status: "unrun"}, result("exports", "fail")]),
		receipt(3, [result("chromium"), {id: "firefox", status: "unrun"}, result("contract"), result("exports")])]);
	assert.equal(output.conditionalFailures["firefox|chromium-pass"].runs, 1);
	assert.equal(output.conditionalFailures["firefox|chromium-pass"].fail, 1);
	assert.equal(output.conditionalFailures["exports|contract-pass"].runs, 2);
	assert.equal(output.conditionalFailures["exports|contract-pass"].fail, 1);
});

test("rotation needs twenty known green executions, with the last twenty defining the streak", () => {
	assert.equal(row(ledger(greenHistory()), "slow").rotationEligible, true);
	assert.equal(row(ledger(greenHistory("slow", 200_000)), "slow").recommendation, "rotate");
	assert.equal(row(ledger(greenHistory("slow", 199_999)), "slow").rotationEligible, false);
	assert.equal(row(ledger(greenHistory("slow", 250_000, 19)), "slow").rotationEligible, false);
	const input = greenHistory("slow", 250_000, 21); input[0].steps[0].status = "fail";
	assert.equal(row(ledger(input), "slow").rotationEligible, true);
	input[1].steps[0].status = "fail";
	assert.equal(row(ledger(input), "slow").rotationEligible, false);
});

test("a recovered first failure breaks a green streak without changing its final pass count", () => {
	const input = greenHistory(); input[9].steps[0].attempts = [{status: "fail"}, {status: "pass"}];
	const slow = row(ledger(input), "slow");
	assert.equal(slow.pass, 20); assert.equal(slow.fail, 0); assert.equal(slow.flaky, 1);
	assert.equal(slow.rotationEligible, false);
});

test("one measured sample cannot qualify nineteen unknown parallel durations", () => {
	const input = greenHistory(); for (let index = 0; index < 19; index++) delete input[index].steps[0].runMs;
	const slow = row(ledger(input), "slow");
	assert.equal(slow.medianMs, 250_000); assert.equal(slow.timingSamples, 1); assert.equal(slow.unknownTimings, 19);
	assert.equal(slow.rotationEligible, false);
	assert.deepEqual(slow.timingSources, {telemetry: 1, unknown: 19});
	assert.equal(row(ledger(greenHistory("slow", null)), "slow").medianMs, null);
});

test("artifact dependencies and cheap steps stay kept despite expensive parallel receipt spans", () => {
	const input = greenHistory("build");
	for (const item of input) item.steps.push(result("captures"), result("producer"), result("cheap", "pass", 18_000));
	const output = ledger(input, {steps: [{id: "consumer", needs: "producer"}]});
	for (const id of ["build", "captures", "producer", "cheap"]) {
		assert.equal(row(output, id).rotationEligible, false); assert.equal(row(output, id).recommendation, "keep");
	}
});

test("cost provenance distinguishes serial receipts, retained child logs and unknown queue time", () => {
	const input = [receipt(1, [{id: "logged", status: "pass", durationMs: 900_000, logs: ["actual"]}]),
		receipt(2, [{id: "serial", status: "pass", durationMs: 1825}], {kind: "prod", parallel: 1}),
		receipt(3, [{id: "gone", status: "pass", durationMs: 900_000, logs: ["pruned"]}])];
	const output = ledger(input, {readLog: path => path === "actual" ? "rv-gate: exit 0 after 2727 ms\n" : null});
	assert.equal(row(output, "logged").medianMs, 2727);
	assert.equal(row(output, "logged").medianQueuedMs, null);
	assert.deepEqual(row(output, "logged").timingSources, {"log-footer-lower-bound": 1});
	assert.equal(row(output, "serial").medianMs, 1825);
	assert.equal(row(output, "gone").medianMs, null);
	assert.match(output.timing, /lower bound/);
});

test("rotation uses durable receipt timing while retained command logs remain advisory",()=>{
	const logged=greenHistory();for(const r of logged){delete r.steps[0].runMs;r.steps[0].logs=["prunable.log"];}
	const advisory=row(ledger(logged,{readLog:()=>"rv-gate: exit 0 after 300000 ms\n"}),"slow");
	assert.equal(advisory.timingSamples,20);assert.equal(advisory.medianMs,300000);
	assert.equal(advisory.rotationEligible,false,"Pruning a command log must not invalidate a once-accepted historical rotation");
	const serial=structuredClone(logged);for(const r of serial){r.parallel=1;r.steps[0].durationMs=300000;}
	assert.equal(row(ledger(serial,{readLog:()=>{throw Error("pruned");}}),"slow").rotationEligible,true,"Immutable serial receipts remain replayable after log cleanup");
});

test("inclusive date filtering, stable ordering and immutable receipt identity prevent inflated history", () => {
	const a = receipt(1, [result("slow")]), b = receipt(2, [result("slow")]);
	const output = ledger([b, a, structuredClone(a), receipt(3, [result("slow")], {kind: "dev"})], {since: "2026-10-01"});
	assert.deepEqual(output.receipts.map(item => item.id), ["full-1", "full-2"]);
	assert.equal(row(output, "slow").runs, 2);
	assert.equal(row(ledger([a, b], {since: "2026-10-02"}), "slow"), undefined);
	for (const since of ["2026-02-30", "2026-2-1", "yesterday"]) assert.throws(() => ledger([], {since}), /calendar date/);
	assert.doesNotThrow(() => ledger([], {since: "2024-02-29"}));
	assert.throws(() => ledger([a, {...a, steps: [result("different")]}]), /Conflicting immutable receipt/);
	assert.throws(() => ledger([receipt(1, [result("same"), result("same")])]), /duplicate steps/);
	assert.throws(() => ledger([receipt(1, [{id: "bad", status: "maybe"}])]), /Unknown step verdict/);
	const history=greenHistory();for(const r of history)r.phase="prelaunch";
	const plan=rotationFromHistory({phase:"prelaunch",since:"2026-10-01",by:"Daniel",rotation:["slow"]},{mode:"full",steps:[{id:"slow"}],receipts:[...history,...history]});
	assert.equal(plan.ordinal,20,"Duplicate immutable receipts cannot advance the rotation cycle");
	assert.equal(plan.qualifications[0].receipts.length,20);assert.equal(plan.qualifications[0].receiptHashes.length,20);
});

test("Markdown exposes unknown timing, conditional denominators and advisory recommendations", () => {
	const output = ledger([receipt(1, [result("chromium"), {id: "firefox", status: "fail", durationMs: 900_000}])]);
	const markdown = valueMarkdown(output);
	assert.match(markdown, /\| firefox \| 1 \| 0 \| 1 \| 0 \| 1 \| unknown \| 0\/1 \| unknown: 1 \| keep \|/);
	assert.match(markdown, /firefox\\\|chromium-pass \| 1 \| 1 \| 0/);
	assert.match(markdown, /Daniel decides retirement/);
	assert.match(valueMarkdown(ledger(greenHistory("slow", 199_999))), /199\.999/);
});

test("writeValue persists both review artifacts while leaving source receipts unchanged", t => {
	const parent = join(process.cwd(), "tmp"); mkdirSync(parent, {recursive: true});
	const root = mkdtempSync(join(parent, "value-test-")); t.after(() => rmSync(root, {recursive: true, force: true}));
	const log = join(root, "command.log"); writeFileSync(log, "rv-gate: exit 0 after 20 ms\n");
	const source = join(root, "receipt.json"), original = JSON.stringify(receipt(1, [{id: "cheap", status: "pass", durationMs: 900, logs: [log]}]));
	writeFileSync(source, original); const output = writeValue({receipts: [JSON.parse(readFileSync(source))], dir: join(root, "out")});
	assert.equal(output.json, join(root, "out", "value.json")); assert.equal(output.markdown, join(root, "out", "value.md"));
	assert.equal(JSON.parse(readFileSync(output.json)).steps[0].medianMs, 20);
	assert.deepEqual(JSON.parse(readFileSync(output.json)).steps[0].timingSources, {"log-footer-lower-bound": 1});
	assert.match(readFileSync(output.markdown, "utf8"), /\| cheap \| 1 \| 1 \| 0/);
	assert.equal(readFileSync(source, "utf8"), original);
});


test("reused passes neither lengthen a green streak nor act as executed conditional partners", () => {
    const input = greenHistory("slow", 250_000, 19);
    input.push(receipt(20, [{...result("slow", "pass", 0), reused: "previous-receipt"}]));
    const slow = row(ledger(input), "slow");
    assert.equal(slow.runs, 19); assert.equal(slow.reused, 1); assert.equal(slow.unrun, 0);
    assert.equal(slow.rotationEligible, false);
    const paired = ledger([receipt(1, [{...result("chromium"), reused: "previous"}, result("webkit", "fail")]),
        receipt(2, [result("chromium"), {...result("webkit"), reused: "previous"}])]);
    assert.equal(paired.conditionalFailures["webkit|chromium-pass"], undefined);
    assert.equal(row(paired, "webkit").runs, 1); assert.equal(row(paired, "webkit").reused, 1);
});

test("historical correlation cannot create an unapproved retirement recommendation", () => {
    const input = greenHistory("webkit", 100_000);
    for (const item of input) item.steps.push(result("chromium", "pass", 100_000));
    const output = ledger(input);
    assert.equal(output.conditionalFailures["webkit|chromium-pass"].runs, 20);
    assert.equal(output.conditionalFailures["webkit|chromium-pass"].fail, 0);
    assert.equal(row(output, "webkit").recommendation, "keep");
    assert.match(output.retirement, /withheld/);
});
