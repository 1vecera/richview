// Host reason (ARC-03): test the affected-only gate's selection without running a gate.
import test from "node:test";
import assert from "node:assert/strict";
import type { WorkspaceCrate } from "../lib/impact.mts";
import type { GateStep } from "../lib/steps.mts";
import type { StoredReceipt } from "../lib/store.mts";
import { commandId, failureCounts, selectAffected } from "../lib/selection.mts";

const gateFields = {tier: "full" as const, title: "Fixture", cards: []};
const steps = [
	{...gateFields, id: "chromium", parallel: 2, run: [
		{cmd: "node", args: ["scripts/verify-chart.mjs"], entry: "scripts/verify-chart.mjs"},
		{cmd: "node", args: ["scripts/verify-site.mjs"], entry: "scripts/verify-site.mjs"},
	]},
	{...gateFields, id: "parallel-shaders", parallel: 3, run: ["chromium", "firefox", "webkit"].map(browser => ({cmd: "node", args: ["scripts/verify-shader.mjs"], env: {RICHVIEW_VERIFY_BROWSER: browser}}))},
	{...gateFields, id: "tool-tests", run: [{cmd: "node", args: ["--test", "tools/rv-gate/test/*.mjs"]}]},
	{...gateFields, id: "firefox", run: [{cmd: "node", args: ["scripts/verify-chart.mjs"]}]},
];
const areas = {version: 1 as const, areas: [
	{name: "chart", paths: ["chart/"], steps: ["chromium", "parallel-shaders"], commands: {chromium: ["scripts/verify-chart.mjs"]}},
	{name: "gate", paths: ["tools/rv-gate/"], steps: ["tool-tests"]},
]};
const read = (path: string) => path === "scripts/verify-chart.mjs" ? 'import "./chart-helper.mjs"' : null;
const find = <T extends { id: string; reason?: string },>(plan: readonly T[], id: string) => {
	const row = plan.find(item => item.id === id);
	assert.ok(row, id);
	return row;
};
const unit = (id: string, index: number) => { const step = steps.find(item => item.id === id); assert.ok(step, id); return commandId(step, step.run[index]); };

test("a TypeScript path keeps the command receipt identity and still distinguishes options", () => {
	const step = steps[0];
	const before = step.run[0];
	const after = { ...before, args: ["scripts/verify-chart.ts"], entry: "scripts/verify-chart.ts" };
	assert.equal(commandId(step, after), commandId(step, before));
	assert.notEqual(commandId(step, { ...after, args: [...after.args, "--strict"] }), commandId(step, before));
});

test("mapped areas choose commands and Chrome only, while launch restores engines", () => {
	const plan = selectAffected(steps, ["chart/scale.rs"], {areas, read});
	assert.equal(find(plan, unit("chromium", 0)).status, "run");
	assert.equal(find(plan, unit("chromium", 1)).status, "skipped");
	assert.equal(find(plan, unit("parallel-shaders", 0)).status, "run");
	assert.match(find(plan, unit("parallel-shaders", 1)).reason ?? "", /Chrome only/);
	assert.equal(find(plan, "firefox").status, "skipped");
	assert.equal(find(selectAffected(steps, ["chart/scale.rs"], {areas, read, phase: "launch"}), unit("parallel-shaders", 1)).status, "run");
});

test("traced imports select their command; unknown product fallback uses the failure threshold", () => {
	const traced = selectAffected(steps, ["scripts/chart-helper.mjs"], {areas, read});
	assert.equal(find(traced, unit("chromium", 0)).status, "run");
	const counts = new Map([["chromium", {step: 0, commands: [5, 4]}]]);
	const plan = selectAffected(steps, ["unmapped/product.rs"], {areas, counts, read});
	assert.equal(find(plan, unit("chromium", 0)).status, "run");
	assert.equal(find(plan, unit("chromium", 1)).status, "paused");
	assert.match(find(plan, unit("chromium", 1)).reason ?? "", /4 recorded failures/);
	assert.equal(find(selectAffected(steps, ["tools/rv-gate/x.mjs"], {areas, read}), "tool-tests").status, "run");
});

test("only final full failures count; ambiguous old results use a conservative step upper bound", () => {
	const receipts: Pick<StoredReceipt, "kind" | "steps">[] = [
		{kind: "full", steps: [{...gateFields, id: "chromium", status: "fail", details: {commands: [{status: "fail", command: "node scripts/verify-chart.mjs"}, {status: "pass", command: "node scripts/verify-site.mjs"}]}}]},
		{kind: "full", steps: [{...gateFields, id: "chromium", status: "fail"}]},
		{kind: "release", steps: [{...gateFields, id: "chromium", status: "fail"}]},
		{kind: "full", steps: [{...gateFields, id: "chromium", status: "pass", flaky: true}]},
	];
	assert.deepEqual(failureCounts(receipts, steps).get("chromium"), {step: 2, commands: [2, 1], estimated: [true, true]});
});

test("a previous failed command is selected without selecting its peers", () => {
	const plan = selectAffected(steps, [], {areas, read, retry: [unit("chromium", 1)]});
	assert.equal(find(plan, unit("chromium", 0)).status, "skipped");
	assert.equal(find(plan, unit("chromium", 1)).status, "run");
});


