// Tests for the release run's plan (DEL-81): which steps a batch runs, in which phase, and how
// a failure stops the rest.

import assert from "node:assert/strict";
import test from "node:test";
import { workspaceGraph } from "../lib/impact.mjs";
import { batchCommits, batchFields, releasePlan, reusable, reuseEnvironment, reuseKey, runPool, stepSources } from "../lib/release.mjs";
import { STEPS, WASM_CRATES } from "../lib/steps.mjs";
import { wasmLinted } from "../lib/fast.mjs";

const root = "/work/richview";

test("combined-state history has no batch limit", () => {
	const log = n => Array.from({ length: n }, (_, i) => `${i.toString(16).padStart(40, "0")}\x1fChange ${i}\n\nRefs S27-772\x1e`).join("\n");
	assert.equal(batchCommits(log(2000)).length, 2000);
});
const graph = workspaceGraph(
	{
		packages: [
			{ name: "richview-chart", manifest_path: `${root}/richview-chart/Cargo.toml`, dependencies: [], targets: [{ kind: ["rlib"], name: "richview_chart", src_path: `${root}/richview-chart/src/lib.rs` }] },
			{
				name: "richview-core",
				manifest_path: `${root}/crates/richview-core/Cargo.toml`,
				dependencies: [{ name: "richview-chart" }],
				targets: [{ kind: ["rlib"], name: "richview_core", src_path: `${root}/crates/richview-core/src/lib.rs` }],
			},
			{
				name: "richview-publish",
				manifest_path: `${root}/crates/richview-publish/Cargo.toml`,
				dependencies: [{ name: "richview-core" }],
				targets: [{ kind: ["lib"], name: "richview_publish", src_path: `${root}/crates/richview-publish/src/lib.rs` }],
			},
		],
	},
	root,
);
const ids = (steps) => steps.map((step) => step.id);

test("combined diagnostics include the whole system even when pre-prod already equals dev", () => {
	const plan = releasePlan(STEPS, [], graph, new Map(), { combined: true });
	const selected = new Set(ids([...plan.cheap, ...plan.parallel, ...plan.afterBuild, ...plan.alone]));
	for (const step of ["build", "rust-tests", "rust-slow", "chromium", "perf", "fonts", "exports", "assets", "packaging"]) assert.ok(selected.has(step), step);
	assert.deepEqual(plan.crates, graph.map(crate => crate.name));
	assert.equal(plan.skipped.length, 0);
});

test("a product change runs the paperwork first, the build beside the Cargo checks, then every suite that reads the build", () => {
	const plan = releasePlan(STEPS, ["crates/richview-core/src/render.rs"], graph, new Map());
	assert.deepEqual(ids(plan.cheap).slice(0, 2), ["pins", "entrypoints"]);
	assert.ok(ids(plan.cheap).includes("coverage") && ids(plan.cheap).includes("retired-tests"));
	assert.ok(!ids([...plan.cheap, ...plan.parallel, ...plan.afterBuild]).includes("pr-fields"), "PR fields belong to one PR, not a batch");
	assert.equal(plan.parallel[0].id, "build");
	assert.ok(ids(plan.parallel).includes("rust-tests") && ids(plan.parallel).includes("clippy"));
	const firstCargo = plan.parallel.findIndex((step, i) => i > 0 && step.run?.some((c) => c.cmd === "cargo"));
	assert.ok(plan.parallel.slice(firstCargo).every((step) => step.run?.some((c) => c.cmd === "cargo")), "quick steps without Cargo start before the Cargo checks");
	assert.ok(ids(plan.afterBuild).includes("chromium") && ids(plan.afterBuild).includes("captures"));
	assert.ok(plan.afterBuild.every((step) => step.needs === "build"));
	assert.ok(ids(plan.afterBuild).includes("exports"), "the full tier's exports step joins when figures or rendering change");
	assert.deepEqual(plan.crates.sort(), ["richview-core", "richview-publish"]);
	assert.ok(ids(plan.parallel).includes("tool-tests"), "the tool tests read more than tools/ and run in every release");
});

