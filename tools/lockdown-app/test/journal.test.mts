// Unit tests for the compiled browser journal of unsaved answer changes.
// Run: node --test tools/lockdown-app/test/journal.test.mts
import assert from "node:assert/strict";
import path from "node:path";
import { test } from "node:test";
import vm from "node:vm";
import { compileBrowserScript } from "../browser.mts";

const SOURCE = await compileBrowserScript(path.join(import.meta.dirname, "..", "public", "journal.ts"));

function load() {
	const context: { Date: DateConstructor; Math: Math; JSON: JSON; Object: ObjectConstructor; globalThis?: unknown; LockdownJournal?: LockdownJournalApi } = { Date, Math, JSON, Object };
	context.globalThis = context;
	vm.runInNewContext(SOURCE, context);
	return context.LockdownJournal!;
}

function memoryStorage({ failWrites = false } = {}) {
	const data = new Map<string, string>();
	return {
		data,
		getItem: (key: string) => (data.has(key) ? data.get(key)! : null),
		setItem: (key: string, value: string) => {
			if (failWrites) throw new Error("QuotaExceededError");
			data.set(key, String(value));
		},
		removeItem: (key: string) => data.delete(key),
	};
}

test("an intent survives until the server confirms its own send", () => {
	const J = load();
	const store = memoryStorage();
	J.setIntent(store, "01-intent", "INT-01", { decision: "accept", note: "why" }, null, "t1");
	assert.equal(J.counts(store).unsaved, 1);
	assert.equal((J.get(store, "01-intent", "INT-01")!.intent! as JournalAnswerInput).decision, "accept");
	// A confirmation of an older send keeps the newer intent and moves its base forward.
	J.setIntent(store, "01-intent", "INT-01", { decision: "reject", note: "" }, null, "t2");
	J.settle(store, "01-intent", "INT-01", "t1", "2026-10-23T10:00:00.000Z");
	const entry = J.get(store, "01-intent", "INT-01");
	assert.equal(entry!.intent!.token, "t2");
	assert.equal(entry!.intent!.base, "2026-10-23T10:00:00.000Z");
	J.settle(store, "01-intent", "INT-01", "t2", "2026-10-23T10:00:01.000Z");
	assert.equal(J.get(store, "01-intent", "INT-01"), null);
	assert.equal(store.getItem(J.KEY), null, "an empty journal leaves nothing in storage");
});

test("a note typed before deciding is kept, and a decision absorbs it", () => {
	const J = load();
	const store = memoryStorage();
	J.setNoteDraft(store, "02-delivery", "DEL-01", "Check this with Steven");
	assert.deepEqual({ ...J.counts(store) }, { unsaved: 0, notes: 1 });
	assert.equal(J.entries(store, "02-delivery")[0].noteDraft, "Check this with Steven");
	assert.equal(J.entries(store, "01-intent").length, 0);
	J.setIntent(store, "02-delivery", "DEL-01", { decision: "defer", note: "Check this with Steven" }, null, "t1");
	const entry = J.get(store, "02-delivery", "DEL-01");
	assert.equal(entry!.noteDraft, undefined);
	assert.equal((entry!.intent! as JournalAnswerInput).note, "Check this with Steven");
	// Emptying a draft removes it.
	J.setNoteDraft(store, "02-delivery", "DEL-02", "x");
	J.setNoteDraft(store, "02-delivery", "DEL-02", "   ");
	assert.equal(J.get(store, "02-delivery", "DEL-02"), null);
});

test("classify retries only on the answer the change was made on", () => {
	const J = load();
	const saved = { decision: "accept", note: "", updatedAt: "2026-10-23T10:00:00.000Z" } satisfies JournalAnswer;
	// Made on no answer, server still has none: send it.
	assert.equal(J.classify({ decision: "accept", note: "", base: null }, null), "retry");
	// Made on no answer, but an answer was saved since: never overwrite it.
	assert.equal(J.classify({ decision: "reject", note: "", base: null }, saved), "conflict");
	// Made on the saved answer: send it.
	assert.equal(J.classify({ decision: "reject", note: "", base: saved.updatedAt }, saved), "retry");
	// Made on an older answer: conflict.
	assert.equal(J.classify({ decision: "reject", note: "", base: "2026-10-23T09:00:00.000Z" }, saved), "conflict");
	// The server already has it (whitespace as the server trims it): drop it.
	assert.equal(J.classify({ decision: "accept", note: "  ", base: null }, saved), "saved");
	assert.equal(
		J.classify({ decision: "edit", text: "Mine.\n", note: "n", base: null }, { decision: "edit", text: "Mine.", note: "n", updatedAt: "x" }),
		"saved",
	);
	// Clearing.
	assert.equal(J.classify({ clear: true, base: saved.updatedAt }, null), "saved");
	assert.equal(J.classify({ clear: true, base: saved.updatedAt }, saved), "retry");
	assert.equal(J.classify({ clear: true, base: null }, saved), "conflict");
	assert.equal(J.classify(undefined, saved), "none");
});

test("entries are per part and item; dropping one leaves the others", () => {
	const J = load();
	const store = memoryStorage();
	J.setIntent(store, "01-intent", "INT-01", { decision: "accept" }, null, "a");
	J.setIntent(store, "05-types", "TYP-01", null, "2026-10-23T10:00:00.000Z", "b");
	J.setNoteDraft(store, "05-types", "TYP-02", "note");
	assert.equal(J.entries(store).length, 3);
	assert.equal(J.get(store, "05-types", "TYP-01")!.intent!.clear, true);
	J.dropIntent(store, "01-intent", "INT-01");
	J.dropNoteDraft(store, "05-types", "TYP-02");
	assert.deepEqual(
		J.entries(store).map((entry) => `${entry.partId}/${entry.itemId}`),
		["05-types/TYP-01"],
	);
});

test("unreadable or failing storage never throws", () => {
	const J = load();
	const broken = memoryStorage();
	broken.data.set("lockdown.journal.v1", "{not json");
	assert.equal(J.entries(broken).length, 0);
	assert.equal(J.setIntent(broken, "p", "i", { decision: "accept" }, null, "t"), true);
	assert.equal(J.entries(broken).length, 1);
	const full = memoryStorage({ failWrites: true });
	assert.equal(J.setIntent(full, "p", "i", { decision: "accept" }, null, "t"), false);
	assert.equal(J.counts(null).unsaved, 0);
	assert.equal(J.setNoteDraft(null, "p", "i", "x"), true);
});
