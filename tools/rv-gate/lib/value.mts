// Host reason (ARC-03): read immutable gate receipts and write DEL-82's advisory value ledger.
import {mkdirSync, readFileSync, writeFileSync, renameSync, rmSync} from "node:fs";
import type {PathLike} from "node:fs";
import type {StoredReceipt} from "./store.mts";
import type {ArtifactStep, PhasePolicy, PhaseRules} from "./phase.mts";
import {join} from "node:path";
import {createHash, randomBytes} from "node:crypto";
import {evidenceDir, listReceipts} from "./store.mts";
import {median, qualifiesForRotation, requiredArtifacts, planRotation} from "./phase.mts";
import {receiptStepTime} from "./step-time.mts";

type ReceiptStep = NonNullable<StoredReceipt["steps"]>[number];
type Observation = Omit<ReceiptStep, "runMs" | "queuedMs"> & ReturnType<typeof receiptStepTime> & {receipt: string};
interface ValueOptions { since?: string; steps?: readonly ArtifactStep[]; now?: Date; readLog?: (path: string) => unknown }

const PARTNERS: Record<string, string> = {webkit: "chromium", firefox: "chromium", exports: "contract"};
const readLogFile = (path: string) => readFileSync(path, "utf8");
const digest = (value: unknown) => createHash("sha256").update(JSON.stringify(value)).digest("hex");
const validTime = (value: unknown): value is number => typeof value === "number" && Number.isFinite(value) && value >= 0;
const flaky = (step: ReceiptStep) => step.status === "pass" && (step.flaky === true
	|| Array.isArray(step.attempts) && step.attempts[0]?.status === "fail" && step.attempts.at(-1)?.status === "pass");

function selectReceipts(receipts: readonly StoredReceipt[], since?: string) {
	if (since !== undefined && (!/^\d{4}-\d{2}-\d{2}$/.test(since) || !Number.isFinite(Date.parse(since))
		|| new Date(since).toISOString().slice(0, 10) !== since)) throw new Error("--since must be an ISO calendar date (YYYY-MM-DD)");
	const byId = new Map<string, {receipt: StoredReceipt; sha256: string}>();
	for (const receipt of receipts) {
		if (!["full", "prod"].includes(receipt.kind)) continue;
		if (typeof receipt.id !== "string" || !receipt.id || typeof receipt.at !== "string" || !Number.isFinite(Date.parse(receipt.at)))
			throw new Error("Full/prod receipts need an id and timestamp");
		if (since && Date.parse(receipt.at) < Date.parse(since)) continue;
		const sha256 = digest(receipt), prior = byId.get(receipt.id);
		if (prior && prior.sha256 !== sha256) throw new Error(`Conflicting immutable receipt: ${receipt.id}`);
		byId.set(receipt.id, {receipt, sha256});
	}
	return [...byId.values()].sort((a, b) => Date.parse(a.receipt.at) - Date.parse(b.receipt.at) || a.receipt.id.localeCompare(b.receipt.id));
}

