// Host reason (ARC-03): exercise production promotion against disposable, local Git remotes.
import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { PRE_PUSH } from "../lib/hooks.mjs";
import { backmergeHotfix } from "../lib/hotfix-backmerge.mjs";
import { hotfixSteps } from "../lib/hotfix.mjs";
import { mergeProd } from "../lib/report.mjs";
import { acquireLock, ensureStore, receiptNotes, receiptPath, writeOnce } from "../lib/store.mjs";

const scratch = fileURLToPath(new URL("../../../tmp/", import.meta.url));
const go = "Daniel: promote candidate/7, 2026-11-09";

function fixture(t, path = "tools/fix.mjs") {
	mkdirSync(scratch, { recursive: true });
	const root = mkdtempSync(join(scratch, "hotfix-promotion-"));
	const cwd = join(root, "checkout"), remote = join(root, "origin.git");
	const previous = process.env.RV_GATE_HOME;
	process.env.RV_GATE_HOME = join(root, "gate");
	t.after(() => {
		if (previous === undefined) delete process.env.RV_GATE_HOME;
		else process.env.RV_GATE_HOME = previous;
		rmSync(root, { recursive: true, force: true });
	});
	const git = (...args) => {
		const result = spawnSync("git", args, { cwd, encoding: "utf8", env: { ...process.env, GIT_CONFIG_NOSYSTEM: "1", GIT_CONFIG_GLOBAL: "/dev/null" } });
		assert.equal(result.status, 0, `${args.join(" ")}\n${result.stderr}`);
		return result.stdout.trim();
	};
	mkdirSync(cwd);
	git("init", "-q", "--bare", remote);
	git("init", "-q", "-b", "dev");
	git("config", "user.name", "Fixture");
	git("config", "user.email", "fixture@example.invalid");
	git("config", "commit.gpgSign", "false");
	git("config", "core.hooksPath", join(root, "no-hooks"));
	git("remote", "add", "origin", remote);
	const edit = (name, value) => { mkdirSync(dirname(join(cwd, name)), { recursive: true }); writeFileSync(join(cwd, name), value); };
	const commit = (message) => { git("add", "."); git("commit", "-qm", message); return git("rev-parse", "HEAD"); };
	edit(path, "broken\n");
	const prod = commit("production");
	edit("dev-only.txt", "unreleased work\n");
	commit("unreleased development");
	edit(path, "fixed\n");
	const sourceCommit = commit("fix on dev");
	git("branch", "source", sourceCommit);
	git("checkout", "-qb", "candidate/7", prod);
	git("cherry-pick", sourceCommit);
	const head = git("rev-parse", "HEAD"), tree = git("rev-parse", "HEAD^{tree}");
	git("push", "-q", "origin", `${prod}:refs/heads/prod`, `${sourceCommit}:refs/heads/dev`, `${head}:refs/heads/candidate/7`);
	const receipt = {
		id: `hotfix-20261109T120000Z-${head.slice(0, 10)}`, kind: "hotfix", result: "pass",
		candidate: "candidate/7", commit: head, tree, prod, sourceCommit,
		steps: hotfixSteps([path]).map(id => ({ id, status: "pass" })),
		build: { snapshot: join(root, "snapshot"), manifestSha256: "a".repeat(64) },
	};
	const save = (value = receipt) => { ensureStore(); writeOnce(receiptPath(value.id), value); };
	const notifications = [], notifyFailure = notice => notifications.push(notice);
	const promote = (approval = go, notify = notifyFailure) => mergeProd({ cwd, candidate: "candidate/7", go: approval, notifyBackmergeFailure: notify });
	const remoteProd = () => git("--git-dir", remote, "rev-parse", "refs/heads/prod");
	const remoteDev = () => git("--git-dir", remote, "rev-parse", "refs/heads/dev");
	return { cwd, remote, git, edit, commit, prod, sourceCommit, head, tree, receipt, save, promote, remoteProd, remoteDev, notifications, notifyFailure };
}

