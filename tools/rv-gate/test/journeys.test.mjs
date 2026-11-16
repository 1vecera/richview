// Tests for the journeys step's pure parts (DEL-76): the diagnostic shape, the fix taken from a
// diagnostic's own text, the release contract's hash, the recorder and the registry wiring. None of
// them needs the release CLI, a browser or the Wasm.

import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import test from "node:test";
import { journeyEngines, ADDED_BLOCKS, OPEN_WORLD, READ_ONLY, SETS, TOOLS, diagnosticProblems, fixFromDiagnostic, modelDigest, pageText, readReport, refusedReport, resultProblems, serverUrl } from "../journeys.mjs";
import { PKG_STAMP, findWasm, makeRelease, pkgFresh, previewTags, stampPkg, storySource } from "../journeys-publish.mjs";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, utimesSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Journey, reportLines, runJourney, show } from "../lib/journey.mjs";
import { STEPS } from "../lib/steps.mjs";

test("a diagnostic needs a level, code, message and path; line and tag are optional", () => {
	assert.deepEqual(diagnosticProblems({ level: "error", code: "block.schema", message: "m", path: "chart.type", line: 21, tag: "chart" }), []);
	assert.deepEqual(diagnosticProblems({ level: "info", code: "c", message: "m", path: "p" }), []);
	assert.equal(diagnosticProblems({ level: "fatal", code: "c", message: "m", path: "p" }).length, 1);
	assert.equal(diagnosticProblems({ level: "error", code: "c", message: "", path: "p" }).length, 1);
	assert.equal(diagnosticProblems({ level: "error", code: "c", message: "m" }).length, 1);
	assert.equal(diagnosticProblems({ level: "error", code: "c", message: "m", path: "p", line: 0 }).length, 1);
	assert.equal(diagnosticProblems({ level: "error", code: "c", message: "m", path: "p", tag: 3 }).length, 1);
	assert.deepEqual(diagnosticProblems(null), ["is not an object"]);
});

test("a result needs ok and a diagnostics array whose members are all well formed", () => {
	assert.deepEqual(resultProblems({ ok: true, diagnostics: [] }), []);
	assert.deepEqual(resultProblems({ ok: "yes", diagnostics: [] }), ["ok is not a boolean"]);
	assert.deepEqual(resultProblems({ ok: true }), ["diagnostics is not an array"]);
	const problems = resultProblems({ ok: false, diagnostics: [{ level: "error", code: "block.schema", message: "m" }] });
	assert.equal(problems.length, 1);
	assert.match(problems[0], /diagnostics\[0\] \(block\.schema\) path/);
});

const callout = "```callout\nkind: loud\ntitle: T\n```\n";
test("a schema error's own text names the value the fix writes, on the diagnostic's line", () => {
	const source = `intro\n${callout}`;
	const fix = fixFromDiagnostic({ message: "callout.kind: expected up, down, neutral or hold", path: "callout.kind", line: 3 }, source);
	assert.deepEqual({ line: fix.line, from: fix.from, to: fix.to }, { line: 3, from: "loud", to: "up" });
	assert.equal(fix.source, "intro\n```callout\nkind: up\ntitle: T\n```\n");
});

test("a diagnostic's fix text wins over its message, and a line with no such key is not guessed", () => {
	const source = `intro\n${callout}`;
	const fix = fixFromDiagnostic({ message: "Bad kind", fix: "expected hold or up", path: "callout.kind", line: 3 }, source);
	assert.equal(fix.to, "hold");
	assert.equal(fixFromDiagnostic({ message: "Bad kind", expected: "neutral, up or down", path: "callout.kind", line: 3 }, source).to, "neutral");
	assert.equal(fixFromDiagnostic({ message: "Bad kind", expected: "one of 'up', 'down', 'neutral' or 'hold'", path: "callout.kind", line: 3 }, source).to, "up");
	assert.equal(fixFromDiagnostic({ message: "callout.kind: expected up or down", path: "callout.kind", line: 1 }, "a\nb\nc\nd\ne\nf\n"), null);
	assert.equal(fixFromDiagnostic({ message: "Asset 'x' is absent. Add the file.", path: "chart.data.csv", line: 3 }, source), null);
	assert.equal(fixFromDiagnostic({ message: "expected a or b", path: "k" }, source), null);
});

