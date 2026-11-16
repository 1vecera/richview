#!/usr/bin/env node
// The gate's `fresh-agent` step (DEL-78, INT-69, FEA-SF-65): runs fresh agents serially by default,
// three runs per product, against one RichView binary and writes the result and the packet
// appendix.
//
//   node tools/agent-use/gate.mjs --richview <binary> --out <folder> [--products claude,codex] [--runs 3]
//                                 [--claude-model sonnet] [--codex-model <m>] [--timeout-min 25]
//                                 [--jobs 1|2] [--unrun <product>:<reason>]...
//
// Writes `fresh-agent.json`, `appendix.md`, and the kept `transcripts/` and `excerpts/`. Every run
// is also appended to the score history. Exit 0 unless a run could not execute (see
// `exitDecision`); exit 1 then, exit 2 on bad arguments. Each run uses subscription capacity.

import { spawn, spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, realpathSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { setTimeout as sleep } from "node:timers/promises";
import { appendHistory, historyEntry, historyPath, readHistory } from "./history.mjs";
import { aggregate, excerptOf, exitDecision, failedRecord, findings, issueCandidates, RUNS_PER_PRODUCT, renderAppendix, runRecord, taskPlan } from "./lib/gate.mjs";
import { parseJsonl, readTranscript } from "./lib/transcript.mjs";
import { series } from "./report.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const PRODUCTS = ["claude", "codex"];

export function parseArgs(argv) {
	const args = { richview: null, out: null, products: PRODUCTS, runs: RUNS_PER_PRODUCT, claudeModel: "sonnet", codexModel: null, timeoutMin: null, unrun: [] };
	for (let i = 0; i < argv.length; i++) {
		const a = argv[i];
		const take = () => argv[++i];
		if (a === "--richview") args.richview = take();
		else if (a === "--out") args.out = take();
		else if (a === "--products") args.products = take().split(",").filter(Boolean);
		else if (a === "--runs") args.runs = Number(take());
		else if (a === "--claude-model") args.claudeModel = take();
		else if (a === "--codex-model") args.codexModel = take();
		else if (a === "--timeout-min") args.timeoutMin = Number(take());
		else if (a === "--jobs") args.jobs = Number(take());
		else if (a === "--unrun") { const [product, ...reason] = take().split(":"); args.unrun.push({ product, reason: reason.join(":").trim() }); }
		else throw new Error(`unknown argument ${a}`);
	}
	if (!args.richview || !args.out) throw new Error("--richview and --out are required");
	if (!args.products.length || args.products.some((p) => !PRODUCTS.includes(p))) throw new Error("--products takes claude, codex or both, comma separated");
	if (args.unrun.some((u) => !PRODUCTS.includes(u.product) || !u.reason || args.products.includes(u.product)))
		throw new Error("--unrun takes <product>:<reason> for a product that --products leaves out");
	if (!Number.isInteger(args.runs) || args.runs < 1) throw new Error("--runs takes a whole number of at least 1");
	if (args.jobs !== undefined && args.jobs !== 1 && args.jobs !== 2) throw new Error("--jobs takes 1 or 2");
	return args;
}

function listTasks() {
	return readdirSync(join(here, "tasks"), { withFileTypes: true })
		.filter((d) => d.isDirectory() && existsSync(join(here, "tasks", d.name, "task.json")))
		.map((d) => d.name)
		.sort();
}

function runOne({ product, task }, args, binary) {
	const argv = [join(here, "run.mjs"), "--product", product, "--task", task, "--richview", binary];
	const model = product === "claude" ? args.claudeModel : args.codexModel;
	if (model) argv.push("--model", model);
	if (args.timeoutMin) argv.push("--timeout-min", String(args.timeoutMin));
	console.error(`gate: ${product} on ${task}`);
	const child = spawnSync("node", argv, { encoding: "utf8", stdio: ["ignore", "pipe", "inherit"], maxBuffer: 64 * 1024 * 1024 });
	const runDir = child.stdout?.match(/"runDir":\s*"([^"]+)"/)?.[1];
	if (child.status !== 0 || !runDir || !existsSync(join(runDir, "score.json"))) return failedRecord(product, task, `run.mjs exited ${child.status ?? child.signal}${runDir ? ` (${runDir})` : ""}`);
	const meta = JSON.parse(readFileSync(join(runDir, "run.json"), "utf8"));
	return runRecord(meta, JSON.parse(readFileSync(join(runDir, "score.json"), "utf8")));
}

