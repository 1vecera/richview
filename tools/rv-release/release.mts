#!/usr/bin/env node
// Host reason (ARC-03): the one release command for a candidate (DEL-83): one go, a fixed order of parts, checks before the deploys, parts that stand alone.
//
//   node tools/rv-release/release.mjs plan   --release <release.json> [--parts windows,prod,site,hosted,public]
//   node tools/rv-release/release.mjs run    --release <release.json> --go "<Daniel's words and date>" [--parts ...]
//   node tools/rv-release/release.mjs run    --release <release.json> --stub --replay <recorded steps.tsv> --scratch <folder> [--go "<words>"]
//   node tools/rv-release/release.mjs report --release <release.json>
//   node tools/rv-release/release.mjs go-text --release <release.json> --part <part>      (the kit's outward steps call this)
//
// `run` needs Daniel's words (--go) before anything is spent or changed. It never reads or writes Cloudflare, AWS or GitHub itself: the kit's
// steps do, one at a time, as before. `--stub` runs the whole command with every step replaced by a replay of a recorded journal.
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { parseArgs } from "node:util";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";
import { loadDescriptor } from "./lib/descriptor.mts";
import { loadParts, selectParts, selectable, STAGES } from "./lib/parts.mts";
import { openJournal } from "./lib/journal.mts";
import { validateWords, verifiedGo } from "./lib/go.mts";
import { observedWorker, kitRunner, stubRunner, KIT_DIR } from "./lib/kit.mts";
import { runRelease } from "./lib/run.mts";
import { renderReport } from "./lib/report.mts";

import type { ReleaseDescriptor } from "./lib/descriptor.mts";
import type { Journal } from "./lib/journal.mts";
import type { PartsModel, Step, Builtin } from "./lib/parts.mts";
import type { PartState } from "./lib/run.mts";

const USAGE = `Usage: release.mjs <plan|run|report|go-text> --release <release.json> [--parts a,b] [--go "<words>"] [--stub --replay <steps.tsv> --scratch <dir>] [--part <id> [--stage prepare|deploy]] [--kit <dir>] [--repo <dir>]`;
function fail(message: string, code = 2): never { process.stderr.write(`${message}\n`); process.exit(code); };

