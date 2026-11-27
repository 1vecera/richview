// Host reason (ARC-03): runs a release's parts in their fixed order with a journal for each part (DEL-83).
// One go covers the parts it names. Checks that can run before a deploy run first. A part that fails stops and rolls back alone:
// the parts that do not need it still run, and the parts that do are skipped and named.
import { activeParts } from "./parts.mts";
import { runBuiltin } from "./predeploy.mts";
import { verifiedGo } from "./go.mts";

import type { ReleaseDescriptor } from "./descriptor.mts";
import type { Journal } from "./journal.mts";
import type { PartsModel, Part, Stage, Step } from "./parts.mts";
import type { CheckContext } from "./predeploy.mts";

export type Status = keyof typeof OUTCOME;
type StageOrRollback = Stage | "rollback";
type StepResult = {stage: Stage; step: string; args: string[]; status: string; at: string};
type RollbackOutcome = {basis?: string; steps: {step: string; args: string[]; code: number}[]; result?: string};
export type PartState = {
	id: string; title: string; status: Status; reason: string; steps: StepResult[];
	checks: {id: string; status: string; detail: string}[]; done: Partial<Record<Stage, boolean>>;
	rollback: {title?: string; command?: string; automatic?: boolean; words?: string; ready?: boolean; ran?: RollbackOutcome} | null;
	carried?: boolean; updatedAt?: string;
};
export type Runner = {step(input: {step: string; args: string[]; part: string; stage: StageOrRollback; spec: Step}): {code?: number} | Promise<{code?: number}>};
type RunOptions = {model: PartsModel; desc: ReleaseDescriptor; chosen: string[]; journal: Journal; runner: Runner; ctx: CheckContext; log?: (line: string) => void; now?: () => string; stubbed?: boolean};

const BAD = new Set(["refused", "failed-unchanged", "failed-rolled-back", "failed-left", "skipped"]);
export const isBad = (status: Status) => BAD.has(status);

/** The outcome words of the final report, in plain terms. */
export const OUTCOME = {
	passed: "passed",
	held: "updated; sign-up reopen held",
	refused: "refused before deploy (nothing changed)",
	"failed-unchanged": "failed (nothing deployed)",
	"failed-rolled-back": "failed and rolled back",
	"failed-left": "failed and left in place",
	skipped: "skipped",
	excluded: "left out by the go",
	pending: "not reached",
	deployed: "deployed",
};

const STAGES = ["prepare", "checks", "deploy"];

/**
 * Run the stages over the active parts. `runner.step({step, args, part, stage})` returns `{code}`; `ctx` carries what the built-in
 * checks need (and `newWorker()`, which reads the Worker a failed deploy left). The result is the state of every part.
 */