test("a report read follows the report-origin redirect with the token on every hop", async () => {
	const seen = [];
	const pages = {
		"http://127.0.0.1:1/r/a": [303, "http://127.0.0.1:2/r/a"],
		"http://127.0.0.1:2/r/a": [200, null, "Title"],
		"http://127.0.0.1:2/r/b": [303, "/r/b?rv=signin"],
		"http://127.0.0.1:2/r/b?rv=signin": [303, "http://127.0.0.1:2/?m=Sign%20in"],
		"http://127.0.0.1:2/?m=Sign%20in": [200, null, "sign-in form"],
	};
	const fake = async (url, init) => {
		seen.push([url, init.headers.authorization ?? null, init.redirect]);
		const [status, location, text = ""] = pages[url];
		return { status, headers: new Headers(location ? { location } : {}), text: async () => text };
	};
	assert.deepEqual(await readReport("http://127.0.0.1:1/r/a", "t", fake), { status: 200, url: "http://127.0.0.1:2/r/a", text: "Title", headers: new Headers() });
	assert.deepEqual(seen, [["http://127.0.0.1:1/r/a", "Bearer t", "manual"], ["http://127.0.0.1:2/r/a", "Bearer t", "manual"]]);
	const anonymous = await readReport("http://127.0.0.1:2/r/b", undefined, fake);
	assert.deepEqual(anonymous, { status: 200, url: "http://127.0.0.1:2/?m=Sign%20in", text: "sign-in form", headers: new Headers() });
	assert.equal(refusedReport(anonymous, "Title"), true);
	assert.equal(refusedReport({ ...anonymous, text: "private Title" }, "Title"), false, "even a sign-in page must not expose the title");
	assert.equal(refusedReport({ ...anonymous, url: "http://127.0.0.1:2/r/a" }, "Title"), false, "an empty successful page is not an access refusal");
	assert.equal(refusedReport({ ...anonymous, status: 303 }, "Title"), false, "an unexamined redirect body cannot prove refusal");
});

test("the bearer token goes only to the server's own two origins, never to a third", async () => {
	const seen = [];
	const pages = {
		"http://127.0.0.1:1/r/a": [303, "http://127.0.0.1:2/r/a"],
		"http://127.0.0.1:2/r/a": [303, "http://127.0.0.1:1/back"],
		"http://127.0.0.1:1/back": [303, "https://evil.example/r/a"],
		"https://evil.example/r/a": [303, "http://127.0.0.1:1/again"],
		"http://127.0.0.1:1/again": [200, null, "done"],
		"http://127.0.0.1:1/r/f": [303, "https://evil.example/r/f"],
		"https://evil.example/r/f": [200, null, "foreign"],
		"http://127.0.0.1:1/r/g": [303, "http://127.0.0.1:2/g"],
		"http://127.0.0.1:2/g": [303, "http://127.0.0.1:3/g"],
		"http://127.0.0.1:3/g": [200, null, "third"],
	};
	const fake = async (url, init) => {
		seen.push([url, init.headers.authorization ?? null]);
		const [status, location, text = ""] = pages[url];
		return { status, headers: new Headers(location ? { location } : {}), text: async () => text };
	};
	await readReport("http://127.0.0.1:1/r/a", "t", fake);
	assert.deepEqual(seen.map(([url, auth]) => [new URL(url).host, auth]), [
		["127.0.0.1:1", "Bearer t"],
		["127.0.0.1:2", "Bearer t"],
		["127.0.0.1:1", "Bearer t"],
		["evil.example", null],
		["127.0.0.1:1", "Bearer t"],
	]);
	seen.length = 0;
	await readReport("http://127.0.0.1:1/r/f", "t", fake);
	assert.equal(seen.find(([url]) => url.startsWith("https://evil"))[1], null, "a redirect to a foreign origin carries no token");
	seen.length = 0;
	await readReport("http://127.0.0.1:1/r/g", "t", fake);
	assert.equal(seen.at(-1)[1], null, "a third origin carries no token");
});