test("a complete hotfix receipt promotes exactly one cherry-picked dev patch and records the go", t => {
	const f = fixture(t);
	const hooks = join(f.cwd, ".git", "hooks");
	writeFileSync(join(hooks, "pre-push"), PRE_PUSH, { mode: 0o755 });
	f.git("config", "core.hooksPath", hooks);
	f.save();
	const immutable = readFileSync(receiptPath(f.receipt.id), "utf8");
	f.edit("local-notes.txt", "keep the staged local work\n");
	f.git("add", "local-notes.txt");
	const checkoutBefore = [f.git("rev-parse", "HEAD"), f.git("branch", "--show-current"), f.git("status", "--porcelain"), readFileSync(join(f.cwd, ".git", "index")).toString("base64")];
	assert.equal(f.promote(), 0);
	assert.equal(f.remoteProd(), f.head);
	assert.equal(readFileSync(receiptPath(f.receipt.id), "utf8"), immutable);
	const mergedDev = f.remoteDev();
	assert.notEqual(mergedDev, f.sourceCommit);
	assert.equal(f.git("rev-list", "--parents", "-n", "1", mergedDev), `${mergedDev} ${f.sourceCommit} ${f.head}`);
	assert.equal(f.git("rev-parse", `${mergedDev}^{tree}`), f.git("rev-parse", `${f.sourceCommit}^{tree}`));
	const message = f.git("log", "-1", "--format=%B", mergedDev);
	assert.match(message, /Refs S27-979/);
	assert.ok(message.includes(f.receipt.id));
	assert.ok(message.includes(f.head));
	assert.deepEqual([f.git("rev-parse", "HEAD"), f.git("branch", "--show-current"), f.git("status", "--porcelain"), readFileSync(join(f.cwd, ".git", "index")).toString("base64")], checkoutBefore);
	assert.deepEqual(receiptNotes(f.receipt.id).map(note => [note.kind, note.commit]), [["go", f.head], ["hotfix-backmerge", mergedDev], ["prod-merge", f.head]]);
	assert.equal(receiptNotes(f.receipt.id).find(note => note.kind === "go").go, go);
	assert.ok(receiptNotes(f.receipt.id).every(note => note.lane === "hotfix"));
	assert.deepEqual(f.notifications, []);
});

test("a hotfix receipt cannot be used after prod advances", t => {
	const f = fixture(t);
	f.save();
	f.git("checkout", "-qb", "advance", f.prod);
	f.edit("production-note.txt", "another release\n");
	const advanced = f.commit("advance production");
	f.git("push", "-q", "origin", `${advanced}:refs/heads/prod`);
	assert.throws(() => f.promote(), /prod.*changed|stale|current prod/i);
	assert.equal(f.remoteProd(), advanced);
	assert.deepEqual(receiptNotes(f.receipt.id), []);
});

test("a hotfix must be exactly one child of the receipt's current prod", t => {
	const f = fixture(t);
	f.edit("extra.txt", "second candidate patch\n");
	const extra = f.commit("another patch");
	f.git("push", "-q", "origin", `${extra}:refs/heads/candidate/7`);
	f.save({ ...f.receipt, commit: extra, tree: f.git("rev-parse", "HEAD^{tree}") });
	assert.throws(() => f.promote(), /exactly one|single.*child|one.*child/i);
	assert.equal(f.remoteProd(), f.prod);
});

test("a hotfix source must be on current dev and contain the candidate patch", async t => {
	for (const mode of ["different patch", "not on dev", "not a commit"]) await t.test(mode, t => {
		const f = fixture(t);
		let sourceCommit;
		if (mode === "different patch") sourceCommit = f.git("rev-parse", `${f.sourceCommit}^`);
		else if (mode === "not on dev") {
			f.git("checkout", "-qb", "outsider", f.prod);
			f.edit("tools/fix.mjs", "fixed\n");
			sourceCommit = f.commit("patch outside dev");
		} else sourceCommit = "f".repeat(40);
		f.save({ ...f.receipt, sourceCommit });
		assert.throws(() => f.promote(), /source.*dev|patch|source.*commit/i);
		assert.equal(f.remoteProd(), f.prod);
		assert.deepEqual(receiptNotes(f.receipt.id), []);
	});
});

test("hotfix receipt validation rejects failed or incomplete evidence", async t => {
	for (const [label, alter, error] of [
		["failed result", r => r.result = "fail", /failed/],
		["wrong tree", r => r.tree = "0".repeat(40), /tree/],
		["missing steps", r => delete r.steps, /step/],
		["missing fast", r => r.steps = r.steps.filter(s => s.id !== "fast"), /fast/],
		["failed step", r => r.steps[0].status = "fail", /step|pass/],
		["unrun step", r => r.steps[0].status = "unrun", /step|pass/],
		["duplicate step", r => r.steps.push({ ...r.steps[0] }), /step|duplicate/],
		["unrun subpart", r => r.steps[0].details = { unrunInputs: [{ what: "fixture", reason: "missing" }] }, /unrun|incomplete|not run/],
		["missing build", r => delete r.build, /build/],
		["incomplete build", r => r.build = {}, /build/],
		["relative build", r => r.build.snapshot = "relative", /build/],
		["invalid build digest", r => r.build.manifestSha256 = "unknown", /build/],
	]) await t.test(label, t => {
		const f = fixture(t), receipt = structuredClone(f.receipt);
		alter(receipt);
		f.save(receipt);
		assert.throws(() => f.promote(), error);
		assert.equal(f.remoteProd(), f.prod);
		assert.deepEqual(receiptNotes(f.receipt.id), []);
	});
});

