// Fast-gate pass records for the pre-push hook (review 1k should-fix 2, review 1l should-fix 2).
// One passing `rv-gate fast` run covers a stack: run at HEAD over a clean tree, it records every
// commit in merge-base(HEAD, origin/dev)..HEAD and records HEAD as a tested tip. The hook allows a
// push to `dev` when every commit it carries, merges included, has a record and the pushed tip is a
// tested tip. So a prefix of the stack, a commit amended or dropped after the run, and a merge with
// lines of its own each need a run of their own.
//
// A commit's record is the patch id of its diff against its first parent, computed with
// `--verbatim`, so a whitespace-only amend changes the id and loses the record. A tip's record is
// its tree and the patch id of the stack's whole diff from the merge base. A rebase that changes no
// line of the stack keeps that id (patch ids ignore hunk positions), so a tested stack keeps its
// record across the rebase onto `origin/dev`, while dropping a commit changes it.
//
// A run does not establish that each commit below the tip builds alone (review 1l found two that
// did not). Each record says whether its commit was the HEAD of a run, and `rv-gate release` lists
// the batch's commits that were only ever below a tip.

import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { gateHome } from "./store.mts";

/** The folder of pass records: one file per commit patch id, `tip-<id>` and `tree-<sha>` per tip. */
export const fastPassDir = () => join(gateHome(), "fast-pass");

/**
 * The command that prints the diff of commit `$1` against its first parent, for
 * `git patch-id --verbatim`. The pre-push hook embeds the same text (lib/hooks.mts), so the record
 * and the check cannot drift apart. The options pin what a user's git config could vary.
 */
export const PATCH_ID_SHOW = "git show --no-color --no-ext-diff --no-textconv --no-renames --first-parent -m --format=";

/** The command that prints a stack's whole diff, from `$1` (its merge base) to `$2` (its tip). */
export const STACK_DIFF = "git diff --no-color --no-ext-diff --no-textconv --no-renames";

/** The patch id of what `script` prints: "" for no change, undefined when git fails. */
function patchIdOf(cwd: string, script: string, ...args: string[]) {
	const shown = spawnSync("sh", ["-c", script, "sh", ...args], { cwd, encoding: "utf8", maxBuffer: 1 << 30 });
	if (shown.status !== 0) return undefined;
	const id = spawnSync("git", ["patch-id", "--verbatim"], { cwd, input: shown.stdout, encoding: "utf8", maxBuffer: 1 << 30 });
	if (id.status !== 0) return undefined;
	return id.stdout.trim().split(/\s+/)[0] ?? "";
}

/**
 * The patch id of commit `sha`: "" for a commit that changes nothing, undefined when git fails
 * (callers fail closed on undefined).
 */
export const patchId = (cwd: string, sha: string) => patchIdOf(cwd, `${PATCH_ID_SHOW} "$1"`, sha);

/** The patch id of the whole diff from `base` to `tip`, with the same "" and undefined. */
export const stackId = (cwd: string, base: string, tip: string) => patchIdOf(cwd, `${STACK_DIFF} "$1" "$2"`, base, tip);

/** Whether a passing fast run recorded `id`. */
export const hasRecord = (id: string) => existsSync(join(fastPassDir(), id));

/** Whether the record of `id` comes from a run with that commit at HEAD (records before the stack rule were all HEAD). */
const builtAlone = (id: string) => !readFileSync(join(fastPassDir(), id), "utf8").startsWith("stack ");

function git(cwd: string, ...args: string[]) {
	const out = spawnSync("git", args, { cwd, encoding: "utf8" });
	return out.status === 0 ? out.stdout.trim() : undefined;
}

/**
 * Record a fast pass for the stack at HEAD of `cwd`: every commit from its merge base with
 * `upstream` to HEAD, and HEAD as a tested tip. It records only when the working tree equals HEAD:
 * a pass over uncommitted edits says nothing about the commits, and an untracked file that is not
 * ignored may be a file the run needed. Returns `{ head, commits }` (the tip and the number of
 * commits recorded), or `{ refused }` saying why nothing was recorded. Every id is computed before
 * any is written, so a git failure records nothing.
 */
export function recordFastPass(cwd: string, upstream = "refs/remotes/origin/dev") {
	const status = spawnSync("git", ["status", "--porcelain", "--untracked-files=normal"], { cwd, encoding: "utf8" });
	if (status.status !== 0) return { refused: "git status failed" };
	if (status.stdout.trim()) return { refused: "the working tree has uncommitted or untracked changes; commit them, then run fast again" };
	const head = git(cwd, "rev-parse", "HEAD");
	const tree = git(cwd, "rev-parse", "HEAD^{tree}");
	const base = git(cwd, "merge-base", "HEAD", upstream);
	if (!head || !tree) return { refused: "git rev-parse failed" };
	if (!base) return { refused: `HEAD has no merge base with ${upstream}; run 'git fetch origin dev', then fast again` };
	const listed = git(cwd, "rev-list", `${base}..HEAD`);
	if (listed === undefined) return { refused: "git rev-list failed" };
	const ids = listed.split("\n").filter(Boolean).map((sha) => ({ sha, id: patchId(cwd, sha) }));
	const stack = stackId(cwd, base, head);
	if (stack === undefined || ids.some(({ id }) => id === undefined)) return { refused: "git could not compute a patch id" };
	const dir = fastPassDir();
	mkdirSync(dir, { recursive: true });
	for (const { sha, id } of ids) {
		if (!id) continue;
		if (sha === head) writeFileSync(join(dir, id), `head ${sha}\n`);
		else if (!hasRecord(id) || !builtAlone(id)) writeFileSync(join(dir, id), `stack ${sha} ${head}\n`);
	}
	if (stack) writeFileSync(join(dir, `tip-${stack}`), `${head}\n`);
	writeFileSync(join(dir, `tree-${tree}`), `${head}\n`);
	return { head, commits: ids.filter(({ id }) => id).length };
}

/**
 * The commits of `shas` that have no record, as `{ commit, reason }`, and those recorded only below
 * a run's tip, as `{ commit }` (never built alone). `rv-gate release` reports both for a batch, so a
 * PR merged on the forge without the hook, and a stack commit that may not build alone, are visible.
 */
export function unrecordedCommits(cwd: string, shas: readonly string[]) {
	const missing = [];
	const belowTip = [];
	for (const commit of shas) {
		const id = patchId(cwd, commit);
		if (id === undefined) missing.push({ commit, reason: "patch id failed" });
		else if (id && !hasRecord(id)) missing.push({ commit, reason: "no record" });
		else if (id && !builtAlone(id)) belowTip.push({ commit });
	}
	return { missing, belowTip };
}