test("a tools-only batch runs the paperwork and the tool tests, and no build", () => {
	const plan = releasePlan(STEPS, ["tools/rv-gate/lib/fast.mjs", "AGENTS.md"], graph, new Map());
	assert.deepEqual(ids(plan.parallel), ["tool-tests"]);
	assert.deepEqual(plan.afterBuild, []);
	assert.ok(plan.skipped.some((skip) => skip.id === "rust-tests"));
	assert.ok(plan.skipped.some((skip) => skip.id === "chromium"));
});

test("a product file no crate traces still runs the Rust steps; docs the build renders count as product", () => {
	const styles = releasePlan(STEPS, ["assets/styles/palette.yaml"], graph, new Map());
	assert.ok(ids(styles.parallel).includes("rust-tests"), "Rust tests may read the file at run time");
	const publicDocs = releasePlan(STEPS, ["docs/public/charts.md"], graph, new Map());
	assert.equal(publicDocs.parallel[0].id, "build");
	const guide = releasePlan(STEPS, ["docs/guide/styles.md"], graph, new Map());
	assert.deepEqual(guide.afterBuild, []);
});

test("a step runs when a module it imports or a gate fixture it names changes, not only its entry script", () => {
	const perf = releasePlan(STEPS, ["tools/rv-gate/lib/perf.mjs"], graph, new Map());
	assert.deepEqual(ids(perf.alone), ["perf"]);
	const fixture = releasePlan(STEPS, ["tools/rv-gate/fixtures/cross-style/precision.json"], graph, new Map());
	assert.ok(ids(fixture.afterBuild).includes("cross-style"));
	const journey = releasePlan(STEPS, ["tools/rv-gate/lib/journey.mjs"], graph, new Map());
	assert.ok(ids(journey.afterBuild).includes("journeys"));
	// The sources come from the step's scripts and their relative imports, read through `read`.
	const fake = { "a.mjs": 'import { x } from "./lib/b.mjs";\nconst dir = "tools/rv-gate/fixtures/demo";', "lib/b.mjs": 'export * from "../c.mjs";', "c.mjs": "" };
	const sources = stepSources({ run: [{ cmd: "node", args: ["a.mjs"] }] }, (path) => fake[path] ?? null);
	assert.deepEqual(sources.files.sort(), ["a.mjs", "c.mjs", "lib/b.mjs"]);
	assert.deepEqual(sources.folders, ["tools/rv-gate/fixtures/demo/"]);
});

test("a changed suite script runs that suite's step against a build even when no product file changed", () => {
	const plan = releasePlan(STEPS, ["scripts/verify-no-js.mjs"], graph, new Map());
	assert.deepEqual(ids(plan.afterBuild), ["no-js"]);
	assert.equal(plan.parallel[0].id, "build");
});

test("batch commits come oldest first with their fields, and the batch's retired tests and items are pooled", () => {
	const log = [
		"bbb\x1fSecond change\n\nRefs S27-2\ndecision: feature\nitems: FEA-CH-67\n\n## Retired tests\n- `old.mjs`: replaced by new.mjs\n\x1e",
		"aaa\x1fFirst change (#12)\n\nRefs S27-1\n\x1e",
	].join("");
	const commits = batchCommits(log);
	assert.deepEqual(
		commits.map((c) => [c.commit, c.subject, c.fields.issue]),
		[
			["aaa", "First change (#12)", "S27-1"],
			["bbb", "Second change", "S27-2"],
		],
	);
	const fields = batchFields(commits);
	assert.deepEqual(fields.items, ["FEA-CH-67"]);
	assert.equal(fields.retired.length, 1);
	// Prose in an items line is no item: it is listed for the receipt, not sent to coverage.
	const prose = batchFields(batchCommits("ccc\x1fSurface\n\nitems: folder input (4f5cf3e); STY-05\nassumed: ARC-68 (as proposed)\n\x1e"));
	assert.deepEqual(prose.items, ["STY-05"]);
	assert.deepEqual(prose.assumed, ["ARC-68"]);
	// One entry per commit, with a count of the words and no list of them.
	assert.deepEqual(prose.malformed, [{ commit: "ccc", near: [], words: 5 }]);
	// An ID in parentheses is that ID; a near miss is named, not dropped (review 1e note 12).
	const typo = batchFields(batchCommits("ddd\x1fT\n\nitems: (INT-17), FEA-CH37 [DEL-05]\nassumed: (STY-05)\n\x1e"));
	assert.deepEqual(typo.items, ["INT-17", "DEL-05"]);
	assert.deepEqual(typo.assumed, ["STY-05"]);
	assert.deepEqual(typo.malformed, [{ commit: "ddd", near: ["FEA-CH37"], words: 0 }]);
});