test("the server's address is the first loopback URL it logs", () => {
	assert.equal(serverUrl("richview: warning\nRichView at http://127.0.0.1:49671 (data: /x)\n"), "http://127.0.0.1:49671");
	assert.equal(serverUrl("nothing yet"), undefined);
});

test("page text drops markup, scripts and styles", () => {
	assert.equal(pageText("<style>p{}</style><h1>A &amp; B</h1><script>var x=1</script><p>c</p>"), "A & B c");
});

test("a model digest keeps what compares two renderings of one chart", () => {
	assert.deepEqual(modelDigest([{ width: 520, geometry: { kind: "bar" }, settled: true, canvas: {} }]), [{ width: 520, geometry: { kind: "bar" } }]);
});

test("the scripted edit adds one chart and one table", () => {
	assert.equal((ADDED_BLOCKS.match(/```chart/g) ?? []).length, 1);
	assert.equal((ADDED_BLOCKS.match(/```table/g) ?? []).length, 1);
});

test("FEA-SF-09 lists nine tools, five read-only and five open-world, and the release set is the cheap journeys", () => {
	assert.equal(TOOLS.length, 9);
	assert.equal(READ_ONLY.length, 5);
	assert.equal(OPEN_WORLD.length, 5);
	assert.deepEqual([...READ_ONLY, ...OPEN_WORLD].filter((t) => !TOOLS.includes(t)), []);
	assert.deepEqual(SETS.release, ["author", "agent"]);
	assert.deepEqual(SETS.full, ["author", "agent", "cowork", "publish"]);
});

const sha = (bytes) => createHash("sha256").update(bytes).digest("hex");
test("a release's id is the hash of its manifest with an empty id, and its objects are registered", () => {
	const { release, blobs } = makeRelease("1".repeat(40), [{ id: "story", state: "published", audience: "public", body: "v1" }]);
	assert.equal(release.id, sha(Buffer.from(JSON.stringify({ ...release, id: "" }))));
	assert.equal(release.schema, 1);
	for (const item of release.items) {
		for (const hash of [item.article, item.content, ...Object.values(item.assets)]) assert.ok(release.objects[hash], hash);
	}
	for (const [hash, object] of Object.entries(release.objects)) {
		assert.equal(sha(blobs.get(hash)), hash);
		assert.equal(object.bytes, blobs.get(hash).length);
	}
	assert.deepEqual(Object.keys(release.objects), Object.keys(release.objects).sort());
	assert.equal(Object.keys(release.items[0].assets)[0], "og.png");
});

test("only a published public item has a share image; a changed body changes the release", () => {
	const a = makeRelease("1".repeat(40), [{ id: "note", state: "published", audience: "members", body: "v1" }]);
	assert.deepEqual(a.release.items[0].assets, {});
	const b = makeRelease("1".repeat(40), [{ id: "note", state: "published", audience: "members", body: "v2" }]);
	assert.notEqual(a.release.id, b.release.id);
	assert.equal(makeRelease("1".repeat(40), [{ id: "note", state: "published", audience: "members", body: "v1" }]).release.id, a.release.id);
});

test("preview tags are read from a page head", () => {
	const html = '<meta property="og:title" content="A story"><meta property="og:image" content="https://x/a.png"><meta name="twitter:card" content="summary_large_image"><meta name="viewport" content="x">';
	assert.deepEqual(previewTags(html), { "og:title": "A story", "og:image": "https://x/a.png", "twitter:card": "summary_large_image" });
	assert.deepEqual(previewTags('<meta property="og:type" content="website">'), { "og:type": "website" });
});

test("a story source carries its publish block and body", () => {
	const source = storySource("published", "Body text");
	assert.match(source, /state: "published"/);
	assert.match(source, /Body text\n$/);
});