/** Count final step verdicts once and separate measured execution from receipt elapsed time. */
export function valueLedger(receipts: readonly StoredReceipt[], {since, steps = [], now = new Date(), readLog = readLogFile}: ValueOptions = {}) {
	const selected = selectReceipts(receipts, since), protectedIds = requiredArtifacts(steps), byId = new Map<string, {id: string; observations: Observation[]; unrun: number; reused: number; soleFailures: number}>();
	const conditionalFailures: Record<string, {step: string; given: string; runs: number; fail: number; flaky: number; receipts: string[]}> = {};
	for (const {receipt} of selected) {
		const results = receipt.steps ?? [];
		if (!Array.isArray(results) || new Set(results.map(step => step.id)).size !== results.length)
			throw new Error(`Invalid or duplicate steps in receipt ${receipt.id}`);
		const failed = results.filter(step => step.status === "fail" && !step.reused);
		for (const step of results) {
			if (typeof step.id !== "string" || !step.id || !["pass", "fail", "unrun"].includes(step.status))
				throw new Error(`Unknown step verdict in ${receipt.id}: ${step.id}`);
			if (!byId.has(step.id)) byId.set(step.id, {id: step.id, observations: [], unrun: 0, reused: 0, soleFailures: 0});
			const row = byId.get(step.id)!;
			if (step.reused) { row.reused++; continue; }
			if (step.status === "unrun") { row.unrun++; continue; }
			row.observations.push({...step, ...receiptStepTime(step, receipt, {readLog}), flaky: flaky(step), receipt: receipt.id});
			if (step.status === "fail" && failed.length === 1) row.soleFailures++;
			const partner = PARTNERS[step.id];
			if (partner && results.some(result => result.id === partner && result.status === "pass" && !result.reused)) {
				const key = `${step.id}|${partner}-pass`;
				const record = conditionalFailures[key] ??= {step: step.id, given: partner, runs: 0, fail: 0, flaky: 0, receipts: []};
				record.runs++; record.fail += Number(step.status === "fail"); record.flaky += Number(flaky(step)); record.receipts.push(receipt.id);
			}
		}
	}
	const rows = [...byId.values()].sort((a, b) => a.id.localeCompare(b.id)).map(({observations, ...row}) => {
		const pass = observations.filter((step: { status: string; }) => step.status === "pass").length, fail = observations.length - pass;
		const recent = observations.slice(-20), timings = recent.filter(step => validTime(step.runMs));
		const medianMs = median(timings.map(step => step.runMs!)), protectedStep = protectedIds.has(row.id);
		// Promotion and historical audit must survive the run-log janitor. Footer lower bounds
		// remain useful advice; only timings persisted in immutable receipts authorize a gap.
		const immutableTimingSamples = timings.filter((step: { source: string; }) => ["telemetry", "serial-duration"].includes(step.source)).length;
		const rotationEligible = !protectedStep && immutableTimingSamples === 20 && qualifiesForRotation(observations);
		const recommendation = rotationEligible ? "rotate" : "keep";
		return {...row, runs: observations.length, pass, fail, flaky: observations.filter(step => step.flaky).length,
			medianMs, timingSamples: timings.length, immutableTimingSamples, unknownTimings: recent.length - timings.length,
			medianWallMs: median(recent.map(step => step.durationMs).filter(validTime)),
			medianQueuedMs: median(recent.map(step => step.queuedMs).filter(validTime)),
			timingSources: Object.fromEntries([...new Set(recent.map(step => step.source))].sort().map(source => [source, recent.filter(step => step.source === source).length])),
			protected: protectedStep, rotationEligible, recommendation, recentReceipts: recent.map(step => step.receipt)};
	});
	return {version: 1, at: now.toISOString(), since: since ?? null, receipts: selected.map(({receipt, sha256}) => ({id: receipt.id, sha256})),
		receiptHash: "SHA-256 of JSON.stringify(parsed receipt); source receipt files are never changed.",
		steps: rows, conditionalFailures,
		counts: "Runs count executed pass/fail results once per full/prod receipt; unrun and reused results are separate. Fail and sole failures use final verdicts. Flaky means a final pass after a failed attempt. Conditional denominators require an executed partner final pass and an executed subject step.",
		timing: "Median uses known execution times from the last 20 executed observations: explicit runMs, serial receipt duration, or a maximum command-log lifetime lower bound. Sources and unknown samples remain visible; parallel receipt wall time is never active cost and log-derived queue time stays unknown. Rotation requires all 20 timings to persist in immutable receipts (telemetry or explicitly serial duration), so historical audit survives command-log pruning. Retained log lower bounds remain advisory.",
		retirement: "Recommendations only; the lead approves rotation list changes and Daniel decides retirement. Candidate-to-retire advice is withheld pending the lead’s policy ruling."};
}