test("the pool stops starting tasks after the first failure and aborts the running ones", async () => {
	const started = [];
	const controller = new AbortController();
	const task = (name, status, ms) => async (signal) => {
		started.push(name);
		await new Promise((done) => {
			const timer = setTimeout(done, ms);
			signal.addEventListener("abort", () => {
				clearTimeout(timer);
				done();
			});
		});
		return { status: signal.aborted && status === "pass" ? "fail" : status, name };
	};
	const results = await runPool([task("slow", "pass", 500), task("bad", "fail", 5), task("later", "pass", 5)], 2, controller);
	assert.deepEqual(started, ["slow", "bad"]);
	assert.equal(results[1].status, "fail");
	assert.equal(results[2], undefined, "nothing starts after a failure");
	assert.ok(controller.signal.aborted);
	const going = new AbortController();
	started.length = 0;
	const all = await runPool([task("slow", "pass", 50), task("bad", "fail", 5), task("later", "pass", 5)], 2, going, { keepGoing: true });
	assert.deepEqual(started, ["slow", "bad", "later"], "with keepGoing every task runs after a failure");
	assert.deepEqual(all.map((r) => r.status), ["pass", "fail", "pass"]);
	assert.ok(!going.signal.aborted);
});

test("a command stopped mid-output never writes into its ended log", async () => {
	const { mkdtempSync, readFileSync } = await import("node:fs");
	const { tmpdir } = await import("node:os");
	const { join } = await import("node:path");
	const { runLogged } = await import("../lib/proc.mjs");
	const logPath = join(mkdtempSync(join(tmpdir(), "rv-gate-log-")), "step.log");
	const controller = new AbortController();
	// Output still buffered in the pipe when the command stops arrives after the log has ended.
	const writer = "setInterval(() => process.stdout.write('x'.repeat(65536) + '\\n'), 1)";
	const running = runLogged(process.execPath, ["-e", writer], { cwd: tmpdir(), env: process.env, logPath, signal: controller.signal });
	setTimeout(() => controller.abort(), 150);
	const result = await running;
	await new Promise((done) => setTimeout(done, 300));
	assert.equal(result.status, "fail");
	assert.match(readFileSync(logPath, "utf8"), /rv-gate: exit \d+ after \d+ ms\n$/);
});

test("a capture set that is empty or lost more than a tenth of the baseline never replaces it", async () => {
	const { baselineRefusal } = await import("../lib/store.mjs");
	const before = Object.fromEntries(Array.from({ length: 20 }, (_, i) => [`capture-${i}`, `h${i}`]));
	assert.match(baselineRefusal(before, {}), /empty/);
	const lostThree = Object.fromEntries(Object.entries(before).slice(3));
	assert.match(baselineRefusal(before, lostThree), /3 of 20 captures are missing/);
	const changedAndOneGone = { ...Object.fromEntries(Object.entries(before).slice(1)), "capture-5": "new", added: "x" };
	assert.equal(baselineRefusal(before, changedAndOneGone), undefined, "changed and added captures are what a release records");
	assert.equal(baselineRefusal(undefined, { first: "x" }), undefined);
});

