// The a11y step of the release run (DEL-58, FEA-RP-49): an automated WCAG audit and a keyboard pass
// over the every-block fixture, in every builtin style at 390 and 1440 px, in the light scheme the
// captures use (DEL-27). Any violation fails.
//
// Audit: axe-core (a dev-only test tool, ARC-68) injected from node_modules, run with the WCAG 2.0,
// 2.1 and 2.2 A and AA rule tags. No rule is disabled; a node can be excluded only through
// EXCLUSIONS below, with its reason, and the report lists what was excluded.
//
// Keyboard pass: Tab through the page and check that every interactive element is reached in DOM
// order, shows a focus indicator, that focus never gets trapped and that each chart's frame is
// reachable and named with its title and generated summary (FEA-CH-55). Charts draw with WebGL
// (INT-31), so a chart is judged by its container's name, summary and Table, never by its marks.
//
// Usage: node tools/rv-gate/a11y.mts --bin target/release/richview --out <dir> [--public <dist/web>] [--only <style>] [--cwd <repo>]

import { spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { builtinStyles } from "./lib/captures.mts";
import type { BrowserHarness } from "./lib/captures.mts";
import type { NodeResult } from "axe-core";
import type { Page } from "playwright";

type Violation = { id: string; impact?: string | null; help?: string; helpUrl?: string; nodes?: { target?: readonly unknown[]; impact?: string | null }[] };
type AuditPage = { style: string; width: number; violations?: Violation[] };
type ViolationRow = ReturnType<typeof summarize>[number];
type FocusStyle = Partial<Record<"outlineStyle" | "outlineWidth" | "outlineColor" | "outlineOffset" | "boxShadow" | "borderColor" | "borderWidth" | "backgroundColor" | "color" | "textDecorationLine" | "pseudo", string>>;
type FocusStop = { id: number | null; label: string; native?: boolean; before?: FocusStyle | null; after?: FocusStyle | null };
type ChartStop = { id: string; title: string; name: string; tabbable: boolean };
type KeyboardPass = { expected: { id: number; label: string }[]; visits: FocusStop[]; wrapped: boolean; charts: ChartStop[] };
type PageProbe = { ids: WeakMap<Element, number>; signature: (el: Element) => FocusStyle; label: (el: Element) => string; before: Map<number, FocusStyle> };
type AuditWindow = Window & { __a11y: PageProbe; __richview?: { settled?: () => Promise<unknown> }; axe: typeof import("axe-core") };
type AuditRun = { cwd: string; bin: string; out: string; publicDir?: string };
type Diagnostic = { level?: string; code: string; message: string };
type Exclusion = { rule: string; selector: string; reason: string };


export const FIXTURE = "examples/every-block";
/** DEL-58: the widths of the audit; the light scheme is the only one DEL-27 captures. */
export const WIDTHS = [1440, 390];
export const SCHEME = "light";
export const AXE_TAGS = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"];
/**
 * Nodes an axe rule does not apply to, each with its reason: `{rule, selector, reason}`. A node
 * matching both is dropped from the violations and listed under `excluded`. Never a whole rule.
 */
export const EXCLUSIONS: Exclusion[] = [];
/** The selector of a chart's container and of its focusable frame (FEA-CH-55). */
export const CHART = { figure: "figure[data-rv-chart]", frame: ".rv-chart__frame", title: ".rv-chart__title" };
const SETTLE_MS = 30_000;

/** Every style at every width: the audit's pages. */
export function matrix(styles: string[], widths = WIDTHS) {
	return styles.flatMap((style) => widths.map((width) => ({ style, width, id: `${style}-${width}` })));
}

/** One flat row per violating node, with the rule's id, impact, help URL and the node's selector. */
export function summarize(pages: AuditPage[]) {
	const rows = [];
	for (const page of pages) {
		for (const rule of page.violations ?? []) {
			for (const node of rule.nodes ?? [{ target: ["(page)"] }]) {
				rows.push({ style: page.style, width: page.width, rule: rule.id, impact: rule.impact ?? node.impact ?? "unknown", selector: (node.target ?? []).join(" "), help: rule.help, helpUrl: rule.helpUrl });
			}
		}
	}
	return rows;
}

/** The rows grouped by rule: `{rule, impact, count, example, helpUrl, pages}` ordered by count. */
export function groupByRule(rows: ViolationRow[]) {
	const groups = new Map<string, Omit<ViolationRow, "style" | "width" | "selector"> & { count: number; example: string; pages: Set<string> }>();
	for (const row of rows) {
		const group = groups.get(row.rule) ?? { rule: row.rule, impact: row.impact, count: 0, example: `${row.selector} (${row.style}, ${row.width})`, help: row.help, helpUrl: row.helpUrl, pages: new Set() };
		group.count += 1;
		group.pages.add(`${row.style}-${row.width}`);
		groups.set(row.rule, group);
	}
	return [...groups.values()].map((g) => ({ ...g, pages: [...g.pages].sort() })).sort((a, b) => b.count - a.count || a.rule.localeCompare(b.rule));
}

/** Whether a focus indicator shows: the focused signature differs from the unfocused one in outline, shadow, border, fill or underline, or a painted ::before or ::after appears (the chart frame's ring, STY-38). */
export function focusIndicated(before: FocusStyle | null | undefined, after: FocusStyle | null | undefined) {
	if (!after) return false;
	const outline = (s: FocusStyle) => (s.outlineStyle === "none" || !(parseFloat(s.outlineWidth!) > 0) ? "none" : `${s.outlineStyle} ${s.outlineWidth} ${s.outlineColor} ${s.outlineOffset}`);
	// A stop the probe did not list (the browser made it focusable) has no unfocused state: it needs an outline or a shadow.
	if (!before) return outline(after) !== "none" || (after.boxShadow ?? "none") !== "none" || Boolean(after.pseudo);
	if (outline(before) !== outline(after)) return true;
	if (after.pseudo && after.pseudo !== (before.pseudo ?? "")) return true;
	return (["boxShadow", "borderColor", "borderWidth", "backgroundColor", "color", "textDecorationLine"] as const).some((key) => before[key] !== after[key]);
}

/**
 * Judge a keyboard pass. `data`:
 *  - `expected`: [{id, label}] the interactive elements in DOM order;
 *  - `visits`: [{id, label, before, after}] each Tab stop in order (`id` is null for an element outside `expected`);
 *  - `wrapped`: Tab left the page's last stop (focus returned to the start) before the limit;
 *  - `charts`: [{id, title, name, tabbable}].
 * Returns `[{kind, message}]`.
 */
export function judgeKeyboard({ expected, visits, wrapped, charts }: KeyboardPass) {
	const failures = [];
	const seen = new Map<number, number>();
	let lastIndex = -1;
	const index = new Map(expected.map((e, i) => [e.id, i]));
	visits.forEach((visit, i: number) => {
		if (visit.id !== null) {
			if (seen.has(visit.id)) failures.push({ kind: "trap", message: `Focus came back to ${visit.label} before leaving the page (stop ${i + 1}, first seen at stop ${seen.get(visit.id)! + 1})` });
			seen.set(visit.id, i);
			const at = index.get(visit.id)!;
			if (at < lastIndex) failures.push({ kind: "order", message: `${visit.label} was reached after a later element in the DOM (stop ${i + 1})` });
			lastIndex = Math.max(lastIndex, at);
		}
		if (!visit.native && !focusIndicated(visit.before, visit.after)) failures.push({ kind: "focus-visible", message: `${visit.label} shows no focus indicator` });
	});
	for (const e of expected) if (!seen.has(e.id)) failures.push({ kind: "unreachable", message: `${e.label} was never reached by Tab` });
	if (!wrapped) failures.push({ kind: "trap", message: "Tab never left the page: the focus order has no end (a focus trap, or a loop)" });
	for (const chart of charts) {
		const where = `Chart ${chart.id}`;
		if (!chart.tabbable) failures.push({ kind: "chart", message: `${where} (${chart.title}) has no focusable frame` });
		if (!chart.name) failures.push({ kind: "chart", message: `${where} (${chart.title}) has no accessible name` });
		else if (!chart.name.startsWith(chart.title)) failures.push({ kind: "chart", message: `${where}: accessible name "${chart.name}" does not start with its title "${chart.title}"` });
		else if (chart.name.slice(chart.title.length).replace(/[\s.]+/g, "").length === 0) failures.push({ kind: "chart", message: `${where} (${chart.title}) is named by its title alone, with no generated summary (FEA-CH-55)` });
	}
	return failures;
}

/** Runs in the page: tag the interactive elements, in DOM order, and record their unfocused style. */
export function pageProbe() {
	// A ::before or ::after counts when it is painted: it has content and a border, outline, shadow or fill.
	const PAINTED = (el: Element) =>
		["::before", "::after"]
			.map((at) => {
				const p = getComputedStyle(el, at);
				if (p.content === "none" || p.content === "normal") return "";
				const border = p.borderTopStyle !== "none" && parseFloat(p.borderTopWidth) > 0;
				const outline = p.outlineStyle !== "none" && parseFloat(p.outlineWidth) > 0;
				const fill = p.backgroundColor !== "transparent" && !/rgba\(.*,\s*0\)$/.test(p.backgroundColor);
				return border || outline || p.boxShadow !== "none" || fill ? `${at} ${p.borderTopStyle} ${p.borderTopWidth} ${p.borderTopColor} ${p.outlineStyle} ${p.boxShadow} ${p.backgroundColor}` : "";
			})
			.filter(Boolean)
			.join(" | ");
	const SIGNATURE = (el: Element) => {
		const s = getComputedStyle(el);
		return { outlineStyle: s.outlineStyle, outlineWidth: s.outlineWidth, outlineColor: s.outlineColor, outlineOffset: s.outlineOffset, boxShadow: s.boxShadow, borderColor: s.borderTopColor, borderWidth: s.borderTopWidth, backgroundColor: s.backgroundColor, color: s.color, textDecorationLine: s.textDecorationLine, pseudo: PAINTED(el) };
	};
	const label = (el: Element) => {
		const text = (el.getAttribute("aria-label") || el.textContent || el.getAttribute("href") || "").replace(/\s+/g, " ").trim().slice(0, 50);
		const cls = el.classList.length ? `.${[...el.classList].slice(0, 2).join(".")}` : "";
		return `<${el.tagName.toLowerCase()}${el.id ? `#${el.id}` : cls}>${text ? ` "${text}"` : ""}`;
	};
	const visible = (el: Element) => {
		const r = el.getBoundingClientRect();
		return r.width > 0 && r.height > 0 && el.checkVisibility({ checkVisibilityCSS: true, visibilityProperty: true });
	};
	const candidates = [...document.querySelectorAll<HTMLInputElement>("a[href], button, input, select, textarea, summary, iframe, audio[controls], video[controls], [tabindex], [contenteditable]:not([contenteditable=false])")];
	const radios = new Set();
	const tabbable = candidates.filter((el) => {
		if (el.disabled || el.closest("[inert], fieldset[disabled]") || el.tabIndex < 0 || !visible(el)) return false;
		const details = el.closest("details:not([open])");
		if (details && !(el.tagName === "SUMMARY" && el.parentElement === details)) return false;
		if (el.type === "radio" && el.name) {
			const group = `${el.form?.id ?? ""}/${el.name}`;
			const checked = document.querySelector(`input[type=radio][name="${CSS.escape(el.name)}"]:checked`);
			if (radios.has(group) || (checked && checked !== el)) return false;
			radios.add(group);
		}
		return true;
	});
	(window as unknown as AuditWindow).__a11y = { ids: new WeakMap(), signature: SIGNATURE, label, before: new Map() };
	return tabbable.map((el, i) => {
		(window as unknown as AuditWindow).__a11y.ids.set(el, i);
		(window as unknown as AuditWindow).__a11y.before.set(i, SIGNATURE(el));
		return { id: i, label: label(el) };
	});
}

/** Runs in the page: the focused element as a Tab stop, or `{gone: true}` when focus is on the page itself. */
export function currentStop(): (FocusStop & { gone?: never; tag: string }) | { gone: true } {
	const el = document.activeElement;
	if (!el || el === document.body || el === document.documentElement) return { gone: true };
	const a = (window as unknown as AuditWindow).__a11y;
	const id = a.ids.has(el) ? a.ids.get(el)! : null;
	// Keyboard exclusion: a media element's native controls and an iframe's document draw their own focus rings inside the browser's shadow tree or the frame, which the element's computed style cannot show.
	return { id, label: a.label(el), tag: el.tagName, native: ["AUDIO", "VIDEO", "IFRAME"].includes(el.tagName), before: id === null ? null : a.before.get(id), after: a.signature(el) };
}

/** Runs in the page: each chart's figure with its title, frame name and whether the frame is tabbable. */
export function chartProbe(sel: typeof CHART) {
	return [...document.querySelectorAll(sel.figure)].map((fig) => {
		const frame = fig.querySelector<HTMLElement>(sel.frame);
		return { id: fig.id, title: fig.querySelector(sel.title)?.textContent!.trim() ?? "", name: (frame?.getAttribute("aria-label") ?? "").trim(), tabbable: Boolean(frame) && frame!.tabIndex >= 0 };
	});
}

/** Tab through the page and judge it. Tab leaving the last stop for the page itself (as in Chromium) is the end; `page` is a settled Playwright page. */
export async function keyboardPass(page: Page) {
	const expected = await page.evaluate(pageProbe);
	const charts = await page.evaluate(chartProbe, CHART);
	await page.evaluate(() => {
		(document.activeElement as HTMLElement | null)?.blur?.();
		window.scrollTo(0, 0);
	});
	const visits: FocusStop[] = [];
	let wrapped = false;
	const limit = expected.length * 2 + 10;
	for (let press = 0; press < limit; press++) {
		await page.keyboard.press("Tab");
		const stop = await page.evaluate(currentStop);
		if (stop.gone) {
			wrapped = true;
			break;
		}
		const last = visits.at(-1);
		// A frame or a media element's controls keep the focus while Tab moves through their insides.
		if (last && last.id === stop.id && stop.native) continue;
		visits.push(stop);
		// Focus back on a stop already visited is a loop; it is judged as a trap, and Tab need not go on.
		if (stop.id !== null && visits.slice(0, -1).some((v) => v.id === stop.id)) break;
	}
	return { expected, visits, wrapped, charts, failures: judgeKeyboard({ expected, visits, wrapped, charts }) };
}

/** Wait for fonts and the runtime to settle; returns why it did not, or undefined. */
export async function settle(page: Page) {
	try {
		await page.evaluate(
			async (ms: number) => {
				const timeout = new Promise((_, reject) => setTimeout(() => reject(new Error(`the runtime did not settle within ${ms / 1000} s`)), ms));
				// A page with charts, tabs or scrollytelling needs the runtime: wait for it to appear rather than
				// auditing the static page as if it had settled (review 1d note 10).
				const needsRuntime = Boolean(document.querySelector("figure[data-rv-chart], [data-rv-tab], [data-rv-scrolly]"));
				const runtime = async () => {
					while (needsRuntime && typeof (window as unknown as AuditWindow).__richview?.settled !== "function") await new Promise((done) => setTimeout(done, 50));
					return (window as unknown as AuditWindow).__richview?.settled?.();
				};
				await Promise.race([Promise.all([document.fonts?.ready, runtime()]), timeout]);
			},
			SETTLE_MS,
		);
	} catch (error) {
		return (error as Error).message.split("\n")[0];
	}
	return undefined;
}

/** Run axe on the page and split off the declared exclusions. */
export async function audit(page: Page) {
	const result = await page.evaluate(async (tags) => {
		const out = await (window as unknown as AuditWindow).axe.run(document, { runOnly: { type: "tag", values: tags }, resultTypes: ["violations"] });
		return { violations: out.violations, version: (window as unknown as AuditWindow).axe.version };
	}, AXE_TAGS);
	const matches = (node: NodeResult, selector: string) => page.evaluate(([target, sel]) => document.querySelector(target)?.matches(sel) ?? false, [node.target.join(" "), selector]);
	const exclusions = EXCLUSIONS;
	if (!exclusions.length) return { ...result, excluded: [] };
	const kept = [];
	const excluded = [];
	for (const rule of result.violations) {
		const nodes = [];
		for (const node of rule.nodes) {
			const hit = exclusions.find((e) => e.rule === rule.id && node.target.length === 1);
			if (hit && (await matches(node, hit.selector))) excluded.push({ rule: rule.id, selector: node.target.join(" "), reason: hit.reason });
			else nodes.push(node);
		}
		if (nodes.length) kept.push({ ...rule, nodes });
	}
	return { ...result, violations: kept, excluded };
}

/** Render the fixture once per style into real folders under `out` (a symlinked /tmp trips report.root.escape). */
export function renderStyles({ cwd, bin, out, styles, publicDir }: AuditRun & { styles: string[] }) {
	const rendered: Record<string, string> = {};
	for (const style of styles) {
		const folder = join(out, "fixtures", style);
		rmSync(folder, { recursive: true, force: true });
		cpSync(join(cwd, FIXTURE), folder, { recursive: true, filter: (src) => !/[\\/](Rendered|research)$/.test(src) });
		const env = { ...process.env, ...(publicDir ? { RICHVIEW_PUBLIC_DIR: publicDir } : {}) };
		const run = spawnSync(bin, ["generate", join(folder, "report.rmd"), "--style", style, "-o", "report.html", "--format", "json"], { cwd, encoding: "utf8", env, maxBuffer: 64 << 20 });
		let errors: Diagnostic[] = [];
		try {
			errors = ((JSON.parse(run.stdout) as { diagnostics?: Diagnostic[] }).diagnostics ?? []).filter(d => d.level === "error");
		} catch {
			errors = [{ code: "cli", message: `exit ${run.status}: ${(run.stderr || run.stdout).trim().slice(0, 300)}` }];
		}
		if (run.status !== 0 || errors.length) throw new Error(`generate ${FIXTURE} --style ${style} failed: ${errors.map((e) => `${e.code}: ${e.message}`).join("; ")}`);
		rendered[style] = join(folder, "Rendered");
	}
	return rendered;
}

export async function runA11y({ cwd, bin, out, publicDir, only }: AuditRun & { only?: string }) {
	const started = Date.now();
	const binary = resolve(cwd, bin);
	if (!existsSync(binary)) throw new Error(`The release CLI ${binary} does not exist; the a11y step needs the build step first`);
	if (!existsSync(join(cwd, FIXTURE, "report.rmd"))) throw new Error(`The every-block fixture ${FIXTURE} is missing (FEA-RP-49)`);
	mkdirSync(out, { recursive: true });
	const real = realpathSync(out);
	const styles = builtinStyles(cwd).filter((s) => !only || s === only);
	const rendered = renderStyles({ cwd, bin: binary, out: real, styles, publicDir: publicDir ? resolve(cwd, publicDir) : existsSync(join(cwd, "dist/web")) ? join(cwd, "dist/web") : undefined });
	const { chromium } = await import(join(cwd, "node_modules/playwright/index.mjs")) as typeof import("playwright");
	const { serveStatic } = await import(join(cwd, "scripts/browser-harness.ts")) as BrowserHarness;
	const axePath = join(cwd, "node_modules/axe-core/axe.min.js");
	const server = await serveStatic({ "/": join(real, "fixtures") });
	const browser = await chromium.launch({ headless: true });
	mkdirSync(join(real, "reports"), { recursive: true });
	const pages = [];
	const problems = [];
	try {
		for (const { style, width, id } of matrix(styles)) {
			const context = await browser.newContext({ viewport: { width, height: 900 }, deviceScaleFactor: 1, reducedMotion: "reduce", colorScheme: SCHEME });
			try {
				// An init script is injected by the browser itself, so the report's own CSP (which allows no
				// inline script) stays in force while axe is present.
				await context.addInitScript({ path: axePath });
				const page = await context.newPage();
				const response = await page.goto(`${server.base}/${style}/Rendered/report.html`, { waitUntil: "load" });
				if (!response || response.status() >= 400) throw new Error(`HTTP ${response?.status() ?? "no response"}`);
				const unsettled = await settle(page);
				if (unsettled) throw new Error(unsettled);
				const axe = await audit(page);
				const keyboard = await keyboardPass(page);
				const report = { style, width, scheme: SCHEME, axe: axe.version, tags: AXE_TAGS, violations: axe.violations, excluded: axe.excluded, keyboard: { stops: keyboard.visits.length, expected: keyboard.expected.length, wrapped: keyboard.wrapped, charts: keyboard.charts.length, failures: keyboard.failures } };
				writeFileSync(join(real, "reports", `${id}.json`), `${JSON.stringify(report, null, 2)}\n`);
				pages.push(report);
			} catch (error) {
				problems.push({ page: id, message: (error as Error).message.split("\n")[0] });
			} finally {
				await context.close();
			}
		}
	} finally {
		await browser.close();
		await server.close();
	}
	const rows = summarize(pages);
	const keyboardFailures = pages.flatMap((p) => p.keyboard.failures.map((f) => ({ style: p.style, width: p.width, ...f })));
	const summary = {
		pages: pages.length,
		matrix: matrix(styles).length,
		seconds: (Date.now() - started) / 1000,
		violations: rows.length,
		byRule: groupByRule(rows),
		rows,
		keyboardFailures,
		excluded: pages.flatMap((p) => p.excluded.map((e) => ({ style: p.style, width: p.width, ...e }))),
		problems,
	};
	writeFileSync(join(real, "a11y.json"), `${JSON.stringify(summary, null, 2)}\n`);
	return summary;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
	const arg = (name: string) => {
		const at = process.argv.indexOf(name);
		return at < 0 ? undefined : process.argv[at + 1];
	};
	const bin = arg("--bin");
	const out = arg("--out");
	if (!bin || !out) {
		console.error("usage: a11y.mts --bin <richview> --out <dir> [--public <dist/web>] [--only <style>] [--cwd <repo>]");
		process.exit(2);
	}
	const summary = await runA11y({ cwd: resolve(arg("--cwd") ?? process.cwd()), bin, out: resolve(out), publicDir: arg("--public"), only: arg("--only") });
	for (const p of summary.problems) console.log(`FAIL ${p.page}: ${p.message}`);
	for (const g of summary.byRule) console.log(`FAIL axe ${g.rule} (${g.impact}) x${g.count} on ${g.pages.length} pages, e.g. ${g.example}: ${g.help} ${g.helpUrl}`);
	for (const f of summary.keyboardFailures) console.log(`FAIL keyboard [${f.style} ${f.width}] ${f.kind}: ${f.message}`);
	for (const e of summary.excluded) console.log(`excluded ${e.rule} ${e.selector}: ${e.reason}`);
	console.log(`a11y: ${summary.pages}/${summary.matrix} pages in ${summary.seconds.toFixed(1)} s; ${summary.violations} axe violations, ${summary.keyboardFailures.length} keyboard failures, ${summary.problems.length} pages not audited`);
	process.exit(summary.violations || summary.keyboardFailures.length || summary.problems.length ? 1 : 0);
}
