// The evidence store (DEL-61): receipts, the dev change log and content-addressed captures,
// kept on the gate host outside every checkout. Receipts are written once and never edited
// (DEL-20); a later fact about a run, such as its merge, is a separate write-once record.

import { createHash, randomBytes } from "node:crypto";
import {
	appendFileSync,
	copyFileSync,
	existsSync,
	linkSync,
	mkdirSync,
	readFileSync,
	readdirSync,
	rmSync,
	statSync,
	writeFileSync,
} from "node:fs";
import { homedir } from "node:os";
import { basename, dirname, join } from "node:path";
import {readPhase, validatePhase} from "./phase.mts";
import type {Phase, PhasePolicy} from "./phase.mts";
import type {Retirement} from "./pr.mts";
import type {StepOutcome} from "./results.mts";

interface ReceiptIdentity { id: string; phase?: Phase; phasePolicy?: PhasePolicy }
export interface StoredReceipt extends ReceiptIdentity {
	kind: string;
	result: string | { status: string };
	commit: string;
	tree: string;
	at: string;
	pr?: number;
	parallel?: number;
	steps?: (Omit<StepOutcome, "reused"> & { id: string; reused?: string | boolean })[];
}
export interface ReceiptNote { kind: string; receipt: string; at: string; commit?: string; tree?: string }
export interface GateChange {
	at?: string;
	commit: string;
	receipt: string;
	pr?: number;
	title?: string;
	class?: string;
	cause?: { kind: string; detail: string };
	issue?: string;
	revert?: string;
	items?: string[];
	assumed?: string[];
	deletedTests?: Retirement[];
}
type Baseline = Record<string, string>;

/** Root of the gate's own state: its checkout, evidence store and locks. */
export function gateHome() {
	return process.env.RV_GATE_HOME || join(homedir(), ".local/share/richview-gate");
}

export function evidenceDir() {
	return join(gateHome(), "evidence");
}

export function ensureStore() {
	const root = evidenceDir();
	for (const dir of ["receipts", "runs", "captures", "baselines", "log"]) {
		mkdirSync(join(root, dir), { recursive: true });
	}
	return root;
}

/** A receipt ID sorts by time and names the gated commit. */
export function newReceiptId(kind: string, commit: string, now = new Date()) {
	const stamp = now.toISOString().replace(/[-:]/g, "").replace(/\.\d+Z$/, "Z");
	return `${kind}-${stamp}-${commit.slice(0, 10)}`;
}

/**
 * Write a JSON record once; refuses to overwrite (DEL-20). The bytes go to a hidden temp file first and
 * are linked to the final name, which fails with EEXIST when the record exists, so a reader never sees
 * a half-written record. The temp name starts with a dot, so no lister of the folder matches it.
 */
export function writeOnce(path: string, value: unknown) {
	const temp = join(dirname(path), `.${basename(path)}.${process.pid}-${randomBytes(6).toString("hex")}.tmp`);
	try {
		writeFileSync(temp, `${JSON.stringify(value, null, 2)}\n`, { flag: "wx" });
		linkSync(temp, path);
	} finally {
		rmSync(temp, { force: true });
	}
	return path;
}

export function receiptPath(id: string) {
	// Ids come from receipts and reuse records on disk; one with a separator would leave the store.
	if (!/^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(id) || id.includes("..")) throw new Error(`Not a receipt id: ${JSON.stringify(id)}`);
	return join(evidenceDir(), "receipts", `${id}.json`);
}

export function writeReceipt<T extends ReceiptIdentity>(receipt: T) {
	ensureStore();
	const phasePolicy = receipt.phasePolicy ? validatePhase(receipt.phasePolicy) : readPhase();
	const phase = receipt.phase ?? phasePolicy.phase;
	if (!["prelaunch", "launch"].includes(phase) || phasePolicy.phase !== phase) throw new Error("Receipt phase differs from its frozen policy");
	return writeOnce(receiptPath(receipt.id), {...receipt, phase, phasePolicy});
}

export function readReceipt<T extends StoredReceipt = StoredReceipt>(id: string): T {
	return JSON.parse(readFileSync(receiptPath(id), "utf8"));
}

/** Record a fact about an existing receipt (merge, revert, review verdict) without editing it. */
export function writeReceiptNote<T extends object>(id: string, kind: string, value: T) {
	ensureStore();
	const path = join(evidenceDir(), "receipts", `${id}.${kind}.json`);
	return writeOnce(path, { receipt: id, kind, at: new Date().toISOString(), ...value });
}

export function listReceipts<T extends StoredReceipt = StoredReceipt>(): T[] {
	const dir = join(evidenceDir(), "receipts");
	if (!existsSync(dir)) return [];
	return readdirSync(dir)
		.filter((name) => /^[a-z]+-\d{8}T\d{6}Z-[0-9a-f]+\.json$/.test(name))
		.sort()
		.map((name) => JSON.parse(readFileSync(join(dir, name), "utf8")));
}

export function receiptNotes<T extends ReceiptNote = ReceiptNote>(id: string): T[] {
	const dir = join(evidenceDir(), "receipts");
	if (!existsSync(dir)) return [];
	return readdirSync(dir)
		.filter((name) => name.startsWith(`${id}.`) && name !== `${id}.json`)
		.map((name) => JSON.parse(readFileSync(join(dir, name), "utf8")));
}

/** The directory for one run's logs and artifacts. */
export function runDir(id: string) {
	const dir = join(evidenceDir(), "runs", id);
	mkdirSync(dir, { recursive: true });
	// Cleanup reads this to tell a run in progress from an interrupted one.
	writeFileSync(join(dir, "owner.json"), JSON.stringify({ pid: process.pid, at: new Date() }));
	return dir;
}