/**
 * The receipts a rotation plan reads: none while no step is on the approved list. A run with nothing to
 * rotate then never parses the store, so a malformed receipt cannot stop it before its first step.
 */
export function rotationReceipts(policy: PhasePolicy, read: () => StoredReceipt[]) {
	return policy.rotation.length ? read() : [];
}

/** Derive admission and its receipt bindings from the same observed history as the value ledger. */
export function rotationFromHistory(policy: PhaseRules, {mode, steps, receipts}: {mode: string; steps: readonly ArtifactStep[]; receipts: readonly StoredReceipt[]}) {
	const value = valueLedger(receipts, {steps});
	const originals = new Map(receipts.filter((receipt: { kind: string; }) => ["full", "prod"].includes(receipt.kind)).map(receipt => [receipt.id, receipt]));
	const unique = value.receipts.map(receipt => originals.get(receipt.id)!);
	const plan = planRotation(policy, {mode, steps, receipts: unique, eligible: value.steps.filter(row => row.rotationEligible).map(row => row.id)});
	const hashes = new Map(value.receipts.map(receipt => [receipt.id, receipt.sha256]));
	return {...plan, qualifications: plan.eligible.map(id => {
		const row = value.steps.find(row => row.id === id)!;
		return {id, receipts: row.recentReceipts, receiptHashes: row.recentReceipts.map(id => hashes.get(id)), medianRunMs: row.medianMs, timingSamples: row.timingSamples};
	})};
}

const cell = (value: string) => String(value).replaceAll("|", "\\|").replace(/[\r\n]+/g, " ");

/** Render the ledger's counts, timing provenance and conditional denominators for review. */
export function valueMarkdown(ledger: ReturnType<typeof valueLedger>) {
	const lines = ["# Gate value ledger", "", `${ledger.receipts.length} full/prod receipts${ledger.since ? ` since ${ledger.since}` : ""}. ${ledger.counts}`, "", ledger.timing, "",
		"| Step | Runs | Pass | Fail | Flaky | Sole failures | Median run s | Known/last 20 | Timing sources | Recommendation |",
		"| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |"];
	for (const row of ledger.steps) lines.push(`| ${cell(row.id)} | ${row.runs} | ${row.pass} | ${row.fail} | ${row.flaky} | ${row.soleFailures} | ${row.medianMs === null ? "unknown" : (row.medianMs / 1000).toFixed(3)} | ${row.timingSamples}/${row.timingSamples + row.unknownTimings} | ${cell(Object.entries(row.timingSources).map(([source, count]) => `${source}: ${count}`).join(", "))} | ${row.recommendation} |`);
	lines.push("", "| Conditional failure | Runs with partner pass | Fail | Flaky |", "| --- | ---: | ---: | ---: |");
	for (const [key, row] of Object.entries(ledger.conditionalFailures)) lines.push(`| ${cell(key)} | ${row.runs} | ${row.fail} | ${row.flaky} |`);
	lines.push("", ledger.retirement, ""); return lines.join("\n");
}

/** Write advisory JSON and Markdown outputs without modifying the input receipts or verdicts. */
export function writeValue({since, steps, receipts = listReceipts(), dir = evidenceDir()}: ValueOptions & {receipts?: readonly StoredReceipt[]; dir?: string} = {}) {
	const ledger = valueLedger(receipts, {since, steps}); mkdirSync(dir, {recursive: true});
	const json = join(dir, "value.json"), markdown = join(dir, "value.md");
	const replace = (path: PathLike, bytes: string|NodeJS.ArrayBufferView<ArrayBufferLike>) => {
		const temp = `${path}.${process.pid}-${randomBytes(6).toString("hex")}.tmp`;
		try { writeFileSync(temp, bytes, {flag: "wx"}); renameSync(temp, path); }
		finally { rmSync(temp, {force: true}); }
	};
	replace(json, JSON.stringify(ledger, null, 2) + "\n"); replace(markdown, valueMarkdown(ledger));
	return {ledger, json, markdown};
}
