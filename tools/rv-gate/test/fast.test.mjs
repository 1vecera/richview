// Host reason (ARC-03): run the actual fast selector against Cargo metadata without compiling or using the shared target.
import assert from "node:assert/strict";
import childProcess, { execFileSync } from "node:child_process";
import { EventEmitter } from "node:events";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { syncBuiltinESMExports } from "node:module";
import { dirname, join } from "node:path";
import { PassThrough } from "node:stream";
import test from "node:test";
import { committedCoverage, fastGate, testBinaries } from "../lib/fast.mjs";

test("chart changes run the core authored type contracts without selecting every core integration", async t => {
	mkdirSync("tmp", { recursive: true });
	const cwd = mkdtempSync(join(process.cwd(), "tmp/fast-chart-contracts-"));
	t.after(() => rmSync(cwd, { recursive: true, force: true }));
	const put = (file, source) => {
		mkdirSync(dirname(join(cwd, file)), { recursive: true });
		writeFileSync(join(cwd, file), source);
	};
	put("Cargo.toml", '[workspace]\nresolver = "3"\nmembers = ["richview-chart", "crates/richview-core"]\n');
	for (const [name, dir] of [["richview-chart", "richview-chart"], ["richview-core", "crates/richview-core"]]) {
		put(`${dir}/Cargo.toml`, `[package]\nname = "${name}"\nversion = "0.0.0"\nedition = "2024"\n${name === "richview-core" ? '[dependencies]\nrichview-chart = { path = "../../richview-chart" }\n' : ""}`);
		put(`${dir}/src/lib.rs`, "pub fn value() -> u8 { 1 }\n");
	}
	for (const name of ["chart_types", "spec_docs", "type_contracts", "boxplot_contract", "histogram_contract", "interval_contract", "numeric_range_contract", "host_contract"])
		put(`crates/richview-core/tests/${name}.rs`, "#[test]\nfn contract() {}\n");
	const git = (...args) => execFileSync("git", ["-c", "user.name=Fixture", "-c", "user.email=fixture@example.test", "-c", "commit.gpgsign=false", ...args], { cwd, encoding: "utf8", stdio: "pipe" }).trim();
	git("init", "-q"); git("add", "."); git("commit", "-qm", "Base");
	const base = git("rev-parse", "HEAD");
	put("richview-chart/src/lib.rs", "pub fn value() -> u8 { 2 }\n");
	const lines = [];
	t.mock.method(console, "log", (...args) => lines.push(args.join(" ")));
	assert.equal(await fastGate({ cwd, target: join(cwd, "protocol-cache"), base, plan: true }), 0);
	const plan = lines.join("\n");
	assert.ok(plan.includes("richview-core --test type_contracts (integration)"), plan);
	assert.ok(plan.includes("richview-core --test chart_types (integration)"), "Keep the existing chart contract");
	assert.ok(!plan.includes("richview-core --test host_contract"), "Unrelated dependent integrations stay outside the default chart plan");
});