/** Keeps the transcript and one excerpt per confusing code outside the temporary run folder. */
function keep(record, out) {
	if (!record.runDir || !existsSync(join(record.runDir, "transcript.jsonl"))) return;
	mkdirSync(join(out, "transcripts"), { recursive: true });
	record.transcript = join(out, "transcripts", `${record.runId}.jsonl`);
	copyFileSync(join(record.runDir, "transcript.jsonl"), record.transcript);
	record.excerpts = {};
	const { calls } = readTranscript(parseJsonl(readFileSync(record.transcript, "utf8")), record.product);
	for (const c of record.confusions) {
		const text = excerptOf(calls, c.code);
		if (!text) continue;
		mkdirSync(join(out, "excerpts"), { recursive: true });
		record.excerpts[c.code] = join(out, "excerpts", `${record.runId}-${c.code}.md`);
		writeFileSync(record.excerpts[c.code], text);
	}
}

// The parallel path suppresses only the child's history append. The provider call, sandbox,
// scorer and raw files remain run.mjs's; the parent commits finished rows in plan order.
async function runParallel(plan, args, binary, out, byProduct) {
	const completed = new Array(plan.length), live = new Set(), errors = [];
	let ordered = 0, stopped, forced;
	const signalGroup = (item, signal) => {
		if (!item.child.pid) return;
		try { process.kill(process.platform === "win32" ? item.child.pid : -item.child.pid, signal); }
		catch (error) { if (!["ESRCH", "EPERM"].includes(error.code)) errors.push(error); }
	};
	const stop = name => {
		if (stopped) return;
		stopped = name;
		for (const item of live) signalGroup(item, "SIGTERM");
		forced = sleep(1000).then(() => { for (const item of live) signalGroup(item, "SIGKILL"); });
	};
	const handlers = ["SIGINT", "SIGTERM"].map(name => { const handler = () => stop(name); process.on(name, handler); return [name, handler]; });
	const one = step => new Promise(resolveRun => {
		const argv = [join(here, "run.mjs"), "--product", step.product, "--task", step.task, "--richview", binary, "--no-history"];
		const model = step.product === "claude" ? args.claudeModel : args.codexModel;
		if (model) argv.push("--model", model);
		if (args.timeoutMin) argv.push("--timeout-min", String(args.timeoutMin));
		console.error(`gate: ${step.product} on ${step.task}`);
		const child = spawn("node", argv, { detached: process.platform !== "win32", stdio: ["ignore", "pipe", "inherit"] });
		const item = { child }; live.add(item);
		const stdout = [];
		let stdoutBytes = 0, failure;
		child.once("spawn", () => { if (stopped) signalGroup(item, "SIGTERM"); });
		child.stdout.on("data", chunk => {
			if (failure) return;
			stdoutBytes += chunk.length;
			if (stdoutBytes <= 64 * 1024 * 1024) stdout.push(chunk);
			if (stdoutBytes > 64 * 1024 * 1024) { failure = "run.mjs stdout exceeded 64 MiB"; signalGroup(item, "SIGKILL"); }
		});
		child.once("error", error => { failure = error.message; });
		child.once("close", (code, signal) => {
			if (!stopped || !child.pid) live.delete(item);
			const runDir = Buffer.concat(stdout).toString("utf8").match(/"runDir":\s*"([^"]+)"/)?.[1];
			const failed = reason => ({ record: failedRecord(step.product, step.task, reason) });
			if (failure || code !== 0 || !runDir || !existsSync(join(runDir, "score.json"))) {
				resolveRun(failed(stopped ? `Interrupted by ${stopped}` : failure || `run.mjs exited ${code ?? signal}${runDir ? ` (${runDir})` : ""}`)); return;
			}
			try {
				const meta = JSON.parse(readFileSync(join(runDir, "run.json"), "utf8")), scored = JSON.parse(readFileSync(join(runDir, "score.json"), "utf8"));
				if (meta.product !== step.product || meta.task !== step.task || meta.model !== (model ?? null) || meta.binary !== binary) throw new Error("Run metadata does not match the dispatched product, task, model and binary");
				resolveRun({ record: runRecord(meta, scored), history: historyEntry(meta, scored) });
			} catch (error) { resolveRun(failed(`Invalid run artifacts in ${runDir}: ${error.message}`)); }
		});
	});
	const flush = (interrupted = false) => {
		while (ordered < completed.length) {
			if (!completed[ordered]) {
				if (!interrupted) break;
				ordered++; continue;
			}
			const { record, history } = completed[ordered++];
			try { if (history) appendHistory(history); keep(record, out); } catch (error) { errors.push(error); }
			byProduct.get(record.product).push(record);
			console.error(`gate: ${record.product} ${record.task}: ${record.executed ? `score ${record.score}, ${record.pass ? "pass" : "no pass"}` : `could not execute (${record.failure})`}`);
		}
	};
	try {
		// Each provider still runs one task at a time, with its original ordering and sign-in.
		// The two providers overlap; no account gains concurrent requests from this check.
		const products = [...new Set(plan.map(step => step.product))];
		const settled = await Promise.allSettled(products.map(async product => {
			for (const index of plan.map((step, index) => step.product === product ? index : -1).filter(index => index >= 0)) {
				if (stopped) break;
				try { completed[index] = await one(plan[index]); }
				catch (error) { completed[index] = { record: failedRecord(plan[index].product, plan[index].task, error.message) }; }
				flush();
			}
		}));
		if (stopped) {
			// No more calls can start: skip unstarted slots, retaining every scored peer in plan order.
			flush(true);
			await forced;
			const deadline = Date.now() + 5000;
			while (live.size) {
				for (const item of live) {
					try { process.kill(process.platform === "win32" ? item.child.pid : -item.child.pid, 0); }
					catch (error) { if (error.code === "ESRCH") live.delete(item); else if (error.code !== "EPERM") throw error; }
				}
				if (!live.size) break;
				if (Date.now() >= deadline) throw new Error("Fresh-agent workers did not drain after interruption");
				await sleep(25);
			}
			throw new Error(`Fresh-agent gate interrupted by ${stopped}`);
		}
		for (const result of settled) if (result.status === "rejected") errors.push(result.reason);
		if (errors.length) throw errors[0];
	} finally { for (const [name, handler] of handlers) process.removeListener(name, handler); }
}

