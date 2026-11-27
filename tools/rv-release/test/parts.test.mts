// Host reason (ARC-03): tests of the part model: the fixed order, what each part needs, and that the website and hosted parts stand alone (DEL-83).
import test from "node:test";
import assert from "node:assert/strict";
import { activeParts, loadParts, selectParts, selectable, validateParts } from "../lib/parts.mts";

const model = loadParts();
const needsOf = (id: string, seen = new Set<string>()) => { for (const need of model.byId.get(id)!.needs) if (!seen.has(need)) { seen.add(need); needsOf(need, seen); } return seen; };

test("the parts are in the order Daniel named: Windows walk, prod merge, richview.ai, app.richview.ai, public source and release", () => {
	assert.deepEqual(selectable(model), ["windows", "prod", "site", "hosted", "public"]);
	assert.deepEqual(model.parts.map((p) => p.id), ["shared", "windows", "prod", "build", "site", "hosted", "public"]);
});

test("richview.ai and app.richview.ai never need each other, nor does the public release need either", () => {
	for (const [a, b] of [["site", "hosted"], ["hosted", "site"], ["public", "site"], ["public", "hosted"], ["site", "public"], ["hosted", "public"]]) {
		assert.ok(!needsOf(a).has(b), `${a} must not need ${b}`);
	}
	assert.deepEqual([...needsOf("site")].sort(), ["prod", "windows"]);
	assert.deepEqual([...needsOf("hosted")].sort(), ["prod", "windows"].concat(["build"]).sort());
	assert.deepEqual([...needsOf("public")].sort(), ["build", "prod", "windows"]);
	assert.deepEqual([...needsOf("prod")], ["windows"]);
});

test("a selection must carry its prerequisites, unless an earlier run already passed them, and comes back in the fixed order", () => {
	assert.deepEqual(selectParts(model, ["hosted", "windows", "prod", "site"]), ["windows", "prod", "site", "hosted"]);
	assert.throws(() => selectParts(model, ["site"]), /site needs part prod/);
	assert.throws(() => selectParts(model, ["prod"]), /prod needs part windows/);
	assert.deepEqual(selectParts(model, ["site", "hosted"], new Set(["windows", "prod"])), ["site", "hosted"]);
	assert.deepEqual(selectParts(model, ["windows"]), ["windows"]);
	assert.throws(() => selectParts(model, ["windows", "windows"]), /once/);
	assert.throws(() => selectParts(model, ["build"]), /Unknown part/);
	assert.throws(() => selectParts(model, []), /at least one/);
});

test("the internal build part is active only when a part that needs it is chosen", () => {
	const ids = (chosen: string[]) => activeParts(model, chosen).map((p) => p.id);
	assert.deepEqual(ids(["windows", "prod", "site"]), ["shared", "windows", "prod", "site"]);
	assert.deepEqual(ids(["windows", "prod", "site", "hosted"]), ["shared", "windows", "prod", "build", "site", "hosted"]);
	assert.deepEqual(ids(["windows", "prod", "public"]), ["shared", "windows", "prod", "build", "public"]);
});

test("every part that deploys has a rollback ready, and only the Worker's runs by itself (DEL-48)", () => {
	for (const part of model.parts.filter((p) => p.deploy)) {
		assert.ok(part.rollback?.title && part.rollback.command, `${part.id} names its rollback`);
		assert.ok(Array.isArray(part.rollback.requires) && part.rollback.requires.length, `${part.id} names the evidence its rollback needs`);
		assert.ok(part.gated!.includes("deploy"), `${part.id} deploys only under a go`);
	}
	assert.deepEqual(model.parts.filter((p) => p.rollback?.automatic).map((p) => p.id), ["site"]);
	assert.match(model.byId.get("site")!.rollback!.basis!, /^DEL-48/);
});

test("spending and every outward step sit behind a go", () => {
	assert.deepEqual(model.byId.get("windows")!.gated, ["prepare"]);
	assert.deepEqual(model.byId.get("public")!.gated, ["prepare", "deploy"]);
	assert.ok(model.byId.get("hosted")!.gated!.includes("closing"));
	for (const part of model.parts) for (const stage of ["deploy", "closing"] as const) for (const spec of part[stage] ?? []) if (spec.outward) assert.ok(part.gated!.includes(stage), `${spec.step} is outward and gated`);
});

test("a malformed model is refused", () => {
	const copy = () => structuredClone({ schema: 1, parts: [...model.parts] });
	let raw = copy(); raw.parts[2] = { ...raw.parts[2], needs: ["site"] };
	assert.throws(() => validateParts(raw), /not an earlier part/);
	raw = copy(); raw.parts.push({ ...raw.parts[1], id: "windows" });
	assert.throws(() => validateParts(raw), /repeated part id/);
	raw = copy(); raw.parts[4] = { ...raw.parts[4], deploy: [...raw.parts[4].deploy!, raw.parts[4].deploy![0]] };
	assert.throws(() => validateParts(raw), /is in two places/);
	raw = copy(); raw.parts[4] = { ...raw.parts[4], rollback: { ...raw.parts[4].rollback!, basis: undefined } };
	assert.throws(() => validateParts(raw), /without steps and a basis/);
});