test("a change to any chart page selects the editor's snippets test, even when its example is unchanged", async t => {
	mkdirSync("tmp", { recursive: true });
	const cwd = mkdtempSync(join(process.cwd(), "tmp/fast-snippets-"));
	t.after(() => rmSync(cwd, { recursive: true, force: true }));
	const put = (file, source) => {
		mkdirSync(dirname(join(cwd, file)), { recursive: true });
		writeFileSync(join(cwd, file), source);
	};
	put("Cargo.toml", '[workspace]\nresolver = "3"\nmembers = ["crates/richview-editor"]\n');
	put("crates/richview-editor/Cargo.toml", '[package]\nname = "richview-editor"\nversion = "0.0.0"\nedition = "2024"\n');
	put("crates/richview-editor/src/lib.rs", "pub fn value() -> u8 { 1 }\n");
	put("crates/richview-editor/tests/snippets.rs", "#[test]\nfn committed_snippets_match_the_generator() {}\n");
	const page = (prose) => `# Bar\n\n## Source\n\n\`\`\`\`rmd\n\`\`\`chart\ntype: bar\n\`\`\`\n\`\`\`\`\n\n${prose}\n`;
	put("docs/public/charts/bar.md", page("Bars compare."));
	const git = (...args) => execFileSync("git", ["-c", "user.name=Fixture", "-c", "user.email=fixture@example.test", "-c", "commit.gpgsign=false", ...args], { cwd, encoding: "utf8", stdio: "pipe" }).trim();
	git("init", "-q"); git("add", "."); git("commit", "-qm", "Base");
	const base = git("rev-parse", "HEAD");
	// Prose only: the page's first example, the snippet itself, is unchanged.
	put("docs/public/charts/bar.md", page("Bars compare amounts across categories."));
	const lines = [];
	t.mock.method(console, "log", (...args) => lines.push(args.join(" ")));
	assert.equal(await fastGate({ cwd, target: join(cwd, "protocol-cache"), base, plan: true }), 0);
	const plan = lines.join("\n");
	assert.ok(plan.includes("richview-editor --test snippets (integration)"), plan);
	assert.match(plan, /snippets: docs\/public\/charts\/bar\.md \(the snippets' source\)/);
});

test("fast runs a changed crate's unchanged Cargo integration targets, and with --dependent-tests its dependents' too, without expanding features", async t => {
	mkdirSync("tmp", { recursive: true });
	const root = mkdtempSync(join(process.cwd(), "tmp/fast-integrations-")), cwd = join(root, "workspace");
	t.after(() => rmSync(root, { recursive: true, force: true }));
	mkdirSync(cwd);
	const packages = ["writer", "reader", "integration"].map(name => {
		mkdirSync(join(cwd, name, "src"), { recursive: true });
		mkdirSync(join(cwd, name, "tests"));
		const manifest_path = join(cwd, name, "Cargo.toml"), src_path = join(cwd, name, "src/lib.rs");
		writeFileSync(manifest_path, `[package]\nname="${name}"\nversion="0.1.0"\nedition="2024"\n`);
		writeFileSync(src_path, name === "integration" ? "pub fn value() {}\n" : "#[test]\nfn unit_contract() {}\n");
		return { name, manifest_path, dependencies: name === "reader" ? [{ name: "writer" }] : [], targets: [{ name, kind: ["lib"], src_path }] };
	});
	const addTest = (pkg, name, file = `tests/${name}.rs`, flags = {}) => {
		const src_path = join(cwd, pkg.name, file); mkdirSync(join(src_path, ".."), { recursive: true });
		writeFileSync(src_path, name === "ignored" ? '#[test]\n#[ignore = "requires the release runtime"]\nfn release_case() {}\n' : "#[test]\nfn contract() {}\n");
		pkg.targets.push({ name, kind: ["test"], test: true, src_path, ...flags });
	};
	addTest(packages[0], "contract"); addTest(packages[0], "ignored"); addTest(packages[0], "nested", "tests/nested/main.rs");
	addTest(packages[0], "outside", "verification/check.rs"); addTest(packages[0], "disabled", undefined, { test: false });
	addTest(packages[1], "consumer"); addTest(packages[2], "only_contract");
	writeFileSync(packages[0].manifest_path, readFileSync(packages[0].manifest_path, "utf8") + '\n[[test]]\nname="outside"\npath="verification/check.rs"\n\n[[test]]\nname="disabled"\ntest=false\n');
	writeFileSync(packages[1].manifest_path, readFileSync(packages[1].manifest_path, "utf8") + '\n[dependencies]\nwriter={path="../writer"}\n');
	writeFileSync(join(cwd, "Cargo.toml"), '[workspace]\nmembers=["writer","reader","integration"]\n');
	const git = (...args) => execFileSync("git", ["-c", "user.name=Fixture", "-c", "user.email=fixture@example.test", ...args], { cwd, stdio: "pipe" });
	git("init", "-q"); git("add", "."); git("commit", "-qm", "base"); git("update-ref", "refs/remotes/origin/dev", "HEAD");
	const previousHome = process.env.RV_GATE_HOME; process.env.RV_GATE_HOME = join(root, "gate-home");
	t.after(() => { if (previousHome === undefined) delete process.env.RV_GATE_HOME; else process.env.RV_GATE_HOME = previousHome; });
	const originalSync = childProcess.spawnSync, originalSpawn = childProcess.spawn;
	t.mock.method(childProcess, "spawnSync", (command, args, options) => {
		if (command === "df") return { status: 0, stdout: "Filesystem blocks used available capacity mounted\nfixture 200 1 100 1% /\n" };
		if (command !== "cargo") return originalSync(command, args, options);
		assert.equal(args[0], "metadata"); return { status: 0, stdout: JSON.stringify({ packages }) };
	});
	const commands = [];
	t.mock.method(childProcess, "spawn", (command, args, options) => {
		if (command !== "cargo") return originalSpawn(command, args, options);
		commands.push([...args]);
		const child = new EventEmitter(); child.stdout = new PassThrough(); child.stderr = new PassThrough();
		setImmediate(() => { child.stdout.end("test result: ok. 1 passed; 0 failed\n"); child.emit("close", 0); });
		return child;
	});
	syncBuiltinESMExports(); t.after(() => { t.mock.restoreAll(); syncBuiltinESMExports(); });
	const selected = () => commands.filter(args => args[0] === "test").flatMap(args => args.flatMap((value, i) => value === "--test" ? [`${args[args.indexOf("-p") + 1]}:${args[i + 1]}`] : []));
	const run = async dependentTests => {
		commands.length = 0;
		assert.equal(await fastGate({ cwd, target: join(root, "protocol-target"), dependentTests }), 0);
		assert.ok(commands.every(args => !["--ignored", "--all-features", "--no-default-features", "--features"].some(flag => args.includes(flag))), "Existing feature and ignore modes stay unchanged");
	};
	writeFileSync(join(cwd, "writer/src/lib.rs"), readFileSync(join(cwd, "writer/src/lib.rs"), "utf8") + "pub fn changed() {}\n");
	for (const dependentTests of [false, true]) {
		await run(dependentTests);
		// The dependent's integration test runs only on request; the unrelated crate's never does.
		assert.deepEqual(selected(), ["writer:contract", "writer:ignored", "writer:nested", "writer:outside", ...(dependentTests ? ["reader:consumer"] : [])]);
		const unit = commands.find(args => args[0] === "test" && args.includes("--lib"));
		assert.equal(unit.includes("reader"), dependentTests, "The existing dependent-unit opt-in stays exact");
	}
	git("restore", ".");
	writeFileSync(join(cwd, "writer/tests/contract.rs"), "#[test]\nfn contract() {}\n// changed assertion source\n");
	await run(false);
	assert.deepEqual(selected(), ["writer:contract", "writer:ignored", "writer:nested", "writer:outside"], "A changed integration target is not scheduled twice");
	git("restore", ".");
	writeFileSync(join(cwd, "integration/src/lib.rs"), "pub fn changed() {}\n");
	await run(false);
	assert.deepEqual(selected(), ["integration:only_contract"], "Integration-only crates still exercise their unchanged contract");
	assert.ok(commands.filter(args => args[0] === "test").every(args => !args.includes("--lib")), "No library unit target is invented");
	const fixtureFile = join(cwd, "writer/src/lib.rs"), before = readFileSync(fixtureFile, "utf8");
	writeFileSync(fixtureFile, before + `\nconst secret = "${"CANARY_".repeat(5)}";\n`);
	commands.length = 0;
	assert.equal(await fastGate({ cwd, target: join(root, "protocol-target") }), 1, "The unchanged credential-assignment rule runs before testing");
	assert.deepEqual(commands, [], "No Cargo build/test begins for a secrets failure");
	writeFileSync(fixtureFile, before);
	git("add", "."); git("commit", "-qm", "Changed integration\n\nitems: PRF-02");
	commands.length = 0;
	assert.equal(await fastGate({ cwd, target: join(root, "protocol-target") }), 1, "An unmapped committed card fails before testing");
	assert.deepEqual(commands, [], "No Cargo build/test begins for a failed declaration check");
});

test("prospective fast coverage checks every new commit, excludes the base and validates actual check paths", t => {
	mkdirSync("tmp", { recursive: true });
	const cwd = mkdtempSync(join(process.cwd(), "tmp/fast-coverage-"));
	t.after(() => rmSync(cwd, { recursive: true, force: true }));
	const git = (...args) => execFileSync("git", ["-c", "user.name=Fixture", "-c", "user.email=fixture@example.test", "-c", "commit.gpgsign=false", ...args], { cwd, encoding: "utf8", stdio: "pipe" }).trim();
	git("init", "-q"); writeFileSync(join(cwd, "fixture"), "base"); git("add", "."); git("commit", "-qm", "Base\n\nitems: PRF-31");
	const base = git("rev-parse", "HEAD");
	assert.equal(committedCoverage(cwd, base).status, "pass", "The base's declarations are not this stack's work");
	writeFileSync(join(cwd, "fixture"), "first"); git("commit", "-qam", "First\n\nitems: PRF-02");
	writeFileSync(join(cwd, "fixture"), "second"); git("commit", "-qam", "Second\n\nitems: PRF-21");
	const path = join(cwd, "tools/rv-gate/coverage.json"); mkdirSync(join(cwd, "tools/rv-gate"), { recursive: true });
	const map = { items: { "PRF-21": { checks: ["perf"] } } };
	writeFileSync(path, JSON.stringify(map));
	assert.match(committedCoverage(cwd, base).reason, /PRF-02/, "A mapped newest commit cannot hide an earlier missing card");
	map.items["PRF-02"] = { checks: ["exports:scripts/missing.mjs"] }; writeFileSync(path, JSON.stringify(map));
	assert.match(committedCoverage(cwd, base).reason, /missing\.mjs/, "A named mapping still needs a real check path");
	map.items["PRF-02"] = { checks: ["perf"] }; writeFileSync(path, JSON.stringify(map));
	assert.equal(committedCoverage(cwd, base).status, "pass");
});

test("the test binaries of a cargo test run are read from its Running lines", () => {
	const output = [
		"   Compiling richview-chart v0.1.0 (/work/richview-chart)",
		"    Finished `test` profile [unoptimized + debuginfo] target(s) in 41.20s",
		"     Running unittests src/lib.rs (/target/debug/deps/richview_chart-0123456789abcdef)",
		"running 12 tests",
		"     Running tests/line_contract.rs (/target/debug/deps/line_contract-fedcba9876543210)",
		"test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.20s",
		"     Running unittests src/main.rs (/target/debug/deps/richview-00112233445566ff)",
		"   Doc-tests richview_chart",
		"test a_running_example ... ok",
	].join("\n");
	assert.deepEqual(testBinaries(output), [
		"/target/debug/deps/richview_chart-0123456789abcdef",
		"/target/debug/deps/line_contract-fedcba9876543210",
		"/target/debug/deps/richview-00112233445566ff",
	]);
});
