// Host reason (ARC-03): the release's part model (DEL-83): which parts exist, their fixed order, what each needs and how a selection is checked.
import { readFileSync } from "node:fs";

export const STAGES = ["prepare", "checks", "deploy", "closing"] as const;
export type Stage = typeof STAGES[number];
export type Step = {step: string; args: string[]; done?: string; outward?: string; builtin?: never};
export type Builtin = {builtin: string; step?: never; args?: never; outward?: never};
type Rollback = {title: string; command?: string; words?: string; automatic?: boolean; basis?: string; steps?: Step[]; requires?: string[]};
export type Part = {id: string; title: string; needs: string[]; selectable?: boolean; always?: boolean; implicitFor?: string[]; gated?: Stage[]; prepare?: Step[]; checks?: (Step | Builtin)[]; deploy?: Step[]; closing?: Step[]; closingAfter?: string[]; rollback?: Rollback};
export type PartsModel = ReturnType<typeof validateParts>;
const STEP = /^[0-9a-z][0-9a-z-]+$/;

export const PARTS_FILE = new URL("../parts.json", import.meta.url);

/** Read and validate parts.json. */
export function loadParts(file: string | URL = PARTS_FILE) {
	const raw = JSON.parse(readFileSync(file, "utf8"));
	return validateParts(raw);
}

/** Every need points at an earlier part, step names are unique across parts and each step has the shape the kit's runner accepts. */
export function validateParts(input: unknown) {
	const raw = input as {schema: number; parts: Part[]}; // Check the on-disk parts model before scheduling.
	if (raw?.schema !== 1 || !Array.isArray(raw.parts) || !raw.parts.length) throw new Error("parts.json: schema 1 and a parts list are required");
	const seen = new Set<string>(), steps = new Map<string, {part: string; stage: Stage}>();
	for (const part of raw.parts) {
		if (!/^[a-z]+$/.test(part.id ?? "") || seen.has(part.id)) throw new Error(`parts.json: bad or repeated part id ${part.id}`);
		for (const need of part.needs ?? []) if (!seen.has(need)) throw new Error(`parts.json: ${part.id} needs ${need}, which is not an earlier part`);
		seen.add(part.id);
		for (const stage of STAGES) {
			for (const item of part[stage] ?? []) {
				if (item.builtin) { if (stage !== "checks") throw new Error(`parts.json: built-in ${item.builtin} belongs under checks`); continue; }
				if (!STEP.test(item.step ?? "") || !Array.isArray(item.args)) throw new Error(`parts.json: ${part.id} ${stage} has a malformed step`);
				const key = `${item.step} ${item.args!.join(" ")}`.trim();
				if (steps.has(key)) throw new Error(`parts.json: step "${key}" is in two places`);
				steps.set(key, { part: part.id, stage });
			}
		}
		for (const stage of part.gated ?? []) if (!STAGES.includes(stage)) throw new Error(`parts.json: ${part.id} gates unknown stage ${stage}`);
		if (part.rollback?.automatic && !(part.rollback.steps?.length && part.rollback.basis)) throw new Error(`parts.json: ${part.id} rolls back automatically without steps and a basis`);
	}
	for (const part of raw.parts) for (const implicit of part.implicitFor ?? []) if (!seen.has(implicit)) throw new Error(`parts.json: ${part.id} is implicit for unknown ${implicit}`);
	return { parts: raw.parts, byId: new Map(raw.parts.map((p) => [p.id, p])), stepOwner: steps };
}

/** The ids a go may name, in their fixed order. */
export const selectable = (model: PartsModel) => model.parts.filter((p) => p.selectable).map((p) => p.id);

/**
 * Check a chosen list and return it in the fixed order. A need must be chosen too, or already have passed in an earlier run
 * (`passedBefore`): the Windows walk and the prod merge are shared prerequisites, never silently skipped.
 */
export function selectParts(model: PartsModel, list: string[], passedBefore = new Set<string>()) {
	const known = new Set(selectable(model));
	if (!Array.isArray(list) || !list.length) throw new Error("Name at least one part to release.");
	for (const id of list) if (!known.has(id)) throw new Error(`Unknown part "${id}". Parts: ${[...known].join(", ")}.`);
	if (new Set(list).size !== list.length) throw new Error("List each part once.");
	const chosen = new Set(list);
	for (const id of list) {
		for (const need of model.byId.get(id)!.needs) {
			const part = model.byId.get(need)!;
			if (!part.selectable || chosen.has(need) || passedBefore.has(need)) continue;
			throw new Error(`Part ${id} needs part ${need}: name it too, or finish it in an earlier run.`);
		}
	}
	return model.parts.filter((p) => chosen.has(p.id)).map((p) => p.id);
}

/** The parts a run activates: the shared prelude, the chosen parts and the internal parts they pull in, in fixed order. */
export function activeParts(model: PartsModel, chosen: string[]) {
	const set = new Set(chosen);
	return model.parts.filter((p) => p.always || (p.selectable ? set.has(p.id) : p.implicitFor?.some((id) => set.has(id))));
}

/** Where a step lives, for a step name and its arguments. */
export const ownerOf = (model: PartsModel, step: string, args: string[] = []) => model.stepOwner.get(`${step} ${args.join(" ")}`.trim()) ?? [...model.stepOwner].find(([key]) => key.split(" ")[0] === step)?.[1];
