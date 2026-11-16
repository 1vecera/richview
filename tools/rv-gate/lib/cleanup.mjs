// Disk housekeeping for gate runs (S27-725). A run's temp folders go when it ends, pass or fail,
// and the evidence store keeps run folders only for the latest run per open PR. Receipts, stored
// captures, baselines and the change log always stay.

import { mkdtempSync, readFileSync, readdirSync, rmSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

/**
 * Where run temp folders live. Steps get theirs as `TMPDIR`, and macOS caps a socket path at 103
 * bytes: Chromium's singleton socket under the 48-byte `/var/folders/…/T` would not fit, so on
 * macOS the folders live in `/tmp`.
 */
export const TEMP_ROOT = process.platform === "darwin" ? "/tmp" : tmpdir();

/**
 * A fresh temp folder for one run. Steps get it as `TMPDIR`, so the temp files of the tests they
 * start go with it.
 */
export function runTemp(root = TEMP_ROOT) {
	return mkdtempSync(join(root, "rvg-"));
}

/**
 * Delete run temp folders older than a day: a run stopped by a signal skips its own cleanup.
 * Younger ones may belong to runs still going in other checkouts.
 */
export function sweepRunTemps(root = TEMP_ROOT, now = Date.now()) {
	for (const name of entries(root)) {
		if (!name.startsWith("rvg-")) continue;
		const path = join(root, name);
		try {
			if (now - statSync(path).mtimeMs > 24 * 3600_000) removeTree(path);
		} catch {
			// Gone already, or not ours to read.
		}
	}
}

export function removeTree(path) {
	rmSync(path, { recursive: true, force: true, maxRetries: 3 });
}

function entries(dir) {
	try {
		return readdirSync(dir);
	} catch {
		return [];
	}
}

/** Caches outlive runs; everything else a run writes under `tmp/` is that run's output. */
const CACHE = /cache/;

/** The checkout's `tmp/` entries and those of `tmp/verification`, where the suites write. */
export function tmpSnapshot(cwd) {
	const tmp = join(cwd, "tmp");
	return new Set([...entries(tmp), ...entries(join(tmp, "verification")).map((name) => `verification/${name}`)]);
}

/**
 * Delete what runs left under the checkout's `tmp/`, caches excepted. With `before`, only the
 * entries a run added since that snapshot go, so an author's own suite output stays; without it
 * the gate owns the checkout and every entry goes. Returns how many entries were deleted.
 */
export function cleanCheckoutTmp(cwd, before) {
	let removed = 0;
	for (const name of tmpSnapshot(cwd)) {
		if (CACHE.test(name.split("/")[0]) || before?.has(name)) continue;
		removeTree(join(cwd, "tmp", name));
		removed++;
	}
	return removed;
}

/** How many failed full and prod runs per kind keep their logs, and for how long every failed one does (reviewer, 1 Nov). */
export const FAILED_RUNS_KEPT = 10;
export const FAILED_RUN_HOURS = 48;

const failedRun = (receipt) => ["full", "prod"].includes(receipt?.kind) && (receipt.result === "fail" || receipt.result?.status === "fail");

/**
 * The failed full and prod runs whose evidence stays so a failure can be told from a flake after the
 * fact: the latest ten of each kind, and every one from the last 48 hours. Only their logs and the folders
 * of their failed steps stay once a newer run exists (`trimFailedRun`); a full run folder holds gigabytes of captures.
 */
export function failedRunsKept({ runs, receipts, now = Date.now() }) {
	const kept = new Set(), byKind = new Map();
	for (const id of [...runs].sort()) {
		const receipt = receipts.get(id);
		if (!failedRun(receipt)) continue;
		byKind.set(receipt.kind, [...(byKind.get(receipt.kind) ?? []), id]);
		if (now - Date.parse(receipt.at) < FAILED_RUN_HOURS * 3600_000) kept.add(id);
	}
	for (const ids of byKind.values()) for (const id of ids.slice(-FAILED_RUNS_KEPT)) kept.add(id);
	return kept;
}

/**
 * Remove the heavy artifact folders of a kept failed run, leaving its files, `logs`, `attempts` and the
 * folder of every step that failed. Returns how many entries went; running it again removes nothing.
 */
export function trimFailedRun(dir, receipt) {
	const failedSteps = new Set((receipt.steps ?? []).filter((step) => step.status === "fail").map((step) => step.id));
	let removed = 0;
	for (const entry of readdirSync(dir, { withFileTypes: true })) {
		if (!entry.isDirectory() || ["logs", "attempts"].includes(entry.name) || failedSteps.has(entry.name)) continue;
		rmSync(join(dir, entry.name), { recursive: true, force: true });
		removed++;
	}
	return removed;
}

/**
 * The evidence run folders to delete. A dev run stays while its PR is open and it is that PR's
 * latest run. The latest full run and the latest prod run also stay: no PR owns them, and their
 * logs explain a failure on dev or on a candidate. A folder without a receipt is left from an
 * interrupted run and goes, unless it is `current`, the run that is ending now, or `active(id)` says
 * its run is still going: a run without a receipt yet is never pruned.
 *
 * The failed full and prod runs of `failedRunsKept` stay too. `runs` are folder names (receipt IDs, which
 * sort by time within a kind), `receipts` maps IDs to receipts and `openPrs` holds the open PR numbers.
 */
export function runsToPrune({ runs, receipts, openPrs, current, active = () => false, now = Date.now() }) {
	const latest = new Map();
	const passing = new Map();
	for (const id of [...runs].sort()) {
		const receipt = receipts.get(id);
		if (!receipt) continue;
		const owner = receipt.kind === "dev" ? (openPrs.has(receipt.pr) ? `pr ${receipt.pr}` : undefined) : receipt.kind;
		if (!owner) continue;
		latest.set(owner, id);
		// The newest passing run of each kind is the evidence a release or candidate stands on (DEL-21).
		if (receipt.result === "pass" || receipt.result?.status === "pass") passing.set(owner, id);
	}
	const keep = new Set([...latest.values(), ...passing.values(), ...failedRunsKept({ runs, receipts, now })]);
	return runs.filter((id) => !keep.has(id) && !(!receipts.has(id) && (id === current || active(id))));
}

const processAlive = (pid) => {
	try {
		process.kill(pid, 0);
		return true;
	} catch (error) {
		return error.code === "EPERM";
	}
};

/**
 * Whether the run in `dir` may still be going: its owner process is alive, or, without an owner
 * file, the folder changed within a day (the age `sweepRunTemps` uses).
 */
export function runActive(dir, now = Date.now()) {
	try {
		const owner = JSON.parse(readFileSync(join(dir, "owner.json"), "utf8"));
		if (Number.isInteger(owner.pid)) return processAlive(owner.pid);
	} catch {
		// No owner file: fall back to the age.
	}
	try {
		return now - statSync(dir).mtimeMs < 24 * 3600_000;
	} catch {
		return false;
	}
}
