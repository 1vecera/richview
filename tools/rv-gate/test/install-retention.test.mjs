// Host reason (ARC-03): verify immutable installer files across real target snapshots and retained builds.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import test from "node:test";
import { BUILD_MANIFEST } from "../lib/build-manifest.mjs";
import { captureBuildSnapshot } from "../lib/build-snapshot.mjs";
import { runFullSteps } from "../lib/full.mjs";
import { readReceipt, writeReceipt } from "../lib/store.mjs";
import { keepReleaseBuild, leaseReleaseBuild, snapshotRelease } from "../lib/target.mjs";

const commit = "a".repeat(40), stale = "b".repeat(40);
function fixture(t) {
	mkdirSync(resolve("tmp"), { recursive: true });
	const root = mkdtempSync(resolve("tmp/install-retention-"));
	t.after(() => rmSync(root, { recursive: true, force: true }));
	const target = join(root, "shared"), cwd = join(root, "checkout"), dir = join(root, "kept"), outputs = {};
	const put = (path, bytes) => { mkdirSync(dirname(path), { recursive: true }); writeFileSync(path, bytes); };
	put(join(cwd, "dist/web/index.html"), "web");
	put(join(target, "release/richview"), "native");
	const installFiles = {
		"install.sh": "installer",
		[`${commit}/richview-aarch64-apple-darwin.tar.gz`]: "archive",
		[`${commit}/richview-aarch64-apple-darwin.tar.gz.sha256`]: "checksum",
	};
	for (const [path, bytes] of Object.entries(installFiles)) {
		put(join(target, "release/install", path), bytes);
		outputs[`target:release/install/${path}`] = { bytes: Buffer.byteLength(bytes), sha256: createHash("sha256").update(bytes).digest("hex") };
	}
	put(join(target, "release/install", stale, "richview-aarch64-apple-darwin.tar.gz"), "stale archive");
	const manifest = { inputs: { commit, dirty: false }, outputs };
	const save = () => put(join(target, BUILD_MANIFEST), JSON.stringify(manifest));
	save(); symlinkSync(target, join(cwd, "target"));
	return { root, target, cwd, dir, commit, installFiles, manifest, save };
}

test("the private snapshot and leased retained bundle keep only this manifest's installer bytes", t => {
	const f = fixture(t), files = [], snapshot = join(f.root, "snapshot");
	snapshotRelease({ ...f, dir: snapshot, files });
	for (const [path, bytes] of Object.entries(f.installFiles)) assert.equal(readFileSync(join(snapshot, "release/install", path), "utf8"), bytes);
	assert.equal(existsSync(join(snapshot, "release/install", stale)), false);
	assert.equal(files.filter(row => row.path.startsWith("release/install/")).length, 3);
	const folder = keepReleaseBuild({ ...f, target: snapshot });
	const lease = leaseReleaseBuild({ dir: f.dir, commit });
	try {
		for (const [path, bytes] of Object.entries(f.installFiles)) {
			writeFileSync(join(f.target, "release/install", path), "another checkout");
			assert.equal(readFileSync(join(folder, "install", path), "utf8"), bytes);
		}
		assert.equal(existsSync(join(folder, "install", stale)), false);
		assert.throws(() => keepReleaseBuild({ ...f, target: snapshot }), /live leased build/);
	} finally { lease.release(); }
});

test("declared missing, changed, foreign-commit and unpaired installer outputs are refused", t => {
	for (const kind of ["missing", "missing-root", "changed", "foreign", "unpaired"]) {
		const f = fixture(t), archive = `${commit}/richview-aarch64-apple-darwin.tar.gz`;
		if (kind === "missing") rmSync(join(f.target, "release/install", archive));
		if (kind === "missing-root") rmSync(join(f.target, "release/install"), { recursive: true });
		if (kind === "changed") writeFileSync(join(f.target, "release/install", archive), "other bytes");
		if (kind === "foreign") f.manifest.outputs[`target:release/install/${stale}/richview-aarch64-apple-darwin.tar.gz`] = { bytes: 13, sha256: "0".repeat(64) };
		if (kind === "unpaired") delete f.manifest.outputs[`target:release/install/${archive}.sha256`];
		f.save();
		assert.throws(() => snapshotRelease({ ...f, dir: join(f.root, "snapshot") }), /install|ENOENT/);
	}
});

test("old manifests do not copy an unregistered stale install directory", t => {
	const f = fixture(t); f.manifest.outputs = {}; f.save();
	const snapshot = join(f.root, "snapshot"); snapshotRelease({ ...f, dir: snapshot });
	assert.equal(existsSync(join(snapshot, "release/install")), false);
	const folder = keepReleaseBuild({ ...f, target: snapshot });
	assert.equal(existsSync(join(folder, "install")), false);
});

test("invalid installer snapshots produce a receipted build failure and unrun dependants at serial and parallel budgets", async t => {
	for (const parallel of [1, 2]) {
		const f = fixture(t), ctx = { cwd: f.cwd, tempDir: join(f.root, "run") };
		f.manifest.outputs[`target:release/install/${stale}/foreign.tar.gz`] = { bytes: 0, sha256: "0".repeat(64) }; f.save();
		let build = { id: "build", status: "pass" }, ranDependent = false;
		const results = await runFullSteps([{ id: "build" }, { id: "reader", needs: "build" }], async step => {
			if (step.id === "build") return build = captureBuildSnapshot({ ctx, target: f.target, result: build, notAfter: Date.now() });
			if (build.status !== "pass") return { status: "unrun", reason: "The build step failed." };
			ranDependent = true; return { status: "pass" };
		}, parallel);
		assert.equal(results[0].status, "fail"); assert.match(results[0].reason, /Cannot preserve.*Invalid installer output paths/);
		assert.equal(results[1].status, "unrun"); assert.equal(ranDependent, false);
		assert.equal(ctx.snapshot, undefined); assert.deepEqual(ctx.snapshotFiles, []);
		const oldHome = process.env.RV_GATE_HOME;
		try {
			process.env.RV_GATE_HOME = join(f.root, "gate-store");
			const id = `full-snapshot-refusal-${parallel}`;
			writeReceipt({ id, result: "fail", steps: results });
			assert.deepEqual(readReceipt(id).steps.map(step => step.status), ["fail", "unrun"]);
		} finally { if (oldHome === undefined) delete process.env.RV_GATE_HOME; else process.env.RV_GATE_HOME = oldHome; }
	}
});
