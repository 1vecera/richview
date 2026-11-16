// The pure parts of the fresh-agent gate step (DEL-78): task rotation, pass-rate aggregation,
// the exit rule, issue candidates and the packet appendix. Nothing here starts an agent, reads a
// folder or touches the history; `gate.mjs` does that and feeds these functions.

import { parseCheck } from "./transcript.mjs";
import { render as renderSeries, series } from "../report.mjs";

/** DEL-78: the result is a pass rate over three runs per product. */
export const RUNS_PER_PRODUCT = 3;
/** A pass rate under this is a finding in the appendix, never a gate failure (see `exitDecision`). */
export const LOW_PASS_RATE = 2 / 3;

/**
 * The (product, task) pairs of one gate run, one at a time in this order. `offset` is how many
 * runs the history already holds, so each run takes the next task after the previous one: no two
 * consecutive runs share a task, and the next candidate starts where this one stopped.
 */
export function taskPlan(tasks, products, runs, offset = 0) {
	if (!tasks.length) throw new Error("no tasks to rotate through");
	const plan = [];
	for (const product of products) for (let i = 0; i < runs; i++) plan.push({ product, task: tasks[(offset + plan.length) % tasks.length] });
	return plan;
}

const uniq = (list) => [...new Set(list)];
const mean = (list) => (list.length ? Math.round((list.reduce((s, n) => s + n, 0) / list.length) * 100) / 100 : null);

/**
 * One run as the gate keeps it, from the run's run.json (`meta`) and score.json (`scored`).
 * `executed` is false when the run could not execute at all: it timed out, or the agent exited
 * with an error before making a single tool call (CLI missing, not signed in).
 */
export function runRecord(meta, scored) {
	const met = scored.diagnosticsMet ?? {};
	const confusion = (kind, code, level) => ({ kind, code, level: level ?? met[code]?.level ?? null, message: met[code]?.message ?? "" });
	const confusions = [
		...scored.misread.map((m) => confusion("misread", m.code, m.level)),
		...(scored.ignored ?? []).map((m) => confusion("ignored", m.code, m.level)),
		...scored.unresolved.map((code) => confusion("unresolved", code)),
	];
	const crashed = Boolean(meta.timedOut) || (meta.exitCode !== 0 && scored.toolCalls === 0);
	return {
		runId: meta.runId,
		product: meta.product,
		model: meta.model ?? null,
		profile: meta.profile ?? null,
		task: meta.task,
		score: scored.score,
		pass: Boolean(scored.pass),
		attempts: scored.clean.attempts,
		firstCleanAttempt: scored.clean.firstAttempt,
		toolCalls: scored.toolCalls,
		diagnosticCodes: Object.keys(met),
		misread: scored.misread.map((m) => m.code),
		unresolved: [...scored.unresolved],
		confusions,
		factsFailed: scored.facts.items.filter((i) => !i.ok).map((i) => i.id),
		durationMs: meta.finishedAt && meta.startedAt ? Date.parse(meta.finishedAt) - Date.parse(meta.startedAt) : null,
		runDir: meta.runDir,
		executed: !crashed,
		failure: meta.timedOut ? "timed out" : crashed ? `agent exited with code ${meta.exitCode} before any tool call` : null,
	};
}

/** A run that could not execute at all, for when run.mjs itself failed. */
export const failedRecord = (product, task, error) => ({ product, task, score: 0, pass: false, attempts: 0, firstCleanAttempt: null, toolCalls: 0, diagnosticCodes: [], misread: [], unresolved: [], confusions: [], factsFailed: [], runDir: null, executed: false, failure: error });

/** Per product: the pass rate and what the runs did. `runs` may be empty (the product was skipped). */
export function aggregate(product, runs) {
	const passed = runs.filter((r) => r.pass).length;
	return {
		product,
		model: runs.find((r) => r.model)?.model ?? null,
		profile: runs.find((r) => r.profile)?.profile ?? null,
		runs: runs.length,
		passed,
		passRate: runs.length ? Math.round((passed / runs.length) * 1000) / 1000 : null,
		scores: runs.map((r) => r.score),
		meanScore: mean(runs.map((r) => r.score)),
		attempts: runs.map((r) => r.attempts),
		toolCalls: runs.map((r) => r.toolCalls),
		meanToolCalls: mean(runs.map((r) => r.toolCalls)),
		diagnosticCodes: uniq(runs.flatMap((r) => r.diagnosticCodes)),
		misread: uniq(runs.flatMap((r) => r.misread)),
		unresolved: uniq(runs.flatMap((r) => r.unresolved)),
		details: runs,
	};
}