test("browser coverage comes from the actual candidate paths", async t => {
	for (const path of ["assets/landing/home.css", "crates/richview-core/src/lib.rs", "unclassified.txt"]) await t.test(path, t => {
		const f = fixture(t, path);
		f.save({ ...f.receipt, steps: f.receipt.steps.filter(s => ["fast", "build"].includes(s.id)) });
		assert.throws(() => f.promote(), /report-runtime|verify-site/);
		assert.equal(f.remoteProd(), f.prod);
	});
});

test("promotion requires the exact candidate receipt and a recorded go", async t => {
	for (const mode of ["different candidate", "different commit", "no go"]) await t.test(mode, t => {
		const f = fixture(t), receipt = structuredClone(f.receipt);
		if (mode === "different candidate") receipt.candidate = "candidate/8";
		if (mode === "different commit") receipt.commit = f.sourceCommit;
		f.save(receipt);
		assert.throws(() => f.promote(mode === "no go" ? "" : go), /receipt|go/i);
		assert.equal(f.remoteProd(), f.prod);
	});
});

test("existing full and prod-candidate receipts still promote fast-forward candidates", async t => {
	for (const kind of ["full", "prod"]) await t.test(kind, t => {
		const f = fixture(t);
		f.save({ id: `${kind}-20261109T120000Z-${f.head.slice(0, 10)}`, kind, result: "pass", candidate: "candidate/7", commit: f.head, tree: f.tree });
		assert.equal(f.promote(), 0);
		assert.equal(f.remoteProd(), f.head);
	});
});

test("the leased push cannot turn an existing full receipt into a non-fast-forward promotion", t => {
	const f = fixture(t);
	f.save({ id: `full-20261109T120000Z-${f.head.slice(0, 10)}`, kind: "full", result: "pass", candidate: "candidate/7", commit: f.head, tree: f.tree });
	f.git("push", "-q", "origin", `${f.sourceCommit}:refs/heads/prod`);
	assert.throws(() => f.promote(), /prod is not an ancestor/);
	assert.equal(f.remoteProd(), f.sourceCommit);
});

test("the expected-old-prod lease rejects a race that an ordinary fast-forward push would accept", t => {
	const f = fixture(t);
	const tree = f.git("rev-parse", `${f.sourceCommit}^{tree}`);
	const advanced = f.git("rev-parse", `${f.sourceCommit}^`);
	f.git("--git-dir", f.remote, "update-ref", "refs/heads/candidate/7", f.sourceCommit, f.head);
	f.save({ id: `full-20261109T120000Z-${f.sourceCommit.slice(0, 10)}`, kind: "full", result: "pass", candidate: "candidate/7", commit: f.sourceCommit, tree });
	// upload-pack advertises the old prod to fetch, then advances the real remote before push.
	const afterFetch = join(f.cwd, ".git", "advance-after-fetch.sh");
	writeFileSync(afterFetch, `git-upload-pack "$@" || exit "$?"\ngit --git-dir="$1" update-ref refs/heads/prod ${advanced} ${f.prod}\n`);
	f.git("config", "remote.origin.uploadpack", `sh '${afterFetch.replaceAll("'", "'\\''")}'`);
	assert.throws(() => f.promote(), /stale info/);
	assert.equal(f.remoteProd(), advanced);
});

test("merge-prod waits for the shared promotion lock before reading or moving prod", { timeout: 30_000 }, async t => {
	const f = fixture(t);
	f.save();
	let unlock = acquireLock("merge-prod");
	t.after(() => unlock?.());
	const module = new URL("../lib/report.mjs", import.meta.url).href;
	const child = spawn(process.execPath, ["--input-type=module", "-e", `import { mergeProd } from ${JSON.stringify(module)}; process.send('starting'); mergeProd({...${JSON.stringify({ cwd: f.cwd, candidate: "candidate/7", go })}, notifyBackmergeFailure() {}});`], { stdio: ["ignore", "pipe", "pipe", "ipc"] });
	t.after(() => { if (child.exitCode === null) child.kill(); });
	let output = "";
	child.stdout.on("data", chunk => output += chunk);
	child.stderr.on("data", chunk => output += chunk);
	const ended = new Promise((resolve, reject) => { child.once("exit", resolve); child.once("error", reject); });
	await new Promise(resolve => child.once("message", resolve));
	await new Promise(resolve => setTimeout(resolve, 100));
	assert.equal(child.exitCode, null);
	assert.equal(f.remoteProd(), f.prod);
	assert.deepEqual(receiptNotes(f.receipt.id), []);
	unlock(); unlock = undefined;
	assert.equal(await ended, 0, output);
	assert.equal(f.remoteProd(), f.head);
});

