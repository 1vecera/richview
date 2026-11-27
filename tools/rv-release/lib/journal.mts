// Host reason (ARC-03): the release journal on disk (the kit's steps.tsv, the go record and one journal file for each part).
import { existsSync, mkdirSync, readFileSync, renameSync, writeFileSync, appendFileSync, readdirSync } from "node:fs";
import { createHash, randomUUID } from "node:crypto";
import { join } from "node:path";

import type { PartState } from "./run.mts";

export type GoEntry = {at: string; words: string; parts: string[]; stub?: boolean};
export type Journal = ReturnType<typeof openJournal>;

export const sha256 = (text: string | Buffer) => createHash("sha256").update(text).digest("hex");

/** Parse the kit's tab-separated journal; the last value of a key wins. */
export function parseSteps(text: string) {
	const map = new Map<string, string>();
	for (const line of text.split("\n")) {
		const tab = line.indexOf("\t");
		if (tab > 0) map.set(line.slice(0, tab), line.slice(tab + 1));
	}
	return map;
}

function atomic(path: string, text: string) {
	const tmp = `${path}.${randomUUID()}.tmp`;
	writeFileSync(tmp, text, { mode: 0o600, flag: "wx" });
	renameSync(tmp, path);
}

/** One release's journal folder (`go-record-<sha8>`). */
export function openJournal(here: string) {
	mkdirSync(join(here, "parts"), { recursive: true, mode: 0o700 });
	const stepsFile = join(here, "steps.tsv");
	return {
		here,
		evidence: () => (existsSync(stepsFile) ? parseSteps(readFileSync(stepsFile, "utf8")) : new Map<string, string>()),
		get(key: string) { return this.evidence().get(key); },
		done(key: string) { return this.evidence().get(key) === "yes"; },
		/** Append one key as the kit's `save` does. */
		save(key: string, value: string) {
			if (/[\t\n]/.test(`${key}${value}`)) throw new Error("A journal value cannot hold a tab or a line break.");
			appendFileSync(stepsFile, `${key}\t${value}\n`, { mode: 0o600 });
		},
		hasLatch: (part: string) => existsSync(join(here, `FAILED.${part}`)),
		readPart(id: string): PartState | null {
			const file = join(here, "parts", `${id}.json`);
			return existsSync(file) ? JSON.parse(readFileSync(file, "utf8")) : null;
		},
		writePart(id: string, record: PartState) { atomic(join(here, "parts", `${id}.json`), `${JSON.stringify(record, null, 1)}\n`); },
		/** An append-only event line, so a run can be read back in order. */
		event(entry: {at: string; line: string}) { appendFileSync(join(here, "events.jsonl"), `${JSON.stringify(entry)}\n`, { mode: 0o600 }); },
		goEntries(): GoEntry[] {
			return readdirSync(here).filter((n) => /^go-\d+\.json$/.test(n)).sort().map((n) => JSON.parse(readFileSync(join(here, n), "utf8")));
		},
		writeGo(entry: GoEntry) {
			const n = String(this.goEntries().length + 1).padStart(2, "0");
			atomic(join(here, `go-${n}.json`), `${JSON.stringify(entry, null, 1)}\n`);
			return `go-${n}.json`;
		},
		bindingFile: (part: string, stage = "deploy") => join(here, `go-binding-${part}-${stage}.txt`),
	};
}
