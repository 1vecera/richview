// Host reason (ARC-03): the report at the end of a release run: each part's outcome, why, and what is left to do (DEL-83).
import { OUTCOME } from "./run.mts";
import type { ReleaseDescriptor } from "./descriptor.mts";
import type { GoEntry } from "./journal.mts";
import type { PartState } from "./run.mts";

/** What the lead does next for one part, in a sentence. */
function next(s: PartState) {
	switch (s.status) {
		case "passed": return s.carried ? "passed in an earlier run" : "";
		case "held": return "reopen when Daniel decides; the update itself is complete";
		case "refused": return "nothing changed; fix the cause and run again";
		case "failed-unchanged": return "nothing was deployed; fix the cause and run again";
		case "failed-rolled-back": return "the previous version is live again; fix forward and run again";
		case "failed-left": return s.rollback?.command ? `rollback ready: ${s.rollback!.command}${s.rollback!.words ? ` (words: ${s.rollback!.words})` : ""}` : "inspect the journal";
		case "skipped": return "runs after what it needs has passed";
		default: return "";
	}
}

/** The report as Markdown. `states` is the Map the runner returns. */
export function renderReport({ desc, chosen, states, goEntries = [], stubbed = false, at }: {desc: ReleaseDescriptor; chosen: string[]; states: Map<string, PartState>; goEntries?: GoEntry[]; stubbed?: boolean; at: string}) {
	const rows = [...states.values()].filter((s) => s.id !== "shared" || s.status !== "passed");
	const lines = [
		`# Release report: ${desc.tag}, ${desc.candidate} ${desc.final.slice(0, 10)}${stubbed ? " (dry run: every step stubbed)" : ""}`,
		"",
		`Generated ${at}. Parts chosen: ${chosen.join(", ")}.`,
		"",
		"| Part | Outcome | Why | Next |",
		"| --- | --- | --- | --- |",
		...rows.map((s) => `| ${s.title} | ${OUTCOME[s.status] ?? s.status} | ${s.reason.replaceAll("|", "/") || (s.status === "passed" ? "every step and check passed" : "")} | ${next(s).replaceAll("|", "/")} |`),
		"",
	];
	const checks = rows.flatMap((s) => s.checks.map((c) => `- ${s.title}: ${c.id}: ${c.status}${c.detail ? ` — ${c.detail}` : ""}`));
	if (checks.length) lines.push("## Checks before the deploy", "", ...checks, "");
	const rollbacks = rows.filter((s) => s.rollback);
	if (rollbacks.length) lines.push("## Rollbacks", "", ...rollbacks.map((s) => `- ${s.title}: ${s.rollback!.title}${s.rollback!.ready ? "" : " (NOT READY)"}; ${s.rollback!.automatic ? "runs by itself when its checks after the deploy fail" : `needs Daniel's words${s.rollback!.words ? ` (${s.rollback!.words})` : ""}`}${s.rollback!.ran ? `; ran: ${s.rollback!.ran.result}` : ""}`), "");
	lines.push("## The go", "", ...(goEntries.length ? goEntries.map((e) => `- ${e.at}: "${e.words}" covering ${e.parts.join(", ")}${e.stub ? " (stub)" : ""}`) : ["- none recorded"]), "");
	return `${lines.join("\n")}\n`;
}