function promotedFixture(t) {
	const f = fixture(t);
	f.save();
	f.git("--git-dir", f.remote, "update-ref", "refs/heads/prod", f.head, f.prod);
	f.backmerge = (notify = f.notifyFailure) => backmergeHotfix({ cwd: f.cwd, receiptId: f.receipt.id, prod: f.head, notifyFailure: notify });
	return f;
}

test("a conflicting back-merge stops after prod moved and records the lead handoff", t => {
	const f = fixture(t);
	f.git("checkout", "-q", "source");
	f.edit("tools/fix.mjs", "different dev fix\n");
	const dev = f.commit("later incompatible development");
	f.git("push", "-q", "origin", `${dev}:refs/heads/dev`);
	f.save();
	assert.throws(() => f.promote(), /prod already moved[\s\S]*conflicts[\s\S]*Mail the lead/);
	assert.equal(f.remoteProd(), f.head);
	assert.equal(f.remoteDev(), dev);
	const note = receiptNotes(f.receipt.id).find(note => note.kind === "hotfix-backmerge");
	assert.equal(note.status, "fail");
	assert.equal(note.pushAttempted, false);
	assert.match(note.reason, /prod already moved[\s\S]*Mail the lead/);
	assert.equal(f.notifications.length, 1);
	assert.equal(f.notifications[0].receiptId, f.receipt.id);
	assert.equal(f.notifications[0].prod, f.head);
	assert.match(f.notifications[0].reason, /conflicts/);
	assert.equal(f.git("status", "--porcelain"), "");
});

test("a clean back-merge that would change dev's tree stops without pushing", t => {
	const f = fixture(t);
	f.git("checkout", "-q", "source");
	f.git("revert", "--no-edit", f.sourceCommit);
	const dev = f.git("rev-parse", "HEAD");
	f.git("push", "-q", "origin", `${dev}:refs/heads/dev`);
	f.save();
	assert.throws(() => f.promote(), /prod already moved[\s\S]*change the pinned dev tree[\s\S]*Mail the lead/);
	assert.equal(f.remoteProd(), f.head);
	assert.equal(f.remoteDev(), dev);
	assert.equal(receiptNotes(f.receipt.id).find(note => note.kind === "hotfix-backmerge").pushAttempted, false);
});

test("an already inherited prod records success without a dev push", t => {
	const f = promotedFixture(t);
	f.git("--git-dir", f.remote, "update-ref", "refs/heads/dev", f.head, f.sourceCommit);
	const note = f.backmerge();
	assert.equal(note.status, "pass");
	assert.equal(note.alreadyMerged, true);
	assert.equal(note.pushAttempted, false);
	assert.equal(f.remoteDev(), f.head);
});

test("back-merge uses the trusted current hook without reconfiguring an installed older hook", t => {
	const f = promotedFixture(t);
	const hooks = join(f.cwd, ".git", "old-hooks");
	mkdirSync(hooks);
	writeFileSync(join(hooks, "pre-push"), "#!/bin/sh\nexit 1\n", { mode: 0o755 });
	f.git("config", "core.hooksPath", hooks);
	assert.equal(f.backmerge().status, "pass");
	assert.equal(f.git("config", "core.hooksPath"), hooks);
	assert.notEqual(f.remoteDev(), f.sourceCommit);
});

