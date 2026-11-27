// Parse and validate the fields every PR into `dev` declares in its body (DEL-32, DEL-44, DEL-70).
//
// A PR body carries one field per line near the top:
//
//   Refs S27-<n>
//   decision: none | design | feature
//   cause: request "<Daniel's words>" | request S27-<n> | loop:<run-id> | issue S27-<n>
//   items: FEA-CH-37, STY-05
//   assumed: FEA-CH-12, TYP-04
//
// and an optional "## Retired tests" section whose list items name each deleted or skipped
// test with its reason: "- `path::name` — reason".

/** A word that looks like an item ID but is none: "FEA-CH37", "int-17". */
export const NEAR_ID = /^[A-Za-z]{2,}(?:-[A-Za-z]+)*-?\d+$/;
export const ITEM_ID = /^(?:INT|DEL|ARC|FEA-(?:CH|RP|SF|BR)|TYP|STY|PRF|OPN)-\d+$/;
const CLASSES = new Set(["none", "design", "feature"]);

/** Return the value of a `key: value` line, or undefined. Keys match case-insensitively. */
function field(body: string, key: string) {
	// Only spaces and tabs around the value: an empty field must not take the next line's text.
	const match = body.match(new RegExp(`^[ \\t]*${key}[ \\t]*:[ \\t]*(.*?)[ \\t]*$`, "im"));
	return match?.[1] || undefined;
}

/** Split a comma- or space-separated list of lockdown item IDs. */
export function parseItems(text?: string) {
	if (!text) return [];
	return text
		.split(/[,\s]+/)
		// An ID in parentheses or brackets, "(INT-17)", is that ID.
		.map((value) => value.trim().replace(/^[([]+/, "").replace(/[)\]]+$/, "").replace(/[.;,]$/, ""))
		.filter(Boolean);
}

/** Parse the cause field into `{kind, detail}`; kind is request, loop or issue. */
export function parseCause(text?: string) {
	if (!text) return undefined;
	const loop = text.match(/^loop:([\w.-]+)/i);
	if (loop) return { kind: "loop", detail: loop[1] };
	const request = text.match(/^request\b\s*(.*)$/i);
	if (request) return { kind: "request", detail: request[1].trim() };
	const issue = text.match(/^issue\b\s*(.*)$/i);
	if (issue) return { kind: "issue", detail: issue[1].trim() };
	return { kind: "unknown", detail: text };
}

/** Parse the "## Retired tests" section into `[{test, reason}]`. */
export interface Retirement { test: string; reason: string }

export function parseRetired(body: string) {
	// A stack's joined commit messages can hold one such section per commit: read every one.
	const retired: Retirement[] = [];
	for (const section of body.matchAll(/^##\s+Retired tests[ \t]*$([\s\S]*?)(?=^##\s|(?![\s\S]))/gim)) {
		for (const line of section[1].split("\n")) {
			const match = line.match(/^\s*[-*]\s+`([^`]+)`\s*(?:—|-|:)\s*(.+?)\s*$/);
			if (match) retired.push({ test: match[1], reason: match[2] });
		}
	}
	return retired;
}

/** Parse every declared field of a PR body. */
export function parsePrBody(body = "") {
	const refs = body.match(/^\s*Refs\s+(S27-\d+)\b/im);
	return {
		issue: refs ? refs[1] : undefined,
		decision: field(body, "decision")?.toLowerCase(),
		cause: parseCause(field(body, "cause")),
		items: parseItems(field(body, "items")),
		assumed: parseItems(field(body, "assumed")),
		retired: parseRetired(body),
	};
}

/**
 * Validate parsed fields and return a list of problems; an empty list means the PR may be gated.
 * `knownItems` is the set of lockdown item IDs from the part files.
 */
export function validatePrFields(fields: ReturnType<typeof parsePrBody>, { knownItems }: { knownItems?: ReadonlySet<string> } = {}) {
	const problems = [];
	if (!fields.issue) problems.push("Name one RichView issue with `Refs S27-<n>` (DEL-44).");
	if (!fields.decision) {
		problems.push("Declare `decision: none`, `design` or `feature` (DEL-32).");
	} else if (fields.decision.includes("design") && fields.decision.includes("feature")) {
		problems.push("A PR is never both design and feature; split it (DEL-70).");
	} else if (!CLASSES.has(fields.decision)) {
		problems.push(`Unknown decision class '${fields.decision}'; use none, design or feature (DEL-32).`);
	}
	if (fields.decision === "design" || fields.decision === "feature") {
		if (!fields.cause) problems.push("A design or feature PR declares its cause (DEL-70).");
		else if (fields.cause.kind === "unknown")
			problems.push(`Cause '${fields.cause.detail}' is not request, loop:<run> or issue (DEL-70).`);
	}
	for (const id of [...fields.items, ...fields.assumed]) {
		if (!ITEM_ID.test(id)) problems.push(`'${id}' is not a lockdown item ID.`);
		else if (knownItems && !knownItems.has(id)) problems.push(`'${id}' is not in the lockdown parts.`);
	}
	return problems;
}

/**
 * Wait until GitHub reports `commit` as the PR's head and the PR as mergeable. After the gate
 * pushes the rebased commit, GitHub recomputes mergeability in the background; merging before it
 * has finished fails with "Pull Request is not mergeable" (PR #110). `view()` returns the PR's
 * `{headRefOid, mergeable}`; `mergeable` is `MERGEABLE`, `CONFLICTING` or `UNKNOWN`.
 */
interface Mergeability { headRefOid?: string; mergeable: string }
interface PollOptions { tries?: number; delayMs?: number; sleep?: (ms: number) => Promise<unknown> }

export async function waitForMergeable(view: () => Mergeability, commit: string, { tries = 30, delayMs = 2000, sleep = (ms) => new Promise((r) => setTimeout(r, ms)) }: PollOptions = {}) {
	let last: Mergeability | undefined;
	let failure: Error | undefined;
	for (let attempt = 0; attempt < tries; attempt++) {
		// A failed or unreadable answer is "not yet": the push has happened, so keep polling.
		try {
			last = view();
			failure = undefined;
		} catch (error) {
			failure = error as Error;
			await sleep(delayMs);
			continue;
		}
		if (last.headRefOid === commit) {
			if (last.mergeable === "MERGEABLE") return;
			if (last.mergeable === "CONFLICTING") throw new Error(`GitHub reports the gated commit ${commit.slice(0, 10)} as conflicting with dev; re-queue (DEL-43).`);
		}
		await sleep(delayMs);
	}
	throw new Error(
		`GitHub did not report the gated commit ${commit.slice(0, 10)} as mergeable within ${(tries * delayMs) / 1000} s (head ${last?.headRefOid?.slice(0, 10)}, ${last?.mergeable}${failure ? `; last error: ${failure.message.split("\n")[0]}` : ""}); re-run the gate (DEL-43).`,
	);
}