test("the Wasm is found from the flag, then the environment, then the shared target", () => {
	assert.equal(findWasm({ flag: "/no/such", env: {}, home: "/no/home", cwd: "/no/cwd" }), undefined);
	assert.equal(findWasm({ flag: process.execPath, env: {}, home: "/no/home", cwd: "/no/cwd" }), process.execPath);
	assert.equal(findWasm({ env: { RICHVIEW_PUBLISH_WASM: process.execPath }, home: "/no/home", cwd: "/no/cwd" }), process.execPath);
});

test("the Wasm is looked for in one place only, so a gate run's copy never borrows another checkout's build", () => {
	const name = "wasm32-unknown-unknown/release/richview_publish_server.wasm";
	const root = mkdtempSync(join(tmpdir(), "rv-wasm-"));
	const shared = join(root, "shared");
	mkdirSync(join(shared, "wasm32-unknown-unknown/release"), { recursive: true });
	writeFileSync(join(shared, name), "theirs");
	const env = { CARGO_TARGET_DIR: shared };
	assert.equal(findWasm({ flag: "/no/such", env, home: "/no/home", cwd: "/no/cwd" }), undefined, "a named path that is missing is not replaced by another build");
	assert.equal(findWasm({ env, home: "/no/home", cwd: "/no/cwd" }), join(shared, name), "a checkout without a target uses the shared one");
	const cwd = join(root, "checkout");
	mkdirSync(join(cwd, "target/release"), { recursive: true });
	assert.equal(findWasm({ env, home: "/no/home", cwd }), undefined, "a gate run's copy without the Wasm does not borrow the shared target's");
	mkdirSync(join(cwd, "target/wasm32-unknown-unknown/release"), { recursive: true });
	writeFileSync(join(cwd, "target", name), "ours");
	assert.equal(findWasm({ env, home: "/no/home", cwd }), join(cwd, "target", name));
	rmSync(root, { recursive: true });
});

test("a journey records failures with their step, expected, actual and owner", async () => {
	const j = new Journey("demo", "Demo");
	j.info("a fact");
	j.equal("passes", "renderer", [1], [1]);
	j.equal("fails", "chart engine", [1], [2]);
	j.notBuilt("thumbnail", "publish", "the thumbnail");
	j.assert("holds", "site", true, "x", "y");
	await runJourney(j, async (journey) => {
		journey.need("stops", "app surfaces", false, "ok", "bad");
		journey.assert("never reached", "site", true, "", "");
	});
	assert.equal(j.passed, false);
	assert.deepEqual(j.failures.map((f) => f.step), ["fails", "thumbnail", "stops"]);
	assert.equal(j.failures[1].notBuilt, true);
	assert.equal(j.failures[1].actual, "not built");
	assert.equal(j.steps.some((s) => s.step === "never reached"), false);
	const lines = reportLines([j]);
	assert.match(lines[0], /^FAIL demo \(Demo\) in [\d.]+ s: 2\/5 steps$/);
	assert.match(lines[1], /FAIL \[chart engine\] fails: expected \[2\]; got \[1\]/);
});

test("a crash is recorded as a gate failure, and an unknown owner is refused", async () => {
	const j = new Journey("demo", "Demo");
	await runJourney(j, async () => {
		throw new Error("boom");
	});
	assert.equal(j.failures[0].owner, "gate");
	assert.throws(() => j.record("x", "nobody", true));
	assert.equal(show("a   b"), "a b");
	assert.equal(show("x".repeat(300)).length, 243);
});

test("the registry wires the release journeys and the full ones", () => {
	const release = STEPS.find((s) => s.id === "journeys");
	const full = STEPS.find((s) => s.id === "journeys-full");
	assert.equal(release.tier, "integration");
	assert.equal(full.tier, "full");
	for (const step of [release, full]) {
		assert.equal(step.needs, "build");
		assert.equal(step.unrun, undefined);
		assert.equal(step.run[0].entry, "tools/rv-gate/journeys.mjs");
	}
	assert.ok(release.run[0].args.join(" ").endsWith("--set release"));
	assert.ok(full.run[0].args.join(" ").endsWith("--set full"));
	assert.deepEqual(full.cards, ["DEL-76", "INT-07", "FEA-SF-55", "FEA-BR-26", "FEA-BR-29"]);
});

