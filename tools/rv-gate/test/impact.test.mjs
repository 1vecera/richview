// Tests for impact tracing (DEL-81): which unit tests a change can affect.

import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";
import { affected, crateOf, includeInputs, unitTargets, withDependencies, withDependents, workspaceGraph } from "../lib/impact.mjs";

const root = "/work/richview";

test("API checker and registered producer/resource changes run their checks without narrowing Rust dependents", () => {
	for (const path of ["tools/api-contracts/contracts.json", "tools/api-contracts/check.mjs", "tools/api-contracts/snapshots/core-document.json", "crates/richview-core/src/lib.rs", "crates/richview-app/build.rs", "crates/richview-build/src/runtime.rs"]) {
		const selected = affected([path], graph, new Map());
		assert.ok(selected.suites.has("api-contracts"), path);
		assert.equal(selected.untraced.includes(path), false, path);
		if (path === "crates/richview-core/src/lib.rs") assert.ok(selected.crates.has("richview-editor"));
	}
});

test("dependency rule source and tests select their real Node suite", () => {
	for (const path of ["tools/dependency-rules/rules.json", "tools/dependency-rules/indirect.mjs", "tools/dependency-rules/test/check.test.mjs", "tools/rv-gate/lib/layers.mjs"]) {
		const selected = affected([path], graph, new Map());
		assert.ok(selected.suites.has("dependency-rules"), path);
		assert.equal(selected.untraced.includes(path), false, path);
	}
});

test("preview recovery helper changes select their gate regressions", () => {
	const path = "scripts/preview-recovery.mjs";
	const selected = affected([path], graph, new Map());
	assert.ok(selected.suites.has("rv-gate"));
	assert.equal(selected.untraced.includes(path), false);
});
const pkg = (name, dir, deps, targets) => ({
	name,
	manifest_path: `${root}/${dir}/Cargo.toml`,
	dependencies: deps.map((dep) => ({ name: dep, kind: null })),
	targets: targets.map(([kind, target, src]) => ({ kind: [kind], name: target, src_path: `${root}/${dir}/${src}` })),
});

// A small copy of the workspace: chart ← core ← app ← editor, and a test-only registry crate.
const graph = workspaceGraph(
	{
		packages: [
			pkg("richview-chart", "richview-chart", ["serde"], [["rlib", "richview_chart", "src/lib.rs"], ["test", "geometry", "tests/geometry.rs"]]),
			pkg("richview-core", "crates/richview-core", ["richview-chart"], [["rlib", "richview_core", "src/lib.rs"]]),
			pkg("richview-app", "crates/richview-app", ["richview-core"], [["lib", "richview_app", "src/lib.rs"], ["bin", "richview", "src/main.rs"]]),
			pkg("richview-editor", "crates/richview-editor", ["richview-app", "richview-core"], [["lib", "richview_editor", "src/lib.rs"], ["bin", "richview-editor", "src/main.rs"]]),
			pkg("richview-bench", "crates/richview-bench", ["richview-app"], [["bin", "richview-bench", "src/main.rs"]]),
		],
	},
	root,
);

test("VS Code adapter, contributions and host tests select the runtime-free Node suite", () => {
	for (const path of ["packages/vscode/extension.cjs", "packages/vscode/package.json", "packages/vscode/test/export.test.cjs", "packages/vscode/snippets/rmd.json"]) {
		const selected = affected([path], graph, new Map());
		assert.ok(selected.suites.has("vscode"), path);
		assert.equal(selected.untraced.includes(path), false, path);
		if (path.includes("/snippets/")) assert.ok(selected.suites.has("rv-gate"), "Keep existing snippet checks");
	}
	assert.equal(affected(["cloudflare/publish/adapter.mjs"], graph, new Map()).suites.has("vscode"), false);
});

