// Tests for the connector examples in the gate (lead, 28 Oct: seeding in test tooling, MetricFlow
// unrun with reason; FEA-BR-14, FEA-BR-15). The seed binary is a fake; none of these needs a build.

import assert from "node:assert/strict";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import test from "node:test";
import type { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { RUNTIME_SKIPPED, nativeOnlySurfaces } from "../contract.mts";
import { stageInput } from "../golden.mts";
import { MF_ENV, MF_REASON, MF_TEST, SEED_BIN, classifyConnectors, missingEnv, parseConnectorsYaml, prepareSeedExample, seedBinPath } from "../lib/connectors.mts";
import { RELEASE_OUTPUTS } from "../lib/target.mts";
import { ignoredTests } from "../lib/proc.mts";
import { STEPS } from "../lib/steps.mts";

const repo = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const tmp = () => mkdtempSync(join(tmpdir(), "rv-connectors-test-"));

/** An example folder with the given connectors yaml and optional seed. */
function example(root: string, name: string, yaml: string | Uint8Array, { seed = true } = {}) {
	const dir = join(root, name);
	mkdirSync(join(dir, "data"), { recursive: true });
	writeFileSync(join(dir, "report.rmd"), "---\ntitle: T\n---\n\nText.\n");
	writeFileSync(join(dir, "report.connectors.yaml"), yaml);
	if (seed) writeFileSync(join(dir, "data/seed.sql"), "create table t (a integer);\n");
	return dir;
}

/** A seed binary that records its arguments, creates the database and refuses to overwrite it. */
function fakeSeedBin(root: string) {
	const bin = join(root, "seed_duckdb");
	writeFileSync(bin, `#!/bin/sh\n[ -e "$2" ] && { echo "exists" >&2; exit 3; }\necho "$1 $2" >> "${root}/seed-calls"\ntouch "$2"\n`);
	chmodSync(bin, 0o755);
	return bin;
}

function connector(dir: string): {kind: string; reason?: string} {
	const result = classifyConnectors(dir);
	assert.ok(result);
	return result;
}

const DUCK = "connectors:\n  local-duckdb:\n    kind: duckdb\n    database: data/warehouse.duckdb\n";

test("the real examples classify: sql-report is seeded, metricflow-report needs the runtime", () => {
	assert.equal(connector(join(repo, "examples/sql-report")).kind, "seed");
	const mf = connector(join(repo, "examples/metricflow-report"));
	assert.equal(mf.kind, "runtime");
	assert.equal(mf.reason, MF_REASON);
	assert.equal(classifyConnectors(join(repo, "examples/hello-world")), null);
});

test("classification: duckdb without a seed, other kinds and empty files are unsupported", () => {
	const root = tmp();
	try {
		assert.equal(connector(example(root, "noseed", DUCK, { seed: false })).kind, "unsupported");
		assert.match(connector(example(root, "pg", "connectors:\n  p:\n    kind: postgres\n")).reason ?? "", /postgres/);
		assert.equal(connector(example(root, "empty", "connectors: {}\n")).kind, "unsupported");
		assert.equal(connector(example(root, "mixed", `${DUCK}  mf:\n    kind: metricflow\n    runtime: r\n`)).kind, "runtime");
	} finally {
		rmSync(root, { recursive: true, force: true });
	}
});

test("the connectors yaml reader handles comments and quoted values", () => {
	assert.deepEqual(parseConnectorsYaml('# c\nconnectors:\n  a:  # x\n    kind: duckdb\n    database: "d/x.duckdb"\n  b:\n    kind: metricflow\n    runtime: r\n'), {
		a: { kind: "duckdb", database: "d/x.duckdb" },
		b: { kind: "metricflow", runtime: "r" },
	});
});

test("a seed example is copied, seeded at the copy's database path, and gets a connectors file beside the copy", () => {
	const root = tmp();
	try {
		const source = example(root, "src", DUCK);
		const bin = fakeSeedBin(root);
		const dest = join(root, "run/sql");
		mkdirSync(dirname(dest), { recursive: true });
		const prepared = prepareSeedExample({ source, dest, seedBin: bin });
		assert.equal(readFileSync(join(root, "seed-calls"), "utf8").trim(), `${prepared.folder}/data/seed.sql ${prepared.folder}/data/warehouse.duckdb`);
		assert.ok(existsSync(join(prepared.folder, "data/warehouse.duckdb")));
		assert.equal(existsSync(join(source, "data/warehouse.duckdb")), false, "the checkout is never written");
		assert.equal(prepared.connectorsFile, `${prepared.folder}.connectors.toml`);
		assert.ok(existsSync(prepared.connectorsFile), "the trusted settings file is staged beside the report copy");
		assert.deepEqual(prepared.args, ["--connectors", prepared.connectorsFile]);
		assert.equal(prepared.env.RICHVIEW_CONNECTORS, prepared.connectorsFile);
		// The binary still refuses to overwrite; the gate removes the copy's database first, so a second preparation seeds afresh.
		assert.doesNotThrow(() => prepareSeedExample({ source, dest, seedBin: bin }));
		// Cached rows, in the checkout or left in the copy by an earlier run, never stand in for the seed.
		mkdirSync(join(source, ".richview-cache"), { recursive: true });
		writeFileSync(join(source, ".richview-cache/rows"), "stale");
		mkdirSync(join(dest, ".richview-cache"), { recursive: true });
		writeFileSync(join(dest, ".richview-cache/rows"), "stale");
		prepareSeedExample({ source, dest, seedBin: bin });
		assert.equal(existsSync(join(dest, ".richview-cache")), false);
	} finally {
		rmSync(root, { recursive: true, force: true });
	}
});

const PARQUET = "connectors:\n  orders:\n    kind: parquet\n    file: data/orders.parquet\n";

test("a parquet connector example is seeded by its own script, run once in the copy against an in-memory database", () => {
	const root = tmp();
	try {
		assert.equal(connector(join(repo, "examples/parquet-report")).kind, "seed");
		assert.match(connector(example(root, "nofile", "connectors:\n  orders:\n    kind: parquet\n")).reason ?? "", /parquet connector 'orders' names no file/);
		assert.match(connector(example(root, "noseed", PARQUET, { seed: false })).reason ?? "", /a parquet connector needs data\/seed.sql/);
		// A seed binary that, like the real script, writes the Parquet file relative to where it runs.
		const bin = join(root, "seed_duckdb");
		writeFileSync(bin, `#!/bin/sh\necho "$1 $2 $(pwd -P)" >> "${root}/seed-calls"\nmkdir -p data && echo PAR1 > data/orders.parquet\n`);
		chmodSync(bin, 0o755);
		const source = example(root, "parquet", PARQUET);
		writeFileSync(join(source, "data/orders.parquet"), "left in the checkout by a test");
		const dest = join(root, "run/parquet");
		mkdirSync(dirname(dest), { recursive: true });
		const prepared = prepareSeedExample({ source, dest, seedBin: bin });
		const [seed, database, cwd] = readFileSync(join(root, "seed-calls"), "utf8").trim().split(" ");
		assert.equal(seed, `${prepared.folder}/data/seed.sql`);
		assert.equal(database, ":memory:");
		assert.equal(resolve(cwd), resolve(realpathSync(prepared.folder)));
		assert.equal(readFileSync(join(prepared.folder, "data/orders.parquet"), "utf8"), "PAR1\n", "the copy holds the seeded file, not the checkout's");
		assert.equal(readFileSync(join(source, "data/orders.parquet"), "utf8"), "left in the checkout by a test", "the checkout is never written");
		// A script that does not write the file the connector names is a failure that says so.
		writeFileSync(bin, "#!/bin/sh\nexit 0\n");
		assert.throws(() => prepareSeedExample({ source, dest, seedBin: bin }), /data\/seed.sql did not write 'data\/orders.parquet'/);
		const escape = example(root, "esc", "connectors:\n  o:\n    kind: parquet\n    file: ../out.parquet\n");
		assert.throws(() => prepareSeedExample({ source: escape, dest: join(root, "o1"), seedBin: bin }), /file '\.\.\/out\.parquet' escapes the report folder/);
	} finally {
		rmSync(root, { recursive: true, force: true });
	}
});

test("a database outside the report folder, a missing seed binary and a non-seed example are refused", () => {
	const root = tmp();
	try {
		const bin = fakeSeedBin(root);
		const escape = example(root, "esc", "connectors:\n  d:\n    kind: duckdb\n    database: ../out.duckdb\n");
		assert.throws(() => prepareSeedExample({ source: escape, dest: join(root, "o1"), seedBin: bin }), /escapes the report folder/);
		assert.throws(() => prepareSeedExample({ source: example(root, "ok", DUCK), dest: join(root, "o2"), seedBin: join(root, "nope") }), /seed binary .* does not exist/);
		assert.throws(() => prepareSeedExample({ source: example(root, "mf", "connectors:\n  m:\n    kind: metricflow\n    runtime: r\n"), dest: join(root, "o3"), seedBin: bin }), /not a seed example/);
	} finally {
		rmSync(root, { recursive: true, force: true });
	}
});

test("seed binary lookup: the build step's output in the checkout, or --seed-bin", () => {
	assert.equal(seedBinPath("/work", undefined), `/work/${SEED_BIN}`);
	assert.equal(SEED_BIN, "target/release/examples/seed_duckdb");
	assert.equal(seedBinPath("/work", "/x/seed"), "/x/seed");
});

test("golden stages a seed example from a seeded copy and renders it with the connectors file", () => {
	const root = tmp();
	try {
		example(root, "examples/sql", DUCK);
		const staged = stageInput({ cwd: root, bin: "unused", folder: join(root, "out"), input: { id: "examples/sql", dir: "examples/sql", why: [] }, seedBin: fakeSeedBin(root) });
		assert.equal(staged.skip, undefined);
		assert.equal(staged.args[0], "--connectors");
		assert.ok(existsSync(join(root, "out/data/warehouse.duckdb")));
	} finally {
		rmSync(root, { recursive: true, force: true });
	}
});

test("a database the checkout already holds is replaced by a fresh seed, not refused (review 1h should-fix 6)", () => {
	const root = tmp();
	try {
		example(root, "examples/sql", DUCK);
		mkdirSync(join(root, "examples/sql/data"), { recursive: true });
		writeFileSync(join(root, "examples/sql/data/warehouse.duckdb"), "stale, seeded by a test in the checkout");
		const seen: boolean[] = [];
		const run = (_bin: string, [, database]: string[]) => {
			seen.push(existsSync(database));
			writeFileSync(database, "fresh");
			return { status: 0 };
		};
		prepareSeedExample({ source: join(root, "examples/sql"), dest: join(root, "out"), seedBin: fakeSeedBin(root), run: run as unknown as typeof spawnSync });
		assert.deepEqual(seen, [false], "the copy's stale database is gone before seeding");
		assert.equal(readFileSync(join(root, "out/data/warehouse.duckdb"), "utf8"), "fresh");
	} finally {
		rmSync(root, { recursive: true, force: true });
	}
});

test("golden stages a runtime example as unrun with its reason when the environment lacks the runtime, and renders it when present", () => {
	const root = tmp();
	try {
		example(root, "examples/mf", "connectors:\n  m:\n    kind: metricflow\n    runtime: r\n", { seed: false });
		// A fake CLI whose validate is clean.
		const clean = join(root, "richview");
		writeFileSync(clean, `#!/bin/sh\necho '{"diagnostics":[]}'\n`);
		chmodSync(clean, 0o755);
		const input = { id: "examples/mf", dir: "examples/mf", why: [] };
		const without = stageInput({ cwd: root, bin: clean, folder: join(root, "a"), input, seedBin: undefined, environment: { RICHVIEW_MF_OKURKA_DIR: "/x" } });
		assert.equal(without.skip, true);
		assert.deepEqual(without.failures, [], "clean validate: neither pass nor failure");
		assert.ok(without.unrun);
		assert.match(without.unrun.reason, /^needs the MetricFlow runtime from the server's configuration \(FEA-BR-15\); unset: DBT_PROFILES_DIR/);
		assert.equal(without.unrun.validated, true);

		const noisy = join(root, "richview-noisy");
		writeFileSync(noisy, `#!/bin/sh\necho '{"diagnostics":[{"level":"warning","code":"w.x","message":"m"}]}'\n`);
		chmodSync(noisy, 0o755);
		const failing = stageInput({ cwd: root, bin: noisy, folder: join(root, "b"), input, seedBin: undefined, environment: {} });
		assert.ok(failing.failures?.length);
		assert.equal(failing.failures[0].kind, "validate warning", "validation must be clean");

		const present = Object.fromEntries(MF_ENV.map((name) => [name, "/set"]));
		const withEnv = stageInput({ cwd: root, bin: clean, folder: join(root, "c"), input, seedBin: undefined, environment: present });
		assert.equal(withEnv.skip, undefined, "renders like any example");
		assert.equal(withEnv.unrun, undefined);
	} finally {
		rmSync(root, { recursive: true, force: true });
	}
});

test("an unsupported connector example is a failure, not an unrun input", () => {
	const root = tmp();
	try {
		example(root, "examples/pg", "connectors:\n  p:\n    kind: postgres\n");
		const staged = stageInput({ cwd: root, bin: "unused", folder: join(root, "o"), input: { id: "examples/pg", dir: "examples/pg", why: [] }, seedBin: undefined });
		assert.equal(staged.skip, true);
		assert.equal(staged.unrun, undefined);
		assert.ok(staged.failures?.[0].message);
		assert.match(staged.failures[0].message, /postgres/);
	} finally {
		rmSync(root, { recursive: true, force: true });
	}
});

test("contract compares a runtime example on the validate surfaces only and says why", () => {
	const skipped = nativeOnlySurfaces({ id: "examples/metricflow-report", files: ["report.rmd", "report.connectors.yaml"], connectorKind: "runtime" });
	assert.ok(skipped);
	for (const surface of RUNTIME_SKIPPED) assert.ok(skipped.surfaces.includes(surface), surface);
	assert.match(skipped.reason, /FEA-BR-15/);
	for (const surface of ["cli.validate", "mcp.validate", "editor.lint"]) assert.ok(!skipped.surfaces.includes(surface), `${surface} stays compared`);
	// A seed example keeps CLI, MCP and editor; only the Wasm and HTTP surfaces are skipped.
	const seed = nativeOnlySurfaces({ id: "examples/sql-report", files: ["report.connectors.yaml"], connectorKind: "seed" });
	assert.ok(seed);
	for (const surface of ["cli.check", "mcp.check", "cli.generate", "cli.validate", "mcp.validate", "editor.lint"]) assert.ok(!seed.surfaces.includes(surface), surface);
});

test("the build step builds the seed example and the private build copy carries it", () => {
	const build = STEPS.find((s) => s.id === "build");
	assert.ok(build?.run);
	const seed = build.run.find((c) => c.args.includes("seed_duckdb"));
	assert.ok(seed);
	assert.deepEqual(seed.args, ["build", "--release", "--locked", "-p", "richview-app", "--example", "seed_duckdb"]);
	assert.ok(build.run.indexOf(seed) > 0, "after the existing build commands");
	assert.ok(RELEASE_OUTPUTS.includes("release/examples/seed_duckdb"));
});

test("rust-slow skips the MetricFlow test when the runtime is missing, and says so", () => {
	const slow = STEPS.find((s) => s.id === "rust-slow");
	assert.ok(slow?.run);
	const command = slow.run[0];
	assert.ok(command.whenEnvMissing);
	assert.deepEqual(command.args.slice(-2), ["--", "--ignored"]);
	assert.deepEqual(command.whenEnvMissing.vars, MF_ENV);
	assert.deepEqual(command.whenEnvMissing.args, ["--skip", MF_TEST, "--exact"], "an exact skip, not every test whose name contains it (review 1g note 11)");
	assert.equal(MF_TEST, "metricflow_e2e::okurka_metricflow_runtime_returns_real_rows_through_the_connector");
	assert.equal(command.whenEnvMissing.unrun[0].reason, MF_REASON);
	assert.equal(slow.reportsIgnored, true);
	assert.equal(STEPS.find((s) => s.id === "rust-tests")?.reportsIgnored, true);
});

test("environment check names what is unset", () => {
	assert.deepEqual(missingEnv(["A", "B", "C"], { A: "1", B: "" }), ["B", "C"]);
});

test("cargo's ignored tests are read with and without a reason", () => {
	const log = [
		"running 4 tests",
		"test a::plain ... ok",
		"test a::bare ... ignored",
		"test b::why ... ignored, needs the MetricFlow runtime (FEA-BR-15)",
		"src/lib.rs - doc (line 3) ... ignored",
		"test src/lib.rs - doc (line 9) ... ignored",
		"test result: ok. 1 passed; 0 failed; 3 ignored",
	].join("\n");
	assert.deepEqual(ignoredTests(log), [
		{ name: "a::bare", reason: "" },
		{ name: "b::why", reason: "needs the MetricFlow runtime (FEA-BR-15)" },
		{ name: "src/lib.rs - doc (line 9)", reason: "" },
	]);
	assert.deepEqual(ignoredTests("nothing here"), []);
});

test("unrun inputs and skipped tests are one Not run list: UNRUN log lines and skipped Rust tests", async () => {
	const { notRunParts, unrunLines } = await import("../lib/proc.mts");
	const log = "golden: 3 inputs\nUNRUN examples/metricflow-report: needs the MetricFlow runtime (validated only)\n  UNRUN indented: not a marker\nok\n";
	assert.deepEqual(unrunLines(log), [{ what: "examples/metricflow-report", reason: "needs the MetricFlow runtime (validated only)" }]);
	const step = { details: { unrunTests: [{ name: MF_TEST, reason: MF_REASON }], unrunInputs: unrunLines(log) } };
	assert.deepEqual(
		notRunParts(step).map((p) => p.what),
		[MF_TEST, "examples/metricflow-report"],
	);
	assert.deepEqual(notRunParts({}), []);
});