test("the release target keeps its dependencies and loses the workspace's own outputs", async () => {
	const { mkdtempSync, mkdirSync, writeFileSync, readdirSync } = await import("node:fs");
	const { tmpdir } = await import("node:os");
	const { join } = await import("node:path");
	const { pruneWorkspaceOutputs } = await import("../lib/target.mjs");
	const target = mkdtempSync(join(tmpdir(), "rv-gate-target-"));
	const files = {
		"debug/deps": ["librichview_core-0123456789abcdef.rlib", "fonts-0123456789abcdef", "fonts-0123456789abcdef.d", "richview_core-0123456789abcdef.richview_core.1a2b-cgu.0.rcgu.o", "libserde-0123456789abcdef.rlib"],
		debug: ["richview-editor", "richview-editor.d"],
		"wasm32-unknown-unknown/release/deps": ["richview_runtime-0123456789abcdef.wasm", "libwasm_bindgen-0123456789abcdef.rlib"],
	};
	for (const [dir, names] of Object.entries(files)) {
		mkdirSync(join(target, dir), { recursive: true });
		for (const name of names) writeFileSync(join(target, dir, name), "x");
	}
	const metadata = {
		packages: [
			{ targets: [{ name: "richview_core" }, { name: "fonts" }] },
			{ targets: [{ name: "richview-editor" }] },
			{ targets: [{ name: "richview_runtime" }] },
		],
	};
	assert.ok(pruneWorkspaceOutputs(target, metadata) > 0);
	assert.deepEqual(readdirSync(join(target, "debug/deps")), ["libserde-0123456789abcdef.rlib"]);
	assert.deepEqual(readdirSync(join(target, "debug")).filter((name) => name !== "deps"), []);
	assert.deepEqual(readdirSync(join(target, "wasm32-unknown-unknown/release/deps")), ["libwasm_bindgen-0123456789abcdef.rlib"]);
});

test("a step's reuse key changes with every tracked file a step could read and its command, and nothing else", () => {
	const tree = (overrides = {}) =>
		Object.entries({
			"crates/richview-core/src/lib.rs": "a1",
			"scripts/verify-no-js.mjs": "b1",
			"scripts/verify-exports.mjs": "c1",
			"tools/rv-gate/lib/fast.mjs": "d1",
			"docs/guide/styles.md": "e1",
			...overrides,
		}).map(([path, hash]) => `100644 blob ${hash}\t${path}`);
	const step = (id) => STEPS.find((s) => s.id === id);
	const noJs = step("no-js");
	const base = reuseKey(noJs, tree());
	assert.equal(reuseKey(noJs, tree({ "docs/lockdown/answers/01-intent.json": "x2", "tools/rv-gate/test/gate.test.mjs": "y2" })), reuseKey(noJs, tree({ "docs/lockdown/answers/01-intent.json": "x1", "tools/rv-gate/test/gate.test.mjs": "y1" })), "the lockdown's records and the tools' tests are not inputs");
	for (const [path, why] of [
		["crates/richview-core/src/lib.rs", "a product file"],
		["docs/guide/styles.md", "a guide (docs tests read docs)"],
		["tools/rv-gate/lib/fast.mjs", "the gate's own code"],
		["scripts/verify-exports.mjs", "another suite's script"],
	])
		assert.notEqual(reuseKey(noJs, tree({ [path]: "z9" })), base, `${why} is an input`);
	assert.notEqual(reuseKey(noJs, tree({ "AGENTS.md": "m2", ".claude/rules/a.md": "r2" })), reuseKey(noJs, tree({ "AGENTS.md": "m1", ".claude/rules/a.md": "r1" })), "root Markdown and .claude are inputs: docs_match reads them");
	assert.notEqual(reuseKey(noJs, tree({ "scripts/verify-no-js.mjs": "b2" })), base, "its own script is");
	assert.notEqual(reuseKey({ ...noJs, run: [...noJs.run, { cmd: "true", args: [] }] }, tree()), base, "its command is");
	const tools = step("tool-tests");
	assert.notEqual(reuseKey(tools, tree({ "tools/rv-gate/lib/fast.mjs": "d2" })), reuseKey(tools, tree()));
	assert.notEqual(reuseKey(tools, tree({ "docs/lockdown/map/build-map.mjs": "m2" })), reuseKey(tools, tree()), "the lockdown-app tests copy the map builder");
	assert.notEqual(reuseKey(tools, tree({ "examples/every-block/report.rmd": "e2" })), reuseKey(tools, tree()), "the golden tests read the every-block fixture");
	assert.equal(reuseKey(tools, tree({ "docs/lockdown/evidence/linear.md": "v2" })), reuseKey(tools, tree()), "the evidence is read by no test");
	assert.equal(reuseKey(step("deps"), tree()), undefined, "deps reads the advisory database, outside the tree");
	assert.notEqual(reuseKey(noJs, tree({ "Cargo.lock": "f2", "rust-toolchain.toml": "g2" })), reuseKey(noJs, tree({ "Cargo.lock": "f1", "rust-toolchain.toml": "g1" })), "the lock file and toolchain pin are inputs");
	assert.notEqual(reuseKey(noJs, tree({ "crates/richview-core/tests/fixtures/a.csv": "h2" })), reuseKey(noJs, tree({ "crates/richview-core/tests/fixtures/a.csv": "h1" })), "fixtures are inputs");
	const env = (extra) => reuseEnvironment({ PATH: "/bin", TMPDIR: "/tmp/x", CARGO_TARGET_DIR: "/t", RV_GATE_HOLDER: "1", ...extra }, ["rustc 1.99.0", "v26.5.0"]);
	assert.equal(env({ TMPDIR: "/tmp/y", RV_GATE_HOLDER: "2", HOME: "/h" }), env({}), "paths and per-run values are not inputs");
	assert.notEqual(reuseKey(noJs, tree(), env({ RUSTFLAGS: "-C debug-assertions" })), reuseKey(noJs, tree(), env({})), "build flags are");
	assert.notEqual(reuseEnvironment({}, ["rustc 1.99.0"]), reuseEnvironment({}, ["rustc 1.100.0"]), "and so is the toolchain installed");
	for (const name of ["WASM_BINDGEN_TEST_TIMEOUT", "CHROMIUM_PATH", "HOSTILE_CASES", "DBT_PROFILES_DIR", "GARAGE_BIN"]) assert.notEqual(env({ [name]: "a" }), env({ [name]: "b" }), `${name} is an input`);
	assert.equal(reuseKey(step("captures"), tree()), undefined, "captures feed the baseline and always run");
	assert.equal(reuseKey(step("pins"), tree()), undefined, "built-in checks take seconds and always run");
});