test("a built Publish Wasm counts only when it is newer than every source it is built from", () => {
	const cwd = mkdtempSync(join(tmpdir(), "rv-pkg-"));
	try {
		mkdirSync(join(cwd, "crates/richview-core/src"), { recursive: true });
		writeFileSync(join(cwd, "crates/richview-core/src/lib.rs"), "");
		const wasm = join(cwd, "server_bg.wasm");
		writeFileSync(wasm, "");
		const past = new Date(Date.now() - 60_000);
		utimesSync(join(cwd, "crates/richview-core/src/lib.rs"), past, past);
		assert.equal(pkgFresh(cwd, wasm), true);
		utimesSync(wasm, new Date(Date.now() - 120_000), new Date(Date.now() - 120_000));
		assert.equal(pkgFresh(cwd, wasm), false, "a source changed after the build");
		assert.equal(pkgFresh(cwd, join(cwd, "missing.wasm")), false);
	} finally {
		rmSync(cwd, { recursive: true, force: true });
	}
});

test("a Publish Wasm with its sources' digest counts while their bytes match it, whatever their file times (S27-858)", () => {
	const cwd = mkdtempSync(join(tmpdir(), "rv-pkg-"));
	try {
		const lib = join(cwd, "crates/richview-core/src/lib.rs");
		mkdirSync(join(cwd, "crates/richview-core/src"), { recursive: true });
		writeFileSync(lib, "pub fn a() {}");
		writeFileSync(join(cwd, "Cargo.lock"), "lock");
		const pkg = join(cwd, "cloudflare/publish/pkg");
		mkdirSync(pkg, { recursive: true });
		const wasm = join(pkg, "richview_publish_server_bg.wasm");
		writeFileSync(wasm, "");
		stampPkg(cwd);
		assert.match(readFileSync(join(pkg, PKG_STAMP), "utf8"), /^[0-9a-f]{64}\n$/);
		// The Cargo shim touched every source after the build: the bytes still match.
		const later = new Date(Date.now() + 120_000);
		utimesSync(lib, later, later);
		utimesSync(join(cwd, "Cargo.lock"), later, later);
		assert.equal(pkgFresh(cwd, wasm), true, "touched, not changed");
		writeFileSync(join(cwd, "crates/richview-core/src/.DS_Store"), "finder");
		assert.equal(pkgFresh(cwd, wasm), true, "a dot file is not a source");
		writeFileSync(lib, "pub fn b() {}");
		assert.equal(pkgFresh(cwd, wasm), false, "a changed source");
		writeFileSync(lib, "pub fn a() {}");
		assert.equal(pkgFresh(cwd, wasm), true);
		writeFileSync(join(cwd, "crates/richview-core/src/new.rs"), "");
		assert.equal(pkgFresh(cwd, wasm), false, "an added source");
	} finally {
		rmSync(cwd, { recursive: true, force: true });
	}
});

test("publish-tests records the glue's source digest right after building it (S27-858)", () => {
	const run = STEPS.find((s) => s.id === "publish-tests").run;
	const build = run.findIndex((c) => c.cmd === "npm" && c.args.includes("build:wasm"));
	assert.ok(build >= 0);
	const stamp = run[build + 1];
	assert.equal(stamp.cmd, "node");
	assert.match(stamp.args.join(" "), /journeys-publish\.mjs.*stampPkg\(process\.cwd\(\)\)/);
});

test("author journey selects Chromium only for an explicitly admitted prelaunch run", () => {
	assert.deepEqual(journeyEngines({}),["chromium","firefox","webkit"]);
	assert.deepEqual(journeyEngines({RV_GATE_PRELAUNCH_CHROMIUM_ONLY:"0"}),["chromium","firefox","webkit"]);
	assert.deepEqual(journeyEngines({RV_GATE_PRELAUNCH_CHROMIUM_ONLY:"1"}),["chromium"]);
});
