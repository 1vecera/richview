// Unsaved answer changes, kept in localStorage until the server confirms them.
// Plain script: it defines globalThis.LockdownJournal for app.js and for the node tests.
//
// One entry per part and item:
//   intent     the answer that is not confirmed yet: { decision, text, note, token, base }
//              or { clear: true, token, base }. `base` is the updatedAt of the saved answer the
//              change was made on (null when there was none); `token` identifies one send.
//   noteDraft  a note typed on a card without a decision; it is saved with the decision.
//   at         when the entry last changed.
(function (root) {
	"use strict";

	const KEY = "lockdown.journal.v1";
	const norm = (value) => (typeof value === "string" ? value.trim() : "");

	function read(storage) {
		try {
			const raw = storage?.getItem(KEY);
			const data = raw ? JSON.parse(raw) : null;
			if (data && typeof data.entries === "object" && data.entries) return data;
		} catch {
			// Unreadable or blocked storage reads as empty; it never breaks the app.
		}
		return { entries: {} };
	}

	function write(storage, data) {
		try {
			if (Object.keys(data.entries).length) storage?.setItem(KEY, JSON.stringify(data));
			else storage?.removeItem(KEY);
			return true;
		} catch {
			return false;
		}
	}

	const keyOf = (partId, itemId) => `${partId}/${itemId}`;

	function change(storage, partId, itemId, mutate) {
		const data = read(storage);
		const key = keyOf(partId, itemId);
		const entry = { partId, itemId, ...(data.entries[key] ?? {}) };
		mutate(entry);
		if (!entry.intent && !entry.noteDraft) delete data.entries[key];
		else data.entries[key] = { ...entry, at: new Date().toISOString() };
		return write(storage, data);
	}

	let counter = 0;
	const newToken = () => `${Date.now().toString(36)}-${(counter++).toString(36)}-${Math.random().toString(36).slice(2, 8)}`;

	// Records an answer that is about to be sent (or failed to send). next null means "clear".
	function setIntent(storage, partId, itemId, next, base, token) {
		const intent = next
			? { decision: next.decision, text: next.decision === "edit" ? next.text ?? "" : undefined, note: next.note ?? "" }
			: { clear: true };
		return change(storage, partId, itemId, (entry) => {
			entry.intent = { ...intent, token, base: base ?? null };
			// A decided note travels in the intent.
			delete entry.noteDraft;
		});
	}

	// The server confirmed the send `token`, producing `updatedAt`. A newer intent for the same
	// item now builds on that answer, so its base moves forward instead of reading as a conflict.
	function settle(storage, partId, itemId, token, updatedAt) {
		return change(storage, partId, itemId, (entry) => {
			if (!entry.intent) return;
			if (entry.intent.token === token) delete entry.intent;
			else entry.intent = { ...entry.intent, base: updatedAt ?? null };
		});
	}

	function dropIntent(storage, partId, itemId) {
		return change(storage, partId, itemId, (entry) => {
			delete entry.intent;
		});
	}

	function setNoteDraft(storage, partId, itemId, note) {
		return change(storage, partId, itemId, (entry) => {
			if (entry.intent) return;
			if (norm(note)) entry.noteDraft = note;
			else delete entry.noteDraft;
		});
	}

	function dropNoteDraft(storage, partId, itemId) {
		return change(storage, partId, itemId, (entry) => {
			delete entry.noteDraft;
		});
	}

	function entries(storage, partId) {
		const all = Object.values(read(storage).entries);
		return partId === undefined ? all : all.filter((entry) => entry.partId === partId);
	}

	const get = (storage, partId, itemId) => read(storage).entries[keyOf(partId, itemId)] ?? null;

	// What an intent means against the answer the server holds now:
	//   "saved"     the server already has it; drop it.
	//   "retry"     the server answer is the one the change was made on; send it.
	//   "conflict"  the server answer changed since (another tab, device or writer); ask Daniel.
	function classify(intent, answer) {
		if (!intent) return "none";
		const saved = answer ?? null;
		if (intent.clear ? !saved : sameAnswer(intent, saved)) return "saved";
		return (intent.base ?? null) === (saved?.updatedAt ?? null) ? "retry" : "conflict";
	}

	function sameAnswer(intent, saved) {
		if (!saved || saved.decision !== intent.decision) return false;
		if (intent.decision === "edit" && norm(saved.text) !== norm(intent.text)) return false;
		return norm(saved.note) === norm(intent.note);
	}

	function counts(storage) {
		let unsaved = 0;
		let notes = 0;
		for (const entry of entries(storage)) {
			if (entry.intent) unsaved += 1;
			else if (entry.noteDraft) notes += 1;
		}
		return { unsaved, notes };
	}

	root.LockdownJournal = {
		KEY,
		newToken,
		setIntent,
		settle,
		dropIntent,
		setNoteDraft,
		dropNoteDraft,
		entries,
		get,
		classify,
		counts,
	};
})(globalThis);