test("the build is reused only when every step that reads it is, and a step that runs alone is never reused", () => {
	const full = releasePlan(STEPS, ["crates/richview-core/src/render.rs"], graph, new Map());
	assert.deepEqual(ids(full.alone), ["perf"], "the perf step runs alone (DEL-51)");
	assert.ok(!ids([...full.parallel, ...full.afterBuild]).includes("perf"));
	const noKeys = new Map([...full.parallel, ...full.afterBuild].map((step) => [step.id, `k-${step.id}`]));
	assert.ok(!reusable(full, noKeys, () => "release-1").has("build"), "the perf step measures this run's binaries");
	const plan = { ...full, alone: [] };
	const keys = new Map([...plan.parallel, ...plan.afterBuild].map((step) => [step.id, `k-${step.id}`]));
	const all = reusable(plan, keys, () => "release-1");
	assert.ok(all.has("build") && all.has("rust-tests") && all.has("chromium"));
	const oneSuiteNew = reusable(plan, keys, (id) => (id === "chromium" ? undefined : "release-1"));
	assert.ok(!oneSuiteNew.has("build"), "a suite that runs needs this run's binaries");
	assert.ok(oneSuiteNew.has("rust-tests"));
});

test("a reuse record counts only when its receipt ran the step to a pass at that key, not by reuse", async () => {
	const { reuseSourceHolds } = await import("../lib/store.mjs");
	const receipt = (step) => ({ kind: "release", steps: [step] });
	assert.ok(reuseSourceHolds(receipt({ id: "chromium", status: "pass", reuseKey: "k1" }), "chromium", "k1"));
	assert.ok(!reuseSourceHolds(receipt({ id: "chromium", status: "pass", reuseKey: "k0" }), "chromium", "k1"), "another key");
	assert.ok(!reuseSourceHolds(receipt({ id: "chromium", status: "fail", reuseKey: "k1" }), "chromium", "k1"), "a failure");
	assert.ok(!reuseSourceHolds(receipt({ id: "chromium", status: "pass", reuseKey: "k1", reused: "release-0" }), "chromium", "k1"), "a pass that was itself reused");
	assert.ok(!reuseSourceHolds({ kind: "dev", steps: [{ id: "chromium", status: "pass", reuseKey: "k1" }] }, "chromium", "k1"), "a receipt that is not a release");
	assert.ok(!reuseSourceHolds(undefined, "chromium", "k1"), "a missing receipt");
});