export async function runRelease({ model, desc, chosen, journal, runner, ctx, log = () => {}, now = () => new Date().toISOString(), stubbed = false }: RunOptions) {
	const active = activeParts(model, chosen), activeIds = new Set(active.map((p) => p.id));
	const states = new Map<string, PartState>();
	for (const part of model.parts) {
		const isActive = activeIds.has(part.id);
		if (!part.selectable && !isActive) continue;
		const fresh: PartState = { id: part.id, title: part.title, status: isActive ? "pending" : "excluded", reason: "", steps: [], checks: [], rollback: null, done: {} };
		const before = journal.readPart(part.id);
		if (before?.status === "passed") Object.assign(fresh, before, { carried: !isActive || undefined });
		else if (before && isActive && journal.hasLatch(part.id)) Object.assign(fresh, before, { reason: `${before.reason} (latched: reconcile FAILED.${part.id} before a retry)`.trim() });
		states.set(part.id, fresh);
	}
	const state = (id: string) => states.get(id)!; // The runner only works on known part ids.
	const save = (id: string) => { state(id).updatedAt = now(); journal.writePart(id, state(id)); };
	const say = (line: string) => { log(line); journal.event({ at: now(), line }); };
	const set = (id: string, status: Status, reason = "") => { Object.assign(state(id), { status, reason }); save(id); say(`${id}: ${status}${reason ? ` — ${reason}` : ""}`); };

	// A part waits for the shared prelude and for each need to have finished the same stage; a need that failed, was refused or was skipped blocks it.
	const blocker = (part: Part, stage: Stage) => {
		for (const need of part.id === "shared" ? [] : ["shared", ...part.needs]) {
			const s = state(need);
			if (!s || s.status === "excluded") return `${need} was left out of this release`;
			if (isBad(s.status)) return `${need} (${OUTCOME[s.status]})`;
			if (!s.done[stage]) return `${need} (not finished: ${stage})`;
		}
		return null;
	};
	// Could this part still run if the part `ignore` were done? The internal build part is skipped when no part that needs it can run.
	const viable = (id: string, ignore: string): boolean => {
		const s = state(id);
		if (!s || s.status === "excluded" || isBad(s.status)) return false;
		return id === "shared" || [...model.byId.get(id)!.needs, "shared"].filter((need) => need !== ignore).every((need) => viable(need, ignore));
	};

	// The date of the first go recorded in this journal (UTC): the recovery sentences of the kit carry it, so the first run fixes it.
	const goDate = () => (journal.goEntries?.()[0]?.at ?? now()).slice(0, 10);
	const resolve = (args: string[], extra: Record<string, string> = {}) => args.map((arg) => arg.replace(/\{(\w+)\}/g, (_, key: string) => {
		const variables: Record<string, string> = { full: desc.full, parts: chosen.join(","), route: desc.route, candidate: desc.candidate, final: desc.final, goDate: goDate(), ...extra };
		const value = variables[key];
		if (value === undefined) throw new Error(`No value for {${key}}`);
		return value;
	}));
	const doneKey = (spec: Step) => spec.done ?? `ok-${spec.step}`;
	const record = (part: Part, entry: Omit<StepResult, "at">) => { state(part.id).steps.push({ ...entry, at: now() }); save(part.id); };
	async function exec(part: Part, stage: Stage, spec: Step, extra?: Record<string, string>) {
		const args = resolve(spec.args, extra);
		if (!stubbed && journal.done(doneKey(spec))) { record(part, { stage, step: spec.step, args, status: "already done" }); say(`${part.id}: ${spec.step} ${args.join(" ")} already done`); return true; }
		say(`${part.id}: ${stage} ${spec.step} ${args.join(" ")}`.trimEnd());
		const { code = 1 } = await runner.step({ step: spec.step, args, part: part.id, stage, spec });
		record(part, { stage, step: spec.step, args, status: code === 0 ? "ok" : `failed (${code})` });
		return code === 0;
	}
	/** The go must cover a part before its gated stage begins, and its evidence must bind; otherwise the part does not start. */
	const gate = (part: Part, stage: Stage) => {
		if (!(part.gated ?? []).includes(stage)) return null;
		try { verifiedGo(journal, part.id, desc, { stubbed, stage: stage === "prepare" ? "prepare" : "deploy" }); return null; } catch (error) { return (error as Error).message; }
	};

	// Stage 1: the work that needs no deploy.
	for (const part of active) {
		const s = state(part.id);
		if (s.status !== "pending" || s.done.prepare) continue;
		const why = blocker(part, "prepare");
		if (why) { set(part.id, "skipped", `needs ${why}`); continue; }
		if (part.implicitFor && !part.implicitFor.some((id) => activeIds.has(id) && viable(id, part.id))) { set(part.id, "skipped", "no part that needs it can run"); continue; }
		const closed = gate(part, "prepare");
		if (closed) { set(part.id, "failed-unchanged", closed); continue; }
		let ok = true;
		for (const spec of part.prepare ?? []) if (!(await exec(part, "prepare", spec))) { set(part.id, "failed-unchanged", `${spec.step} failed; nothing was deployed`); ok = false; break; }
		if (ok) { s.done.prepare = true; save(part.id); }
	}

	// Stage 2: every check that can run before a deploy. A failure refuses that part's deploy and nothing else.
	for (const part of active) {
		const s = state(part.id);
		if (s.status !== "pending" || !s.done.prepare || s.done.checks) continue;
		const why = blocker(part, "checks");
		if (why) { set(part.id, "skipped", `needs ${why}`); continue; }
		let refused = null;
		for (const item of part.checks ?? []) {
			if (item.builtin) {
				const result = await runBuiltin(item.builtin, ctx);
				s.checks.push({ id: result.id, status: result.status, detail: result.detail });
				say(`${part.id}: check ${result.id} ${result.status} — ${result.detail}`);
				if (result.status === "fail") { refused = `check ${result.id} failed: ${result.detail}`; break; }
			} else {
				const ok = await exec(part, "checks", item as Step);
				s.checks.push({ id: item.step!, status: ok ? "pass" : "fail", detail: ok ? "kit step passed" : "kit step failed" });
				if (!ok) { refused = `${item.step} failed`; break; }
			}
		}
		if (refused) set(part.id, "refused", refused);
		else { s.done.checks = true; save(part.id); }
	}

	// Stage 3: the deploys, in the fixed order. Each part has its rollback ready first, and a failure stays inside its own part.
	async function rollBack(part: Part) {
		const rollback = part.rollback!; // Automatic rollback is validated with a basis and steps.
		const outcome: RollbackOutcome = { basis: rollback.basis, steps: [] };
		say(`${part.id}: rolling back (${rollback.basis})`);
		let newWorker = "";
		for (const step of rollback.steps!) {
			if (step.args.includes("{newWorker}")) {
				newWorker = ctx.newWorker?.() ?? "";
				if (!newWorker) { outcome.result = "nothing to restore: the history shows no new version"; break; }
			}
			const args = resolve(step.args, { newWorker });
			const { code = 1 } = await runner.step({ step: step.step, args, part: part.id, stage: "rollback", spec: step });
			outcome.steps.push({ step: step.step, args, code });
			if (code !== 0) { outcome.result = `${step.step} failed (${code})`; break; }
		}
		outcome.result ??= "restored";
		state(part.id).rollback = { ...state(part.id).rollback, ran: outcome };
		save(part.id);
		return { ...outcome, result: outcome.result };
	}
	for (const part of active) {
		const s = state(part.id);
		if (s.status !== "pending" || !s.done.checks || s.done.deploy) continue;
		if (!part.deploy) { s.done.deploy = true; if (!part.closing) set(part.id, "passed"); else save(part.id); continue; }
		const why = blocker(part, "deploy");
		if (why) { set(part.id, "skipped", `needs ${why}`); continue; }
		const missing = (part.rollback?.requires ?? []).filter((key) => journal.get(key) === undefined);
		s.rollback = { title: part.rollback?.title, command: part.rollback?.command, automatic: !!part.rollback?.automatic, words: part.rollback?.words, ready: !missing.length };
		if (missing.length) { set(part.id, "refused", `no rollback ready: the journal lacks ${missing.join(", ")}`); continue; }
		const closed = gate(part, "deploy");
		if (closed) { set(part.id, "refused", closed); continue; }
		say(`${part.id}: rollback ready — ${part.rollback!.title}${part.rollback!.automatic ? `; it runs by itself if the checks after the deploy fail (${part.rollback!.basis})` : `; it needs Daniel's words (${part.rollback!.words ?? "recovery"})`}`);
		let outward = false, failed = false;
		for (const spec of part.deploy) {
			if (spec.outward) outward = true;
			if (await exec(part, "deploy", spec)) continue;
			failed = true;
			if (!outward) set(part.id, "refused", `${spec.step} failed before any change`);
			else if (part.rollback?.automatic) {
				const result = await rollBack(part);
				if (result.result === "restored") set(part.id, "failed-rolled-back", `${spec.step} failed; ${part.rollback.title}: restored`);
				else if (result.result.startsWith("nothing to restore")) set(part.id, "failed-unchanged", `${spec.step} failed; ${result.result}`);
				else set(part.id, "failed-left", `${spec.step} failed and the rollback did not finish (${result.result}); run ${part.rollback.command}`);
			} else set(part.id, "failed-left", `${spec.step} failed after a change was made; ${part.rollback?.title ?? "inspect the journal"}: ${part.rollback?.command ?? "see the journal"} (needs Daniel's words: ${part.rollback?.words ?? "recovery"})`);
			break;
		}
		if (failed) continue;
		s.done.deploy = true;
		if (part.closing) { s.status = "deployed"; save(part.id); say(`${part.id}: deployed`); } else set(part.id, "passed");
	}

	// Stage 4: the close-out. Sign-up reopens last, and only when the parts it waits for have passed.
	for (const part of active) {
		const s = state(part.id);
		if (s.status !== "deployed") continue;
		const waiting = (part.closingAfter ?? []).filter((id) => state(id) && state(id).status !== "excluded" && state(id).status !== "passed");
		if (waiting.length) { set(part.id, "held", `${part.closing!.map((s) => s.outward ?? s.step).join(", ")}: waiting for ${waiting.map((id) => `${id} (${OUTCOME[state(id).status]})`).join(", ")}, so it stays as it is`); continue; }
		const closed = gate(part, "closing");
		if (closed) { set(part.id, "held", closed); continue; }
		let ok = true;
		for (const spec of part.closing!) if (!(await exec(part, "closing", spec))) { set(part.id, "held", `${spec.step} refused; the state stays as it is until Daniel decides`); ok = false; break; }
		if (ok) { s.done.closing = true; set(part.id, "passed"); }
	}
	for (const s of states.values()) if (s.status === "deployed") set(s.id, "passed");
	return states;
}

export { STAGES };