test("the workspace graph keeps members' folders, member dependencies and buildable targets", () => {
	const app = graph.find((crate) => crate.name === "richview-app");
	assert.equal(app.dir, "crates/richview-app");
	assert.deepEqual(app.deps, ["richview-core"]);
	assert.deepEqual(app.targets, [
		{ name: "richview_app", kind: "lib", src: "crates/richview-app/src/lib.rs" },
		{ name: "richview", kind: "bin", src: "crates/richview-app/src/main.rs" },
	]);
	assert.deepEqual(graph.find((crate) => crate.name === "richview-chart").deps, [], "registry dependencies are not members");
	assert.equal(crateOf("crates/richview-core/src/render.rs", graph).name, "richview-core");
	assert.equal(crateOf("crates/richview-corex/src/lib.rs", graph), undefined, "a folder prefix is not a member");
});

test("dependents and dependencies follow member edges transitively", () => {
	assert.deepEqual([...withDependents(["richview-core"], graph)].sort(), ["richview-app", "richview-bench", "richview-core", "richview-editor"]);
	assert.deepEqual([...withDependencies(["richview-editor"], graph)].sort(), ["richview-app", "richview-chart", "richview-core", "richview-editor"]);
});

test("a change affects its crate, the crates that compile it in, their dependents and the tools that own it", () => {
	const inputs = includeInputs(
		[
			["crates/richview-core/src/highlight.rs", "../../../examples/flagship/report.rmd"],
			["crates/richview-core/src/style.rs", "../assets/report.css"],
		],
		graph,
	);
	assert.deepEqual([...inputs.get("richview-core")], ["examples/flagship/report.rmd"], "includes inside the crate's folder are already its own");

	const leaf = affected(["crates/richview-editor/src/lib.rs"], graph, inputs);
	assert.deepEqual([...leaf.crates], ["richview-editor"]);

	const example = affected(["examples/flagship/report.rmd", "tools/rv-gate/lib/fast.mjs", "README.md"], graph, inputs);
	assert.deepEqual([...example.direct], ["richview-core"]);
	assert.deepEqual([...example.crates].sort(), ["richview-app", "richview-bench", "richview-core", "richview-editor"]);
	assert.deepEqual([...example.suites], ["rv-gate"]);
	assert.deepEqual(example.untraced, ["README.md"], "what no unit test traces is left to the release run");

	const lock = affected(["Cargo.lock"], graph, inputs);
	assert.equal(lock.crates.size, graph.length, "a workspace file affects every crate");
});

test("only targets whose sources hold a unit test run, a library's and a binary's apart", () => {
	const testFiles = new Set([
		"crates/richview-core/src/style.rs",
		"crates/richview-app/src/main.rs",
		"crates/richview-editor/tests/navigation.rs",
		"richview-chart/tests/geometry.rs",
	]);
	const crates = new Set(graph.map((crate) => crate.name));
	assert.deepEqual(unitTargets(crates, graph, testFiles), {
		libs: ["richview-core"],
		bins: [{ crate: "richview-app", name: "richview" }],
	});
});

test("integration tests that name a changed file or its folder by path read it at run time", async () => {
	const { readerLiterals, testTargets } = await import("../lib/impact.mjs");
	assert.deepEqual(readerLiterals(["assets/showcase/showcase.css", "assets/report.css"]).sort(), ["assets/report.css", "assets/showcase", "assets/showcase/showcase.css"], "a one-segment folder is too broad to count");
	assert.deepEqual(testTargets(["crates/richview-core/tests/palettes.rs", "richview-chart/tests/geometry.rs", "docs/x.rs"], graph), [
		{ crate: "richview-core", test: "palettes" },
		{ crate: "richview-chart", test: "geometry" },
	]);
	// A support file under tests/ (common/mod.rs, a fixture) may feed any test: all of that crate's run.
	const listTests = (crate) => (crate.name === "richview-core" ? ["palettes", "figures"] : []);
	assert.deepEqual(testTargets(["crates/richview-core/tests/common/mod.rs", "crates/richview-core/tests/fixtures/a.json", "crates/richview-core/tests/palettes.rs"], graph, { listTests }), [
		{ crate: "richview-core", test: "palettes" },
		{ crate: "richview-core", test: "figures" },
	]);
});