/**
 * The gate's verdict. DEL-78 asks for a pass rate over three runs, and neither INT-69 nor
 * FEA-SF-65 sets a threshold for it (INT-69 is the per-run bar; FEA-SF-65 wants a series, not a
 * gate). So the step fails only when a run could not execute or fewer runs finished than asked
 * for. Low pass rates and drops are findings in the appendix.
 */
export function exitDecision(products, requestedRuns) {
	const reasons = [];
	for (const p of products) {
		if (p.runs < requestedRuns) reasons.push(`${p.product}: ${p.runs} of ${requestedRuns} runs finished`);
		for (const r of p.details.filter((d) => !d.executed)) reasons.push(`${p.product} ${r.task}: could not execute (${r.failure})`);
	}
	return { status: reasons.length ? "fail" : "pass", reasons };
}

/** Findings that never fail the gate: low pass rates, score drops against recent runs, unmet facts. */
export function findings(products, historySeries) {
	const out = [];
	for (const p of products) {
		if (p.passRate !== null && p.passRate < LOW_PASS_RATE) out.push(`${p.product}: pass rate ${p.passed}/${p.runs} is under ${Math.round(LOW_PASS_RATE * 100)}%`);
		else if (p.passRate !== null && p.passRate < 1) out.push(`${p.product}: pass rate ${p.passed}/${p.runs}`);
		for (const r of p.details.filter((d) => d.executed && !d.pass && d.factsFailed.length)) out.push(`${p.product} ${r.task}: facts not met: ${r.factsFailed.join(", ")}`);
	}
	const ids = new Set(products.flatMap((p) => p.details.map((r) => r.runId)));
	for (const g of historySeries) for (const r of g.runs) if (r.flagged && ids.has(r.runId)) out.push(`${g.key}: score ${r.score} is ${Math.abs(r.delta)} points below its recent runs`);
	return out;
}

/**
 * Codes that confused an agent, one per code with every run it confused. DEL-78 turns each
 * confusion caused by docs or diagnostics into an issue; the release agent decides which
 * are and files them. `excerptOf(run, code)` gives the path of the transcript excerpt.
 */
export function issueCandidates(products, { excerptOf = () => null, transcriptOf = (r) => r.transcript ?? (r.runDir ? `${r.runDir}/transcript.jsonl` : null) } = {}) {
	const byCode = new Map();
	for (const p of products) {
		for (const r of p.details) {
			for (const c of r.confusions) {
				const e = byCode.get(c.code) ?? { code: c.code, level: c.level, message: c.message, occurrences: [] };
				e.message ||= c.message;
				e.level ||= c.level;
				e.occurrences.push({ product: r.product, task: r.task, runId: r.runId, kind: c.kind, transcript: transcriptOf(r), excerpt: excerptOf(r, c.code) });
				byCode.set(c.code, e);
			}
		}
	}
	return [...byCode.values()].sort((a, b) => b.occurrences.length - a.occurrences.length || a.code.localeCompare(b.code));
}

/** The transcript around the check that showed `code`: that check's output and the next calls. */
export function excerptOf(calls, code, { after = 3, max = 1500 } = {}) {
	const cut = (v) => {
		const text = typeof v === "string" ? v : JSON.stringify(v ?? "");
		return text.length > max ? `${text.slice(0, max)}... [${text.length - max} more characters]` : text;
	};
	const i = calls.findIndex((c) => c.kind === "check" && parseCheck(c).diagnostics.some((d) => d.code === code));
	if (i < 0) return null;
	const lines = [`# ${code}`, ""];
	calls.slice(i, i + 1 + after).forEach((c, k) => {
		lines.push(`## ${k === 0 ? "The check that showed it" : `Next call ${k}`}: ${c.tool} (${c.kind})`, "", "input:", "```", cut(c.input), "```");
		if (c.output) lines.push("output:", "```", cut(c.output), "```");
		lines.push("");
	});
	return lines.join("\n");
}

