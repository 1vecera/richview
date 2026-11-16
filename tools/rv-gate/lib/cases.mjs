// Host reason (ARC-03): admit explicitly requested affected cases without changing full suites.

/** Leave an unfiltered suite untouched; filtered cases retain the complete and selected inventories. */
export function filterCases(cases, {key, env = process.env, parent = "affected-interaction"} = {}) {
	if (env.RICHVIEW_VERIFY_CASES === undefined) return {cases};
	if (env.RICHVIEW_VERIFY_PARENT !== parent) throw new Error("Case filtering needs its affected dispatcher");
	if (!Array.isArray(cases) || typeof key !== "function") throw new Error("Case filtering needs a case inventory");
	const cohort = cases.map(key), wanted = JSON.parse(env.RICHVIEW_VERIFY_CASES);
	const valid = values => Array.isArray(values) && values.length > 0 && values.every(value => typeof value === "string" && value.length > 0) && new Set(values).size === values.length;
	if (!valid(cohort) || !valid(wanted) || wanted.some(value => !cohort.includes(value))) throw new Error("Case filter must name unique existing cases");
	const selected = cohort.filter(value => wanted.includes(value));
	return {cases: cases.filter((_, index) => selected.includes(cohort[index])), scope: {cohort, selected}};
}

/** A caller's environment cannot narrow any full, candidate or launch-phase verification. */
export function forbidInheritedCases(command, phase, env = process.env) {
	if (Object.hasOwn(env, "RICHVIEW_VERIFY_CASES") && (["full", "prod"].includes(command)
		|| phase === "launch" && ["fast", "pre-pr", "dev", "release", "affected"].includes(command))) throw new Error("RICHVIEW_VERIFY_CASES cannot be set for a full, candidate or launch run");
}
