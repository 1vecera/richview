// The perf step of the release run (DEL-51, PRF-02, PRF-04, PRF-05): sizes, instruction counts and
// peak footprints always; timing holds the host benchmark lock and waits up to two minutes for
// work and load to drain, then records any remaining load. A failed timing under load repeats the
// whole timing stage once; attempt 2 alone decides it (lead30Oct, PRF-02). Memory follows macOS methods
// (PRF-03, PRF-40): `/usr/bin/time -l`, peak physical footprint.
//
// Usage: node tools/rv-gate/perf.mts --bin target/release/richview --out <dir>
//          [--dist dist/web] [--stages sizes,cli,timing] [--baseline <file>] [--update-baseline]
//          [--max-wait <ms>] [--load-limit <n>] [--no-lock]
// The `types` stage (PRF-07's all-types fixture, FEA-CH-75) times every chart of examples/every-block alone;
// `--stages types` is an optional disclosure, never a gate, and is omitted from full runs (Daniel29Oct).
// `--no-lock` skips the lock for a check by hand, and perf.json says so.
// The accepted baseline (PRF-05) lives in the gate home, not in the checkout, so it outlives every release
// worktree: the first capture on a host becomes the baseline (PRF-08); only `--update-baseline` after
// Daniel accepted a declared cost, or an improvement beyond the margin, changes it.
// Exit: 0 pass, 1 a budget failed, 3 performance inconclusive.
import { launchBrowser } from "./lib/browser-launch.mts";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { accessSync, constants, cpSync, existsSync, readdirSync, mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { loadavg, platform } from "node:os";
import { basename, delimiter, dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { setTimeout as sleep } from "node:timers/promises";
import { BUDGETS, RELAX_SIZE_BUDGETS_UNTIL_LAUNCH, acceptSizesAndMemory, busyProcesses, chartBlocks, chartKey, compareBaseline, frameStats, isSizeBudget, judge, judgeTypes, median, oneChartReport, parseTimeL, quantile, quietCapture, reportHtmlBytes, typeIdentity, typeKey } from "./lib/perf.mts";
import { acquireLock, gateHome } from "./lib/store.mts";
import type { AddressInfo } from "node:net";
import type { BrowserContext } from "playwright";
import type { MeasurementCosts, PerfAcceptance, PerfBaseline, PerfMeasured, PerfSource, TypeBaseline } from "./lib/perf.mts";
import { CONTROL_RESPONSE_BUDGETS, measureControlResponse } from "./control-response.mts";
import { QUIET_FILE } from "./lib/quiet.mts";
export { QUIET_FILE } from "./lib/quiet.mts";

/** Release budgets, including every control case measured under the existing quiet-host hold. */
export const PERF_BUDGETS: typeof BUDGETS = { ...BUDGETS, ...CONTROL_RESPONSE_BUDGETS };
const WEBKIT_BUDGETS = ["webkit.frame.p95", "webkit.frame.max"];

/** Only the explicitly admitted prelaunch full omits the two WebKit measurements. */
export function perfBudgetsFor(chromiumOnly: boolean) {
	return chromiumOnly ? Object.fromEntries(Object.entries(PERF_BUDGETS).filter(([key]) => !WEBKIT_BUDGETS.includes(key))) : PERF_BUDGETS;
}

/** The CLI fixtures, small to dense (docs/PERFORMANCE.md's three sizes). */
export const FIXTURES: Record<string, string> = {
	small: "examples/hello-world",
	flagship: "examples/openai-vs-anthropic",
	dense: "benchmarks/chart-regression",
};
/** PRF-07's all-types fixture: 57 charts over all 56 chart types and every rich block type. */
const ALL_TYPES = "examples/every-block";
const SKIP_COPY = new Set(["research", "Rendered", "node_modules", ".git"]);
export const PERF_SAMPLING = { cli: 7, timing: 10, frame: 10, warmup: 1 };
export const DEFAULT_STAGES = ["sizes", "cli", "timing"];
const RUNS = PERF_SAMPLING.cli;
const TIMING_RUNS = PERF_SAMPLING.timing;
// The all-types rounds run every chart once per round (the native rounds discard round 0), so that host drift spreads
// over all charts instead of landing on the last ones. The median of the rest is a chart's time.
const TYPE_NATIVE_ROUNDS = 5;
const TYPE_MOUNT_ROUNDS = 3;

const read = (path: string) => readFileSync(path);
const readText = (path: string) => (existsSync(path) ? readFileSync(path, "utf8") : undefined);

/**
 * The host's gzip that the size stage runs: the first on PATH, and its version line. Both go in
 * perf.json and the receipt, so a macOS change to it shows.
 */
export function hostGzip(path = process.env.PATH ?? "") {
	for (const dir of path.split(delimiter).filter(Boolean)) {
		const file = join(dir, "gzip");
		try { accessSync(file, constants.X_OK); } catch { continue; }
		const version = spawnSync(file, ["--version"], { encoding: "utf8" });
		return { path: file, version: `${version.stdout ?? ""}${version.stderr ?? ""}`.trim().split("\n")[0] };
	}
	throw new Error("No gzip on PATH: the size stage measures gzip -9 with the host's gzip");
}

/**
 * The gzip -9 bytes of a file as the host's gzip writes them, without the file's name or time (`-n`).
 * Node's own zlib differs by build: the official Node 26.5.0 that DEL-49 pins bundles Chromium's zlib,
 * which compresses the same bytes larger than the system zlib the accepted baseline was captured
 * with. The baseline is the host's (PRF-08), and so is the compressor.
 */
export function gzipBytes(path: string, gzip = "gzip") {
	const result = spawnSync(gzip, ["-9", "-n", "-c", path], { maxBuffer: 2 ** 31 - 1 });
	if (result.error || result.status !== 0) throw new Error(`gzip -9 -n ${path} failed: ${result.error?.message ?? result.stderr}`);
	return result.stdout.length;
}

/** Sizes after gzip -9 of what the build delivers (PRF-30, PRF-31) and of what it ships (PRF-05). */
export function measureSizes({ dist, bin }: {dist: string; bin: string}) {
	const compressor = hostGzip();
	const gz = (path: string) => gzipBytes(path, compressor.path);
	const site = ["richview_site_bg.wasm", "richview_site.js", "richview-fonts.css"];
	const runtime = ["richview_runtime_bg.wasm", "richview_runtime.js"];
	const sum = (files: string[]) => files.reduce((total: number, file: string) => total + gz(join(dist, file)), 0);
	return {
		values: { "site.payload": sum(site), "report-runtime.payload": sum(runtime) },
		sizes: {
			"wasm.site": gz(join(dist, "richview_site_bg.wasm")),
			"wasm.runtime": gz(join(dist, "richview_runtime_bg.wasm")),
			"binary.richview": gz(bin),
		},
		compressor,
	};
}

function stage(cwd: string, out: string, name: string) {
	const folder = join(out, "inputs", name);
	rmSync(folder, { recursive: true, force: true });
	cpSync(join(cwd, FIXTURES[name]), folder, { recursive: true, filter: (src) => !SKIP_COPY.has(basename(src)) });
	return folder;
}

/** One complete offline render under `time -l`; the card says peak physical footprint, not RSS (PRF-03). */
function renderOnce(bin: string, folder: string) {
	const run = spawnSync("/usr/bin/time", ["-l", bin, "generate", "report.rmd", "-o", "out.html"], { cwd: folder, encoding: "utf8", maxBuffer: 1 << 26 });
	const cost = parseTimeL(run.stderr ?? "");
	if (run.status !== 0 || !cost) throw new Error(`render of ${folder} failed (exit ${run.status}): ${(run.stderr || run.stdout || "").slice(-300)}`);
	return cost;
}

/** Instruction counts and footprints per fixture (PRF-05, PRF-40): one discarded run, then RUNS. */
interface PerfPaths {cwd: string; bin: string; out: string}
type Metrics = Record<string, number>;

export function measureCli({ cwd, bin, out }: PerfPaths) {
	const cli: NonNullable<MeasurementCosts["cli"]> = {};
	const html: Metrics = {};
	const values: Metrics = {};
	for (const name of Object.keys(FIXTURES)) {
		const folder = stage(cwd, out, name);
		renderOnce(bin, folder); // The first launch of a new binary costs 0.2 to 0.6 s (PRF-03): discarded.
		const runs = Array.from({ length: RUNS }, () => renderOnce(bin, folder));
		cli[name] = { instructions: median(runs.map((r) => r.instructions)), footprint: median(runs.map((r) => r.footprint)) };
		values[`cli.${name}.footprint`] = Math.max(...runs.map((r) => r.footprint));
		const rendered = readFileSync(join(folder, "Rendered/out.html"), "utf8");
		html[`html.${name}`] = Buffer.byteLength(rendered);
		values[`html.${name}.payload`] = reportHtmlBytes(rendered);
	}
	return { cli, values, sizes: html };
}

/** Complete offline CLI p95 per fixture: ten samples after one warmup, Daniel29Oct. */
export function measureTiming({ bin, out }: Pick<PerfPaths, "bin" | "out">) {
	const values: Metrics = {};
	for (const name of Object.keys(FIXTURES)) {
		const folder = join(out, "inputs", name);
		const once = () => {
			const started = process.hrtime.bigint();
			const run = spawnSync(bin, ["generate", "report.rmd", "-o", "out.html"], { cwd: folder, encoding: "utf8" });
			if (run.status !== 0) throw new Error(`render of ${name} failed: ${run.stderr.slice(-300)}`);
			return Number(process.hrtime.bigint() - started) / 1e6;
		};
		once();
		values[`cli.${name}.p95`] = quantile(Array.from({ length: TIMING_RUNS }, once), 0.95);
	}
	return values;
}

/**
 * The all-types fixture and one folder per chart. The runtime exposes no per-chart timing and a
 * report folder holds one `.rmd`, so each chart is timed alone, as a one-chart report in its own folder
 * (with the fixture's data files), next to the whole fixture in `inputs/all-types/`.
 */
export function stageAllTypes({ cwd, out }: Pick<PerfPaths, "cwd" | "out">) {
	const folder = join(out, "inputs", "all-types");
	rmSync(folder, { recursive: true, force: true });
	cpSync(join(cwd, ALL_TYPES), folder, { recursive: true, filter: (src) => !SKIP_COPY.has(basename(src)) });
	const source = readFileSync(join(folder, "report.rmd"), "utf8");
	const root = join(out, "inputs", "all-types-charts");
	rmSync(root, { recursive: true, force: true });
	// A chart's baseline key hashes its own block, its data files and its type's registry entry (PRF-05), not the fixture.
	const registry = readText(join(cwd, "crates/richview-core/src/registry.rs"));
	const dataDir = join(folder, "data");
	const dataFiles = new Map(readdirSync(dataDir).sort().map((file) => [file, read(join(dataDir, file))]));
	const charts = chartBlocks(source).map((chart) => {
		const key = chartKey(chart, typeIdentity(registry, chart.type), dataFiles);
		const dir = join(root, `${String(chart.index).padStart(2, "0")}-${chart.type}`);
		mkdirSync(dir, { recursive: true });
		writeFileSync(join(dir, "report.rmd"), oneChartReport(chart));
		cpSync(join(folder, "data"), join(dir, "data"), { recursive: true });
		return { ...chart, key, dir };
	});
	// The fixture's identity (PRF-07 "fixtures are versioned with hashes"): its source and data.
	const hash = createHash("sha256").update(source);
	for (const file of readdirSync(join(folder, "data")).sort()) hash.update(file).update(read(join(folder, "data", file)));
	return { folder, charts, fixture: hash.digest("hex").slice(0, 16) };
}

/** One offline render, wall clock in ms, like the CLI p95 fixtures; a chart that does not render is an error. */
function renderMs(bin: string, dir: string) {
	const started = process.hrtime.bigint();
	const run = spawnSync(bin, ["generate", "report.rmd", "-o", "out.html"], { cwd: dir, encoding: "utf8" });
	const ms = Number(process.hrtime.bigint() - started) / 1e6;
	if (run.status !== 0) throw new Error(`render of ${dir} failed: ${(run.stderr || run.stdout).slice(-300)}`);
	return ms;
}

/** Chart mount: from the runtime installing `window.__richview`, just before it mounts the figures, until `settled()`. */
type PerfWindow = typeof window & {
	__richview: {settled: () => Promise<unknown>; charts: () => {settled: boolean}[]};
	__rvStart: number;
}
async function mountMs(context: BrowserContext, file: string, expected: number) {
	const page = await context.newPage();
	try {
		// The setter stamps the moment the runtime starts, so the page's own load, parse and Wasm compile stay out.
		await page.addInitScript(() => {
			let hook: PerfWindow["__richview"] | undefined;
			Object.defineProperty(window, "__richview", { configurable: true, get: () => hook, set: (value: PerfWindow["__richview"]) => { hook = value; (window as PerfWindow).__rvStart = performance.now(); } });
		});
		await page.goto(pathToFileURL(file).href, { waitUntil: "load" });
		await page.waitForFunction(() => (window as PerfWindow).__richview, null, { timeout: 45_000 });
		const mounted = await page.evaluate(async () => {
			await (window as PerfWindow).__richview.settled();
			const charts = (window as PerfWindow).__richview.charts();
			return { ms: performance.now() - (window as PerfWindow).__rvStart, charts: charts.length, settled: charts.every((chart) => chart.settled) };
		});
		if (mounted.charts !== expected || !mounted.settled) throw new Error(`${file}: ${mounted.charts} of ${expected} charts mounted${mounted.settled ? "" : ", not all settled"}`);
		return mounted.ms;
	} finally {
		await page.close();
	}
}

/**
 * PRF-07's all-types fixture per chart: native render time and Chromium mount time at 1440x1000 with
 * reduced motion. Returns the fixture hash, the browser, one `{ index, type, native_ms, mount_ms }` per
 * chart and the whole fixture's own native and mount time.
 */
export async function measureAllTypes({ cwd, bin, out }: PerfPaths) {
	const staged = stageAllTypes({ cwd, out });
	const targets = [...staged.charts.map((chart) => ({ dir: chart.dir, expected: 1 })), { dir: staged.folder, expected: staged.charts.length }];
	const native = targets.map((): number[] => []);
	for (let lap = 0; lap <= TYPE_NATIVE_ROUNDS; lap++) {
		targets.forEach((target, at) => {
			const ms = renderMs(bin, target.dir);
			if (lap > 0) native[at].push(ms);
		});
	}
	const { chromium, errors: playwrightErrors }: typeof import("playwright") = await import(pathToFileURL(join(cwd, "node_modules/playwright/index.mjs")).href);
	const browser = await launchBrowser(chromium, { headless: true, ...(process.env.RICHVIEW_CHROMIUM ? { executablePath: process.env.RICHVIEW_CHROMIUM } : {}) }, { TimeoutError: playwrightErrors.TimeoutError });
	const mount = targets.map((): number[] => []);
	try {
		const context = await browser.newContext({ viewport: { width: 1440, height: 1000 }, reducedMotion: "reduce" });
		// One discarded page warms the browser and the file cache; a whole discarded lap would add a quarter to the step's time.
		await mountMs(context, join(targets[0].dir, "Rendered/out.html"), targets[0].expected);
		for (let lap = 0; lap < TYPE_MOUNT_ROUNDS; lap++) {
			for (const [at, target] of targets.entries()) mount[at].push(await mountMs(context, join(target.dir, "Rendered/out.html"), target.expected));
		}
		const version = browser.version();
		const time = (at: number) => ({ native_ms: round(median(native[at])), mount_ms: round(median(mount[at])) });
		return {
			fixture: staged.fixture,
			browser: `chromium ${version}`,
			rounds: { native: TYPE_NATIVE_ROUNDS, mount: TYPE_MOUNT_ROUNDS },
			entries: staged.charts.map((chart, at) => ({ index: chart.index, type: chart.type, key: chart.key, ...time(at) })),
			whole: { charts: staged.charts.length, ...time(targets.length - 1) },
		};
	} finally {
		await browser.close();
	}
}

/** Rows for the probe: 12 series of 50 points, a line and an area view (PRF-21's "up to 1,000 rows"). */
function probeReport() {
	const rows = Array.from({ length: 12 }, (_, s) => Array.from({ length: 50 }, (_, i) => ({ step: i, series: `S${s}`, value: Math.round(50 + 30 * Math.sin(i / 6 + s) + s * 4) }))).flat();
	const block = {
		type: "line",
		title: "Frame probe",
		encoding: { x: "step", y: "value", color: "series" },
		data: { inline: rows },
		controls: [{ kind: "variant", id: "view", label: "View", options: [{ value: "line", label: "Line", chart: {} }, { value: "area", label: "Area", chart: { type: "area" } }] }],
	};
	return `---\ntitle: Frame probe\nversion: "1"\n---\n\nOne chart, two views.\n\n\`\`\`chart\n${JSON.stringify(block)}\n\`\`\`\n`;
}

/**
 * PRF-21 in WebKit at 1x CPU: the main-thread time of each frame while a chart transitions between
 * its views. WebKit has no long-animation-frame entry, so a frame's cost is the time from its
 * animation callback to the next task: script, style, layout and the paint commit of that frame. It
 * counts more than script alone, so a pass here is a pass on script time.
 */
export async function webkitFrames({ cwd, bin, out }: PerfPaths) {
	const folder = join(out, "inputs", "frame-probe");
	mkdirSync(folder, { recursive: true });
	writeFileSync(join(folder, "report.rmd"), probeReport());
	const run = spawnSync(bin, ["generate", "report.rmd", "-o", "out.html"], { cwd: folder, encoding: "utf8" });
	if (run.status !== 0) throw new Error(`the frame probe report did not render: ${(run.stderr || run.stdout).slice(-300)}`);
	const server = createServer((req, res) => {
		const file = join(folder, req.url === "/" ? "Rendered/out.html" : decodeURIComponent(req.url!.slice(1)));
		if (!file.startsWith(folder) || !existsSync(file)) return void res.writeHead(404).end();
		res.writeHead(200, { "content-type": file.endsWith(".html") ? "text/html" : "application/octet-stream" }).end(read(file));
	});
	await new Promise<void>(accept => server.listen(0, "127.0.0.1", () => accept()));
	const { webkit, errors: playwrightErrors }: typeof import("playwright") = await import(join(cwd, "node_modules/playwright/index.mjs"));
	const browser = await launchBrowser(webkit, {}, { TimeoutError: playwrightErrors.TimeoutError });
	try {
		const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
		await page.goto(`http://127.0.0.1:${(server.address() as AddressInfo).port}/`, { waitUntil: "load" });
		await page.waitForFunction(() => (window as PerfWindow).__richview, null, { timeout: 45_000 });
		await page.evaluate(() => (window as PerfWindow).__richview.settled());
		const costs: number[] = [];
		for (let repeat = -PERF_SAMPLING.warmup; repeat < PERF_SAMPLING.frame; repeat++) {
			const next = page.locator('.rv-chart__controls .rv-chart__option[aria-pressed="false"]').first();
			const recording = page.evaluate(
				() =>
					new Promise<number[]>((done) => {
						const costs: number[] = [];
						const channel = new MessageChannel();
						const stop = performance.now() + 450;
						const loop = () => {
							const started = performance.now();
							channel.port1.onmessage = () => costs.push(performance.now() - started);
							channel.port2.postMessage(0);
							if (started < stop) requestAnimationFrame(loop);
							else setTimeout(() => done(costs), 30);
						};
						requestAnimationFrame(loop);
					}),
			);
			await sleep(40);
			await next.click();
			const frames = await recording;
			if (repeat >= 0) costs.push(...frames);
			await page.evaluate(() => (window as PerfWindow).__richview.settled());
		}
		return { version: browser.version(), ...frameStats(costs), proxy: "main-thread time from a frame's animation callback to the next task" };
	} finally {
		await browser.close();
		server.close();
	}
}

const round = <T extends number | null | undefined>(n: T): number | Exclude<T, number> => (typeof n === "number" ? Math.round(n * 100) / 100 : n as Exclude<T, number>);

/** Write the hold with this process's pid; the returned release removes it if it is still ours. */
export function holdStreams(file: string, pid = process.pid) {
	mkdirSync(dirname(file), { recursive: true });
	writeFileSync(file, `${pid}\n`);
	return () => {
		try {
			if (readFileSync(file, "utf8").trim() === String(pid)) rmSync(file, { force: true });
		} catch {
			// Already gone.
		}
	};
}

interface PerfOptions extends PerfPaths {
	dist: string;
	stages: ReadonlySet<string>;
	baselinePath: string;
	updateBaseline?: boolean;
	acceptedGrowth?: PerfAcceptance;
	source?: PerfSource;
	maxWaitMs?: number;
	loadLimit?: number;
	quietFile?: string;
	lock?: boolean;
	chromiumOnly?: boolean;
}
interface PerfCapture {
	allTypes: Awaited<ReturnType<typeof measureAllTypes>> & {seconds: number};
	frames: Awaited<ReturnType<typeof webkitFrames>>;
	controls: Awaited<ReturnType<typeof measureControlResponse>>;
}

export async function runPerf({ cwd, bin, dist, out, stages, baselinePath, updateBaseline = false, acceptedGrowth, source, maxWaitMs, loadLimit = 2, quietFile = QUIET_FILE, lock = true, chromiumOnly = false }: PerfOptions) {
	const started = Date.now();
	mkdirSync(out, { recursive: true });
	if (platform() !== "darwin") throw new Error("The performance step measures on the macOS reference host (PRF-03): `time -l` reports instruction counts and peak footprint there.");
	for (const required of [bin, join(dist, "richview_site_bg.wasm")]) if (!existsSync(required)) throw new Error(`${required} does not exist; the perf step needs the build step first`);
	const budgets = perfBudgetsFor(chromiumOnly);
	const values: Metrics = {};
	const sizes: Metrics = {};
	let cli: NonNullable<MeasurementCosts["cli"]> = {};
	let compressor;
	if (stages.has("sizes")) {
		const m = measureSizes({ dist, bin });
		Object.assign(values, m.values);
		Object.assign(sizes, m.sizes);
		compressor = m.compressor;
	}
	if (stages.has("cli")) {
		const m = measureCli({ cwd, bin, out });
		cli = m.cli;
		Object.assign(values, m.values);
		Object.assign(sizes, m.sizes);
	}
	// Compare with the accepted baseline: a missing file means the first capture sets it (PRF-08).
	const storedBaseline: PerfBaseline | undefined = existsSync(baselinePath) ? JSON.parse(readFileSync(baselinePath, "utf8")) : undefined;
	const accepted = acceptSizesAndMemory(storedBaseline, { sizes, cli }, { acceptance: acceptedGrowth, source });
	const baseline = accepted.baseline;
	const { regressions, improvements } = baseline ? compareBaseline(baseline, { sizes, cli }) : { regressions: [], improvements: [] };
	let timing;
	if (stages.has("timing") || stages.has("types")) {
		const release = lock ? acquireLock("benchmark", { wait: true, timeoutMs: 30 * 60_000 }) : () => {};
		try {
			timing = await quietCapture<PerfCapture>({
				limit: loadLimit,
				maxWaitMs,
				hold: () => holdStreams(quietFile),
				inFlight: () => busyProcesses(spawnSync("ps", ["-axo", "pid=,command="], { encoding: "utf8" }).stdout ?? ""),
				loadavg: () => round(loadavg()[0]),
				load5: () => round(loadavg()[1]),
				sleep,
				now: Date.now,
				budgets,
				capture: async (attempt: number) => {
					const measured: PerfMeasured<PerfCapture> = { values: {} };
					const captureOut = attempt === 1 ? out : join(out, attempt === 2 ? "timing-repeat" : `timing-repeat-${attempt}`);
					if (stages.has("timing")) {
						const timed = measureTiming({ bin, out });
						const frames = chromiumOnly ? undefined : await webkitFrames({ cwd, bin, out: captureOut });
						const controls = await measureControlResponse({ cwd, bin, out: captureOut });
						Object.assign(measured, { values: { ...timed, ...(frames && {"webkit.frame.p95": frames.p95, "webkit.frame.max": frames.max}), ...controls.values }, frames, controls });
					}
					if (stages.has("types") && attempt === 1) {
						const typesStarted = Date.now();
						measured.allTypes = { ...(await measureAllTypes({ cwd, bin, out })), seconds: (Date.now() - typesStarted) / 1000 };
					}
					return measured;
				},
			});
		} finally {
			release();
		}
		if (timing.measured) Object.assign(values, timing.measured.values);
	}
	// Per-type timings are optional disclosures only (Daniel29Oct), never regression failures.
	const measuredTypes = timing?.attempts?.[0]?.measured?.allTypes;
	const judgedTypes = measuredTypes && judgeTypes(baseline?.allTypes, measuredTypes.entries);
	const verdict = judge({ values, regressions, timing, stages, budgets });
	const summary = {
		...verdict,
		values: Object.fromEntries(Object.entries(values).map(([k, v]) => [k, round(v)])),
		budgets: Object.fromEntries(Object.entries(PERF_BUDGETS).map(([k, b]) => [k, { limit: b.limit, unit: b.unit, why: b.why, gate: b.gate !== false && !(chromiumOnly && WEBKIT_BUDGETS.includes(k)) && !(RELAX_SIZE_BUDGETS_UNTIL_LAUNCH && isSizeBudget(k)), ...(chromiumOnly && WEBKIT_BUDGETS.includes(k) ? { note: "Prelaunch policy: Chromium only until launch." } : {}), ...(RELAX_SIZE_BUDGETS_UNTIL_LAUNCH && isSizeBudget(k) ? { measurementRequired: true, note: "relaxed until launch" } : {}) }])),
		sizes,
		compressor,
		cli,
		improvements,
		timing: timing && { attempts: timing.attempts, decidingAttempt: timing.decidingAttempt, frames: timing.measured?.frames, controls: timing.measured?.controls },
		allTypes: measuredTypes && {
			gate: false,
			...measuredTypes,
			entries: judgedTypes!.entries.map((e) => ({ ...e, limit_native_ms: round(e.limit_native_ms), limit_mount_ms: round(e.limit_mount_ms) })),
			status: Object.fromEntries(["pass", "fail", "no baseline yet"].map((status) => [status, judgedTypes!.entries.filter((e: { status: string; }) => e.status === status).length])),
			gap: judgedTypes!.gap ?? null,
		},
		gaps: [...(judgedTypes?.gap ? [`allTypes: ${judgedTypes!.gap.text}`] : []), ...(chromiumOnly && stages.has("timing") ? WEBKIT_BUDGETS.map(key => `${key}: Prelaunch policy: Chromium only until launch.`) : [])],
		lock,
		baseline: accepted.advisory ? "compared; size budgets relaxed until launch" : accepted.acceptance ? "Daniel accepted sizes and memory from this run's measurements" : baseline ? "compared" : verdict.failures.length ? "none: a failing capture never becomes the baseline" : "none: this capture becomes the baseline",
		baselineAcceptance: accepted.acceptance,
		host: { platform: platform(), load: loadavg().map(round) },
		seconds: (Date.now() - started) / 1000,
	};
	writeFileSync(join(out, "perf.json"), `${JSON.stringify(summary, null, 2)}\n`);
	// The first capture seeds PRF-05's baseline only when it meets every budget, so a regression
	// that fails today never becomes the yardstick (review 1d note 9).
	const about = "PRF-05's accepted baseline: raised only by a declared feature or design change Daniel accepted (DEL-36); an improvement lowers it.";
	const seedCore = !accepted.advisory && (updateBaseline || (!baseline?.sizes && !verdict.failures.length)) && stages.has("sizes") && stages.has("cli");
	// A chart's entry is seeded by the first capture of its key (PRF-07), only from a quiet host and a passing
	// verdict: a capture under load would become the yardstick of every later one. Other charts keep their
	// baselines; `--update-baseline` replaces the baseline of every chart measured.
	// Optional type timings are disclosed from the first capture, not repeated by this timing gate.
	const quiet = timing?.attempts?.[0]?.result === "measured";
	const seedTypes = !accepted.advisory && measuredTypes && !verdict.failures.length && (updateBaseline || (quiet && judgedTypes!.gap));
	const rebaseAccepted = accepted.acceptance && !accepted.advisory;
	if (seedCore || seedTypes || rebaseAccepted) {
		mkdirSync(dirname(baselinePath), { recursive: true });
		const next: PerfBaseline = { about, ...baseline, ...(seedCore ? { sizes, cli } : {}) };
		if (seedTypes) {
			// Only the charts of this fixture keep an entry, so a chart edited away does not leave its baseline behind.
			const kept = baseline?.allTypes?.entries ?? {};
			const entries: NonNullable<TypeBaseline["entries"]> = {};
			for (const e of measuredTypes.entries) entries[typeKey(e)] = !updateBaseline && kept[typeKey(e)] ? kept[typeKey(e)] : { type: e.type, native_ms: e.native_ms, mount_ms: e.mount_ms };
			next.allTypes = { fixture: measuredTypes.fixture, entries };
		}
		if (rebaseAccepted) cpSync(baselinePath, `${baselinePath}.before-${accepted.acceptance.id}.json`);
		const temporary = `${baselinePath}.${process.pid}.tmp`;
		writeFileSync(temporary, `${JSON.stringify(next, null, "\t")}\n`);
		renameSync(temporary, baselinePath);
	}
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
		console.error("usage: perf.mts --bin <richview> --out <dir> [--dist dist/web] [--stages sizes,cli,timing,types] [--update-baseline] [--no-lock]");
		process.exit(2);
	}
	// A gate run sets RV_GATE_HOLDER; it never measures without the benchmark lock (PRF-02).
	if (process.argv.includes("--no-lock") && process.env.RV_GATE_HOLDER) {
		console.error("perf.mts: --no-lock is for a check by hand; a gate run measures under the benchmark lock");
		process.exit(2);
	}
	const cwd = resolve(arg("--cwd") ?? process.cwd());
	// The organizer supplies this one-run authorization; ordinary runs never raise the baseline.
	const acceptedGrowth = process.env.RV_PERF_ACCEPTED_GROWTH ? JSON.parse(process.env.RV_PERF_ACCEPTED_GROWTH) : undefined;
	let source: PerfSource | undefined;
	if (acceptedGrowth) {
		if (!process.env.RV_GATE_HOLDER || process.argv.includes("--no-lock")) throw new Error("Accepted growth requires a gate run and benchmark hold");
		const commit = spawnSync("git", ["rev-parse", "HEAD"], { cwd, encoding: "utf8" });
		if (commit.status !== 0) throw new Error("Cannot bind accepted growth to the full source");
		source = { commit: commit.stdout.trim(), receipt: basename(dirname(resolve(out))), build: JSON.parse(readFileSync(join(dirname(resolve(out)), "build-manifest.json"), "utf8")).inputs };
	}
	const summary = await runPerf({
		cwd,
		bin: resolve(cwd, bin),
		dist: resolve(cwd, arg("--dist") ?? "dist/web"),
		out: resolve(out),
		stages: new Set(arg("--stages") ? arg("--stages")!.split(",") : DEFAULT_STAGES),
		baselinePath: arg("--baseline") ?? join(gateHome(), "perf-baseline.json"),
		updateBaseline: process.argv.includes("--update-baseline"),
		acceptedGrowth, source,
		chromiumOnly: process.env.RV_GATE_PRELAUNCH_CHROMIUM_ONLY === "1",
		lock: !process.argv.includes("--no-lock"),
		maxWaitMs: arg("--max-wait") ? Number(arg("--max-wait")) : undefined,
		loadLimit: arg("--load-limit") ? Number(arg("--load-limit")) : 2,
	});
	for (const f of summary.failures) console.log(`FAIL ${f.name}: ${f.reason ?? `${round(f.value)} ${f.unit} over ${f.limit} (${f.why})`}`);
	for (const d of summary.disclosures) console.log(`NOTE ${d.name}: ${round(d.value)} ${d.unit} over ${d.limit}${d.note === "relaxed until launch" ? ` (${d.note})` : `; ${d.note}`}`);
	for (const i of summary.improvements) console.log(`NOTE ${i.key} improved from ${i.was} to ${i.now}; lower the baseline with --update-baseline`);
	for (const gap of summary.gaps) console.log(gap.startsWith("webkit.frame.") ? `UNRUN ${gap}` : `GAP ${gap}`);
	for (const a of summary.timing?.attempts ?? []) {
		const waited = a.loadWait ? `; after ${Math.round(a.loadWait.waitedMs / 1000)} s waiting for the 5-minute load to fall under ${a.loadWait.under}: ${a.loadWait.trigger} to ${a.loadWait.load5}` : "";
		console.log(`timing attempt ${a.n}: ${a.result}${a.reason ? ` (${a.reason})` : ""}; 5-minute load ${a.load5Before} before, ${a.load5After} after${waited}`);
	}
	console.log(`perf: ${summary.verdict.toUpperCase()}${summary.verdict === "inconclusive" ? ` (performance inconclusive: ${summary.inconclusive.join("; ")}; rerun on a quiet host)` : ""} in ${summary.seconds.toFixed(1)} s; load ${summary.host.load.join(" ")}`);
	process.exit({ pass: 0, fail: 1, inconclusive: 3 }[summary.verdict]);
}