test("required build and declared artifact dependencies are selected", () => {
	const all: GateStep[] = [{...gateFields, id: "build", run: [{cmd: "build", args: []}]}, {...gateFields, id: "visual", needs: "build"}, {...gateFields, id: "capture", needs: "build", after: ["visual"]}];
	const rules = {version: 1 as const, areas: [{name: "capture", paths: ["capture/"], steps: ["capture"]}]};
	const plan = selectAffected(all, ["capture/file"], {areas: rules, read});
	assert.deepEqual(plan.map(item => item.status), ["run", "run", "run"]);
	assert.match(find(plan, "build").reason ?? "", /Prerequisite/);
});

test("command identities survive registry reorder and reject duplicates", () => {
	const commands = steps[0].run;
	assert.equal(commandId(steps[0], commands[0]), commandId({...steps[0], run: [...commands].reverse()}, commands[0]));
	assert.throws(() => selectAffected([{...steps[0], run: [commands[0], commands[0]]}], [], {areas, read}), /Duplicate/);
});


test("included inputs trace through crate dependents and select Rust verification", () => {
	const graph: WorkspaceCrate[] = [{name: "core", dir: "core", deps: [], targets: []}, {name: "app", dir: "app", deps: ["core"], targets: []}];
	const inputs = new Map([["core", new Set(["external/input.css"])] ]);
	const plan = selectAffected([{...gateFields, id: "rust-tests", run: [{cmd: "cargo", args: ["test"]}]}], ["external/input.css"], {areas, graph, inputs, read});
	assert.equal(plan[0].status, "run");
	assert.match(plan[0].reason ?? "", /app, core/);
});

test("broad fallback can use real-defect steps without changing selection code", () => {
	const fallback = {mode: "real-defects" as const, realDefectSteps: ["chromium"]};
	const plan = selectAffected(steps, ["unmapped/path.rs"], {areas, read, fallback});
	assert.equal(find(plan, unit("chromium", 0)).status, "run");
	assert.equal(find(plan, unit("chromium", 1)).status, "run");
	assert.equal(find(plan, unit("parallel-shaders", 0)).status, "paused");
});


test("explicit entry inputs select built-in checks", () => {
	const plan = selectAffected([{...gateFields, id:"hosted", entries:["tools/rv-gate/hosted-smoke.mjs"], fn:"hostedSmoke"}], ["tools/rv-gate/hosted-smoke.mjs"], {areas:{version: 1, areas:[]}, read});
	assert.equal(plan[0].status, "run");
});


test("real-defect fallback fails closed on an unknown or empty step list", () => {
	for (const realDefectSteps of [[], ["missing"]]) assert.throws(() => selectAffected(steps, ["unknown/file"], {areas, read, fallback:{mode:"real-defects", realDefectSteps}}), /Invalid real-defect/);
});

test("docs-match runs for every reviewed documentation input", async () => {
	const {readFileSync} = await import("node:fs");
	const {stepsForTier} = await import("../lib/steps.mts");
	const registry = stepsForTier("full", "full"), map = JSON.parse(readFileSync(new URL("../areas.json", import.meta.url), "utf8"));
	for (const path of ["AGENTS.md", "README.md", "CLAUDE.md", ".claude/rules/example.md", "skill/SKILL.md", "docs/guide/example.md", "docs/public/example.md"]) {
		assert.equal(find(selectAffected(registry, [path], {areas: map, read: () => null}), "docs-match").status, "run", path);
	}
});

test("golden runs for chart regression files and public index", async () => {
	const {readFileSync} = await import("node:fs");
	const {stepsForTier} = await import("../lib/steps.mts");
	const registry = stepsForTier("full", "full"), map = JSON.parse(readFileSync(new URL("../areas.json", import.meta.url), "utf8"));
	for (const path of ["benchmarks/chart-regression/example.rmd", "docs/public/index.md"]) assert.equal(find(selectAffected(registry, [path], {areas: map, read: () => null}), "golden").status, "run", path);
});

test("dependency policy changes run dependency checks", async () => {
	const {readFileSync} = await import("node:fs");
	const {stepsForTier} = await import("../lib/steps.mts");
	const registry = stepsForTier("full", "full"), map = JSON.parse(readFileSync(new URL("../areas.json", import.meta.url), "utf8"));
	assert.ok(selectAffected(registry, ["deny.toml"], {areas: map, read: () => null}).filter(item => item.step === "deps").every(item => item.status === "run"));
});

test("failed attempts in an ultimately passing flaky full do not count toward the threshold", () => {
	const flaky: Pick<StoredReceipt, "kind" | "steps"> = {kind: "full", steps: [{...gateFields, id: "chromium", status: "pass", flaky: true, details: {commands: [{status: "fail", command: "node scripts/verify-chart.mjs"}, {status: "pass", command: "node scripts/verify-site.mjs"}]}}]};
	const counts = failureCounts(Array(5).fill(flaky), steps);
	assert.equal(counts.get("chromium")?.commands[0], 0);
	assert.equal(find(selectAffected(steps, ["unmapped/product.rs"], {areas, counts, read}), unit("chromium", 0)).status, "paused");
});
