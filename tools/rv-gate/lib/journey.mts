// What a journey records (DEL-76): ordered steps, each passing or failing with the value it
// expected, the value it got and the owner of the surface that broke. A failure is a product
// finding; nothing here weakens an assertion. A feature that is not built yet fails with a
// "not built" message, never a skip.
import { isDeepStrictEqual } from "node:util";

export type JourneyStep = {step: string; owner: string; ok: boolean; expected?: string; actual?: string; info?: boolean; notBuilt?: boolean};

/** The surfaces a failure can be handed to. */
export const OWNERS = ["app surfaces", "core format", "renderer", "chart engine", "styles", "publish", "site", "gate"];

/** Thrown to end a journey when its next step cannot run without the failed one. */
export class Stop extends Error {}

/** Shorten a value for a report line. */
export function show(value: unknown, limit = 240) {
	const text = typeof value === "string" ? value : JSON.stringify(value);
	const flat = String(text ?? String(value)).replace(/\s+/g, " ");
	return flat.length > limit ? `${flat.slice(0, limit)}...` : flat;
}

export class Journey {
	declare name: string;
	declare title: string;
	declare steps: JourneyStep[];
	declare started: number;
	declare seconds: number;

	constructor(name: string, title: string) {
		this.name = name;
		this.title = title;
		this.steps = [];
		this.started = Date.now();
		this.seconds = 0;
	}

	record(step: string, owner: string, ok: boolean, expected: unknown, actual: unknown, extra: Partial<JourneyStep> = {}) {
		if (!OWNERS.includes(owner)) throw new Error(`Unknown owner ${owner} for ${step}`);
		this.steps.push({ step, owner, ok, ...(ok ? {} : { expected: show(expected), actual: show(actual) }), ...extra });
		return ok;
	}

	/** A fact about the run that is not a check, such as which build a journey used. */
	info(text: string) {
		this.steps.push({ step: text, owner: "gate", ok: true, info: true });
	}

	/** `actual` deep-equals `expected`. */
	equal(step: string, owner: string, actual: unknown, expected: unknown) {
		return this.record(step, owner, isDeepStrictEqual(actual, expected), expected, actual);
	}

	/** `condition` holds; `expected` says what should have been true and `actual` what was seen. */
	assert(step: string, owner: string, condition: unknown, expected: unknown, actual: unknown) {
		return this.record(step, owner, Boolean(condition), expected, actual);
	}

	/** A feature the lockdown accepts that the build does not have: a failure that says so. */
	notBuilt(step: string, owner: string, what: string) {
		return this.record(step, owner, false, `${what} built`, "not built", { notBuilt: true });
	}

	/** Record a failure and end the journey. */
	abort(step: string, owner: string, expected: unknown, actual: unknown): never {
		this.record(step, owner, false, expected, actual);
		throw new Stop(`${step}: ${show(actual)}`);
	}

	/** Like `assert`, and ends the journey when it fails. */
	need(step: string, owner: string, condition: unknown, expected: unknown, actual: unknown): asserts condition {
		if (!this.assert(step, owner, condition, expected, actual)) throw new Stop(step);
	}

	finish() {
		this.seconds = (Date.now() - this.started) / 1000;
		return this;
	}

	get failures() {
		return this.steps.filter((s) => !s.ok);
	}

	get passed() {
		return this.steps.length > 0 && this.failures.length === 0;
	}
}

/** Run one journey body; an unexpected error is a gate failure, a `Stop` is already recorded. */
export async function runJourney(journey: Journey, body: (journey: Journey) => unknown | Promise<unknown>) {
	try {
		await body(journey);
	} catch (error) {
		if (!(error instanceof Stop)) journey.record("journey crashed", "gate", false, "the journey runs to its end", (error as Error | undefined)?.stack?.split("\n").slice(0, 3).join(" | ") ?? String(error));
	}
	return journey.finish();
}

/** The lines a run prints: one per journey, then each failure with step, expected, actual and owner. */
export function reportLines(journeys: Journey[]) {
	const lines = [];
	for (const j of journeys) {
		lines.push(`${j.passed ? "PASS" : "FAIL"} ${j.name} (${j.title}) in ${j.seconds.toFixed(1)} s: ${j.steps.filter((s) => s.ok && !s.info).length}/${j.steps.filter((s) => !s.info).length} steps`);
		for (const f of j.failures) lines.push(`  FAIL [${f.owner}] ${f.step}: expected ${f.expected}; got ${f.actual}`);
	}
	return lines;
}
