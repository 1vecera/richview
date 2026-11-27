// Host reason (ARC-03): select and verify independent figure-expansion cases for affected checks.
import {filterCases} from "./cases.mts";
import type {CaseEvidence} from "./cases.mts";
import type {BrowserCaseReport} from "./browser-cases.mts";

interface FigureReport extends BrowserCaseReport {
	cases: { theme: string; width: number; contracted: number; expanded: number; turned: boolean; lifecycle?: { sameCanvas: boolean; charts: number } }[];
	phone?: unknown[];
	upright?: boolean | object;
	tableInView?: boolean | object;
}

export const FIGURE_THEMES = Object.freeze(["sleek", "scientific", "brutalist", "air", "hand-drawn"]);
export const FIGURE_WIDTHS = Object.freeze([1440, 420, 320]);
export const FIGURE_CASES = Object.freeze([...FIGURE_THEMES.flatMap(theme => FIGURE_WIDTHS.map(width => `theme/${theme}/${width}`)), "expanded-regression"]);
const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

/** The affected dispatcher can select themed viewport cases and the shared regression journey. */
export function figureCases({env = process.env} = {}) {
	return filterCases(FIGURE_CASES, {key: value => value, env, parent: "affected-figure-expand"});
}

/** Build only needed themes; the shared regression uses the first theme. */
export function figureBuildThemes(selection: ReturnType<typeof figureCases>) {
	if (!selection.scope) return FIGURE_THEMES;
	const selected = new Set(selection.cases);
	return FIGURE_THEMES.filter(theme => FIGURE_WIDTHS.some(width => selected.has(`theme/${theme}/${width}`)) || theme === FIGURE_THEMES[0] && selected.has("expanded-regression"));
}

/** Require every selected case, its actual themed result and no browser errors. */
export function verifyFigureCases(report: FigureReport | undefined, requested: readonly string[], {engine = "chromium"} = {}): CaseEvidence {
	if (!Array.isArray(requested) || !requested.length) throw new Error("Figure evidence needs explicit requested cases");
	const filtered = figureCases({env: {RICHVIEW_VERIFY_PARENT: "affected-figure-expand", RICHVIEW_VERIFY_CASES: JSON.stringify(requested)}});
	const scope = filtered.scope!;
	const themed = scope.selected.filter(key => key.startsWith("theme/"));
	const actual = Array.isArray(report?.cases) ? report.cases.map(item => `theme/${item.theme}/${item.width}`) : [];
	if (report?.status !== "passed" || report.engine !== engine || !same(report.case_scope, scope) || !same(report.completed, scope.selected)
		|| !Array.isArray(report.cases) || !same(actual, themed) || !Array.isArray(report.errors) || report.errors.length
		|| report.cases.some(item => !Number.isFinite(item.contracted) || item.contracted <= 0 || !Number.isFinite(item.expanded) || item.expanded <= 0 || typeof item.turned !== "boolean" || item.lifecycle?.sameCanvas !== true || item.lifecycle?.charts !== 0)
		|| scope.selected.includes("expanded-regression") && (!Array.isArray(report.phone) || !report.phone.length || !report.upright || !report.tableInView)) throw new Error("Figure evidence does not prove the requested cases");
	return {...scope, completed: [...report.completed]};
}
