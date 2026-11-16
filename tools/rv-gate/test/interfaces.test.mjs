// Host reason (ARC-03): exercise the real API/dependency check on current sources and isolated Cargo metadata.
import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { snapshotContract } from "../../api-contracts/check.mjs";
import { workspaceBoundaries } from "../lib/layers.mjs";

const repo = fileURLToPath(new URL("../../../", import.meta.url));

function fixture(t) {
	mkdirSync(join(repo, "tmp"), { recursive: true });
	const root = mkdtempSync(join(repo, "tmp/interfaces-"));
	t.after(() => rmSync(root, { recursive: true, force: true }));
	const put = (file, value) => {
		mkdirSync(dirname(join(root, file)), { recursive: true });
		writeFileSync(join(root, file), typeof value === "string" ? value : JSON.stringify(value));
	};
	put("Cargo.toml", '[workspace]\nmembers=["app"]\nresolver="3"\n');
	put("app/Cargo.toml", '[package]\nname="richview-app"\nversion="0.0.0"\nedition="2024"\n');
	put("app/src/lib.rs", 'const RUNTIME: &str = "runtime.js";');
	put("tools/dependency-rules/rules.json", readFileSync(join(repo, "tools/dependency-rules/rules.json"), "utf8"));
	const contract = { id: "runtime.embed-files", producer: "richview-runtime", kind: "entries", entries: ["runtime.js"], snapshot: "tools/api-contracts/snapshots/runtime.json", bindings: [{ consumer: "richview-app", source: "app/src/lib.rs", entry: "runtime.js" }] };
	const save = () => {
		put("tools/api-contracts/contracts.json", { schema: "interface-v1", contracts: [contract] });
		put(contract.snapshot, snapshotContract({ root, contract }).snapshot);
	};
	save();
	return { root, put, save, contract };
}

test("the current workspace passes freshly read public API and dependency policies together", () => {
	const result = workspaceBoundaries({ cwd: repo });
	assert.equal(result.api.status, "pass", JSON.stringify(result.api.findings));
	assert.equal(result.dependencies.status, "pass", JSON.stringify(result.dependencies.findings));
	assert.ok(result.dependencies.checked.crates > 0);
	assert.ok(result.api.checked.includes("runtime.embed-files"));
});

test("a changed runtime endpoint fails on the next call, without using an earlier passing report", t => {
	const f = fixture(t);
	assert.equal(workspaceBoundaries({ cwd: f.root }).dependencies.status, "pass");
	f.put("app/src/lib.rs", 'const RUNTIME: &str = "wrong.js";');
	const result = workspaceBoundaries({ cwd: f.root });
	assert.equal(result.api.status, "fail");
	assert.equal(result.dependencies.status, "fail");
	assert.equal(result.dependencies.findings[0].code, "dependency.api.failed");
});

test("a valid reviewed API cannot authorize an unapproved indirect dependency", t => {
	const f = fixture(t);
	f.contract.bindings[0].consumer = "richview-core";
	f.save();
	const result = workspaceBoundaries({ cwd: f.root });
	assert.equal(result.api.status, "pass");
	assert.equal(result.dependencies.status, "fail");
	assert.match(JSON.stringify(result.dependencies.findings), /richview-core/);
});

test("a missing producer manifest fails closed before Cargo metadata is needed", t => {
	const f = fixture(t);
	rmSync(join(f.root, "tools/api-contracts/contracts.json"));
	rmSync(join(f.root, "Cargo.toml"));
	const result = workspaceBoundaries({ cwd: f.root });
	assert.equal(result.api.status, "fail");
	assert.equal(result.dependencies.status, "fail");
	assert.equal(result.dependencies.findings[0].code, "dependency.api.failed");
});
