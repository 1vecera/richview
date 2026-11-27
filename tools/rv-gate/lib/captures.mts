// Captures and the exact pixel diff (DEL-27, DEL-28, DEL-29).
//
// Every dev merge captures, in Chromium on a fixed clock after fonts load and charts settle, with
// reduced motion:
// the fixed chart matrix (from the visual-matrix step), the every-block fixture in every builtin
// style, and the site pages, at the widths DEL-27 names. Captures are stored by content hash;
// identical bytes pass, and otherwise both images are decoded and every changed pixel counted.
// The threshold is exact: zero changed pixels unless the PR declares a visual change.

import { spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import type { Page } from "playwright";
import { listFiles } from "./glob.mts";
import { capturePath, sha256File, storeCapture } from "./store.mts";

export type BrowserHarness = {
	serveStatic(roots: Record<string, string>): Promise<{ base: string; close(): Promise<unknown> }>;
	chromiumPath(): string | undefined;
	waitForBenchmark(): Promise<void>;
};

type CapturePaths = { cwd: string; runDir: string; binDir: string };
type CaptureManifest = Record<string, string>;
type CaptureWindow = Window & {
	richview_runtime?: unknown;
	__richview?: { settled?(): Promise<unknown>; pending?(): unknown; charts?(): { geometry?: { kind?: string } }[] };
	__rvCapture?: { hooked?: boolean; settled?: boolean; error?: string };
};

export const WIDTHS = { fixture: [1440, 390, 320], site: [1440, 390, 320] };

/** The every-block fixture of DEL-27; a checkout without it throws, since no other report stands for it. */
export function everyBlockFixture(cwd: string) {
	const fixture = "examples/every-block";
	if (!existsSync(join(cwd, fixture, "report.rmd"))) throw new Error(`The every-block fixture ${fixture}/report.rmd is missing (DEL-27)`);
	return fixture;
}

/** The typography specimen of FEA-BR-13, captured like the every-block fixture; a checkout without it throws. */
export function typographySpecimen(cwd: string) {
	const specimen = "tools/rv-gate/fixtures/typography";
	if (!existsSync(join(cwd, specimen, "report.rmd"))) throw new Error(`The typography specimen ${specimen}/report.rmd is missing (FEA-BR-13)`);
	return specimen;
}

/** The parts captureAll takes: the matrix, the every-block fixture, the typography specimen and the site pages. */
export const PARTS = ["matrix", "fixture", "specimen", "site"];

/** Builtin style names, read from core's style files so the list follows the code. */
export function builtinStyles(cwd: string) {
	const dir = join(cwd, "crates/richview-core/assets");
	return readdirSync(dir)
		.filter((name) => name.endsWith(".rstyle.yaml"))
		.map((name) => name.replace(/\.rstyle\.yaml$/, ""))
		.sort();
}

export const SITE_PAGES = [
	["home", "/index.html"],
	["docs-index", "/docs/index.html"],
	["docs-guide", "/docs/authoring.html"],
	["docs-gallery", "/docs/charts.html"],
	["playground", "/playground.html"],
];

/**
 * Render a fixture (the every-block fixture by default) in each builtin style with the release CLI into
 * `runDir/<into>/<style>`; returns `{style: htmlPath}`.
 */
export function renderFixture({ cwd, runDir, binDir, source = everyBlockFixture(cwd), into = "fixtures" }: CapturePaths & { source?: string; into?: string }) {
	const fixture = source;
	const out: Record<string, string> = {};
	for (const style of builtinStyles(cwd)) {
		const folder = join(runDir, into, style);
		cpSync(join(cwd, fixture), folder, { recursive: true });
		const result = spawnSync(
			join(binDir, "richview"),
			["generate", join(folder, "report.rmd"), "--style", style, "--format", "json", "-o", "capture.html"],
			{ cwd, encoding: "utf8", env: { ...process.env, RICHVIEW_PUBLIC_DIR: join(cwd, "dist/web") } },
		);
		if (result.status !== 0) throw new Error(`generate ${fixture} --style ${style} failed: ${result.stderr || result.stdout}`);
		out[style] = join(folder, "Rendered", "capture.html");
	}
	return out;
}

/**
 * Pages run on Playwright's paused clock, so whatever reads the time (a render timing, a date, a
 * timer) reads the same value on every run; the clock moves only when `settle` advances it.
 */
export const CLOCK_START = new Date("2026-01-01T00:00:00Z");
const TICK_MS = 100;
const SETTLE_TICKS = 150;
const STABLE_TICKS = 3;
const CAPTURE_ATTEMPTS = 4;
const FONT_WAIT_MS = 15_000;
/**
 * Scrollers at fractional offsets (code blocks, the playground editor) had their glyphs and
 * rounded clips antialiased one level differently from run to run. Painting scrollers into their
 * parent layer and re-rasterising whole tiles instead of invalidated parts made them repeatable.
 * Both are raster settings. Readers' browsers may composite scrollers and use LCD text there, so
 * a capture's text and clip antialiasing can differ from a reader's screen. The review matrix
 * launches with the same settings, because its screenshots are the `matrix/*` captures.
 */
export const CHROMIUM_ARGS = [
	"--no-sandbox",
	"--font-render-hinting=none",
	"--disable-gpu-vsync",
	"--disable-prefer-compositing-to-lcd-text",
	"--disable-partial-raster",
];

/** One step of fake time, then a real pause so observers, fetches and decoding can deliver. */
async function tick(page: Page) {
	await page.clock.runFor(TICK_MS);
	await page.waitForTimeout(25);
}

/**
 * Report every observed element as in view, once, as soon as it is observed. A full-page capture
 * shows the whole page, so lazily hydrated charts and reveal-on-scroll blocks must all have run.
 * Scrolling through the page instead left antialiased edges one level apart between runs of the
 * same tree. As with the native observer, a target is reported once however often it is
 * observed, and not at all once it is unobserved or the observer disconnects. The entries carry
 * the real entry prototype, as own properties, so `instanceof` checks and property reads both work.
 * The cost: a block that fails to reveal on a real scroll cannot show up in a capture.
 */
export function seeEverything() {
	const Native = window.IntersectionObserver;
	const Entry = window.IntersectionObserverEntry;
	if (!Native || !Entry) return;
	window.IntersectionObserver = class extends Native {
		#callback: IntersectionObserverCallback;
		#targets = new Set<Element>();
		constructor(callback: IntersectionObserverCallback, options?: IntersectionObserverInit) {
			super(() => {}, options);
			this.#callback = callback;
		}
		observe(target: Element) {
			if (this.#targets.has(target)) return;
			this.#targets.add(target);
			queueMicrotask(() => {
				if (!this.#targets.has(target)) return;
				const box = target.getBoundingClientRect();
				const entry = Object.setPrototypeOf(
					{ target, isIntersecting: true, isVisible: true, intersectionRatio: 1, boundingClientRect: box, intersectionRect: box, rootBounds: null, time: performance.now() },
					Entry.prototype,
				);
				this.#callback([entry], this);
			});
		}
		unobserve(target: Element) {
			this.#targets.delete(target);
		}
		disconnect() {
			this.#targets.clear();
		}
		takeRecords() {
			return [];
		}
	};
}

/**
 * Advance the clock until the layout signature and the runtime's own `settled()` hold still for a
 * few ticks. Without this the full-page height and the lazy charts depend on how fast the run
 * happened to be. Returns `undefined` once settled, or why it did not settle.
 */
export async function settle(page: Page) {
	// Fonts load on real time, so the wait is bounded on the Node side, where the page clock
	// does not reach.
	const fonts = page.evaluate(async () => {
		// A focused text field blinks its caret on the browser's own timer, which no page clock
		// controls, so the capture shows the page with nothing focused. Focus rings are therefore
		// never captured.
		(document.activeElement as HTMLElement | null)?.blur?.();
		await document.fonts?.ready;
	});
	fonts.catch(() => {}); // after a timeout, closing the browser rejects it; nobody waits then
	let timer: NodeJS.Timeout | undefined;
	const timeout = new Promise<"timeout">((resolve) => {
		timer = setTimeout(resolve, FONT_WAIT_MS, "timeout");
	});
	const waited = await Promise.race([fonts, timeout]).finally(() => clearTimeout(timer));
	if (waited === "timeout") return `fonts did not load within ${FONT_WAIT_MS / 1000} s`;
	let last = "";
	let stable = 0;
	let state;
	for (let i = 0; i < SETTLE_TICKS && stable < STABLE_TICKS; i++) {
		await tick(page);
		state = await page.evaluate(() => {
			// The runtime may start after `load`, so its hook is looked for on every tick. A page
			// that carries the report runtime (its bindings' global or a chart payload) is not
			// ready until the runtime has started and settled.
			const api = (window as CaptureWindow).__richview;
			const watch = ((window as CaptureWindow).__rvCapture ??= {});
			const expectsRuntime = typeof (window as CaptureWindow).richview_runtime !== "undefined" || document.querySelector("[data-rv-payload]") !== null;
			if (api?.settled && !watch.hooked) {
				watch.hooked = true;
				api.settled().then(
					() => {
						watch.settled = true;
					},
					(error) => {
						watch.error = String(error);
					},
				);
			}
			const root = document.documentElement;
			const round = (v: number) => Math.round(v * 100) / 100;
			const boxes = [...document.querySelectorAll("figure, svg, canvas, img, iframe")].map((el) => {
				const r = el.getBoundingClientRect();
				return [r.x, r.y, r.width, r.height].map(round).join(",");
			});
			return {
				ready: document.fonts?.status !== "loading" && (watch.hooked ? watch.settled === true : !expectsRuntime),
				waitingForStart: expectsRuntime && !watch.hooked,
				error: watch.error,
				pending: watch.hooked && !watch.settled ? JSON.stringify(api?.pending?.() ?? null) : undefined,
				signature: JSON.stringify([root.scrollWidth, root.scrollHeight, boxes]),
			};
		});
		if (state.error) return `the runtime's settled() failed: ${state.error}`;
		stable = state.ready && state.signature === last ? stable + 1 : 0;
		last = state.signature;
	}
	if (stable >= STABLE_TICKS) return undefined;
	if (state?.waitingForStart) return `the report runtime did not start within ${(SETTLE_TICKS * TICK_MS) / 1000} s of page time`;
	return `the page did not settle within ${(SETTLE_TICKS * TICK_MS) / 1000} s of page time${state?.pending ? `; pending: ${state.pending}` : ""}`;
}

/**
 * List the visible charts of a settled page as `{name, chart, type, box}`: the capture that holds the chart,
 * its index in the runtime's list, its chart type from the geometry model, and its box in page pixels.
 */
async function chartBoxes(page: Page, name: string) {
	const charts = await page.evaluate(async () => {
		const models = (await (window as CaptureWindow).__richview?.charts?.()) ?? [];
		return [...document.querySelectorAll("figure[data-rv-chart]")].map((figure, chart) => {
			const box = figure.getBoundingClientRect();
			return { chart, type: models[chart]?.geometry?.kind ?? null, box: { x: Math.round(box.x), y: Math.round(box.y + window.scrollY), width: Math.round(box.width), height: Math.round(box.height) } };
		});
	});
	return charts.filter((entry: { box: { width: number; height: number; }; }) => entry.box.width > 0 && entry.box.height > 0).map((entry) => ({ name, ...entry }));
}

/**
 * Capture every page into `runDir/captures`, store each image by hash and return the manifest
 * `{name: sha256}`.
 */
export async function captureAll({ cwd, runDir, binDir, matrixDir, parts = PARTS }: CapturePaths & { matrixDir?: string; parts?: string[] }) {
	const { chromium } = await import(join(cwd, "node_modules/playwright/index.mjs")) as typeof import("playwright");
	const { serveStatic } = await import(join(cwd, "scripts/browser-harness.ts")) as BrowserHarness;
	const shots = join(runDir, "captures");
	mkdirSync(shots, { recursive: true });
	const manifest: CaptureManifest = {};
	const attempts: Record<string, number> = {};
	const add = (name: string, path: string) => {
		manifest[name] = storeCapture(path);
	};

	if (parts.includes("matrix") && matrixDir && existsSync(matrixDir)) {
		for (const file of listFiles(matrixDir, "", new Set()).filter((f) => f.endsWith(".png")).sort()) {
			add(`matrix/${file}`, join(matrixDir, file));
		}
	}

	const fixtures = parts.includes("fixture") ? renderFixture({ cwd, runDir, binDir }) : {};
	const specimens = parts.includes("specimen") ? renderFixture({ cwd, runDir, binDir, source: typographySpecimen(cwd), into: "specimen" }) : {};
	// Where each chart sits in which capture (the page: the captures are not sliced), for the type-evidence tool.
	const index: Awaited<ReturnType<typeof chartBoxes>> = [];
	const server = await serveStatic({ "/fixtures/": join(runDir, "fixtures"), "/specimen/": join(runDir, "specimen"), "/": join(cwd, "dist/web") });
	try {
		// Each capture gets its own browser: the GPU process keeps caches across contexts, and
		// antialiased clip corners came out one level apart depending on which pages ran before.
		const shoot = async (name: string, url: string, width: number) => {
			const browser = await chromium.launch({ headless: true, args: CHROMIUM_ARGS });
			try {
				const context = await browser.newContext({ viewport: { width, height: 900 }, deviceScaleFactor: 1, reducedMotion: "reduce", colorScheme: "light" });
				await context.addInitScript(seeEverything);
				const page = await context.newPage();
				await page.clock.install({ time: CLOCK_START });
				await page.clock.pauseAt(CLOCK_START);
				// Network idle: the runtime's WebAssembly and fonts have arrived before settling.
				const response = await page.goto(url, { waitUntil: "networkidle" });
				if (!response || response.status() >= 400) return `${name}: HTTP ${response?.status() ?? "no response"}`;
				const unsettled = await settle(page);
				if (unsettled) return `${name}: ${unsettled}`;
				// The first full-page capture rasterises the tiles below the fold for the first
				// time, and on some runs differed from every later one. It is discarded. After it,
				// a capture is kept once two consecutive screenshots agree byte for byte, and the
				// page fails when they do not within a few tries. `capture-attempts.json` records
				// how many screenshots each page needed after the discarded one.
				const options = { fullPage: true, animations: "disabled", caret: "hide" } as const;
				await page.screenshot(options);
				await tick(page);
				const path = join(shots, `${name.replaceAll("/", "__")}.png`);
				let previous;
				for (let attempt = 1; attempt <= CAPTURE_ATTEMPTS; attempt++) {
					const png = await page.screenshot(options);
					attempts[name] = attempt;
					if (previous?.equals(png)) {
						writeFileSync(path, png);
						add(name, path);
						index.push(...(await chartBoxes(page, name)));
						return undefined;
					}
					previous = png;
					await tick(page);
					await page.waitForTimeout(100);
				}
				return `${name}: no two consecutive captures out of ${CAPTURE_ATTEMPTS} agreed`;
			} catch (error) {
				return `${name}: ${(error as Error).message.split("\n")[0]}`;
			} finally {
				await browser.close();
			}
		};
		const problems = [];
		for (const [part, set, into] of [["fixture", fixtures, "fixtures"], ["typography", specimens, "specimen"]] as const) {
			for (const [style, html] of Object.entries(set)) {
				const rel = html.slice(join(runDir, into).length + 1);
				for (const width of WIDTHS.fixture) {
					const problem = await shoot(`${part}/${style}/${width}`, `${server.base}/${into}/${rel}`, width);
					if (problem) problems.push(problem);
				}
			}
		}
		for (const [name, path] of parts.includes("site") ? SITE_PAGES : []) {
			for (const width of WIDTHS.site) {
				const problem = await shoot(`site/${name}/${width}`, `${server.base}${path}`, width);
				if (problem) problems.push(problem);
			}
		}
		writeFileSync(join(runDir, "captures.json"), `${JSON.stringify(manifest, null, 2)}\n`);
		writeFileSync(join(runDir, "captures-index.json"), `${JSON.stringify({ widths: WIDTHS.fixture, charts: index }, null, 2)}\n`);
		writeFileSync(join(runDir, "capture-attempts.json"), `${JSON.stringify(attempts, null, 2)}\n`);
		return { manifest, problems, attempts };
	} finally {
		await server.close();
	}
}

/**
 * Compare two manifests. Identical hashes pass; changed images are decoded in a browser and their
 * changed pixels counted, with an overlay written beside the run. Returns per-name results.
 */
export async function diffManifests({ cwd, runDir, before = {}, after = {} }: { cwd: string; runDir: string; before?: CaptureManifest; after?: CaptureManifest }) {
	const names = [...new Set([...Object.keys(before), ...Object.keys(after)])].sort();
	const results = [];
	const changed = names.filter((name) => before[name] !== after[name]);
	if (!changed.length) return names.map((name) => ({ name, status: "same" }));
	const { chromium } = await import(join(cwd, "node_modules/playwright/index.mjs")) as typeof import("playwright");
	const browser = await chromium.launch({ headless: true });
	const page = await browser.newPage();
	await page.setContent("<canvas></canvas>");
	const diffDir = join(runDir, "diffs");
	mkdirSync(diffDir, { recursive: true });
	try {
		for (const name of names) {
			if (before[name] === after[name]) {
				results.push({ name, status: "same" });
				continue;
			}
			if (!before[name] || !after[name]) {
				results.push({ name, status: before[name] ? "removed" : "added", before: before[name], after: after[name] });
				continue;
			}
			const a = readFileSync(capturePath(before[name])).toString("base64");
			const b = readFileSync(capturePath(after[name])).toString("base64");
			const result = await page.evaluate(
				async ([a, b]) => {
					const load = (src: string) =>
						new Promise<HTMLImageElement>((accept, reject) => {
							const image = new Image();
							image.onload = () => accept(image);
							image.onerror = reject;
							image.src = `data:image/png;base64,${src}`;
						});
					const [ia, ib] = await Promise.all([load(a), load(b)]);
					const width = Math.max(ia.width, ib.width);
					const height = Math.max(ia.height, ib.height);
					const read = (image: HTMLImageElement) => {
						const canvas = new OffscreenCanvas(width, height);
						const context = canvas.getContext("2d")!;
						context.drawImage(image, 0, 0);
						return context.getImageData(0, 0, width, height).data;
					};
					const pa = read(ia);
					const pb = read(ib);
					const overlay = new OffscreenCanvas(width, height);
					const octx = overlay.getContext("2d")!;
					octx.drawImage(ib, 0, 0);
					octx.fillStyle = "rgba(255,255,255,0.75)";
					octx.fillRect(0, 0, width, height);
					const out = octx.getImageData(0, 0, width, height);
					let changedPixels = 0;
					for (let i = 0; i < pa.length; i += 4) {
						if (pa[i] !== pb[i] || pa[i + 1] !== pb[i + 1] || pa[i + 2] !== pb[i + 2] || pa[i + 3] !== pb[i + 3]) {
							changedPixels++;
							out.data[i] = 230;
							out.data[i + 1] = 0;
							out.data[i + 2] = 60;
							out.data[i + 3] = 255;
						}
					}
					octx.putImageData(out, 0, 0);
					const blob = await overlay.convertToBlob({ type: "image/png" });
					const bytes = new Uint8Array(await blob.arrayBuffer());
					let binary = "";
					for (const byte of bytes) binary += String.fromCharCode(byte);
					return {
						changedPixels,
						total: width * height,
						sizeChanged: ia.width !== ib.width || ia.height !== ib.height,
						overlay: btoa(binary),
					};
				},
				[a, b],
			);
			const overlayPath = join(diffDir, `${name.replaceAll("/", "__")}.png`);
			writeFileSync(overlayPath, Buffer.from(result.overlay, "base64"));
			results.push({
				name,
				status: result.changedPixels === 0 && !result.sizeChanged ? "same-pixels" : "changed",
				changedPixels: result.changedPixels,
				total: result.total,
				sizeChanged: result.sizeChanged,
				before: before[name],
				after: after[name],
				overlay: overlayPath,
			});
		}
	} finally {
		await browser.close();
	}
	return results;
}

export { sha256File };

/**
 * The captures step's result when no baseline exists to compare with: `unrun`, never a pass, since
 * nothing was compared. The baseline is seeded only after the run's other steps all passed (the
 * gate writes it after a merge or a passing release), as the perf step seeds its own.
 */
export function noBaselineResult(name: string, manifest: CaptureManifest) {
	return { status: "unrun" as const, reason: `No ${name} baseline yet, so nothing was compared; a later run with every other step passing records it.`, details: { captures: Object.keys(manifest).length } };
}
