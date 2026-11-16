// Host reason (ARC-03): read Daniel's gate phase and plan visible prelaunch rotation (DEL-82).
import {existsSync, readFileSync} from "node:fs";
import {fileURLToPath} from "node:url";

export const PHASE_FILE = fileURLToPath(new URL("../phase.json", import.meta.url));

/** Missing historical configuration retains the launch gate's strict behavior. */
export function readPhase(file = PHASE_FILE) {
	return existsSync(file) ? validatePhase(JSON.parse(readFileSync(file, "utf8"))) : {phase: "launch", since: null, by: "legacy", rotation: []};
}

export function validatePhase(value) {
	if (value?.phase === "launch" && value.since === null && value.by === "legacy" && Array.isArray(value.rotation) && value.rotation.length === 0)
		return {phase: "launch", since: null, by: "legacy", rotation: []};
	if (!value || !["prelaunch", "launch"].includes(value.phase) || typeof value.by !== "string" || !value.by.trim()
		|| !/^\d{4}-\d{2}-\d{2}$/.test(value.since ?? "") || !Number.isFinite(Date.parse(value.since)) || new Date(value.since).toISOString().slice(0, 10) !== value.since)
		throw new Error("Gate phase needs phase prelaunch|launch, an ISO date since, and by");
	const rotation = value.rotation ?? [];
	if (!Array.isArray(rotation) || rotation.some(id => typeof id !== "string" || !/^[a-z][a-z0-9-]*$/.test(id)) || new Set(rotation).size !== rotation.length)
		throw new Error("Phase rotation must contain unique step ids approved by the lead");
	const pause = value.freshAgentPause;
	if (pause !== undefined && (!pause || typeof pause.reason !== "string" || !pause.reason.trim()
		|| typeof pause.until !== "string" || !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?Z$/.test(pause.until) || !Number.isFinite(Date.parse(pause.until))))
		throw new Error("Fresh-agent pause needs an explicit UTC expiry and reason");
	const full = value.prelaunchFull;
	if (full !== undefined && (!full || !Array.isArray(full.browsers) || full.browsers.length !== 1 || full.browsers[0] !== "chromium"
		|| full.partialRetry !== true)) throw new Error("Invalid prelaunch full policy");
	const affected = value.affected;
	if (affected !== undefined && (!affected || !Number.isSafeInteger(affected.failureThreshold) || affected.failureThreshold < 0
		|| !affected.fallback || !["failure-threshold", "real-defects"].includes(affected.fallback.mode)
		|| !Array.isArray(affected.fallback.realDefectSteps) || affected.fallback.mode === "real-defects" && !affected.fallback.realDefectSteps.length || affected.fallback.realDefectSteps.some(id => typeof id !== "string" || !/^[a-z][a-z0-9-]*$/.test(id))
		|| new Set(affected.fallback.realDefectSteps).size !== affected.fallback.realDefectSteps.length)) throw new Error("Invalid affected fallback policy");
	return {phase: value.phase, since: value.since, by: value.by, rotation: [...rotation].sort(),
		...(pause !== undefined && {freshAgentPause: {until: pause.until, reason: pause.reason}}),
		...(full !== undefined && {prelaunchFull: {browsers: [...full.browsers], partialRetry: full.partialRetry}}),
		...(affected !== undefined && {affected: {failureThreshold: affected.failureThreshold, fallback: {mode: affected.fallback.mode, realDefectSteps: [...affected.fallback.realDefectSteps]}}})};
}

export const CHROMIUM_ONLY_REASON = "Prelaunch policy: Chromium only until launch.";

/** The frozen exception applies to ordinary full runs only; launch and candidate stay strict. */
export function browserGap(policy, {mode, step, command} = {}) {
	if (policy?.phase !== "prelaunch" || mode !== "full" || !policy.prelaunchFull) return null;
	const engine = command?.env?.RICHVIEW_VERIFY_BROWSER;
	return ["firefox", "webkit"].includes(step?.id) || ["firefox", "webkit"].includes(engine) ? CHROMIUM_ONLY_REASON : null;
}

/** Daniel's dated personal-quota exception applies only to fresh-agent in prelaunch fulls. */
export function prelaunchException(policy, {mode, id, now = new Date()} = {}) {
	const pause = policy.freshAgentPause;
	if (policy.phase !== "prelaunch" || mode !== "full" || id !== "fresh-agent" || !pause
		|| !Number.isFinite(Date.parse(pause.until)) || !Number.isFinite(Number(new Date(now))) || Number(new Date(now)) >= Date.parse(pause.until)) return null;
	return {type: "fresh-agent-quota", until: pause.until, reason: pause.reason};
}

/** Explicit artifact dependencies always run, even if their historical duration is large. */
export function requiredArtifacts(steps) {
	return new Set(["build", "captures", ...steps.flatMap(step => [...(step.after ?? []), ...(step.needs ? [step.needs] : [])])]);
}

/** Only actual, entirely green observations qualify; an initial failed attempt breaks the streak. */
export function qualifiesForRotation(observations) {
	const recent = observations.slice(-20);
	return recent.length === 20 && recent.every(step => step.status === "pass" && !step.flaky && !step.attempts?.some(attempt => attempt.status === "fail")
		&& Number.isFinite(step.runMs) && step.runMs >= 0) && median(recent.map(step => step.runMs)) >= 200_000;
}

export function median(values) {
	if (!values.length) return null;
	const sorted = [...values].sort((a,b) => a-b), middle = Math.floor(sorted.length / 2);
	return sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
}

/** Freeze one deterministic third and force a step due after two rotated fulls across list changes. */
export function planRotation(policy, {mode, steps, receipts = [], eligible = []}) {
	const ids = new Set(steps.map(step => step.id)), protectedIds = requiredArtifacts(steps);
	for (const id of policy.rotation) if (!ids.has(id)) throw new Error(`Unknown phase rotation step: ${id}`);
	const history = receipts.filter(receipt => receipt.kind === "full").sort((a,b) => Date.parse(a.at) - Date.parse(b.at) || a.id.localeCompare(b.id));
	const ordinal = history.filter(receipt => receipt.phase === "prelaunch").length;
	const candidates = policy.phase === "prelaunch" && mode === "full" ? policy.rotation.filter(id => eligible.includes(id) && !protectedIds.has(id)) : [];
	const lastTwo = history.slice(-2);
	const forced = candidates.filter(id => lastTwo.length === 2 && lastTwo.every(receipt => receipt.steps?.some(step => step.id === id && step.status === "unrun")));
	const selected = candidates.filter(id => policy.rotation.indexOf(id) % 3 === ordinal % 3 || forced.includes(id));
	return {ordinal, cycle: 3, approved: [...policy.rotation], eligible: candidates, forced, selected, skipped: candidates.filter(id => !selected.includes(id))};
}
