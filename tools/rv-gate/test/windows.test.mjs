import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { mergeProd } from "../lib/report.mjs";
import { stepsForTier } from "../lib/steps.mjs";
import { writeReceipt } from "../lib/store.mjs";
import { readWindowsRun, shipsWindows, windowsMergeProblem } from "../lib/windows.mjs";

const git = (cwd, ...args) => execFileSync("git", args, { cwd, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
const passingSteps = () => stepsForTier("full", "full").map(({ id }) => ({ id, status: ["voiceover", "hosted-smoke", "windows"].includes(id) ? "unrun" : "pass", reason: "Standing gap" }));

const commit = "a".repeat(40);
const passing = `== install the candidate with the one-liner\nrichview 3.0.0 (commit ${commit.slice(0, 10)}, runtime 0123456789ab)\r\nPASS  the version names the commit\r\n\n== result\n42 checks passed, 0 failed\r\nE2E PASS\r\n`;

const sums = { richviewSha256: "1".repeat(64), workerSha256: "2".repeat(64), editorSha256: "e".repeat(64), databaseSha256: "d".repeat(64), archiveSha256: "a".repeat(64) };
// The build's own list, as walk.sh copies SUMS into the run.
const listed = `${sums.richviewSha256}  richview.exe\n${sums.workerSha256}  richview-sql-worker.exe\n${sums.editorSha256}  richview-editor.exe\n${sums.databaseSha256}  warehouse.duckdb\n`;

function run({ manifest = { commit, walk: "commit", instance: "i-0123456789abcdef0", image: "Windows_Server-2025-English-Full-Base", started: "2026-11-18T10:00:00Z", terminated: "2026-11-18T10:30:00Z", costUsd: 0.4, ...sums }, log = passing, build = listed } = {}) {
	const dir = mkdtempSync(join(tmpdir(), "rv-windows-run-"));
	if (manifest !== null) writeFileSync(join(dir, "manifest.json"), typeof manifest === "string" ? manifest : JSON.stringify(manifest));
	if (log !== null) writeFileSync(join(dir, "e2e.log"), log);
	if (build !== null) writeFileSync(join(dir, "build.sha256"), build);
	return dir;
}

test("a recorded walk passes only for the commit it installed, with its own walk, on a terminated machine, ending E2E PASS", () => {
	const dir = run();
	try {
		const walk = readWindowsRun(dir, commit);
		assert.equal(walk.ok, true, walk.problems.join("; "));
		assert.equal(walk.summary.instance, "i-0123456789abcdef0");
		assert.deepEqual([walk.summary.archiveSha256, walk.summary.richviewSha256, walk.summary.workerSha256, walk.summary.editorSha256, walk.summary.databaseSha256], [sums.archiveSha256, sums.richviewSha256, sums.workerSha256, sums.editorSha256, sums.databaseSha256], "the receipt note carries every hash");
		assert.match(readWindowsRun(dir, "b".repeat(40)).problems.join("; "), /tested a+, not b+/);
	} finally { rmSync(dir, { recursive: true }); }
	for (const [broken, problem] of [
		[{ log: passing.replace("E2E PASS", "E2E FAIL: the PDF is a PDF") }, /ended "E2E FAIL: the PDF is a PDF"/],
		[{ log: passing.replace(/richview 3.*\r?\n/, "") }, /does not name the commit/],
		[{ log: passing.replace("E2E PASS", "") }, /without a result/],
		[{ log: null }, /no e2e\.log/],
		[{ manifest: null }, /no manifest\.json/],
		[{ manifest: "{" }, /not JSON/],
		[{ manifest: { commit, walk: "commit", instance: "i-0123456789abcdef0", ...sums } }, /termination is not recorded/],
		[{ manifest: { commit, walk: "commit", instance: "i-0123456789abcdef0", terminated: "2026-11-18T10:30:00Z", ...sums, editorSha256: undefined } }, /no sha256 of richview-editor\.exe/],
		[{ manifest: { commit, walk: "commit", instance: "i-0123456789abcdef0", terminated: "2026-11-18T10:30:00Z", ...sums, databaseSha256: null } }, /no sha256 of warehouse\.duckdb/],
		[{ manifest: { commit, walk: "commit", instance: "i-0123456789abcdef0", terminated: "2026-11-18T10:30:00Z", ...sums, archiveSha256: null } }, /no sha256 of the archive/],
		[{ manifest: { commit, walk: "commit", instance: "i-0123456789abcdef0", terminated: "2026-11-18T10:30:00Z", ...sums, editorSha256: "f".repeat(64) } }, /richview-editor\.exe with sha256 f+; the build lists e+/],
		[{ build: listed.replace(`${sums.workerSha256}  richview-sql-worker.exe\n`, "") }, /richview-sql-worker\.exe with sha256 2+; the build lists none/],
		[{ build: null }, /no build\.sha256/],
		[{ manifest: { commit, walk: "changed script, sha256 ff", instance: "i-0123456789abcdef0", terminated: "2026-11-18T10:30:00Z" } }, /ran changed script, sha256 ff, not the commit's own e2e\.ps1/],
		[{ manifest: { commit, terminated: "2026-11-18T10:30:00Z" } }, /names no instance/],
	]) {
		const dir = run(broken);
		try {
			const walk = readWindowsRun(dir, commit);
			assert.equal(walk.ok, false);
			assert.match(walk.problems.join("; "), problem);
		} finally { rmSync(dir, { recursive: true }); }
	}
});

test("merge-prod refuses a candidate that ships Windows without a passing walk on its exact commit", () => {
	const head = commit;
	const note = { kind: "windows-e2e-aws-run-20261118T1200", commit: head, result: "pass" };
	assert.equal(windowsMergeProblem({ windows: false, notes: [], head }), null);
	assert.equal(windowsMergeProblem({ windows: true, notes: [note], head }), null);
	for (const notes of [[], [{ ...note, commit: "b".repeat(40) }], [{ ...note, result: "fail" }], [{ ...note, kind: "review" }]]) {
		assert.match(windowsMergeProblem({ windows: true, notes, head }), /ships Windows .* no passing Windows walk is recorded for aaaaaaaaaa/);
	}
	assert.equal(shipsWindows('{ "windows": true }'), true);
	for (const text of ['{ "windows": false }', "{}"]) assert.equal(shipsWindows(text), false);
	for (const [text, refusal] of [[null, /no assets\/install\/release\.json/], ["not json", /not JSON/], ["[]", /not a JSON object/], ["null", /not a JSON object/], ['{ "windows": "yes" }', /true or false/], ['{ "windows": 1 }', /true or false/]]) {
		assert.throws(() => shipsWindows(text), refusal);
	}
});

test("merge-prod itself refuses a candidate whose release.json is missing, unreadable or not boolean, or that ships Windows without a walk", t => {
	const root = mkdtempSync(join(tmpdir(), "rv-windows-merge-"));
	const previous = process.env.RV_GATE_HOME;
	process.env.RV_GATE_HOME = join(root, "home");
	t.after(() => {
		if (previous === undefined) delete process.env.RV_GATE_HOME;
		else process.env.RV_GATE_HOME = previous;
		rmSync(root, { recursive: true, force: true });
	});
	const remote = join(root, "remote.git"), cwd = join(root, "work");
	git(root, "init", "-q", "--bare", remote);
	git(root, "init", "-q", "-b", "dev", cwd);
	for (const [key, value] of [["user.name", "Fixture"], ["user.email", "fixture@example.test"], ["commit.gpgsign", "false"], ["core.hooksPath", "/dev/null"]]) git(cwd, "config", key, value);
	git(cwd, "remote", "add", "origin", remote);
	writeFileSync(join(cwd, "report.txt"), "production\n");
	git(cwd, "add", "-A");
	git(cwd, "commit", "-qm", "production");
	const prod = git(cwd, "rev-parse", "HEAD");
	git(cwd, "push", "-q", "origin", "HEAD:dev", "HEAD:prod");
	mkdirSync(join(cwd, "assets/install"), { recursive: true });
	const cases = [
		["no release.json", null, /no assets\/install\/release\.json/],
		["not JSON", "{", /not JSON/],
		["windows not boolean", '{"enabled": true, "windows": "yes"}', /`windows` as true or false/],
		["ships Windows without a walk", '{"enabled": true, "windows": true}', /no passing Windows walk is recorded/],
	];
	for (const [n, [name, release, refusal]] of cases.entries()) {
		writeFileSync(join(cwd, "report.txt"), `${name}\n`);
		if (release !== null) writeFileSync(join(cwd, "assets/install/release.json"), release);
		git(cwd, "add", "-A");
		git(cwd, "commit", "-qm", name);
		const head = git(cwd, "rev-parse", "HEAD");
		git(cwd, "push", "-q", "origin", `HEAD:refs/heads/candidate/${n + 1}`);
		writeReceipt({ id: `full-2026111${n}T000000Z-${head.slice(0, 10)}`, kind: "full", commit: head, tree: git(cwd, "rev-parse", `${head}^{tree}`), prod, result: "pass", steps: passingSteps() });
		assert.throws(() => mergeProd({ candidate: `candidate/${n + 1}`, go: "Daniel's go, 18 Nov 2026", cwd }), refusal, name);
		assert.equal(git(remote, "rev-parse", "refs/heads/prod"), prod, `${name}: prod must not move`);
	}
});
