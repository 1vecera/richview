// Tests for the gate's own rules: PR fields, the step registry, secret scanning, deleted-test
// and weakening detection, and the boundary checks on this repository.

import assert from "node:assert/strict";
import { execFileSync, spawn, spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, utimesSync, writeFileSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { seeEverything } from "../lib/captures.mjs";
import { TEMP_ROOT, cleanCheckoutTmp, failedRunsKept, runActive, runTemp, runsToPrune, sweepRunTemps, tmpSnapshot, trimFailedRun } from "../lib/cleanup.mjs";
import { arch, deletedTests, entrypoints, newPrivateOnly, productJsProblems, retiredTests, scanText, thresholdChanges, weakening } from "../lib/checks.mjs";
import { globToRegExp } from "../lib/glob.mjs";
import { parsePrBody, validatePrFields, waitForMergeable } from "../lib/pr.mjs";
import { STEPS, TIERS, isRegistry, mergeRegistry, stepsForTier } from "../lib/steps.mjs";
import { subcommands } from "../lib/surface.mjs";
import { HOLD, cargoHolder, holdCargo, runningCargo, waitForCargo, waitUntilIdle, workspaceBuiltAt } from "../lib/target.mjs";
import { PRE_PUSH } from "../lib/hooks.mjs";

const repo = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");

const body = `Refs S27-698
decision: design
cause: request "make the charts sing"
items: FEA-CH-37, STY-05
assumed: STY-21

Some text.

## Retired tests
- \`richview-chart/tests/year_axis.rs::old_axis\` — replaced by the geometry model (ARC-18)
- \`scripts/verify-reel.mjs\`: the reel leaves the product (ARC-42)

## Checklist
- [x] fast tier`;

test("PR fields parse issue, class, cause, items and retired tests", () => {
	const fields = parsePrBody(body);
	assert.equal(fields.issue, "S27-698");
	assert.equal(fields.decision, "design");
	assert.deepEqual(fields.cause, { kind: "request", detail: '"make the charts sing"' });
	assert.deepEqual(fields.items, ["FEA-CH-37", "STY-05"]);
	assert.deepEqual(fields.assumed, ["STY-21"]);
	assert.deepEqual(
		fields.retired.map((r) => r.test),
		["richview-chart/tests/year_axis.rs::old_axis", "scripts/verify-reel.mjs"],
	);
	assert.deepEqual(validatePrFields(fields, { knownItems: new Set(["FEA-CH-37", "STY-05", "STY-21"]) }), []);
});

test("a stack's joined messages keep the retired tests of every commit that lists some", () => {
	const dir = mkdtempSync(join(tmpdir(), "rv-gate-retired-stack-"));
	const git = (...args) => execFileSync("git", ["-C", dir, "-c", "user.name=t", "-c", "user.email=t@example.com", ...args], { encoding: "utf8" }).trim();
	try {
		git("init", "-q");
		git("commit", "-q", "--allow-empty", "-m", "base");
		const base = git("rev-parse", "HEAD");
		for (const name of ["one", "two", "three"]) {
			git("commit", "-q", "--allow-empty", "-m", `Retire ${name}\n\nRefs S27-1\n\n## Retired tests\n- \`tests/${name}.rs::goes\` — obsolete (${name})\n\n## Checklist\n- [x] fast`);
		}
		// Read as fast reads a stack: every message since the base, joined.
		const messages = git("log", "--format=%B", `${base}..HEAD`);
		assert.deepEqual(
			parsePrBody(messages).retired.map((r) => `${r.test} ${r.reason}`).sort(),
			["tests/one.rs::goes obsolete (one)", "tests/three.rs::goes obsolete (three)", "tests/two.rs::goes obsolete (two)"],
		);
	} finally {
		rmSync(dir, { recursive: true, force: true });
	}
});

test("PR validation refuses a missing issue, an unknown class, design plus feature and a causeless design PR", () => {
	assert.match(validatePrFields(parsePrBody("decision: none")).join(" "), /Refs S27/);
	assert.match(validatePrFields(parsePrBody("Refs S27-1\ndecision: big")).join(" "), /Unknown decision/);
	assert.match(validatePrFields(parsePrBody("Refs S27-1\ndecision: design and feature")).join(" "), /split/);
	assert.match(validatePrFields(parsePrBody("Refs S27-1\ndecision: feature")).join(" "), /cause/);
	assert.match(validatePrFields(parsePrBody("Refs S27-1\ndecision: none\nitems: FOO-1")).join(" "), /not a lockdown item/);
	assert.deepEqual(validatePrFields(parsePrBody("Refs S27-1\ndecision: none")), []);
	assert.deepEqual(parsePrBody("cause: loop:night-03").cause, { kind: "loop", detail: "night-03" });
});

test("the registry has unique step IDs, known tiers, and every unbuilt step names its reason", () => {
	const ids = STEPS.map((s) => s.id);
	assert.equal(new Set(ids).size, ids.length);
	for (const step of STEPS) {
		assert.ok(TIERS.includes(step.tier), step.id);
		assert.ok(step.cards.length > 0, `${step.id} names its cards`);
		assert.ok(step.unrun || step.fn || step.run?.length, `${step.id} has something to run`);
	}
	assert.ok(stepsForTier("fast").every((s) => s.tier === "fast"));
	assert.ok(stepsForTier("full", "prod").length > stepsForTier("integration", "dev").length - 3);
	assert.ok(!stepsForTier("integration", "fast").some((s) => s.mode === "prod"));
});

test("glob matching handles one segment, any depth and literal dots", () => {
	assert.ok(globToRegExp("scripts/verify-*").test("scripts/verify-docs.mjs"));
	assert.ok(!globToRegExp("scripts/verify-*").test("scripts/x/verify-docs.mjs"));
	assert.ok(globToRegExp("crates/*/src/**/*.rs").test("crates/a/src/lib.rs"));
	assert.ok(globToRegExp("crates/*/src/**/*.rs").test("crates/a/src/x/y.rs"));
	assert.ok(!globToRegExp("a.b").test("axb"));
});

test("secret and private-data patterns catch tokens, keys, home paths and personal addresses", () => {
	const lines = [
		`token = "ghp_${"a".repeat(36)}"`,
		"-----BEGIN OPENSSH PRIVATE KEY-----",
		"see /Users/danny/notes/plan.md",
		"mail me at someone@gmail.com",
		"postgres://reader:hunter22@db.internal/sales",
		"https://www.notion.so/Private-Page-123",
	].join("\n");
	const labels = scanText("x.md", lines).map((f) => f.label);
	for (const label of ["GitHub token", "private key", "host home path", "personal email", "connection string with password", "Notion link"]) {
		assert.ok(labels.includes(label), label);
	}
	assert.deepEqual(scanText("x.md", "contact: test@example.invalid; path /Users/you/report"), []);
	assert.deepEqual(scanText("docs/a.md", "see /Users/danny/x", [{ file: "docs/*.md", pattern: "host home path" }]), []);
});

test("a rewritten line keeps a private address the file already held; a new address or any secret is a finding", () => {
	const page = "assets/impressum.html";
	const findings = scanText(page, "<p>Contact: owner@outlook.com, or new@gmail.com</p>\nghp_" + "a".repeat(36));
	const base = { [page]: "<p>Write to owner@outlook.com</p>\nghp_" + "a".repeat(36) };
	const kept = newPrivateOnly(findings, (file) => base[file]);
	assert.deepEqual(
		kept.map((f) => [f.label, f.match.slice(0, 7)]),
		[
			["GitHub token", "ghp_aaa"],
			["personal email", "new@gma"],
		],
	);
	assert.equal(newPrivateOnly(findings, () => undefined).length, 3, "a new file has nothing to keep");
});

test("clap subcommands are read from help text without `help`", () => {
	const help = "Usage: richview <COMMAND>\n\nCommands:\n  generate  \n  asset     Upload\n  help      Print this\n\nOptions:\n  -h";
	assert.deepEqual(subcommands(help), ["generate", "asset"]);
});

function scratchRepo() {
	const dir = mkdtempSync(join(tmpdir(), "rv-gate-test-"));
	const git = (...args) => execFileSync("git", args, { cwd: dir, encoding: "utf8" }).trim();
	git("init", "-q", "-b", "dev");
	git("config", "user.email", "gate@example.invalid");
	git("config", "user.name", "Gate");
	return { dir, git };
}

test("a ledger entry explains only its own batch's removal, and a test is moved only to a file where it is new", () => {
	const { dir, git } = scratchRepo();
	try {
		mkdirSync(join(dir, "tests"));
		mkdirSync(join(dir, "tools/rv-gate"), { recursive: true });
		writeFileSync(join(dir, "tests/a.rs"), "#[test]\nfn shared() {}\n\n#[test]\nfn goes() {}\n");
		writeFileSync(join(dir, "tests/b.rs"), "#[test]\nfn shared() {}\n");
		git("add", "-A");
		git("commit", "-qm", "base");
		const old = git("rev-parse", "HEAD");
		writeFileSync(join(dir, "README"), "x\n");
		git("add", "-A");
		git("commit", "-qm", "an earlier batch");
		const base = git("rev-parse", "HEAD");
		// The ledger names an earlier commit for `goes`: it cannot explain this batch's deletion.
		writeFileSync(join(dir, "tools/rv-gate/retired-tests.json"), JSON.stringify({ tests: [{ test: "tests/a.rs::goes", reason: "old", commit: old }] }));
		writeFileSync(join(dir, "tests/a.rs"), "#[test]\nfn goes_not() {}\n");
		git("add", "-A");
		git("commit", "-qm", "head");
		const head = git("rev-parse", "HEAD");
		// `shared` still exists in b.rs, but it was there before: deleting it from a.rs is a removal.
		const result = retiredTests({ cwd: dir, base, head, pr: { retired: [] } });
		assert.equal(result.status, "fail");
		assert.match(result.reason, /tests\/a\.rs::goes/);
		assert.match(result.reason, /tests\/a\.rs::shared/);
		const listed = retiredTests({ cwd: dir, base, head, pr: { retired: parsePrBody("## Retired tests\n- `tests/a.rs::goes`: obsolete\n- `tests/a.rs::tests::shared`: duplicate of b.rs").retired } });
		assert.equal(listed.status, "pass", "a module path between file and name still names the test");
	} finally {
		rmSync(dir, { recursive: true, force: true });
	}
});

test("deleted, removed and newly ignored tests are found and must be listed with a reason", () => {
	const { dir, git } = scratchRepo();
	try {
		mkdirSync(join(dir, "tests"));
		mkdirSync(join(dir, "scripts"));
		writeFileSync(join(dir, "tests/a.rs"), "#[test]\nfn keeps() {}\n\n#[test]\nfn goes() {}\n\n#[test]\nfn sleeps() {}\n\n#[test]\nfn travels() {}\n");
		writeFileSync(join(dir, "scripts/verify-old.mjs"), "// old\n");
		git("add", "-A");
		git("commit", "-qm", "base");
		const base = git("rev-parse", "HEAD");
		writeFileSync(join(dir, "tests/a.rs"), "#[test]\nfn keeps() {}\n\n#[test]\n#[ignore = \"slow\"]\nfn sleeps() {}\n");
		rmSync(join(dir, "scripts/verify-old.mjs"));
		// A test that moves to another file, as in a crate rename, is not retired.
		writeFileSync(join(dir, "tests/moved.rs"), "#[test]\nfn travels() {}\n");
		git("add", "-A");
		git("commit", "-qm", "head");
		const head = git("rev-parse", "HEAD");
		const removed = deletedTests({ cwd: dir, base, head }).map((r) => r.test);
		assert.deepEqual(removed.sort(), ["scripts/verify-old.mjs", "tests/a.rs (skip added)", "tests/a.rs::goes"].sort());
		const unexplained = retiredTests({ cwd: dir, base, head, pr: parsePrBody("## Retired tests\n- `tests/a.rs::goes` — obsolete") });
		assert.equal(unexplained.status, "fail");
		assert.match(unexplained.reason, /verify-old/);
		const explained = retiredTests({
			cwd: dir,
			base,
			head,
			pr: parsePrBody("## Retired tests\n- `tests/a.rs::goes` — obsolete\n- `scripts/verify-old.mjs` — gone\n- `tests/a.rs (skip added)` — slow tier"),
		});
		assert.equal(explained.status, "pass");
	} finally {
		rmSync(dir, { recursive: true, force: true });
	}
});

test("removing a gate step or editing an answer outside the app is held for Daniel", () => {
	const { dir, git } = scratchRepo();
	try {
		mkdirSync(join(dir, "tools/rv-gate/lib"), { recursive: true });
		mkdirSync(join(dir, "docs/lockdown/answers"), { recursive: true });
		writeFileSync(join(dir, "tools/rv-gate/lib/steps.mjs"), '{ id: "fmt" }\n{ id: "perf" }\n');
		writeFileSync(join(dir, "docs/lockdown/answers/log.jsonl"), `${JSON.stringify({ itemId: "INT-01", decision: "accept" })}\n`);
		writeFileSync(join(dir, "docs/lockdown/answers/01-intent.json"), JSON.stringify({ answers: { "INT-01": { decision: "accept" } } }));
		git("add", "-A");
		git("commit", "-qm", "base");
		const base = git("rev-parse", "HEAD");
		writeFileSync(join(dir, "tools/rv-gate/lib/steps.mjs"), '{ id: "fmt" }\n');
		writeFileSync(join(dir, "docs/lockdown/answers/01-intent.json"), JSON.stringify({ answers: { "INT-01": { decision: "reject" } } }));
		git("add", "-A");
		git("commit", "-qm", "head");
		const head = git("rev-parse", "HEAD");
		const result = weakening({ cwd: dir, base, head });
		assert.equal(result.status, "fail");
		assert.match(result.reason, /step 'perf' removed/);
		assert.match(result.reason, /INT-01 answer differs/);
	} finally {
		rmSync(dir, { recursive: true, force: true });
	}
});

test("a gated run keeps dev's steps, skips retired suites and runs the suites a PR adds", () => {
	const node = (script) => ({ cmd: "node", args: [script], entry: script });
	const base = [
		{ id: "fmt", tier: "fast", run: [{ cmd: "cargo", args: ["fmt", "--check"] }] },
		{ id: "chromium", tier: "integration", needs: "build", entries: ["scripts/verify-a.mjs"], run: [node("scripts/verify-a.mjs"), node("scripts/verify-old.mjs")] },
		{ id: "firefox", tier: "full", run: [{ cmd: "node", args: ["scripts/verify-old.mjs"] }] },
		{ id: "motion", tier: "integration", needs: "build", unrun: "Not built yet." },
		{ id: "journeys", tier: "integration", unrun: "Not built yet." },
		{ id: "coverage", tier: "integration", fn: "coverage" },
		{ id: "tool-tests", tier: "fast", run: [{ cmd: "node", args: ["--test", "tools/rv-gate/test/"] }, { cmd: "node", args: ["--test", "tools/x/test/a.test.mjs"], entry: "tools/x/test/*.test.mjs" }, { cmd: "node", args: ["--test", "tools/y/test/*.test.mjs"] }] },
	];
	const head = [
		// The PR drops `fmt`, empties `coverage`, moves `chromium` to the fast tier and claims
		// every script as an entry point; none of that is taken.
		{ id: "chromium", tier: "fast", entries: ["scripts/verify-*.mjs"], run: [node("scripts/verify-a.mjs"), node("scripts/verify-new.mjs"), node("scripts/verify-ghost.mjs")] },
		{ id: "motion", tier: "integration", needs: undefined, parallel: 1e9, run: [node("scripts/verify-motion.mjs")] },
		// A step dev has not built is not adopted with a command that runs no script, even when
		// it claims an existing file as its entry.
		{ id: "journeys", tier: "integration", run: [{ cmd: "true", args: [], entry: "scripts/verify-a.mjs" }] },
		{ id: "firefox", tier: "full", run: [{ cmd: "true", args: [], entry: "scripts/verify-extra.mjs" }] },
		{ id: "coverage", tier: "integration", run: [] },
		{ id: "extra", tier: "integration", run: [node("scripts/verify-extra.mjs")] },
	];
	const files = new Set(["scripts/verify-a.mjs", "scripts/verify-new.mjs", "scripts/verify-motion.mjs", "scripts/verify-extra.mjs"]);
	const { steps, retired, added } = mergeRegistry(base, head, (path) => files.has(path));
	assert.deepEqual(steps.map((s) => s.id), ["fmt", "chromium", "firefox", "motion", "journeys", "coverage", "tool-tests"]);
	const chromium = steps.find((s) => s.id === "chromium");
	assert.equal(chromium.tier, "integration", "the tier stays dev's");
	assert.deepEqual(chromium.entries, ["scripts/verify-a.mjs"], "entry globs stay dev's");
	assert.deepEqual(chromium.run.map((c) => c.entry), ["scripts/verify-a.mjs", "scripts/verify-new.mjs"]);
	assert.deepEqual(steps.find((s) => s.id === "firefox").run, [], "a retired suite without an entry is found by its script, and a fake entry adds nothing");
	const motion = steps.find((s) => s.id === "motion");
	assert.equal(motion.unrun, undefined);
	assert.equal(motion.needs, "build", "an adopted step keeps dev's needs");
	assert.equal(motion.parallel, undefined, "and dev's parallelism");
	assert.deepEqual(motion.run.map((c) => c.entry), ["scripts/verify-motion.mjs"]);
	assert.equal(steps.find((s) => s.id === "journeys").unrun, "Not built yet.");
	assert.equal(steps.find((s) => s.id === "coverage").fn, "coverage");
	assert.equal(steps.find((s) => s.id === "tool-tests").run.length, 3, "commands that name no single script, or a glob, are kept");
	assert.deepEqual(retired, ["scripts/verify-old.mjs"]);
	assert.deepEqual(added, ["scripts/verify-new.mjs", "scripts/verify-motion.mjs"]);
	assert.deepEqual(stepsForTier("fast", "dev", steps).map((s) => s.id), ["fmt", "tool-tests"]);
	// Without a readable registry in the checkout, dev's commands still run where they exist.
	assert.deepEqual(mergeRegistry(base, undefined, (path) => files.has(path)).added, []);
	// Only a well-formed registry is read; anything else counts as none.
	assert.equal(isRegistry(head), true);
	for (const bad of [null, {}, [null], [{ id: 1 }], [{ id: "x", run: {} }], [{ id: "x", run: [{ cmd: "node", args: "a.mjs" }] }], [{ id: "x", run: [{ cmd: "node", args: [1] }] }]]) {
		assert.equal(isRegistry(bad), false, JSON.stringify(bad));
	}
	// Every script the merge drops must be listed under "## Retired tests", even when it is not
	// a deleted `scripts/verify-*` file.
	const { dir, git } = scratchRepo();
	try {
		writeFileSync(join(dir, "README.md"), "base\n");
		git("add", "-A");
		git("commit", "-qm", "base");
		const commit = git("rev-parse", "HEAD");
		const ctx = { cwd: dir, base: commit, head: commit, registry: { retired: ["scripts/verify-packaging.py"] } };
		assert.match(retiredTests({ ...ctx, pr: parsePrBody("Refs S27-1") }).reason, /verify-packaging\.py/);
		const listed = parsePrBody("Refs S27-1\n\n## Retired tests\n- `scripts/verify-packaging.py` — packaging moved to the build step\n");
		assert.equal(retiredTests({ ...ctx, pr: listed }).status, "pass");
	} finally {
		rmSync(dir, { recursive: true, force: true });
	}
});

test("captures report each observed element in view once, as the native observer would, with entries that pass type checks", async () => {
	class Entry {}
	class Native {
		observe() {}
	}
	const saved = globalThis.window;
	globalThis.window = { IntersectionObserver: Native, IntersectionObserverEntry: Entry };
	try {
		seeEverything();
		const seen = [];
		const target = { getBoundingClientRect: () => ({ x: 0, y: 4000, width: 300, height: 200 }) };
		const observer = new window.IntersectionObserver((entries, owner) => seen.push({ entries, owner }), { rootMargin: "0px" });
		observer.observe(target);
		assert.equal(seen.length, 0, "entries arrive asynchronously, as the browser's do");
		await new Promise((resolve) => setTimeout(resolve, 0));
		assert.equal(seen.length, 1);
		const [entry] = seen[0].entries;
		assert.equal(seen[0].owner, observer);
		assert.ok(entry instanceof Entry);
		assert.equal(entry.target, target);
		assert.equal(entry.isIntersecting, true);
		assert.equal(entry.intersectionRatio, 1);
		assert.ok(observer instanceof Native);

		// Like the native observer: once per target, nothing after unobserve or disconnect.
		const calls = [];
		const other = new window.IntersectionObserver((entries) => calls.push(...entries.map((e) => e.target)));
		const [a, b, c] = [1, 2, 3].map((id) => ({ id, getBoundingClientRect: () => ({ x: 0, y: 0, width: 1, height: 1 }) }));
		other.observe(a);
		other.observe(a);
		other.observe(b);
		other.unobserve(b);
		await new Promise((resolve) => setTimeout(resolve, 0));
		assert.deepEqual(calls, [a]);
		other.observe(c);
		other.disconnect();
		await new Promise((resolve) => setTimeout(resolve, 0));
		assert.deepEqual(calls, [a]);
	} finally {
		globalThis.window = saved;
	}
});

test("the merge waits until GitHub reports the gated commit as mergeable, and stops on a conflict or a timeout", async () => {
	const gated = "a".repeat(40);
	const sleep = async () => {};
	const replies = (list) => {
		const seen = [];
		return {
			seen,
			view: () => {
				const next = list[Math.min(seen.length, list.length - 1)];
				seen.push(next);
				return next;
			},
		};
	};
	// The push has not reached GitHub yet, then mergeability is still being computed.
	const ok = replies([
		{ headRefOid: "b".repeat(40), mergeable: "MERGEABLE" },
		{ headRefOid: gated, mergeable: "UNKNOWN" },
		{ headRefOid: gated, mergeable: "MERGEABLE" },
	]);
	await waitForMergeable(ok.view, gated, { sleep });
	assert.equal(ok.seen.length, 3, "an old head that is mergeable does not count");

	const conflict = replies([{ headRefOid: gated, mergeable: "CONFLICTING" }]);
	await assert.rejects(waitForMergeable(conflict.view, gated, { sleep }), /conflicting/);

	const stuck = replies([{ headRefOid: gated, mergeable: "UNKNOWN" }]);
	await assert.rejects(waitForMergeable(stuck.view, gated, { sleep, tries: 4 }), /did not report .* mergeable/);
	assert.equal(stuck.seen.length, 4);
});

test("a failed mergeability poll is retried, and the timeout names the last error", async () => {
	const gated = "c".repeat(40);
	const sleep = async () => {};
	let calls = 0;
	const flaky = () => {
		calls += 1;
		if (calls === 1) throw new Error("gh: HTTP 502\nmore detail");
		return { headRefOid: gated, mergeable: "MERGEABLE" };
	};
	await waitForMergeable(flaky, gated, { sleep });
	assert.equal(calls, 2, "one failure, then the mergeable answer");

	const down = () => {
		throw new Error("gh: rate limit exceeded\nretry later");
	};
	await assert.rejects(waitForMergeable(down, gated, { sleep, tries: 3 }), /last error: gh: rate limit exceeded\)/);
});