test("a passing release build is kept outside the pruned target, newest three only", async () => {
	const { mkdtempSync, mkdirSync, writeFileSync, readFileSync, readdirSync, existsSync, utimesSync } = await import("node:fs");
	const { tmpdir } = await import("node:os");
	const { join } = await import("node:path");
	const { keepReleaseBuild } = await import("../lib/target.mjs");
	const root = mkdtempSync(join(tmpdir(), "rv-keep-"));
	const target = join(root, "target");
	const cwd = join(root, "checkout");
	const dir = join(root, "bin-release");
	mkdirSync(join(target, "release"), { recursive: true });
	mkdirSync(join(cwd, "dist/web"), { recursive: true });
	writeFileSync(join(target, "release/richview"), "bin");
	writeFileSync(join(target, "release/richview-editor"), "editor");
	writeFileSync(join(target, "release/richview-sql-worker"), "worker");
	const manifest = JSON.stringify({ outputs: {} });
	writeFileSync(join(target, "release/richview-build-manifest.json"), manifest);
	writeFileSync(join(cwd, "dist/web/index.html"), "<p>");
	mkdirSync(join(dir, "notes"), { recursive: true });
	const commits = ["aaaaaaaaaa11", "bbbbbbbbbb22", "cccccccccc33", "dddddddddd44"];
	commits.forEach((commit, i) => {
		const folder = keepReleaseBuild({ target, cwd, commit, dir });
		const at = new Date(Date.now() - (commits.length - i) * 60_000);
		utimesSync(folder, at, at);
	});
	assert.deepEqual(readdirSync(dir).sort(), ["cccccccccc", "dddddddddd", "notes"], "the oldest builds go; a folder not named like a commit stays");
	assert.ok(existsSync(join(dir, "dddddddddd/richview")) && existsSync(join(dir, "dddddddddd/richview-editor")) && existsSync(join(dir, "dddddddddd/dist/web/index.html")));
	assert.equal(readFileSync(join(dir, "dddddddddd/richview-sql-worker"), "utf8"), "worker");
	assert.equal(readFileSync(join(dir, "dddddddddd/build-manifest.json"), "utf8"), manifest);
});

test("the steps after the build test a private copy: another checkout's release build cannot swap it", async () => {
	const { mkdtempSync, mkdirSync, writeFileSync, readFileSync, readlinkSync, symlinkSync, existsSync } = await import("node:fs");
	const { tmpdir } = await import("node:os");
	const { join } = await import("node:path");
	const { snapshotRelease } = await import("../lib/target.mjs");
	const root = mkdtempSync(join(tmpdir(), "rv-snap-"));
	const target = join(root, "target");
	const cwd = join(root, "checkout");
	mkdirSync(join(target, "release/font-licenses"), { recursive: true });
	mkdirSync(join(target, "wasm32-unknown-unknown/release"), { recursive: true });
	mkdirSync(cwd, { recursive: true });
	writeFileSync(join(target, "release/richview"), "ours");
	writeFileSync(join(target, "release/richview-sql-worker"), "our worker");
	const manifest = JSON.stringify({ outputs: {} });
	writeFileSync(join(target, "release/richview-build-manifest.json"), manifest);
	writeFileSync(join(target, "release/font-licenses/OFL.txt"), "licence");
	writeFileSync(join(target, "wasm32-unknown-unknown/release/richview_publish_server.wasm"), "wasm");
	symlinkSync(target, join(cwd, "target"));
	const dir = join(root, "run-tmp/target");
	assert.equal(snapshotRelease({ target, cwd, dir }), dir);
	assert.equal(readlinkSync(join(cwd, "target")), dir);
	writeFileSync(join(target, "release/richview"), "theirs");
	writeFileSync(join(target, "release/richview-sql-worker"), "their worker");
	assert.equal(readFileSync(join(cwd, "target/release/richview"), "utf8"), "ours");
	assert.equal(readFileSync(join(cwd, "target/release/richview-sql-worker"), "utf8"), "our worker");
	assert.equal(readFileSync(join(cwd, "target/release/richview-build-manifest.json"), "utf8"), manifest);
	assert.equal(readFileSync(join(cwd, "target/release/font-licenses/OFL.txt"), "utf8"), "licence");
	assert.equal(existsSync(join(cwd, "target/wasm32-unknown-unknown")), false, "the Wasm is copied later, when the pool has finished");
	assert.equal(existsSync(join(cwd, "target/release/richview-editor")), false, "an output the build did not make is not invented");
	const own = join(root, "own");
	mkdirSync(join(own, "target/release"), { recursive: true });
	assert.equal(snapshotRelease({ target, cwd: own, dir: join(root, "other") }), undefined, "a checkout whose target is its own folder is left alone");
	assert.equal(existsSync(join(root, "other")), false);
});

