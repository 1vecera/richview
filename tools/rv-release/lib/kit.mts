// Host reason (ARC-03): runs the release kit's steps (steps/run) as the operator would, and replays recorded steps for a dry run.
import { spawnSync } from "node:child_process";
import { appendFileSync, existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

import type { ReleaseDescriptor } from "./descriptor.mts";
import type { Runner } from "./run.mts";

export const KIT_DIR = new URL("../steps/", import.meta.url).pathname;

/** The environment the kit needs: the candidate, its descriptor and the release folder. */
export const kitEnvironment = (desc: ReleaseDescriptor, base = process.env) => ({ ...base, RV_RELEASE_FINAL: desc.final, RV_RELEASE_DESCRIPTOR: desc.file, RV_RELEASE_DIR: desc.dir });

/** Run one step through steps/run with the operator's terminal, so prompts typed by a person still work. */
export function kitRunner({ desc, kit = KIT_DIR, spawn = spawnSync, env = kitEnvironment(desc) }: {desc: ReleaseDescriptor; kit?: string; spawn?: typeof spawnSync; env?: NodeJS.ProcessEnv}): Runner {
	return {
		step({ step, args }) {
			const result = spawn("bash", [join(kit, "run"), step, ...args], { stdio: "inherit", env });
			return { code: result.status ?? (result.signal ? 128 : 1) };
		},
	};
}

/**
 * Split a recorded journal into the blocks each step saved: every `ok-<step>` line closes the keys saved since the previous one.
 * A step that ran twice (the Windows walk, for the site and then the public archive) has two blocks, in order.
 */
export function replayBlocks(text: string) {
	const blocks = new Map<string, string[][]>(), pending: string[] = [];
	for (const line of text.split("\n")) {
		if (!line.includes("\t")) continue;
		pending.push(line);
		const key = line.slice(0, line.indexOf("\t"));
		if (key.startsWith("ok-")) {
			const step = key.slice(3);
			blocks.set(step, [...(blocks.get(step) ?? []), pending.splice(0)]);
		}
	}
	return blocks;
}

// The evidence a step the record never ran would have saved, so the parts after it can bind it in a dry run. Marked as stubbed.
const STUB_KEYS: Record<string, string[]> = { "15-seed": ["SEED"], "19-kit": ["ASSETS_HASH", "NOTES_HASH"], "49-archive": ["H", "OLD_SHA256"], "41-deploy": ["NEW_WORKER"] };

/**
 * A runner that executes nothing. For each step it appends what the recorded release saved for that step to the dry run's journal, or
 * only the `ok-` line when the record never ran it, so the parts after it see the journal they would have seen.
 */
export function stubRunner({ journalFile, recorded = "", log = () => {} }: {journalFile: string; recorded?: string; log?: (line: string) => void}): Runner {
	const blocks = replayBlocks(recorded);
	return {
		step({ step, args }) {
			const block = blocks.get(step)?.shift();
			const lines = block ?? [...(STUB_KEYS[step] ?? []).map((key) => `${key}\tstub-${key}`), `ok-${step}\tyes`];
			appendFileSync(journalFile, `${lines.join("\n")}\n`);
			log(`STUB ${step} ${args.join(" ")}`.trimEnd() + (block ? ` (replays ${block.length - 1} recorded keys)` : ` (not in the record: ok${STUB_KEYS[step] ? `, stub ${STUB_KEYS[step].join(", ")}` : " only"})`));
			return { code: 0 };
		},
	};
}

/** The Worker version a failed deploy left live, from the history the recovery step recorded; null when it recorded none. */
export function observedWorker(here: string) {
	const file = join(here, "worker-history-observed.json");
	if (!existsSync(file)) return null;
	const { from } = JSON.parse(readFileSync(file, "utf8")) as {from?: string};
	return /^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/.test(from ?? "") ? from! : null;
}