/** Append one entry to the dev change log (DEL-70). Only the gate calls this. */
export function appendChange<T extends GateChange>(entry: T) {
	ensureStore();
	appendFileSync(join(evidenceDir(), "log", "changes.jsonl"), `${JSON.stringify(entry)}\n`);
}

export function readChanges(): GateChange[] {
	const path = join(evidenceDir(), "log", "changes.jsonl");
	if (!existsSync(path)) return [];
	return readFileSync(path, "utf8")
		.split("\n")
		.filter(Boolean)
		.map((line) => JSON.parse(line));
}

export function sha256File(path: string) {
	return createHash("sha256").update(readFileSync(path)).digest("hex");
}

/** Store a capture by content hash and return the hash; identical images are stored once (DEL-29). */
export function storeCapture(path: string) {
	const hash = sha256File(path);
	const dir = join(evidenceDir(), "captures", hash.slice(0, 2));
	const target = join(dir, `${hash}.png`);
	if (!existsSync(target)) {
		mkdirSync(dir, { recursive: true });
		copyFileSync(path, target);
	}
	return hash;
}

export function capturePath(hash: string) {
	return join(evidenceDir(), "captures", hash.slice(0, 2), `${hash}.png`);
}

/** A baseline is a manifest `{name: hash}` for `dev` or `prod` (DEL-29). */
export function readBaseline(name: string): Baseline | undefined {
	const path = join(evidenceDir(), "baselines", `${name}.json`);
	return existsSync(path) ? JSON.parse(readFileSync(path, "utf8")) : undefined;
}

/**
 * Replace a baseline, keeping the old one as `<name>.prev.json`. A manifest that is empty or lost
 * more than a tenth of the old one's captures is refused: a broken run must not become the
 * baseline every later run compares with. Returns `{ written, reason }`.
 */
export function writeBaseline(name: string, manifest: Baseline) {
	ensureStore();
	const path = join(evidenceDir(), "baselines", `${name}.json`);
	const refusal = baselineRefusal(readBaseline(name), manifest);
	if (refusal) return { written: false, reason: refusal };
	if (existsSync(path)) copyFileSync(path, join(evidenceDir(), "baselines", `${name}.prev.json`));
	writeFileSync(path, `${JSON.stringify(manifest, null, 2)}\n`);
	return { written: true };
}

export function baselineRefusal(before: Baseline | undefined, after: Baseline) {
	const names = Object.keys(after ?? {});
	if (!names.length) return "the new capture set is empty";
	if (!before) return undefined;
	const lost = Object.keys(before).filter((key) => !(key in after));
	if (lost.length > Object.keys(before).length / 10) return `${lost.length} of ${Object.keys(before).length} captures are missing (${lost.slice(0, 5).join(", ")})`;
	return undefined;
}

/** Simple exclusive lock with a stale-owner check; returns a release function. */
export function acquireLock(name: string, { wait = false, timeoutMs = 6 * 3600_000 } = {}) {
	const dir = join(gateHome(), "locks");
	mkdirSync(dir, { recursive: true });
	const path = join(dir, `${name}.lock`);
	const started = Date.now();
	for (;;) {
		try {
			mkdirSync(path);
			writeFileSync(join(path, "owner.json"), JSON.stringify({ pid: process.pid, at: new Date() }));
			return () => rmSync(path, { recursive: true, force: true });
		} catch (error) {
			if ((error as NodeJS.ErrnoException).code !== "EEXIST") throw error;
			let owner: { pid: number; at: string };
			try {
				owner = JSON.parse(readFileSync(join(path, "owner.json"), "utf8"));
			} catch {
				// The owner file may not exist yet; treat a lock older than a minute as stale.
				if (Date.now() - statSync(path).mtimeMs > 60_000) rmSync(path, { recursive: true, force: true });
				continue;
			}
			if (!processAlive(owner.pid)) {
				rmSync(path, { recursive: true, force: true });
				continue;
			}
			if (!wait || Date.now() - started > timeoutMs) {
				throw new Error(`Lock '${name}' is held by pid ${owner.pid} since ${owner.at}`);
			}
			Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 5_000);
		}
	}
}

function processAlive(pid: number) {
	try {
		process.kill(pid, 0);
		return true;
	} catch {
		return false;
	}
}

/**
 * The receipt whose run passed `stepId` on inputs with this key, if any (release reruns). The
 * record only points at a receipt: the receipt itself must be a release receipt that ran the step
 * to a pass at this key, not by reuse, so a stray record cannot turn a suite into a pass.
 */
export function reusePass(stepId: string, key: string) {
	const path = join(evidenceDir(), "reuse", stepId, `${key}.json`);
	if (!existsSync(path)) return undefined;
	try {
		const { receipt }: { receipt: string } = JSON.parse(readFileSync(path, "utf8"));
		return reuseSourceHolds(readReceipt(receipt), stepId, key) ? receipt : undefined;
	} catch {
		return undefined;
	}
}

export function reuseSourceHolds(receipt: StoredReceipt | undefined, stepId: string, key: string) {
	const step = receipt?.kind === "release" ? receipt.steps?.find((entry) => entry.id === stepId) : undefined;
	return Boolean(step && step.status === "pass" && !step.reused && step.reuseKey === key);
}

export function recordReusePass(stepId: string, key: string, receipt: string) {
	const dir = join(evidenceDir(), "reuse", stepId);
	mkdirSync(dir, { recursive: true });
	writeFileSync(join(dir, `${key}.json`), `${JSON.stringify({ receipt, at: new Date().toISOString() })}\n`);
}