test("every contract test the fast gate adds for a changed crate exists", async () => {
	const { CONTRACT_TESTS } = await import("../lib/fast.mjs");
	const root = new URL("../../../", import.meta.url).pathname;
	for (const files of Object.values(CONTRACT_TESTS)) for (const file of files) assert.ok(existsSync(join(root, file)), file);
});

test("a changed chart example or snippet generator needs the committed snippet file in the same diff", async () => {
	const { snippetCheck, exampleOf, SNIPPET_FILE } = await import("../lib/snippets.mjs");
	const page = (body) => `# Bar\n\n## Source\n\n\`\`\`\`rmd\ntext\n\`\`\`chart\n${body}\n\`\`\`\n\`\`\`\`\n\nprose\n`;
	assert.equal(exampleOf(page("type: bar")), "```chart\ntype: bar\n```\n");
	assert.equal(exampleOf("no source"), undefined);
	const files = { "docs/public/charts/bar.md": [page("type: bar"), page("type: bar\nx: a")] };
	const check = (changed, extra = {}) =>
		snippetCheck({ changed, before: (f) => files[f]?.[0], after: (f) => files[f]?.[1], mentionsSnippets: () => false, ...extra });
	const stale = check(["docs/public/charts/bar.md"]);
	assert.equal(stale.status, "fail");
	assert.match(stale.reason, /UPDATE_SNIPPETS=1 cargo test -p richview-editor --test snippets committed_snippets_match_the_generator/);
	assert.equal(check(["docs/public/charts/bar.md", SNIPPET_FILE]).status, "pass");
	assert.equal(check([SNIPPET_FILE], { hasFreshnessTest: true }).status, "test", "a hand edit of the committed file runs the freshness test (review 1h note 4)");
	files["docs/public/charts/bar.md"] = [page("type: bar"), `${page("type: bar")}\nmore prose\n`];
	assert.equal(check(["docs/public/charts/bar.md"]).status, "pass", "prose does not change the snippet, so nothing could be regenerated");
	assert.equal(check(["crates/richview-editor/build.rs"]).status, "fail");
	assert.equal(check(["crates/richview-build/src/main.rs"]).status, "pass", "a change that does not touch snippet code");
	assert.equal(check(["crates/richview-build/src/main.rs"], { mentionsSnippets: () => true }).status, "fail");
	assert.equal(check(["docs/public/charts/new.md"], { after: () => page("type: new") }).status, "fail", "a new page adds a snippet");
	// With the editor's freshness test, a change that may alter a snippet runs it, so a generator change
	// whose output is already committed (app surfaces, after b5815c4f regenerated the file) passes.
	const tested = check(["crates/richview-editor/build.rs"], { hasFreshnessTest: true });
	assert.equal(tested.status, "test");
	assert.match(tested.reason, /richview-editor --test snippets/);
	assert.equal(check(["crates/richview-build/src/main.rs"], { hasFreshnessTest: true }).status, "pass", "nothing that could change a snippet");
	assert.equal(check(["crates/richview-core/src/registry.rs"], { hasFreshnessTest: true }).status, "test", "the registry can add a snippet (review 1g note 8)");
	assert.equal(check(["crates/richview-core/src/registry.rs"]).status, "pass", "without the test, a registry change is not judged by the diff");
	// With the test, any change to the generator's source folder runs it, prose and data included.
	for (const file of ["docs/public/charts/bar.md", "docs/public/charts/data/sales.csv"]) {
		const judged = check([file], { hasFreshnessTest: true });
		assert.equal(judged.status, "test", file);
		assert.match(judged.reason, /the snippets' source/);
	}
	assert.equal(check(["docs/public/blocks.md"], { hasFreshnessTest: true }).status, "pass", "another docs page is not the snippets' source");
});

test("Cargo manifests and workflow edits run the layer and CI contract tests", () => {
	for (const file of ["Cargo.toml", "crates/richview-core/Cargo.toml", "richview-chart/Cargo.toml", ".github/workflows/ci.yml", ".github/actions/setup-gate/action.yml"]) {
		assert.ok(affected([file], graph).suites.has("rv-gate"), file);
	}
});