/** Pass rate and mean score per product and build, in the order the builds first appeared. */
export function trendByBuild(entries, product) {
	const sorted = entries.filter((e) => e.product === product).sort((a, b) => String(a.at).localeCompare(String(b.at)));
	const builds = new Map();
	for (const e of sorted) {
		const key = e.build?.commit ?? "unknown";
		const b = builds.get(key) ?? { commit: key, runs: 0, passed: 0, scores: [], first: e.at };
		b.runs++;
		b.passed += e.pass ? 1 : 0;
		b.scores.push(e.score);
		builds.set(key, b);
	}
	return [...builds.values()].map((b) => ({ commit: b.commit, runs: b.runs, passed: b.passed, meanScore: mean(b.scores), first: b.first }));
}

const pct = (r) => (r === null ? "-" : `${Math.round(r * 100)}%`);

/** The packet appendix (DEL-78). */
export function renderAppendix({ at, richview, products, decision, findings: found, candidates, history, requestedRuns, unrunProducts = [] }) {
	const L = [];
	L.push("# Fresh-agent run", "", `Run at ${at} on ${richview.version ?? richview.binary}. ${requestedRuns} run(s) per product, one at a time, rotating through the tasks (DEL-78). Result: **${decision.status}**.`, "");
	if (decision.reasons.length) L.push(...decision.reasons.map((r) => `- ${r}`), "");
	L.push("The gate fails only when a run could not execute. Low pass rates and drops below are findings for the packet, not a failure.", "");
	if (unrunProducts.length) L.push(...unrunProducts.map((u) => `**${u.product} did not run:** ${u.reason}.`), "");
	L.push("## Pass rates", "", "| Product | Model | Pass rate | Runs | Scores | Attempts | Tool calls |", "| --- | --- | --- | --- | --- | --- | --- |");
	for (const p of products) L.push(`| ${p.product} | ${p.model ?? "default"}${p.profile ? ` (clauth ${p.profile})` : ""} | ${pct(p.passRate)} (${p.passed}/${p.runs}) | ${p.details.map((r) => r.task).join(", ")} | ${p.scores.join(", ")} | ${p.attempts.join(", ")} | ${p.toolCalls.join(", ")} |`);
	L.push("");
	for (const p of products) {
		L.push(`### ${p.product} runs`, "", "| Task | Score | Pass | Clean at | Facts not met | Run folder |", "| --- | --- | --- | --- | --- | --- |");
		for (const r of p.details) L.push(`| ${r.task} | ${r.score} | ${r.pass ? "yes" : "no"} | ${r.firstCleanAttempt ?? "never"} | ${r.factsFailed.join(", ") || "-"} | ${r.runDir ?? "-"} |`);
		L.push("");
	}
	L.push("## Findings", "", ...(found.length ? found.map((f) => `- ${f}`) : ["None."]), "");
	L.push("## Trend", "");
	for (const p of products) {
		const trend = trendByBuild(history, p.product);
		L.push(`### ${p.product} by build`, "", "| Build | Runs | Passed | Mean score |", "| --- | --- | --- | --- |");
		for (const b of trend.slice(-8)) L.push(`| ${b.commit.slice(0, 10)} | ${b.runs} | ${b.passed} | ${b.meanScore} |`);
		L.push("");
	}
	const groups = series(history.filter((e) => products.some((p) => p.product === e.product)));
	L.push("### Every task over time", "", "```", renderSeries(groups).trimEnd(), "```", "");
	L.push("## Diagnostics that confused agents", "");
	if (!candidates.length) L.push("None: no code came back after an edit, was left unacted on or stayed open.", "");
	for (const c of candidates) {
		L.push(`- \`${c.code}\`${c.level ? ` (${c.level})` : ""}: ${c.message || "no message recorded"}`);
		for (const o of c.occurrences) L.push(`  - ${o.product}, ${o.task}, ${o.kind}: transcript ${o.transcript ?? "-"}${o.excerpt ? `, excerpt ${o.excerpt}` : ""}`);
	}
	L.push("", "## Issue candidates", "", "Each confusion caused by docs or diagnostics becomes an issue (DEL-78). These are candidates; the release agent files the ones that are.", "");
	if (!candidates.length) L.push("None.", "");
	for (const c of candidates) {
		const first = c.occurrences[0];
		L.push(`- Code \`${c.code}\`, message "${c.message || "none recorded"}", seen in ${c.occurrences.length} run(s); excerpt ${first.excerpt ?? first.transcript ?? "-"}`);
	}
	L.push("");
	return L.join("\n");
}
