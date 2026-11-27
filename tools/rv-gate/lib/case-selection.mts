// Host reason (ARC-03): map source changes to stable, individually provable gate cases.
import {BROWSER_CASES} from "./browser-cases.mts";
import {FIGURE_CASES, FIGURE_WIDTHS} from "./figure-cases.mts";
import {DOCS_PHASES} from "./docs-shards.mts";
import {selection as rustSelection} from "./rust-tests.mts";
import { receiptSource } from "./source-paths.mts";
import type { GateCommand, GateStep } from "./steps.mts";
import type { CoverageReceipt } from "./affected-coverage.mts";

export type CaseKind = "rust-cohort" | "runtime" | "browser" | "figure-expand" | "docs";
export interface CaseSpec { kind: CaseKind; keys: readonly string[]; units: string[] }

export const RUNTIME_EXTRA = Object.freeze(["forms", "histogram-legend", "wide-chart", "legend-cues", "numeric-series-names"]);
const excluded = new Set(["sql-report", "parquet-report", "metricflow-report"]);

/** Match the runtime suite's source-derived example inventory and explicit fixtures. */
export function runtimeInventory(files: readonly string[]) {
	if (!Array.isArray(files)) throw new Error("Runtime case inventory needs source paths");
	const examples = files.flatMap(path => {
		const match = /^examples\/([^/]+)\/report\.rmd$/.exec(path);
		return match && !excluded.has(match[1]) ? [match[1]] : [];
	}).sort();
	const all = [...examples, ...RUNTIME_EXTRA];
	if (new Set(all).size !== all.length) throw new Error("Runtime report and fixture keys must be unique");
	return all;
}

/** An opaque case identity cannot collide with another command or another explicit key. */
export const caseUnitId = (command: string, key: string) => `${command}:case:${Buffer.from(key, "utf8").toString("base64url")}`;

/** Construct the complete case catalogue for the Chromium commands at this source head. */
export function caseCatalog(steps: readonly GateStep[], runtimeCases: readonly string[], commandId: (step: GateStep, command: GateCommand) => string, {cohorts = true} = {}) {
	if (!Array.isArray(runtimeCases) || !runtimeCases.length || new Set(runtimeCases).size !== runtimeCases.length || typeof commandId !== "function") throw new Error("Invalid affected case inventory");
	const definitions = new Map<string, CaseSpec>();
	for (const rust of steps) {
		if (!cohorts || rust.cohort !== "rust-debug" || rust.run?.length !== 1 || rust.run[0].cmd !== "cargo" || !rustSelection(rust.run[0].args)) continue;
		const keys = Array.from({length:4}, (_, index) => `shard-${index}-of-4`);
		definitions.set(rust.id, {kind:"rust-cohort", keys, units:keys.map(key => caseUnitId(rust.id, key))});
	}
	const step = steps.find((item: { id: string; }) => item.id === "chromium");
	for (const command of step?.run ?? []) {
		const entry = command.entry ?? command.args?.[0];
		const entries: Record<string, [CaseKind, readonly string[]]> = {
			"scripts/verify-report-runtime.mjs": ["runtime", runtimeCases],
			"scripts/verify-browser.mjs": ["browser", BROWSER_CASES],
			"scripts/verify-figure-expand.mjs": ["figure-expand", FIGURE_CASES],
			"scripts/verify-docs.mjs": ["docs", DOCS_PHASES],
		};
		const entrySpec = entries[entry && receiptSource(entry)];
		if (!entrySpec) continue;
		const id = commandId(step!, command), [kind, keys] = entrySpec;
		definitions.set(id, {kind, keys: [...keys], units: keys.map(key => caseUnitId(id, key))});
	}
	return definitions;
}

/** Narrow only paths whose case relationship is explicitly known; all other changes select all. */
export function affectedCaseKeys(kind: CaseKind, keys: readonly string[], paths: readonly string[]) {
	if (!paths.length) return [...keys];
	if (kind === "runtime") {
		const names = paths.map((path: string) => /^examples\/([^/]+)\/report\.rmd$/.exec(path)?.[1]);
		if (names.every(name => name && keys.includes(name) && !RUNTIME_EXTRA.includes(name))) return keys.filter(key => names.includes(key));
	}
	if (kind === "browser" && paths.every((path: string) => path.startsWith("assets/landing/hero/"))) return ["landing-deferred", "landing-hero", "browser-journey"];
	if (kind === "figure-expand") {
		const themes = paths.map((path: string) => /^crates\/richview-core\/assets\/(sleek|scientific|brutalist|air|hand-drawn)\.rstyle\.yaml$/.exec(path)?.[1]);
		if (themes.every(Boolean)) return keys.filter((key: string) => key === "expanded-regression" || themes.some(theme => FIGURE_WIDTHS.some(width => key === `theme/${theme}/${width}`)));
	}
	return [...keys];
}

/** Carry a failure of an older whole command to every current case; never reuse its old pass. */
export function expandLegacyCaseOutcomes(receipts: readonly CoverageReceipt[], aliases: Record<string, readonly string[]>, fallbacks: Record<string, string> = {}) {
	return receipts.map(receipt => {
		const outcomes = {...receipt.outcomes};
		for (const [id, units] of Object.entries(aliases)) {
			for (const old of Object.keys(outcomes).filter(key => key === id || key.startsWith(`${id}:case:`) && !units.includes(key))) {
				const status = outcomes[old]; delete outcomes[old];
				if (status === "fail") for (const unit of units) outcomes[unit] = "fail";
			}
		}
		for (const [id, unit] of Object.entries(fallbacks)) {
			for (const old of Object.keys(outcomes).filter(key => key.startsWith(`${id}:case:`))) {
				const status = outcomes[old]; delete outcomes[old];
				if (status === "fail") outcomes[unit] = "fail";
			}
		}
		return {...receipt, outcomes};
	});
}