async function main() {
	const args = parseArgs(process.argv.slice(2));
	const binary = realpathSync(resolve(args.richview));
	const out = resolve(args.out);
	mkdirSync(out, { recursive: true });
	const version = spawnSync(binary, ["--version"], { encoding: "utf8" }).stdout?.trim() || null;
	const plan = taskPlan(listTasks(), args.products, args.runs, readHistory().length);

	const byProduct = new Map(args.products.map((p) => [p, []]));
	const unique = new Set(plan.map(step => `${step.product}\0${step.task}`)).size === plan.length;
	if (args.jobs === 2 && unique && process.platform !== "win32") {
		console.error("gate: two products overlap; each product keeps its original serial task order");
		await runParallel(plan, args, binary, out, byProduct);
	}
	else for (const step of plan) {
		const record = runOne(step, args, binary);
		keep(record, out);
		byProduct.get(step.product).push(record);
		console.error(`gate: ${step.product} ${step.task}: ${record.executed ? `score ${record.score}, ${record.pass ? "pass" : "no pass"}` : `could not execute (${record.failure})`}`);
	}

	const products = args.products.map((p) => aggregate(p, byProduct.get(p)));
	const decision = exitDecision(products, args.runs);
	const history = readHistory();
	const found = findings(products, series(history));
	const candidates = issueCandidates(products, { excerptOf: (r, code) => r.excerpts?.[code] ?? null });
	const at = new Date().toISOString();
	const richview = { binary, version };
	writeFileSync(
		join(out, "fresh-agent.json"),
		`${JSON.stringify({ schema: 1, at, richview, history: historyPath(), requestedRuns: args.runs, unrunProducts: args.unrun, status: decision.status, reasons: decision.reasons, products, findings: found, issueCandidates: candidates }, null, 2)}\n`,
	);
	writeFileSync(join(out, "appendix.md"), renderAppendix({ at, richview, products, decision, findings: found, candidates, history, requestedRuns: args.runs, unrunProducts: args.unrun }));
	for (const p of products) console.log(`${p.product}: pass rate ${p.passed}/${p.runs}, scores ${p.scores.join(", ")}`);
	console.log(`fresh-agent: ${decision.status}${decision.reasons.length ? ` (${decision.reasons.join("; ")})` : ""}; wrote ${out}`);
	process.exit(decision.status === "pass" ? 0 : 1);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
	main().catch((e) => {
		console.error(e.message);
		process.exit(2);
	});
}
