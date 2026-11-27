// Host reason (ARC-03): Daniel's go for a candidate (DEL-83): his own words, the parts they cover, and the evidence each part binds them to.
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { sha256 } from "./journal.mts";
import type { ReleaseDescriptor } from "./descriptor.mts";
import type { Journal } from "./journal.mts";

type BindingOptions = {stubbed?: boolean; stage?: string};

/** Words that cannot be a go: too short, a template, or carrying a placeholder. */
export function validateWords(words: unknown) {
	if (typeof words !== "string") throw new Error("The go needs Daniel's words.");
	const text = words.trim();
	if (text.replace(/\s/g, "").length < 8) throw new Error("The go needs Daniel's actual words and their date.");
	if (/[<>«»]/.test(text) || /placeholder|TODO|TBD|\{\{/i.test(text)) throw new Error("The go holds a placeholder; paste his actual words.");
	if (/[\r\n]/.test(text)) throw new Error("The go is one line.");
	return text;
}

// The journal keys each part's binding quotes. The common ones come first.
const COMMON = ["R", "M"];
const EVIDENCE: Record<string, string[]> = {
	windows: [],
	prod: ["PACKET_SHA"],
	site: ["PACKET_SHA", "DRY_SHA", "BEFORE_WORKER"],
	hosted: ["H", "OLD_SHA256"],
	public: ["SEED", "NOTES_HASH", "ASSETS_HASH"],
};
const LABEL: Record<string, string> = { R: "receipt", M: "manifest", PACKET_SHA: "packet", DRY_SHA: "dry run", BEFORE_WORKER: "prior Worker", H: "archive", OLD_SHA256: "replacing archive", SEED: "seed", NOTES_HASH: "notes", ASSETS_HASH: "assets" };

/** The evidence keys a part's binding quotes. Before a part's outward work only the candidate's own evidence exists (stage "prepare"). */
export const bindingKeys = (part: string, stage = "deploy") => (stage === "prepare" ? COMMON : [...COMMON, ...(EVIDENCE[part] ?? [])]);

/**
 * The binding the command appends to Daniel's words for one part: the candidate, its receipt and manifest, and the digests of what
 * that part will touch. It is composed when the part's first gated step asks for it, and it never changes afterwards.
 */
export function bindingFor(part: string, desc: ReleaseDescriptor, evidence: Map<string, string>, { stubbed = false, stage = "deploy" }: BindingOptions = {}) {
	const parts = [`${desc.candidate} ${desc.final}`];
	for (const key of bindingKeys(part, stage)) {
		let value = evidence.get(key);
		if (value === undefined && stubbed) value = `<stub:${key}>`;
		if (value === undefined) throw new Error(`The go for ${part} binds ${key}, which the journal does not hold yet.`);
		parts.push(`${LABEL[key]} ${value}`);
	}
	return `[${desc.tag} ${part}${stage === "prepare" ? " (before its build)" : ""}: ${parts.join(", ")}]`;
}

/** The most recent go entry that covers a part, or undefined. */
export const goCovering = (journal: Journal, part: string) => journal.goEntries().filter((entry) => entry.parts.includes(part)).at(-1);

/**
 * The go text for a part, verified: a go covers the part, and the binding is the one first composed for it (write-once), so evidence
 * that changed after Daniel's go refuses the step instead of passing under his words.
 */
export function verifiedGo(journal: Journal, part: string, desc: ReleaseDescriptor, options?: BindingOptions) {
	const entry = goCovering(journal, part);
	if (!entry) throw new Error(`No go covers the ${part} part. Give Daniel's words with --go.`);
	const binding = bindingFor(part, desc, journal.evidence(), options);
	if (options?.stubbed) return `${entry.words} ${binding}`;
	const file = journal.bindingFile(part, options?.stage ?? "deploy"), digest = sha256(binding);
	if (existsSync(file)) {
		if (readFileSync(file, "utf8").trim() !== digest) throw new Error(`The evidence bound to the go for ${part} has changed since it was first used. Preserve the journal and ask Daniel again.`);
	} else writeFileSync(file, `${digest}\n`, { flag: "wx", mode: 0o600 });
	return `${entry.words} ${binding}`;
}
