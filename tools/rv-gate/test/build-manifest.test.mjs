// ARC-46: staged byte changes and wrong source provenance fail before a release build is trusted.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import test from "node:test";
import { BUILD_MANIFEST, buildManifestProblems } from "../lib/build-manifest.mjs";

const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");
function fixture(t) {
	mkdirSync("tmp", { recursive: true });
	const root = mkdtempSync(join(process.cwd(), "tmp/build-manifest-"));
	t.after(() => rmSync(root, { recursive: true, force: true }));
	const cwd = join(root, "checkout");
	const target = join(root, "target");
	mkdirSync(cwd);
	const inputs = { commit: "a".repeat(40), dirty: false, source_diff_sha256: hash(""), rustc: "rustc 1.99.0 (build)", wasm_bindgen: "wasm-bindgen 0.2.129", config: {} };
	for (const path of ["Cargo.lock", "package-lock.json", "rust-toolchain.toml"]) {
		writeFileSync(join(cwd, path), path);
		inputs.config[path] = hash(path);
	}
	const outputs = {};
	for (const key of ["target:release/richview", "target:release/richview-editor", "target:release/richview-sql-worker", "checkout:packages/vscode/bin/richview-sql-worker", "checkout:dist/web/richview_runtime.js", "checkout:dist/web/richview_runtime_bg.wasm", "checkout:dist/web/richview_site.js", "checkout:dist/web/richview_site_bg.wasm"]) {
		const [group, path] = key.split(":");
		const file = join(group === "checkout" ? cwd : target, path);
		mkdirSync(dirname(file), { recursive: true });
		const bytes = key.endsWith("richview-sql-worker") ? "worker" : key;
		writeFileSync(file, bytes);
		outputs[key] = { bytes: Buffer.byteLength(bytes), sha256: hash(bytes) };
	}
	const manifest = { schema: 1, mode: "full", inputs, outputs };
	const save = () => writeFileSync(join(target, BUILD_MANIFEST), JSON.stringify(manifest));
	save();
	return { root, cwd, target, manifest, save, check: () => buildManifestProblems({ cwd, target, head: "a".repeat(40), rust: "1.99.0", wasmBindgen: "0.2.129" }) };
}

test("a complete manifest matches exact staged bytes and source inputs", (t) => {
	const f = fixture(t);
	assert.deepEqual(f.check(), []);
	const file = join(f.target, "release/richview");
	const bytes = readFileSync(file);
	bytes[0] ^= 1;
	writeFileSync(file, bytes);
	assert.match(f.check().join(";"), /manifest output changed: target:release\/richview/);
});

test("wrong commits, dirty sources, changed configuration and partial builds fail closed", (t) => {
	const f = fixture(t);
	f.manifest.inputs.commit = "b".repeat(40);
	f.manifest.inputs.dirty = true;
	f.manifest.mode = "assets";
	f.save();
	writeFileSync(join(f.cwd, "Cargo.lock"), "changed");
	const failures = f.check().join(";");
	for (const expected of [/manifest commit/, /dirty sources/, /full schema-1/, /input changed: Cargo.lock/]) assert.match(failures, expected);
});

test("an omitted artifact or a path outside the output roots cannot pass", (t) => {
	const f = fixture(t);
	delete f.manifest.outputs["target:release/richview-editor"];
	delete f.manifest.outputs["target:release/richview-sql-worker"];
	delete f.manifest.outputs["checkout:packages/vscode/bin/richview-sql-worker"];
	f.manifest.outputs["checkout:../outside"] = { bytes: 0, sha256: hash("") };
	f.save();
	assert.match(f.check().join(";"), /omits target:release\/richview-editor/);
	assert.match(f.check().join(";"), /omits target:release\/richview-sql-worker/);
	assert.match(f.check().join(";"), /omits checkout:packages\/vscode\/bin\/richview-sql-worker/);
	assert.match(f.check().join(";"), /invalid manifest output path/);
});

test("changing the packaged worker fails even when the native sibling still matches", (t) => {
	const f = fixture(t);
	writeFileSync(join(f.cwd, "packages/vscode/bin/richview-sql-worker"), "different worker");
	assert.match(f.check().join(";"), /manifest output changed: checkout:packages\/vscode\/bin\/richview-sql-worker/);
	f.manifest.outputs["checkout:packages/vscode/bin/richview-sql-worker"] = { bytes: Buffer.byteLength("different worker"), sha256: hash("different worker") };
	f.save();
	assert.match(f.check().join(";"), /packaged SQL worker differs from its native sibling/);
});

test("missing artifacts and symlinks outside the checkout fail verification", (t) => {
	const f = fixture(t);
	const file = join(f.cwd, "dist/web/richview_runtime.js");
	rmSync(file);
	assert.match(f.check().join(";"), /cannot verify build manifest/);
	writeFileSync(join(f.root, "outside"), "outside");
	symlinkSync(join(f.root, "outside"), file);
	assert.match(f.check().join(";"), /not a regular file within checkout/);
});
