// Host reason (ARC-03): select and verify independent browser journeys for affected checks.
import {filterCases} from "./cases.mjs";

export const BROWSER_CASES = Object.freeze(["landing-deferred", "landing-strip", "site-preview-access", "runtime-loader", "landing-hero", "browser-journey"]);
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

/** Only the affected browser dispatcher may request a subset of the fixed journey inventory. */
export function browserCases({env = process.env} = {}) {
	return filterCases(BROWSER_CASES, {key: value => value, env, parent: "affected-browser"});
}

/** Admit a partial result only when the exact requested journeys completed without errors. */
export function verifyBrowserCases(report, requested, {engine = "chromium"} = {}) {
	if (!Array.isArray(requested) || !requested.length) throw new Error("Browser case evidence needs explicit requested journeys");
	const filtered = browserCases({env: {RICHVIEW_VERIFY_PARENT: "affected-browser", RICHVIEW_VERIFY_CASES: JSON.stringify(requested)}});
	if (report?.status !== "passed" || report.engine !== engine || !same(report.case_scope, filtered.scope) || !same(report.completed, filtered.scope.selected)
		|| !Array.isArray(report.errors) || report.errors.length) throw new Error("Browser case evidence does not prove the requested journeys");
	return {...filtered.scope, completed: [...report.completed]};
}
