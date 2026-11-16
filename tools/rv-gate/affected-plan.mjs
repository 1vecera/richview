// Host reason (ARC-03): show the affected gate plan without building or running tests.
import {readFileSync} from "node:fs";
import {fileURLToPath} from "node:url";
import {execFileSync} from "node:child_process";
import {resolve} from "node:path";
import {selectAffected, validateAreas} from "./lib/selection.mjs";
import {readPhase} from "./lib/phase.mjs";
import {stepsForTier} from "./lib/steps.mjs";
import {fullReceipts, summarizeHistory} from "./lib/affected-history.mjs";
import {workspaceGraph, includeInputs} from "./lib/impact.mjs";
import {includes} from "./lib/fast.mjs";
import {caseCatalog, RUNTIME_EXTRA} from "./lib/case-selection.mjs";
import {commandId} from "./lib/selection.mjs";
import {reportCohort} from "./lib/report-inputs.mjs";

export function printPlan(paths, {write = console.log} = {}) {
	if (!paths.length) throw new Error("Usage: node tools/rv-gate/affected-plan.mjs <changed path> [...]");
	const phase = readPhase();
	if (phase.phase !== "prelaunch" || !phase.affected) throw new Error("Affected planning requires the prelaunch affected policy");
	const areas = JSON.parse(readFileSync(new URL("./areas.json", import.meta.url), "utf8"));
	const steps = stepsForTier("full", "full");
	validateAreas(areas, steps);
	const cwd = resolve(fileURLToPath(new URL("../..", import.meta.url)));
	const metadata = JSON.parse(execFileSync("cargo", ["metadata", "--format-version", "1", "--no-deps", "--offline"], {cwd, encoding: "utf8"}));
	const graph = workspaceGraph(metadata, cwd), inputs = includeInputs(includes(cwd), graph);
	const {counts, durations} = summarizeHistory(fullReceipts(), steps);
	const catalog = caseCatalog(steps, reportCohort(cwd, {extra: RUNTIME_EXTRA}), commandId, {cohorts: phase.prelaunchFull?.partialRetry === true});
	const plan = selectAffected(steps, paths, {areas, counts, threshold: phase.affected.failureThreshold, phase: phase.phase, graph, inputs, catalog, fallback: phase.affected.fallback});
	write(`Changed: ${paths.join(", ")}\nPrelaunch: ${phase.prelaunchFull.browsers.join(", ")}; fallback ${phase.affected.fallback.mode}; failure threshold ${phase.affected.failureThreshold}`);
	for (const item of plan) {
		const ms = durations.get(item.commandId ?? item.id);
		write(`${item.status.toUpperCase().padEnd(7)} ${item.id}${item.caseKey ? ` [case ${item.caseKey}]` : ""}${item.command ? ` [${item.command}${item.engine ? `; ${item.engine}` : ""}]` : ""} (${item.estimated ? "up to " : ""}${item.failures} failures${ms === undefined ? "; timing unknown" : `; ${item.caseKey ? "whole-command " : ""}median ${(ms / 60000).toFixed(1)} min`}): ${item.reason}`);
	}
	const selected = [...new Set(plan.filter(item => item.status === "run").map(item => item.commandId ?? item.id))], timed = selected.filter(id => durations.has(id));
	const total = timed.reduce((sum, id) => sum + durations.get(id), 0) / 60000;
	write(`Estimated test work: ${total.toFixed(1)} min summed whole-command medians across ${timed.length}/${selected.length} selected commands/steps; ${selected.length - timed.length} have no timing. Case subsets use the whole-command median; this is not elapsed wall time.`);
	return plan;
}
if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
	try { printPlan(process.argv.slice(2)); } catch (error) { console.error(error.message); process.exitCode = 1; }
}