test("the Wasm is copied late, provenance is recorded, and the link goes back to the shared target", async () => {
	const { mkdtempSync, mkdirSync, writeFileSync, readFileSync, readlinkSync, symlinkSync, existsSync, rmSync } = await import("node:fs");
	const { tmpdir } = await import("node:os");
	const { join } = await import("node:path");
	const { snapshotRelease, copyOutputs, LATE_OUTPUTS, linkTarget, releaseBinDir } = await import("../lib/target.mjs");
	const root = mkdtempSync(join(tmpdir(), "rv-snap2-"));
	const target = join(root, "target");
	const cwd = join(root, "checkout");
	mkdirSync(join(target, "release"), { recursive: true });
	mkdirSync(join(target, "wasm32-unknown-unknown/release"), { recursive: true });
	mkdirSync(cwd, { recursive: true });
	writeFileSync(join(target, "release/richview"), "binary");
	writeFileSync(join(target, "wasm32-unknown-unknown/release/richview_publish_server.wasm"), "half");
	symlinkSync(target, join(cwd, "target"));
	const dir = join(root, "run-tmp/target");
	const files = [];
	snapshotRelease({ target, cwd, dir, files });
	assert.deepEqual(files.map((file) => file.path), ["release/richview"], "the Wasm is not copied with the build's outputs");
	assert.equal(files[0].size, 6);
	assert.ok(files[0].mtimeMs > 0);
	// The publish tests finish the Wasm; the pool is done, so it is copied now.
	writeFileSync(join(target, LATE_OUTPUTS[0]), "complete wasm");
	files.push(...copyOutputs({ target, dir, outputs: LATE_OUTPUTS }));
	assert.equal(readFileSync(join(cwd, "target", LATE_OUTPUTS[0]), "utf8"), "complete wasm");
	assert.equal(files.at(-1).size, 13);
	// Captures and surface read the private copy when there is one, the shared target (guarded) when not.
	assert.equal(releaseBinDir({ snapshot: dir, cwd, target }), join(cwd, "target", "release"));
	assert.equal(releaseBinDir({ snapshot: undefined, cwd, target }), join(target, "release"));
	// The run ends: the link goes back, and never dangles at the deleted temp folder.
	linkTarget(cwd, target);
	rmSync(join(root, "run-tmp"), { recursive: true });
	assert.equal(readlinkSync(join(cwd, "target")), target);
	assert.ok(existsSync(join(cwd, "target/release/richview")));
});

