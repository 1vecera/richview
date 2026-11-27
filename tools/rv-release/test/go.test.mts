// Host reason (ARC-03): tests of Daniel's go: his words, the parts they cover and the evidence each part binds (DEL-83).
import test from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { validateWords, bindingFor, verifiedGo, bindingKeys } from "../lib/go.mts";
import { harness } from "./harness.mts";

test("a go needs Daniel's actual words", () => {
	assert.equal(validateWords("  Daniel, 26 Nov: release candidate/33  "), "Daniel, 26 Nov: release candidate/33");
	for (const bad of ["", "go", "<Daniel's words and date>", "TODO later", "two\nlines", "{{words}}"]) assert.throws(() => validateWords(bad), bad);
	assert.throws(() => validateWords(undefined));
});

test("each part binds its own evidence to Daniel's words", () => {
	const h = harness({ journalKeys: [["R", "full-x"], ["M", "m".repeat(64)], ["PACKET_SHA", "p".repeat(64)], ["DRY_SHA", "d".repeat(64)], ["BEFORE_WORKER", "w"], ["H", "h"], ["OLD_SHA256", "o"], ["SEED", "s"], ["NOTES_HASH", "n"], ["ASSETS_HASH", "a"]] });
	const evidence = h.journal.evidence();
	assert.equal(bindingFor("prod", h.desc, evidence), `[v3.1.0 prod: candidate/33 ${h.desc.final}, receipt full-x, manifest ${"m".repeat(64)}, packet ${"p".repeat(64)}]`);
	assert.match(bindingFor("site", h.desc, evidence), /dry run d+, prior Worker w\]$/);
	assert.match(bindingFor("hosted", h.desc, evidence), /archive h, replacing archive o\]$/);
	assert.match(bindingFor("public", h.desc, evidence), /seed s, notes n, assets a\]$/);
	assert.deepEqual(bindingKeys("windows"), ["R", "M"]);
	assert.deepEqual(bindingKeys("public", "prepare"), ["R", "M"]);
});

test("a part cannot bind evidence the journal does not hold yet", () => {
	const h = harness({ journalKeys: [["R", "r"], ["M", "m"]] });
	assert.throws(() => verifiedGo(h.journal, "prod", h.desc), /binds PACKET_SHA/);
	assert.match(bindingFor("prod", h.desc, h.journal.evidence(), { stubbed: true }), /packet <stub:PACKET_SHA>/);
});

test("the go text a step receives is Daniel's words and the binding, and the binding is fixed once it is first used", () => {
	const h = harness({ journalKeys: [["R", "r"], ["M", "m"], ["PACKET_SHA", "p1"]] });
	const first = verifiedGo(h.journal, "prod", h.desc);
	assert.equal(first, `Daniel, 26 Nov: release candidate/33 everywhere [v3.1.0 prod: candidate/33 ${h.desc.final}, receipt r, manifest m, packet p1]`);
	assert.equal(verifiedGo(h.journal, "prod", h.desc), first, "the same evidence gives the same text");
	assert.ok(existsSync(h.journal.bindingFile("prod", "deploy")));
	h.journal.save("PACKET_SHA", "p2");
	assert.throws(() => verifiedGo(h.journal, "prod", h.desc), /evidence bound to the go for prod has changed/);
});

test("a go covers only the parts it names", () => {
	const h = harness({ chosen: ["windows", "prod"], journalKeys: [["R", "r"], ["M", "m"], ["PACKET_SHA", "p"]] });
	assert.doesNotThrow(() => verifiedGo(h.journal, "prod", h.desc));
	assert.throws(() => verifiedGo(h.journal, "hosted", h.desc), /No go covers the hosted part/);
});

test("a later go replaces an earlier one for the parts it names", () => {
	const h = harness({ chosen: ["windows"], journalKeys: [["R", "r"], ["M", "m"], ["PACKET_SHA", "p"]] });
	h.journal.writeGo({ at: "2026-11-27T09:00:00Z", words: "Daniel, 27 Nov: now production too", parts: ["windows", "prod"] });
	assert.match(verifiedGo(h.journal, "prod", h.desc), /^Daniel, 27 Nov: now production too \[/);
	assert.match(verifiedGo(h.journal, "windows", h.desc), /^Daniel, 27 Nov/);
});

test("a dry run's binding keeps stub placeholders and writes no binding file", () => {
	const h = harness({ journalKeys: [["R", "r"], ["M", "m"]] });
	assert.match(verifiedGo(h.journal, "hosted", h.desc, { stubbed: true }), /archive <stub:H>/);
	assert.ok(!existsSync(h.journal.bindingFile("hosted", "deploy")));
	writeFileSync(`${h.dir}/unused`, readFileSync(h.desc.journal + "/steps.tsv"));
});
