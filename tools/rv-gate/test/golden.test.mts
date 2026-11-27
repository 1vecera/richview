// Tests for the golden step's pure parts (DEL-60) and the every-block fixture (DEL-27, DEL-58,
// DEL-52 reuse it). None of them needs the release CLI.

import assert from "node:assert/strict";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import {
	GETTING_STARTED,
	GOLDEN_LIST,
	SNIPPETS_FILE,
	renderProblems,
	discoverInputs,
	expandSnippet,
	extractGettingStarted,
	judge,
	parseDeclaration,
	parseKnownStyles,
	sampleCommands,
	snippetReports,
	stageInput,
} from "../golden.mts";
import { STEPS } from "../lib/steps.mts";

const repo = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const spec = readFileSync(join(repo, "docs/RMD-SPEC.md"), "utf8");

/** The first column of a registry table in RMD-SPEC.md, which the docs-match step keeps equal to the code's. */
function registryColumn(name: string) {
	const block = new RegExp(`<!-- registry:begin ${name} -->\\n([\\s\\S]*?)<!-- registry:end -->`).exec(spec)?.[1];
	assert.ok(block, `RMD-SPEC.md has no registry table ${name}`);
	return block
		.split("\n")
		.map((line) => /^\| `([^`]+)` \|/.exec(line)?.[1])
		.filter((name): name is string => Boolean(name));
}

test("golden is a runnable step on the release CLI, writing under the run folder", () => {
	const step = STEPS.find((s) => s.id === "golden")!;
	assert.equal(step.unrun, undefined);
	assert.equal(step.needs, "build");
	assert.deepEqual(step.run![0].args.slice(0, 5), ["tools/rv-gate/golden.mts", "--bin", "target/release/richview", "--out", "{run}/golden"]);
});

test("discovery finds the INT-55 list, every example folder, the specimen and the first report", () => {
	const inputs = discoverInputs(repo);
	const dirs = inputs.map((i) => i.dir);
	for (const dir of GOLDEN_LIST) assert.ok(dirs.includes(dir), dir);
	for (const name of readdirSync(join(repo, "examples"))) {
		if (existsSync(join(repo, "examples", name, "report.rmd"))) assert.ok(dirs.includes(`examples/${name}`), name);
	}
	assert.ok(dirs.includes("tools/rv-gate/fixtures/typography"));
	assert.ok(inputs.find((i) => i.dir === "examples/first-report")!.why.includes("FEA-SF-42"));
	assert.equal(new Set(dirs).size, dirs.length);
	assert.ok(dirs.includes("examples/every-block"));
});

test("discovery refuses an INT-55 input that has no report", () => {
	assert.throws(() => discoverInputs(join(repo, "tools")), /INT-55/);
});

test("snippets expand to their defaults and become one report each", () => {
	assert.equal(expandSnippet(["a: ${1:one}", "b: ${2|up,down|}", "c: $0"]), "a: one\nb: up\nc: ");
	assert.equal(expandSnippet(["- {a: 1\\}", "u: '\\$'", "t: ${1:x\\}y}", "s: \\\\"]), "- {a: 1}\nu: '$'\nt: x}y\ns: \\");
	for (const r of snippetReports(JSON.parse(readFileSync(join(repo, SNIPPETS_FILE), "utf8")))) assert.doesNotMatch(r.rmd, /\\[}$]/, r.id);
	const reports = snippetReports(JSON.parse(readFileSync(join(repo, SNIPPETS_FILE), "utf8")));
	assert.ok(reports.length >= 13);
	for (const r of reports) {
		assert.match(r.rmd, /^---\n/);
		assert.doesNotMatch(r.rmd, /\$\{|\$\d/);
	}
	assert.match(reports.find((r) => r.id === "snippet/rv-metric")!.rmd, /```metric\nvalue: 1234/);
});

test("media snippet golden inputs carry real local assets at the snippet's default paths", (t) => {
	mkdirSync(join(repo, "tmp"), { recursive: true });
	const out = mkdtempSync(join(repo, "tmp/golden-snippet-media-"));
	t.after(() => rmSync(out, { recursive: true, force: true }));
	const snippets = snippetReports(JSON.parse(readFileSync(join(repo, SNIPPETS_FILE), "utf8")));
	for (const [prefix, asset, source] of [
		["rv-image", "photo.png", "fixture-image.png"],
		["rv-scrolly", "photo.png", "fixture-image.png"],
		["rv-audio", "interview.mp3", "tone.mp3"],
		["rv-video", "demo.mp4", "clip.mp4"],
	]) {
		const input = snippets.find((snippet) => snippet.id === `snippet/${prefix}`)!;
		const folder = join(out, prefix);
		assert.match(input.rmd, new RegExp(`media/${asset.replaceAll(".", "\\.")}`));
		stageInput({ cwd: repo, bin: "", folder, input: { ...input, dir: SNIPPETS_FILE, why: [] } });
		assert.deepEqual(readFileSync(join(folder, "media", asset)), readFileSync(join(repo, "examples/every-block/media", source)));
	}
});

test("a warning declaration is a code mapped to every style or to a list of styles", () => {
	assert.deepEqual(parseDeclaration('{"image.missing": "*", "a.b": ["air"]}'), { "image.missing": "*", "a.b": ["air"] });
	assert.throws(() => parseDeclaration('{"a.b": []}'), /non-empty/);
	assert.throws(() => parseDeclaration('{"a.b": true}'), /non-empty/);
	assert.throws(() => parseDeclaration("[]"), /object/);
});