test("the snapshot's provenance is compared with its build: a newer file or a copy of another size is not the run's", async () => {
	const { mkdtempSync, mkdirSync, writeFileSync, symlinkSync, utimesSync } = await import("node:fs");
	const { tmpdir } = await import("node:os");
	const { join } = await import("node:path");
	const { snapshotRelease, provenanceProblems } = await import("../lib/target.mjs");
	const root = mkdtempSync(join(tmpdir(), "rv-snap3-"));
	const target = join(root, "target");
	const cwd = join(root, "checkout");
	mkdirSync(join(target, "release"), { recursive: true });
	mkdirSync(cwd, { recursive: true });
	writeFileSync(join(target, "release/richview"), "ours");
	writeFileSync(join(target, "release/richview-editor"), "editor");
	symlinkSync(target, join(cwd, "target"));
	const buildEnded = Date.now();
	const seconds = (ms) => ms / 1000;
	utimesSync(join(target, "release/richview"), seconds(buildEnded - 60_000), seconds(buildEnded - 60_000));
	// Another checkout's release build wrote the editor a minute after this run's build ended.
	utimesSync(join(target, "release/richview-editor"), seconds(buildEnded + 60_000), seconds(buildEnded + 60_000));
	const files = [];
	snapshotRelease({ target, cwd, dir: join(root, "run-tmp/target"), files });
	assert.deepEqual(provenanceProblems(files, buildEnded), ["release/richview-editor was written 60 s after its build ended"]);
	assert.deepEqual(provenanceProblems(files, buildEnded + 60_000), [], "a file written before the build ended is the run's");
	assert.deepEqual(provenanceProblems([{ path: "release/richview", size: 4, copiedSize: 2, mtimeMs: buildEnded }], buildEnded), ["release/richview changed while it was copied"]);
});

test("candidate checks retain pre-prod's gate while full and release diagnostics use dev", async () => {
	const { gateRefFor, BOOTSTRAP_NOTE } = await import("../lib/release.mjs");
	const has = (refs) => (ref) => refs.includes(ref);
	for (const command of ["prod", "candidate"]) {
		assert.deepEqual(gateRefFor(command, has(["origin/pre-prod", "origin/dev"])), { ref: "origin/pre-prod" }, command);
		const bootstrap = gateRefFor(command, has(["origin/dev"]));
		assert.equal(bootstrap.ref, "origin/dev");
		assert.equal(bootstrap.note, BOOTSTRAP_NOTE);
	}
	assert.match(BOOTSTRAP_NOTE, /^bootstrap: pre-prod has no release gate yet, so this batch's gate runs \(DEL-36 bootstrap\)$/);
	for (const command of ["full", "release", "dev", "merge-prod", "report"]) assert.deepEqual(gateRefFor(command, has(["origin/pre-prod"])), { ref: "origin/dev" }, "release diagnostics and the dev tier use dev's copy");
});

test("fast lints a changed Wasm crate for wasm32, the crates the release's clippy-wasm step lints", () => {
	const step = STEPS.find((s) => s.id === "clippy-wasm");
	const linted = step.run[0].args.flatMap((arg, i, args) => (args[i - 1] === "-p" ? [arg] : []));
	assert.deepEqual(linted, WASM_CRATES);
	assert.deepEqual(wasmLinted(["richview-core", "richview-runtime"]), ["richview-runtime"], "browser code compiles only for wasm32, so the host lint misses it");
	assert.deepEqual(wasmLinted(["richview-core", "richview-app"]), []);
});

test("native runtime tests wait for the real staged build and run with embedded runtime features", () => {
	const runtimeGraph = [...graph, { name: "richview-app", dir: "crates/richview-app", deps: ["richview-core"], targets: [{ name: "richview_app", kind: "lib", src: "crates/richview-app/src/lib.rs" }] }];
	for (const changed of ["crates/richview-core/src/render.rs", "crates/richview-app/tests/identity.rs", "crates/richview-app/tests/unsplash.rs"]) {
		const plan = releasePlan(STEPS, [changed], runtimeGraph, new Map());
		assert.ok(!ids(plan.parallel).includes("rust-slow"));
		const slow = plan.afterBuild.find(step => step.id === "rust-slow");
		assert.ok(slow, "runtime-dependent tests must wait for richview-build");
		assert.ok(STEPS.findIndex(step => step.id === "build") < STEPS.indexOf(slow), "full and prod also run build before rust-slow");
		const args = slow.run[0].args;
		assert.equal(args[args.indexOf("--features") + 1], "richview-app/embedded-runtime,richview-editor/embedded-runtime");
		assert.ok(args.includes("--ignored"), "reasoned release-only tests must execute");
	}
});