test("the merge lock covers the whole merge: devGate awaits mergeIntoDev before releasing", () => {
	// Returning the promise unawaited would run \`finally { release(); }\` while the merge still
	// waits for GitHub, so a second gate could merge in between.
	const source = readFileSync(join(repo, "tools/rv-gate/rv-gate.mjs"), "utf8");
	const devGate = source.slice(source.indexOf("async function devGate("), source.indexOf("async function mergeIntoDev("));
	assert.match(devGate, /return await mergeIntoDev\(/);
	assert.doesNotMatch(devGate, /return mergeIntoDev\(/);
	// Housekeeping runs first, inside its own try, so the lock is released even if it throws.
	assert.match(devGate, /finally \{\s*(?:\/\/[^\n]*\s*)*try \{\s*finishRun\(run\);\s*\} finally \{\s*release\(\);/);
});

test("this repository: every test entry point is in a tier and the boundaries hold", () => {
	const entries = entrypoints({ cwd: repo });
	assert.equal(entries.status, "pass", entries.reason);
	const boundaries = arch({ cwd: repo });
	assert.equal(boundaries.status, "pass", boundaries.reason);
});

test("ARC-03 sees the landing bootstrap and still rejects other product scripts", () => {
	const bootstrap = "assets/landing/bootstrap.js";
	const tracked = execFileSync("git", ["ls-files", "--error-unmatch", bootstrap], { cwd: repo, encoding: "utf8" }).trim();
	assert.equal(tracked, bootstrap);
	assert.deepEqual(productJsProblems(repo, [bootstrap]), []);
	assert.match(productJsProblems(repo, ["assets/landing/unapproved.js"])[0], /outside ARC-03's allowlist/);
	const dir = mkdtempSync(join(tmpdir(), "rv-arch-header-"));
	try {
		mkdirSync(join(dir, "assets/landing"), { recursive: true });
		writeFileSync(join(dir, bootstrap), "window.example = true;\n");
		assert.match(productJsProblems(dir, [bootstrap])[0], /lacks its host-reason header/);
	} finally { rmSync(dir, { recursive: true, force: true }); }
});

test("the pre-push hook lets agents push dev and refuses pre-prod, prod, main and candidates unless the gate marks them", () => {
	const dir = mkdtempSync(join(tmpdir(), "rv-gate-hook-"));
	try {
		const hook = join(dir, "pre-push");
		writeFileSync(hook, PRE_PUSH, { mode: 0o755 });
		const push = (ref, env = {}, local = "1") => {
			try {
				execFileSync("sh", [hook], { input: `refs/heads/x ${local} ${ref} 0\n`, env: { PATH: process.env.PATH, ...env }, stdio: ["pipe", "ignore", "ignore"] });
				return "allowed";
			} catch {
				return "refused";
			}
		};
		assert.equal(push("refs/heads/task/s27-1-x"), "allowed");
		// The commits a push to dev needs records for are tested in the fast-pass tests; a deletion carries none.
		assert.equal(push("refs/heads/dev", {}, "0000000000000000000000000000000000000000"), "allowed", "DEL-81: agents merge into dev freely");
		for (const ref of ["refs/heads/prod", "refs/heads/pre-prod", "refs/heads/main", "refs/heads/candidate/3"]) assert.equal(push(ref), "refused", ref);
		assert.equal(push("refs/heads/candidate/3", { RV_GATE_PUSH: "candidate" }), "allowed");
		assert.equal(push("refs/heads/prod", { RV_GATE_PUSH: "prod" }), "allowed");
		assert.equal(push("refs/heads/pre-prod", { RV_GATE_PUSH: "pre-prod" }), "allowed");
		assert.equal(push("refs/heads/pre-prod", { RV_GATE_PUSH: "prod" }), "refused");
	} finally {
		rmSync(dir, { recursive: true, force: true });
	}
});

/** A scratch repository, the pre-push hook on disk, and a gate home for the fast-pass tests. */
async function fastPassFixture(run) {
	const dir = mkdtempSync(join(tmpdir(), "rv-gate-fastpass-"));
	const previous = process.env.RV_GATE_HOME;
	process.env.RV_GATE_HOME = join(dir, "home");
	try {
		const fastpass = await import(`../lib/fastpass.mjs?${Date.now()}${Math.random()}`);
		const repo = join(dir, "repo");
		const git = (...args) => execFileSync("git", args, { cwd: repo, encoding: "utf8", env: { ...process.env, GIT_AUTHOR_NAME: "t", GIT_AUTHOR_EMAIL: "t@t", GIT_COMMITTER_NAME: "t", GIT_COMMITTER_EMAIL: "t@t" } }).trim();
		execFileSync("git", ["init", "-q", repo]);
		const write = (name, text) => writeFileSync(join(repo, name), text);
		const commit = (name, text, message = name) => {
			write(name, text);
			git("add", name);
			git("commit", "-qm", message);
			return git("rev-parse", "HEAD");
		};
		const hook = join(dir, "pre-push");
		writeFileSync(hook, PRE_PUSH, { mode: 0o755 });
		const push = (local, remote) => {
			try {
				execFileSync("sh", [hook], { cwd: repo, input: `refs/heads/x ${local} refs/heads/dev ${remote}\n`, env: { PATH: process.env.PATH, HOME: dir, RV_GATE_HOME: join(dir, "home") }, stdio: ["pipe", "ignore", "ignore"] });
				return "allowed";
			} catch {
				return "refused";
			}
		};
		const base = commit("a.txt", "a\n", "base");
		// recordFastPass takes the stack from its merge base with origin/dev.
		git("update-ref", "refs/remotes/origin/dev", base);
		await run({ ...fastpass, repo, git, write, commit, push, base });
	} finally {
		if (previous === undefined) delete process.env.RV_GATE_HOME;
		else process.env.RV_GATE_HOME = previous;
		rmSync(dir, { recursive: true, force: true });
	}
}

test("a push to dev needs a recorded fast pass, a dirty tree records nothing, and the record survives a clean rebase (review 1k, 1l)", async () => {
	await fastPassFixture(({ recordFastPass, patchId, repo, git, write, commit, push, base }) => {
		const work = commit("b.txt", "b\n", "work");
		assert.equal(push(work, base), "refused", "no fast pass recorded");
		write("c.txt", "dirty\n");
		git("add", "c.txt");
		assert.ok(recordFastPass(repo).refused, "a pass over uncommitted edits records nothing");
		git("reset", "-q", "c.txt");
		assert.ok(recordFastPass(repo).refused, "an untracked file that is not ignored counts as dirty (review 1l: a commit that needs it)");
		assert.equal(push(work, base), "refused", "nothing was recorded for the dirty runs");
		rmSync(join(repo, "c.txt"));
		assert.deepEqual(recordFastPass(repo), { head: work, commits: 1 }, "the run records the commit at HEAD");
		assert.equal(push(work, base), "allowed");
		// The same change rebased onto a new base keeps its patch id, so the record still holds.
		git("checkout", "-q", base);
		const other = commit("d.txt", "d\n", "other");
		git("cherry-pick", work);
		const rebased = git("rev-parse", "HEAD");
		assert.notEqual(rebased, work);
		assert.equal(patchId(repo, rebased), patchId(repo, work));
		assert.equal(push(rebased, other), "allowed", "a tested commit keeps its record across a clean rebase");
		// A changed commit has a new patch id and needs its own pass.
		write("b.txt", "changed\n");
		git("commit", "-qam", "amend");
		assert.equal(push(git("rev-parse", "HEAD"), other), "refused");
	});
});

test("an amend that changes only whitespace inside a string loses its record (review 1l)", async () => {
	await fastPassFixture(({ recordFastPass, patchId, repo, git, write, commit, push, base }) => {
		const spaced = commit("lib.rs", 'pub fn a() -> &\'static str { "a b c" }\n');
		assert.equal(recordFastPass(repo).head, spaced);
		assert.equal(push(spaced, base), "allowed");
		write("lib.rs", 'pub fn a() -> &\'static str { "abc" }\n');
		git("commit", "-qa", "--amend", "-m", "lib.rs");
		const amended = git("rev-parse", "HEAD");
		assert.notEqual(patchId(repo, amended), patchId(repo, spaced), "--verbatim keeps the whitespace in the id");
		assert.equal(push(amended, base), "refused");
	});
});

test("a merge commit needs a record of its own, even when every commit it brings is recorded (review 1l)", async () => {
	await fastPassFixture(({ recordFastPass, repo, git, write, commit, push, base }) => {
		const main = git("rev-parse", "--abbrev-ref", "HEAD");
		git("checkout", "-qb", "topic");
		const feature = commit("f.txt", "f\n", "feature");
		assert.equal(recordFastPass(repo).head, feature);
		git("checkout", "-q", main);
		const other = commit("o.txt", "o\n", "other");
		assert.equal(recordFastPass(repo).head, other);
		git("merge", "-q", "--no-ff", "--no-commit", "topic");
		write("extra.txt", "lines only the merge carries\n");
		git("add", "extra.txt");
		git("commit", "-qm", "merge topic");
		const merged = git("rev-parse", "HEAD");
		assert.equal(push(merged, other), "refused", "the merge has no record, whatever its parents have");
		assert.equal(recordFastPass(repo).head, merged, "a fast run at the merge records it, against its first parent");
		assert.equal(push(merged, other), "allowed");
		assert.equal(push(merged, base), "allowed", "the recorded commits and the merge are all covered");
	});
});

test("the hook fails closed when git cannot list or hash the commits (review 1l)", async () => {
	await fastPassFixture(({ recordFastPass, repo, commit, push, base }) => {
		const work = commit("b.txt", "b\n", "work");
		recordFastPass(repo);
		assert.equal(push(work, base), "allowed", "control: the same commit with a remote sha this repository has");
		assert.equal(push(work, "1234567890123456789012345678901234567890"), "refused", "a remote sha missing locally refuses, it does not allow");
	});
});

test("one run at a stack's tip covers the stack; a prefix, a dropped commit and an amended one are refused (review 1l)", async () => {
	await fastPassFixture(({ recordFastPass, repo, git, write, commit, push, base }) => {
		const one = commit("one.txt", "1\n");
		const two = commit("two.txt", "2\n");
		const three = commit("three.txt", "3\n");
		assert.deepEqual(recordFastPass(repo), { head: three, commits: 3 }, "a run at the tip records every commit since origin/dev");
		assert.equal(push(three, base), "allowed", "one run covers the whole stack");
		assert.equal(push(one, base), "refused", "the first commit of a passing stack, pushed alone, is a tip no run tested");
		assert.equal(push(two, base), "refused", "so is any other prefix");
		// The stack rebased onto a moved dev, with no line changed, keeps its record.
		git("checkout", "-q", base);
		const other = commit("other.txt", "o\n", "other");
		git("cherry-pick", one, two, three);
		assert.equal(push(git("rev-parse", "HEAD"), other), "allowed", "a clean rebase keeps the stack's whole diff");
		// Dropping a commit leaves every other commit recorded, but the stack's whole diff changes.
		git("checkout", "-q", base);
		git("cherry-pick", one, three);
		assert.equal(push(git("rev-parse", "HEAD"), base), "refused", "a stack without its middle commit was never tested");
		// Amending a commit below the tip after the run loses that commit's record.
		git("checkout", "-q", two);
		write("two.txt", "2, amended\n");
		git("commit", "-qa", "--amend", "-m", "two.txt");
		git("cherry-pick", three);
		assert.equal(push(git("rev-parse", "HEAD"), base), "refused");
	});
});

test("rv-gate release lists the batch's commits with no record and those tested only below a tip, without failing (review 1l)", async () => {
	await fastPassFixture(({ recordFastPass, unrecordedCommits, repo, commit }) => {
		const alone = commit("b.txt", "b\n");
		recordFastPass(repo);
		const below = commit("c.txt", "c\n");
		const tip = commit("d.txt", "d\n");
		recordFastPass(repo);
		const forge = commit("e.txt", "e\n", "merged on the forge");
		const { missing, belowTip } = unrecordedCommits(repo, [alone, below, tip, forge]);
		assert.deepEqual(missing, [{ commit: forge, reason: "no record" }]);
		assert.deepEqual(belowTip, [{ commit: below }], "a commit once at HEAD of a run keeps that record when a later run covers it below its tip");
		assert.equal(unrecordedCommits(repo, ["1234567890123456789012345678901234567890"]).missing[0].reason, "patch id failed");
	});
});

test("receipts are write-once, and the change log feeds the report's design and feature sections", async () => {
	const home = mkdtempSync(join(tmpdir(), "rv-gate-store-"));
	const previous = process.env.RV_GATE_HOME;
	process.env.RV_GATE_HOME = home;
	try {
		const store = await import(`../lib/store.mjs?${Date.now()}`);
		const receipt = { id: store.newReceiptId("dev", "abcdef1234567890"), commit: "abcdef1234567890", tree: "t" };
		store.writeReceipt(receipt);
		assert.throws(() => store.writeReceipt(receipt), /EEXIST/);
		store.writeReceiptNote(receipt.id, "merge", { commit: "m" });
		assert.equal(store.receiptNotes(receipt.id)[0].kind, "merge");
		store.appendChange({ commit: "c1", class: "design", receipt: receipt.id });
		store.appendChange({ commit: "c2", class: "feature", receipt: receipt.id });
		assert.deepEqual(store.readChanges().map((c) => c.class), ["design", "feature"]);
		const release = store.acquireLock("merge");
		assert.throws(() => store.acquireLock("merge"), /held/);
		release();
		store.acquireLock("merge")();
	} finally {
		if (previous === undefined) delete process.env.RV_GATE_HOME;
		else process.env.RV_GATE_HOME = previous;
		rmSync(home, { recursive: true, force: true });
	}
});

test("evidence keeps the latest run per open PR and the latest full and prod runs", () => {
	const dev = (ts, pr) => [`dev-20261027T${ts}Z-aaaa`, { id: `dev-20261027T${ts}Z-aaaa`, kind: "dev", pr }];
	const receipts = new Map([
		dev("100000", 7), // superseded by the later run of PR 7
		dev("110000", 7),
		dev("120000", 8), // PR 8 merged
		["full-20261027T090000Z-bbbb", { kind: "full" }],
		["full-20261027T130000Z-bbbb", { kind: "full" }],
		["prod-20261027T080000Z-cccc", { kind: "prod" }],
	]);
	const runs = [...receipts.keys(), "dev-20261027T050000Z-dddd", "dev-20261027T140000Z-eeee"];
	const doomed = runsToPrune({ runs, receipts, openPrs: new Set([7]), current: "dev-20261027T140000Z-eeee" });
	assert.deepEqual(doomed.sort(), [
		"dev-20261027T050000Z-dddd", // no receipt: an interrupted run
		"dev-20261027T100000Z-aaaa",
		"dev-20261027T120000Z-aaaa",
		"full-20261027T090000Z-bbbb",
	]);
	// The run that just ended goes as soon as its receipt shows its PR is no longer open.
	receipts.set("dev-20261027T140000Z-eeee", { kind: "dev", pr: 9 });
	assert.ok(runsToPrune({ runs, receipts, openPrs: new Set([7]), current: "dev-20261027T140000Z-eeee" }).includes("dev-20261027T140000Z-eeee"));
	// The newest passing release run stays beside the newest run, and older attempts go.
	const release = new Map([
		["release-20261028T010000Z-aaaa", { kind: "release", result: "pass" }],
		["release-20261028T020000Z-bbbb", { kind: "release", result: "pass" }],
		["release-20261028T030000Z-cccc", { kind: "release", result: "fail" }],
		["release-20261028T040000Z-dddd", { kind: "release", result: "fail" }],
	]);
	assert.deepEqual(runsToPrune({ runs: [...release.keys()], receipts: release, openPrs: new Set(), current: "release-20261028T040000Z-dddd" }).sort(), ["release-20261028T010000Z-aaaa", "release-20261028T030000Z-cccc"]);
});

test("failed full and prod runs keep their evidence: the latest ten per kind and every one from the last 48 hours", () => {
	const now = Date.parse("2026-11-01T20:00:00Z");
	const failed = (kind, ts, hours) => [`${kind}-${ts}-aaaa`, { kind, result: "fail", at: new Date(now - hours * 3600_000).toISOString() }];
	const receipts = new Map([
		...Array.from({ length: 12 }, (_, i) => failed("full", `20261020T${String(i).padStart(2, "0")}0000Z`, 100 + 12 - i)),
		failed("full", "20261031T120000Z", 30), // old by the count, young by the clock
		failed("prod", "20261029T120000Z", 80),
		["full-20261101T100000Z-aaaa", { kind: "full", result: "pass", at: new Date(now - 10 * 3600_000).toISOString() }],
	]);
	const runs = [...receipts.keys()];
	const kept = failedRunsKept({ runs, receipts, now });
	// Ten of the thirteen failed fulls by count (the ten newest, which include the one from 30 hours ago); the two oldest go.
	assert.equal([...kept].filter((id) => id.startsWith("full-")).length, 10);
	assert.ok(kept.has("full-20261031T120000Z-aaaa") && kept.has("prod-20261029T120000Z-aaaa"));
	assert.ok(!kept.has("full-20261020T000000Z-aaaa") && !kept.has("full-20261020T020000Z-aaaa"));
	assert.ok(!kept.has("full-20261101T100000Z-aaaa"), "a passing run is not a failed one");
	const doomed = runsToPrune({ runs, receipts, openPrs: new Set(), current: "none", now });
	assert.deepEqual(doomed.sort(), ["full-20261020T000000Z-aaaa", "full-20261020T010000Z-aaaa", "full-20261020T020000Z-aaaa"]);
	// A failure from the last 48 hours stays even beyond ten.
	const burst = new Map(Array.from({ length: 14 }, (_, i) => failed("full", `20261101T${String(i).padStart(2, "0")}0000Z`, 5)));
	assert.equal(failedRunsKept({ runs: [...burst.keys()], receipts: burst, now }).size, 14);
});

test("trimming a failed run drops its artifact folders and keeps its files, logs, attempts and failed steps", () => {
	const dir = mkdtempSync(join(tmpdir(), "rv-gate-trim-"));
	try {
		for (const folder of ["logs", "attempts", "golden", "captures", "firefox"]) { mkdirSync(join(dir, folder)); writeFileSync(join(dir, folder, "x"), "x"); }
		writeFileSync(join(dir, "owner.json"), "{}");
		const receipt = { steps: [{ id: "firefox", status: "fail" }, { id: "golden", status: "pass" }] };
		assert.equal(trimFailedRun(dir, receipt), 2);
		assert.deepEqual(readdirSync(dir).sort(), ["attempts", "firefox", "logs", "owner.json"]);
		assert.equal(trimFailedRun(dir, receipt), 0);
	} finally { rmSync(dir, { recursive: true, force: true }); }
});

test("a run removes what it added under tmp/, keeps caches, and an owned checkout is emptied", () => {
	const cwd = mkdtempSync(join(tmpdir(), "rv-gate-tmp-"));
	try {
		const make = (path) => mkdirSync(join(cwd, "tmp", path), { recursive: true });
		["fonts-cache", "verification/browser-old", "authored"].forEach(make);
		const before = tmpSnapshot(cwd);
		["verification/browser-new", "cloudflare-review-x1", "performance/fixtures", "fonts-cache/more"].forEach(make);
		assert.equal(cleanCheckoutTmp(cwd, before), 3);
		assert.deepEqual([...tmpSnapshot(cwd)].sort(), ["authored", "fonts-cache", "verification", "verification/browser-old"]);
		cleanCheckoutTmp(cwd);
		assert.deepEqual([...tmpSnapshot(cwd)], ["fonts-cache"]);
	} finally {
		rmSync(cwd, { recursive: true, force: true });
	}
});

test("run temp folders are unique, leave room for a browser's socket path and are swept after a day", () => {
	// Chromium puts its singleton socket two levels below TMPDIR; macOS caps the path at 103 bytes.
	assert.ok(`${TEMP_ROOT}/rvg-XXXXXX/.org.chromium.Chromium.XXXXXX/SingletonSocket`.length < 100, TEMP_ROOT);
	const root = mkdtempSync(join(tmpdir(), "rv-gate-root-"));
	try {
		const [stale, fresh] = [runTemp(root), runTemp(root)];
		assert.notEqual(stale, fresh);
		const now = Date.now();
		const twoDaysAgo = new Date(now - 48 * 3600_000);
		utimesSync(stale, twoDaysAgo, twoDaysAgo);
		mkdirSync(join(root, "unrelated"));
		utimesSync(join(root, "unrelated"), twoDaysAgo, twoDaysAgo);
		sweepRunTemps(root, now);
		assert.deepEqual([existsSync(stale), existsSync(fresh), existsSync(join(root, "unrelated"))], [false, true, true]);
	} finally {
		rmSync(root, { recursive: true, force: true });
	}
});

test("a workspace build in the shared target is seen by profile, and registry dependencies are not", () => {
	const target = mkdtempSync(join(tmpdir(), "rv-gate-target-"));
	try {
		const unit = (profile, name, at) => {
			const dir = join(target, profile, ".fingerprint", name);
			mkdirSync(dir, { recursive: true });
			writeFileSync(join(dir, "invoked.timestamp"), "");
			utimesSync(join(dir, "invoked.timestamp"), new Date(at), new Date(at));
		};
		assert.equal(workspaceBuiltAt(target), 0);
		unit("release", "richview-core-1a2b", 1_000_000);
		unit("debug", "richview-app-3c4d", 3_000_000);
		unit("release", "duckdb-5e6f", 9_000_000);
		unit("wasm32-unknown-unknown/release", "richview-site-7a8b", 2_000_000);
		assert.equal(workspaceBuiltAt(target), 3_000_000);
		assert.equal(workspaceBuiltAt(target, { release: true }), 2_000_000);
	} finally {
		rmSync(target, { recursive: true, force: true });
	}
});

test("a gate run holds the shared target per Cargo command; others wait only for a live holder", async () => {
	const target = mkdtempSync(join(tmpdir(), "rv-gate-hold-"));
	try {
		assert.equal(cargoHolder(target), 0);
		const first = await holdCargo(target);
		const second = await holdCargo(target);
		assert.equal(cargoHolder(target), process.pid);
		first();
		first();
		assert.equal(cargoHolder(target), process.pid, "a second concurrent command keeps the hold");
		second();
		assert.equal(cargoHolder(target), 0);
		assert.equal(existsSync(join(target, HOLD)), false);
		// The holder's own commands never wait for it.
		(await holdCargo(target))();
		await waitForCargo(target, { every: 1 });
		// A hold left by a process that has exited is ignored.
		const gone = execFileSync(process.execPath, ["-e", "process.stdout.write(String(process.pid))"], { encoding: "utf8" });
		writeFileSync(join(target, HOLD), `${gone}\n`);
		assert.equal(cargoHolder(target), 0);
		await waitForCargo(target, { every: 1 });
		// A live holder makes others wait until it lets go.
		const child = spawn(process.execPath, ["-e", "setTimeout(() => {}, 60000)"]);
		writeFileSync(join(target, HOLD), `${child.pid}\n`);
		let waited = 0;
		const waiting = waitForCargo(target, { every: 5, waiting: (pid) => (waited = pid) });
		await new Promise((done) => setTimeout(done, 30));
		assert.equal(waited, child.pid);
		child.kill();
		await waiting;
	} finally {
		rmSync(target, { recursive: true, force: true });
	}
});

test("a holding gate run waits for Cargo processes already running, and gives up after its limit", async () => {
	assert.ok(Array.isArray(runningCargo()));
	const lists = [[11, 12], [12], []];
	const seen = [];
	assert.equal(await waitUntilIdle(() => lists.shift(), { every: 1, waiting: (pids) => seen.push(pids) }), true);
	assert.deepEqual(seen, [[11, 12]], "it reports what it waits for once");
	assert.equal(await waitUntilIdle(() => [7], { every: 1, limit: 5 }), false);
});

test("an empty PR field stays empty instead of taking the next line", () => {
	const fields = parsePrBody("Refs S27-1\ndecision: none\ncause: issue S27-1\nitems:\nassumed:\n\n## What changed and why\n\nText.\n");
	assert.deepEqual(fields.items, []);
	assert.deepEqual(fields.assumed, []);
	assert.equal(fields.decision, "none");
});


test("a gate checkout reinstalls npm packages when the lock gains or changes a package", async () => {
	const { lockDiffers } = await import("../lib/npm.mjs");
	const want = { "": { name: "richview" }, "node_modules/playwright": { version: "1.63.0" }, "node_modules/axe-core": { version: "4.14.0", dev: true }, "node_modules/@img/sharp-win32-x64": { version: "0.34.0", optional: true } };
	const installed = { "node_modules/playwright": { version: "1.63.0" }, "node_modules/axe-core": { version: "4.14.0" } };
	assert.equal(lockDiffers(want, installed), false, "an uninstalled optional platform package is not a difference");
	assert.equal(lockDiffers(want, { "node_modules/playwright": { version: "1.63.0" } }), true, "a new dev dependency is");
	assert.equal(lockDiffers(want, { ...installed, "node_modules/playwright": { version: "1.62.0" } }), true, "a changed version is");
});

test("a gate run never blesses expected outputs, and a receipt id cannot leave the store", async () => {
	const { gateEnv } = await import("../lib/proc.mjs");
	const before = process.env.RICHVIEW_BLESS;
	const previewBefore = process.env.RICHVIEW_DEV_PREVIEW;
	process.env.RICHVIEW_BLESS = "1";
	process.env.RICHVIEW_DEV_PREVIEW = "1";
	try {
		assert.equal("RICHVIEW_BLESS" in gateEnv(), false);
		assert.equal("RICHVIEW_DEV_PREVIEW" in gateEnv(), false);
		process.env.RICHVIEW_UPDATE_GOLDEN = "1";
		assert.equal("RICHVIEW_UPDATE_GOLDEN" in gateEnv(), false);
	} finally {
		delete process.env.RICHVIEW_UPDATE_GOLDEN;
		if (before === undefined) delete process.env.RICHVIEW_BLESS;
		else process.env.RICHVIEW_BLESS = before;
		if (previewBefore === undefined) delete process.env.RICHVIEW_DEV_PREVIEW;
		else process.env.RICHVIEW_DEV_PREVIEW = previewBefore;
	}
	const { receiptPath } = await import("../lib/store.mjs");
	assert.match(receiptPath("release-20261027T230313Z-1fdef6e3d9"), /receipts\/release-20261027T230313Z-1fdef6e3d9\.json$/);
	for (const id of ["../../x", "a/b", "..", ""]) assert.throws(() => receiptPath(id), /Not a receipt id/);
});

test("deleted TypeScript tests are found like JavaScript ones", async () => {
	const dir = mkdtempSync(join(tmpdir(), "rv-ts-"));
	const git = (...args) => execFileSync("git", args, { cwd: dir, encoding: "utf8" }).trim();
	git("init", "-q");
	git("config", "user.email", "t@example.com");
	git("config", "user.name", "t");
	mkdirSync(join(dir, "src"));
	writeFileSync(join(dir, "src/a.test.ts"), 'test("keeps the total", () => {});\ntest("drops nothing", () => {});\n');
	git("add", ".");
	git("commit", "-qm", "a");
	const base = git("rev-parse", "HEAD");
	writeFileSync(join(dir, "src/a.test.ts"), 'test("keeps the total", () => {});\n');
	git("commit", "-qam", "b");
	const { deletedTests } = await import("../lib/checks.mjs");
	assert.deepEqual(deletedTests({ cwd: dir, base, head: "HEAD" }).map((r) => r.test), ["src/a.test.ts::drops nothing"]);
	rmSync(dir, { recursive: true, force: true });
});

test("weakening lists the files that can loosen a check for the batch review", () => {
	assert.deepEqual(
		thresholdChanges(["tools/rv-gate/lib/perf.mjs", "tools/rv-gate/fixtures/cross-style/precision.json", "examples/x/golden-warnings.json", "crates/richview-core/src/render.rs", "tools/rv-gate/test/perf.test.mjs"]),
		["tools/rv-gate/lib/perf.mjs", "tools/rv-gate/fixtures/cross-style/precision.json", "examples/x/golden-warnings.json"],
	);
	// Every gate script and data file counts, whatever its name (review 1e should-fix 29).
	const gate = ["perf.mjs", "lib/release.mjs", "lib/checks.mjs", "lib/proc.mjs", "lib/captures.mjs", "lib/steps.mjs", "rv-gate.mjs", "secrets-allow.json", "bootstrap-merges.json", "journeys-publish.mjs", "newgate.mjs"].map((f) => `tools/rv-gate/${f}`);
	assert.deepEqual(thresholdChanges([...gate, "tools/rv-gate/test/gate.test.mjs", "tools/rv-gate/README.md", "scripts/verify-docs.mjs"]), [...gate, "scripts/verify-docs.mjs"]);
	assert.deepEqual(
		thresholdChanges(["scripts/browser-harness.mjs", "richview-chart/tests/common/mod.rs", "crates/richview-app/tests/common/fixtures/x.rs", "richview-chart/tests/hexbin_contract.rs"]),
		["scripts/browser-harness.mjs", "richview-chart/tests/common/mod.rs", "crates/richview-app/tests/common/fixtures/x.rs"],
		"the loopback headers and shared test helpers can loosen a check (review 1g note 10)",
	);
});

test("a run folder whose run is still going is never pruned (review 1e note 12)", () => {
	const runs = ["release-20261028T010000Z-aaaa", "release-20261028T020000Z-bbbb"];
	const receipts = new Map();
	assert.deepEqual(runsToPrune({ runs, receipts, openPrs: new Set(), current: "x", active: (id) => id.endsWith("bbbb") }), ["release-20261028T010000Z-aaaa"]);
	const dir = mkdtempSync(join(tmpdir(), "rv-gate-active-"));
	try {
		writeFileSync(join(dir, "owner.json"), JSON.stringify({ pid: process.pid }));
		assert.equal(runActive(dir), true, "its process is alive");
		writeFileSync(join(dir, "owner.json"), JSON.stringify({ pid: 2 ** 22 + 12345 }));
		assert.equal(runActive(dir), false, "its process is gone");
		rmSync(join(dir, "owner.json"));
		assert.equal(runActive(dir), true, "a young folder without an owner file may be going");
		assert.equal(runActive(dir, Date.now() + 2 * 24 * 3600_000), false);
	} finally {
		rmSync(dir, { recursive: true, force: true });
	}
});

test("captures with no baseline are unrun with the reason, not a pass (review 1e note 12)", async () => {
	const { noBaselineResult } = await import("../lib/captures.mjs");
	const result = noBaselineResult("pre-prod", { a: "1", b: "2" });
	assert.equal(result.status, "unrun");
	assert.match(result.reason, /No pre-prod baseline/);
	assert.equal(result.details.captures, 2);
});

test("pkgFresh watches the workspace manifest and the toolchain pin", async () => {
	const { pkgFresh } = await import("../journeys-publish.mjs");
	const dir = mkdtempSync(join(tmpdir(), "rv-gate-pkg-"));
	try {
		const wasm = join(dir, "pkg.wasm");
		for (const file of ["Cargo.lock", "Cargo.toml", "rust-toolchain.toml"]) writeFileSync(join(dir, file), "x");
		writeFileSync(wasm, "w");
		const old = new Date(Date.now() - 60_000);
		for (const file of ["Cargo.lock", "Cargo.toml", "rust-toolchain.toml"]) utimesSync(join(dir, file), old, old);
		assert.equal(pkgFresh(dir, wasm), true);
		for (const file of ["Cargo.toml", "rust-toolchain.toml"]) {
			utimesSync(join(dir, file), new Date(Date.now() + 60_000), new Date(Date.now() + 60_000));
			assert.equal(pkgFresh(dir, wasm), false, file);
			utimesSync(join(dir, file), old, old);
		}
	} finally {
		rmSync(dir, { recursive: true, force: true });
	}
});

test("a build that leaves tracked files changed fails and names each file (review 1e should-fix 30)", async () => {
	const { buildTreeResult, trackedChanges } = await import("../lib/checks.mjs");
	const pass = { status: "pass" };
	assert.equal(buildTreeResult(pass, "/x", () => []), pass);
	const failed = buildTreeResult(pass, "/x", () => ["packages/vscode/snippets/rmd.json", "docs/surface/a.md"]);
	assert.equal(failed.status, "fail");
	assert.match(failed.reason, /packages\/vscode\/snippets\/rmd\.json, docs\/surface\/a\.md/);
	const already = { status: "fail", reason: "x" };
	assert.equal(buildTreeResult(already, "/x", () => ["a"]), already);
	// Against a real repository: a modified tracked file counts, an untracked one does not.
	const dir = mkdtempSync(join(tmpdir(), "rv-gate-dirty-"));
	try {
		const git = (...args) => spawnSync("git", args, { cwd: dir, encoding: "utf8" });
		git("init", "-q");
		writeFileSync(join(dir, "a.json"), "1");
		git("add", "a.json");
		git("-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", "a");
		writeFileSync(join(dir, "new.txt"), "x");
		assert.deepEqual(trackedChanges(dir), []);
		writeFileSync(join(dir, "a.json"), "2");
		assert.deepEqual(trackedChanges(dir), ["a.json"]);
	} finally {
		rmSync(dir, { recursive: true, force: true });
	}
});
