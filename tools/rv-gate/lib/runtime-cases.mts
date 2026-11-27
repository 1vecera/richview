// Host reason (ARC-03): restrict report-runtime inputs only when the affected dispatcher requests cases.
import {filterCases} from "./cases.mts";
import {reportCohort} from "./report-inputs.mts";

/** Preserve unfiltered options and derive filtered case names from the source's report inventory. */
export function runtimeCases<Input>(root: string, extra: Record<string, Input>, {env = process.env} = {}) {
	if (env.RICHVIEW_VERIFY_CASES === undefined) return {options: {extra}};
	const cohort = reportCohort(root, {extra: Object.keys(extra)});
	const filtered = filterCases(cohort, {key: name => name, env, parent: "affected-runtime"});
	const wanted = new Set(filtered.cases);
	return {options: {only: filtered.cases, extra: Object.fromEntries(Object.entries(extra).filter(([name]) => wanted.has(name)))}, scope: filtered.scope};
}