test("judge fails errors and undeclared warnings, keeps info and declared warnings, and flags stale declarations", () => {
	const warn = { level: "warning", code: "w.one", message: "w", line: 3 };
	const results = { air: [warn, { level: "info", code: "i.one" }], sleek: [{ level: "error", code: "e.one", message: "boom" }] };
	assert.deepEqual(
		judge(results, {}).map((f) => [f.style, f.code, f.kind]),
		[
			["air", "w.one", "undeclared warning"],
			["sleek", "e.one", "error"],
		],
	);
	assert.deepEqual(judge({ air: [warn] }, { "w.one": "*" }), []);
	assert.deepEqual(judge({ air: [warn], sleek: [warn] }, { "w.one": ["air"] }).map((f) => f.style), ["sleek"]);
	assert.deepEqual(judge({ air: [] }, { "w.one": "*" }).map((f) => f.kind), ["stale declaration"]);
	assert.deepEqual(judge({ air: [warn] }, { "w.one": ["nope"] }).map((f) => f.kind).sort(), ["stale declaration", "undeclared warning"]);
	// Review 1j note 3: with lines, another block raising the same code fails, and so does a missing one.
	const at = (line: number) => ({ ...warn, line });
	const lines = parseDeclaration('{"w.one": {"styles": "*", "lines": [53]}}');
	assert.deepEqual(judge({ sleek: [at(53)] }, lines), []);
	assert.deepEqual(judge({ sleek: [at(53), at(13), at(36)] }, lines).map((f) => [f.line, f.kind]), [[13, "undeclared warning"], [36, "undeclared warning"]]);
	assert.deepEqual(judge({ sleek: [at(13)] }, lines).map((f) => [f.line, f.kind]), [[13, "undeclared warning"], [53, "stale declaration"]]);
	assert.throws(() => parseDeclaration('{"w.one": {"styles": "*", "lines": []}}'), /lines/);
	assert.throws(() => parseDeclaration('{"w.one": {"styles": "*", "lines": [53], "count": 1}}'), /lines/);
});

test("the builtin styles are read from the CLI's own style.unknown finding", () => {
	assert.deepEqual(parseKnownStyles("sleek, air, scientific, brutalist, hand-drawn"), ["sleek", "air", "scientific", "brutalist", "hand-drawn"]);
	assert.throws(() => parseKnownStyles(""), /Cannot read/);
	assert.throws(() => parseKnownStyles(undefined), /Cannot read/);
});

test("the getting-started commands come from the guide page and cannot drift from it", () => {
	const page = readFileSync(join(repo, GETTING_STARTED), "utf8");
	const gs = extractGettingStarted(page);
	assert.equal(gs.commands[0], `richview init ${gs.dir}`);
	assert.deepEqual(
		gs.commands.slice(1).map((c) => c.split(/\s+/)[1]),
		["validate", "check", "generate", "serve"],
	);
	assert.equal(gs.rendered, `${gs.dir}/Rendered/report.html`);
	assert.equal(gs.sample, "orders/report.rmd");
	const sample = sampleCommands(gs);
	assert.equal(sample.length, 4);
	assert.ok(sample.every((c: string|string[]) => c.includes(gs.sample) && !c.includes(gs.dir)));
	assert.throws(() => extractGettingStarted("no commands"), /sh/);
});

test("the every-block fixture uses every fence tag and chart type the registry lists", () => {
	const source = readFileSync(join(repo, "examples/every-block/report.rmd"), "utf8");
	const tags = registryColumn("tags");
	const types = registryColumn("chart-types");
	assert.ok(tags.length >= 9 && types.length >= 27, "the registry tables did not parse");
	const missingTags = tags.filter((tag) => !new RegExp(`^\`\`\`${tag}\\s*$`, "m").test(source));
	assert.deepEqual(missingTags, [], `examples/every-block/report.rmd lacks fence tags: ${missingTags.join(", ")}`);
	const chartTypes = [...source.matchAll(/^```chart\n(?:[^\n]*\n)*?type: (\S+)/gm)].map((m) => m[1]);
	const missingTypes = types.filter((type) => !chartTypes.includes(type));
	assert.deepEqual(missingTypes, [], `examples/every-block/report.rmd lacks chart types: ${missingTypes.join(", ")}`);
});

test("every file the every-block fixture names exists beside it", () => {
	const dir = join(repo, "examples/every-block");
	const source = readFileSync(join(dir, "report.rmd"), "utf8");
	const files = [...source.matchAll(/^\s*(?:src|poster|csv): (?:\{csv: )?((?:data|media)\/[^\s},]+)/gm)].map((m) => m[1]);
	const inline = [...source.matchAll(/\{csv: ([^}]+)\}/g)].map((m) => m[1]);
	assert.ok(files.length + inline.length >= 6);
	for (const file of [...files, ...inline]) assert.ok(existsSync(join(dir, file)), file);
});

test("a host error, a failing exit or a missing page fails the render even with no error diagnostic", () => {
	const ok = { diagnostics: [], ok: true };
	const hostFailure = { diagnostics: [], ok: false, error: "no connector registry" };
	assert.deepEqual(renderProblems({ style: "sleek", status: 0, json: ok, wroteHtml: true }), []);
	assert.equal(renderProblems({ style: "sleek", status: 1, json: hostFailure, wroteHtml: false })[0].kind, "host error");
	assert.equal(renderProblems({ style: "sleek", status: 1, json: { diagnostics: [] }, wroteHtml: false })[0].kind, "no result");
	assert.equal(renderProblems({ style: "sleek", status: 0, json: ok, wroteHtml: false })[0].kind, "no HTML");
	// An error diagnostic is judged elsewhere, so it is not counted twice.
	assert.deepEqual(renderProblems({ style: "sleek", status: 1, json: { diagnostics: [{ level: "error", code: "block.schema" }] }, wroteHtml: false }), []);
});
