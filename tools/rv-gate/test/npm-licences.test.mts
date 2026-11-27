// Tests for the npm licence check of the dependency gate (ARC-68).

import assert from "node:assert/strict";
import test from "node:test";
import { checkLock, satisfies } from "../npm-licences.mts";
import type { NpmLockEntry } from "../lib/npm.mts";

test("SPDX expressions: OR needs one allowed term, AND needs all", () => {
	assert.equal(satisfies("MIT"), true);
	assert.equal(satisfies("(MIT OR WTFPL)"), true);
	assert.equal(satisfies("MIT AND LGPL-3.0-or-later"), false);
	assert.equal(satisfies("(BSD-2-Clause OR MIT) AND (ISC OR GPL-3.0-only)"), true);
	assert.equal(satisfies("GPL-3.0-only"), false);
	assert.equal(satisfies(undefined), false);
	assert.equal(satisfies("MIT OR"), false);
});

test("an unlisted licence fails; a dev-only exception passes only for a dev package", () => {
	const lock = (entry: NpmLockEntry) => ({ packages: { "": {}, "node_modules/tslib": { version: "2.0.0", ...entry } } });
	assert.deepEqual(checkLock(lock({ license: "0BSD", dev: true })), []);
	assert.equal(checkLock(lock({ license: "0BSD" })).length, 1);
	assert.equal(checkLock(lock({ license: "GPL-3.0-only", dev: true })).length, 1);
	assert.equal(checkLock(lock({})).length, 1);
});

test("BlueOak-1.0.0 is on the allowlist for shipped packages; a dev-only licence is not", () => {
	const lock = (name: string, entry: NpmLockEntry) => ({ packages: { "": {}, [`node_modules/${name}`]: { version: "1.0.0", ...entry } } });
	assert.deepEqual(checkLock(lock("minimatch", { license: "BlueOak-1.0.0" })), []);
	assert.equal(checkLock(lock("minimatch", { license: "GPL-3.0-only" })).length, 1);
	assert.equal(checkLock(lock("textextensions", { license: "Artistic-2.0" })).length, 1);
	assert.deepEqual(checkLock(lock("textextensions", { license: "Artistic-2.0", dev: true })), []);
});
