// Host reason (ARC-03): fixtures for the release command's tests: a journal in a temporary folder and a runner that records its calls.
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { loadDescriptor } from "../lib/descriptor.mts";
import { loadParts } from "../lib/parts.mts";
import { openJournal } from "../lib/journal.mts";

import type { ReleaseDescriptor } from "../lib/descriptor.mts";
import type { CheckContext } from "../lib/predeploy.mts";
import type { Runner } from "../lib/run.mts";

export const FIXTURE = new URL("./fixtures/release-31.json", import.meta.url).pathname;
const hex = (c: string) => c.repeat(64);

/** What each step saves in the journal when it passes (the keys later bindings and rollbacks read). */
const SAVES: Record<string, (descriptor: ReleaseDescriptor) => [string, string][]> = {
	"00-bind-full": (d) => [["R", d.full], ["M", hex("a")]],
	"11-windows site": () => [["WINDOWS_site", "yes"]],
	"11-windows bonus": () => [["WINDOWS_bonus", "yes"]],
	"13-packet": () => [["PACKET", "/x/website-review.md"], ["PACKET_SHA", hex("b")]],
	"40-dry-run": (d) => [["DRY_SHA", hex("c")], ["BEFORE_WORKER", d.oldWorker]],
	"41-deploy": () => [["NEW_WORKER", "11111111-2222-3333-4444-555555555555"]],
	"49-archive": () => [["H", hex("d")], ["OLD_SHA256", hex("e")]],
	"15-seed": () => [["SEED", "f".repeat(40)]],
	"19-kit": () => [["ASSETS_HASH", hex("1")], ["NOTES_HASH", hex("2")]],
};

/** A release in a temporary folder, a runner that records `part:stage:step args`, and the knobs a test turns. */
export function harness({ failing = [], builtin = {}, go = true, chosen, journalKeys = [] }: {failing?: string[]; builtin?: CheckContext["builtin"]; go?: boolean; chosen?: string[]; journalKeys?: [string, string][]} = {}) {
	const dir = mkdtempSync(join(tmpdir(), "rvrel-"));
	const desc = { ...loadDescriptor(FIXTURE), dir, journal: join(dir, "go-record-2c220f59") };
	const journal = openJournal(desc.journal);
	journal.save("FINAL", desc.final);
	for (const [key, value] of journalKeys) journal.save(key, value);
	const model = loadParts(), calls: string[] = [], failed = new Set(failing);
	const all = ["windows", "prod", "site", "hosted", "public"];
	const parts = chosen ?? all;
	if (go) journal.writeGo({ at: "2026-11-26T12:00:00Z", words: "Daniel, 26 Nov: release candidate/33 everywhere", parts });
	const runner: Runner = {
		step({ step, args, part, stage }) {
			const name = `${part}:${stage}:${step}${args.length ? ` ${args.join(" ")}` : ""}`;
			calls.push(name);
			if (failed.has(step) || failed.has(`${step} ${args.join(" ")}`.trim())) return { code: 1 };
			for (const [key, value] of SAVES[`${step} ${args.join(" ")}`.trim()]?.(desc) ?? SAVES[step]?.(desc) ?? []) journal.save(key, value);
			journal.save(`ok-${step}`, "yes");
			return { code: 0 };
		},
	};
	const pass = async () => ({ status: "pass", detail: "stub" });
	const ctx = { desc, journal, repo: dir, scratch: join(dir, "scratch"), builtin: { "site-edge": pass, ...builtin }, newWorker: () => journal.get("NEW_WORKER") ?? null };
	const log: string[] = [];
	return { dir, desc, journal, model, calls, runner, ctx, parts, log: (line: string) => log.push(line), lines: log };
}