test("back-merge refuses prod or dev races before pushing", async t => {
	for (const branch of ["prod", "dev"]) await t.test(branch, t => {
		const f = promotedFixture(t), before = branch === "prod" ? f.head : f.sourceCommit;
		f.git("checkout", "-qb", "racer", before);
		f.edit("race.txt", "a concurrent update\n");
		const advanced = f.commit("concurrent branch update");
		f.git("push", "-q", "origin", `${advanced}:refs/heads/racer`);
		const afterFetch = join(f.cwd, ".git", "race-after-fetch.sh");
		writeFileSync(afterFetch, `git-upload-pack "$@" || exit "$?"\ngit --git-dir="$1" update-ref refs/heads/${branch} ${advanced} ${before}\n`);
		f.git("config", "remote.origin.uploadpack", `sh '${afterFetch.replaceAll("'", "'\\''")}'`);
		assert.throws(() => f.backmerge(), new RegExp(`prod already moved[\\s\\S]*${branch} changed[\\s\\S]*Mail the lead`));
		assert.equal(f.remoteDev(), branch === "dev" ? advanced : f.sourceCommit);
		assert.equal(f.remoteProd(), branch === "prod" ? advanced : f.head);
		assert.equal(receiptNotes(f.receipt.id).find(note => note.kind === "hotfix-backmerge").pushAttempted, false);
	});
});

test("back-merge's exact leases reject prod or dev changes at the push advertisement", async t => {
	for (const branch of ["prod", "dev"]) await t.test(branch, t => {
		const f = promotedFixture(t), before = branch === "prod" ? f.head : f.sourceCommit;
		f.git("checkout", "-qb", "racer", before);
		f.edit("race.txt", "a concurrent update\n");
		const advanced = f.commit("concurrent branch update");
		f.git("push", "-q", "origin", `${advanced}:refs/heads/racer`);
		const beforePush = join(f.cwd, ".git", "race-before-push.sh");
		writeFileSync(beforePush, `git --git-dir="$1" update-ref refs/heads/${branch} ${advanced} ${before}\nexec git-receive-pack "$@"\n`);
		f.git("config", "remote.origin.receivepack", `sh '${beforePush.replaceAll("'", "'\\''")}'`);
		assert.throws(() => f.backmerge(), /prod already moved[\s\S]*stale info[\s\S]*Mail the lead/);
		assert.equal(f.remoteDev(), branch === "dev" ? advanced : f.sourceCommit);
		assert.equal(f.remoteProd(), branch === "prod" ? advanced : f.head);
		assert.equal(receiptNotes(f.receipt.id).find(note => note.kind === "hotfix-backmerge").pushAttempted, true);
	});
});

test("a failed dev push preserves prod and records a failed back-merge separately", t => {
	const f = promotedFixture(t);
	writeFileSync(join(f.remote, "hooks", "update"), "#!/bin/sh\n[ \"$1\" != \"refs/heads/dev\" ]\n", { mode: 0o755 });
	const immutable = readFileSync(receiptPath(f.receipt.id), "utf8");
	assert.throws(() => f.backmerge(), /prod already moved[\s\S]*Mail the lead/);
	assert.equal(f.remoteDev(), f.sourceCommit);
	assert.equal(f.remoteProd(), f.head);
	assert.equal(readFileSync(receiptPath(f.receipt.id), "utf8"), immutable);
	const note = receiptNotes(f.receipt.id).find(note => note.kind === "hotfix-backmerge");
	assert.equal(note.status, "fail");
	assert.equal(note.pushAttempted, true);
	assert.throws(() => writeOnce(receiptPath(f.receipt.id).replace(/\.json$/, ".hotfix-backmerge.json"), {}), /EEXIST/);
});

test("a notification failure remains visible while the immutable failure note survives", t => {
	const f = promotedFixture(t);
	writeFileSync(join(f.remote, "hooks", "update"), "#!/bin/sh\nexit 1\n", { mode: 0o755 });
	let calls = 0;
	assert.throws(() => f.backmerge(() => { calls++; throw new Error("mail offline"); }), /prod already moved[\s\S]*Lead mail failed: mail offline/);
	assert.equal(calls, 1);
	assert.equal(receiptNotes(f.receipt.id).find(note => note.kind === "hotfix-backmerge").status, "fail");
	assert.equal(f.remoteDev(), f.sourceCommit);
});

test("a prod-merge note failure stops before back-merge and notifies the lead exactly once", t => {
	const f = fixture(t);
	f.save();
	const existing = receiptPath(f.receipt.id).replace(/\.json$/, ".prod-merge.json");
	writeOnce(existing, { receipt: f.receipt.id, kind: "prod-merge", commit: f.head });
	const before = readFileSync(existing, "utf8");
	assert.throws(() => f.promote(), /prod already moved[\s\S]*could not record prod-merge[\s\S]*Mail the lead/);
	assert.equal(f.remoteProd(), f.head);
	assert.equal(f.remoteDev(), f.sourceCommit);
	assert.equal(f.notifications.length, 1);
	assert.equal(readFileSync(existing, "utf8"), before);
	assert.equal(receiptNotes(f.receipt.id).find(note => note.kind === "hotfix-backmerge").status, "fail");
});