/** The plan: every part in its fixed order with its stages, gates and rollback, as the lead reads it to Daniel. */
export function renderPlan({ model, desc, chosen, journal }: {model: PartsModel; desc: ReleaseDescriptor; chosen: string[]; journal?: Journal}) {
	const lines = [`Release ${desc.tag} of ${desc.candidate} ${desc.final} (full ${desc.full}); it replaces ${desc.oldCandidate} ${desc.old.slice(0, 10)}.`, `Parts chosen, in the fixed order: ${chosen.join(", ")}.`, ""];
	for (const part of model.parts) {
		if (!part.selectable && !part.always && !part.implicitFor?.some((id) => chosen.includes(id))) continue;
		if (part.selectable && !chosen.includes(part.id)) { lines.push(`- ${part.title}: left out`); continue; }
		const step = (s: Step | Builtin) => s.builtin ? `check ${s.builtin}` : `${s.step}${(s as Step).args.length ? ` ${(s as Step).args.join(" ")}` : ""}${(s as Step).outward ? ` [${(s as Step).outward}]` : ""}`;
		const stages = STAGES.filter((k) => part[k]?.length).map((k) => `${k}${(part.gated ?? []).includes(k) ? " (needs the go)" : ""}: ${part[k]!.map(step).join(", ")}`);
		const prior = journal?.readPart(part.id);
		lines.push(`- ${part.title}${part.needs.length ? ` (needs ${part.needs.join(", ")})` : ""}${prior?.status === "passed" ? " — passed in an earlier run" : ""}`, ...stages.map((s) => `    ${s}`));
		if (part.rollback) lines.push(`    rollback: ${part.rollback.title}${part.rollback.automatic ? ` — runs by itself if the checks after the deploy fail (${part.rollback.basis})` : part.rollback.command ? ` — needs Daniel's words (${part.rollback.words ?? "recovery"}): ${part.rollback.command}` : ""}`);
	}
	return lines.join("\n");
}

async function main() {
	const [command, ...argv] = process.argv.slice(2);
	if (!command || command === "--help" || command === "-h") fail(USAGE, command ? 0 : 2);
	const { values } = parseArgs({ args: argv, options: { release: { type: "string" }, parts: { type: "string" }, go: { type: "string" }, stub: { type: "boolean" }, replay: { type: "string" }, scratch: { type: "string" }, part: { type: "string" }, stage: { type: "string" }, kit: { type: "string" }, repo: { type: "string" } }, allowPositionals: false });
	if (!values.release) fail(`--release is required.\n${USAGE}`);
	const desc = loadDescriptor(values.release), model = loadParts();
	// The repository the release folder lives in (the kit derives its ROOT the same way): its deploy worktrees and kept builds are there.
	const repo = resolve(values.repo ?? process.env.RV_RELEASE_REPO ?? execFileSync("git", ["-C", desc.dir, "rev-parse", "--show-toplevel"], { encoding: "utf8" }).trim());
	let here = desc.journal;
	if (values.stub) {
		if (!values.scratch) fail("--stub needs --scratch <folder> for the dry run's own journal.");
		here = join(resolve(values.scratch), `go-record-${desc.final.slice(0, 8)}`);
		mkdirSync(here, { recursive: true, mode: 0o700 });
		if (!existsSync(join(here, "steps.tsv"))) writeFileSync(join(here, "steps.tsv"), `FINAL\t${desc.final}\n`);
	}
	const journal = openJournal(here);
	const passedBefore = new Set(model.parts.filter((p) => journal.readPart(p.id)?.status === "passed").map((p) => p.id));

	if (command === "go-text") {
		if (!values.part) fail("--part is required.");
		try { process.stdout.write(`${verifiedGo(journal, values.part, desc, { stubbed: !!values.stub, stage: values.stage ?? "deploy" })}\n`); } catch (error) { fail((error as Error).message, 1); }
		return;
	}
	if (command === "report") {
		const states = new Map(model.parts.map((p) => [p.id, journal.readPart(p.id)] as const).filter((entry): entry is readonly [string, PartState] => Boolean(entry[1])));
		process.stdout.write(renderReport({ desc, chosen: [...states.keys()], states, goEntries: journal.goEntries(), at: new Date().toISOString() }));
		return;
	}
	let chosen;
	try { chosen = selectParts(model, (values.parts ?? selectable(model).join(",")).split(",").map((s) => s.trim()).filter(Boolean), passedBefore); } catch (error) { fail((error as Error).message); }
	if (command === "plan") { process.stdout.write(`${renderPlan({ model, desc, chosen, journal })}\n`); return; }
	if (command !== "run") fail(USAGE);

	if (values.go === undefined && !values.stub) {
		process.stdout.write(`${renderPlan({ model, desc, chosen, journal })}\n\nNo go: nothing was run, spent or changed. Give Daniel's own words and their date with --go "<words>".\n`);
		process.exitCode = 3;
		return;
	}
	let words;
	try { words = validateWords(values.go ?? "Dry run: no go, every step stubbed (2026-01-01)"); } catch (error) { fail((error as Error).message); }
	const goName = journal.writeGo({ at: new Date().toISOString(), words, parts: chosen, ...(values.stub ? { stub: true } : {}) });
	process.stdout.write(`${renderPlan({ model, desc, chosen, journal })}\n\nGo recorded in ${goName}.\n\n`);
	const scratch = join(values.stub ? resolve(values.scratch!) : tmpdir(), "edge-scratch");
	const runner = values.stub
		? stubRunner({ journalFile: join(here, "steps.tsv"), recorded: values.replay ? readFileSync(values.replay, "utf8") : "", log: (line) => process.stdout.write(`${line}\n`) })
		: kitRunner({ desc, kit: values.kit ?? KIT_DIR });
	const states = await runRelease({ model, desc, chosen, journal, runner, ctx: { desc, journal, repo, scratch, newWorker: () => observedWorker(journal.here) }, log: (line) => process.stdout.write(`${line}\n`), stubbed: !!values.stub });
	const report = renderReport({ desc, chosen, states, goEntries: journal.goEntries(), stubbed: !!values.stub, at: new Date().toISOString() });
	writeFileSync(join(here, "report.md"), report);
	writeFileSync(join(here, "report.json"), `${JSON.stringify([...states.values()], null, 1)}\n`);
	process.stdout.write(`\n${report}Report: ${join(here, "report.md")}\n`);
	process.exitCode = chosen.every((id) => states.get(id)?.status === "passed") ? 0 : 1;
}

if (process.argv[1] && (resolve(process.argv[1]) === fileURLToPath(import.meta.url) ||
	resolve(process.argv[1]) === fileURLToPath(import.meta.url.replace(/\.mts$/, ".mjs")))) await main();
