// The performance step's pure parts (DEL-51, PRF-02, PRF-04, PRF-05, PRF-30, PRF-40): budgets,
// the baseline comparison, the quiet capture, `time -l` parsing and the verdict. Nothing here
// measures anything or touches the host, so tests drive it with fakes.
import { createHash } from "node:crypto";
import { basename } from "node:path";

const KIB = 1024;
const MIB = 1024 * KIB;

// Daniel, 9 Nov 2026: "the size can go up. We can trim the size of the whole runtime once we're done with the launch, so relax all the size checks."
export const RELAX_SIZE_BUDGETS_UNTIL_LAUNCH = true;
export const isSizeBudget = (name: string) => name === "site.payload" || name === "report-runtime.payload" || /^html\..*\.payload$/.test(name);

/**
 * The budgets the step enforces. `gate: false` entries are measured and reported but cannot fail the
 * step, each with the open question that keeps it a disclosure. Payload sizes use gzip -9, except
 * PRF-32's HTML budgets, which count the exported UTF-8 bytes without runtime and media data.
 * Where DEL-51 and part 07 disagree (OPN-50, PRF-02, PRF-04 are unanswered), the step builds DEL-51's
 * recommended option: every budget in each release run, WebKit probe included, after a quiet-host wait.
 */
interface Budget { limit: number; unit: string; why: string; timing?: boolean; gate?: boolean; note?: string }
type Budgets = Record<string, Budget>;
type Metrics = Record<string, number>;
export interface MeasurementCosts { sizes?: Metrics; cli?: Record<string, {instructions: number; footprint: number}> }
interface TypeCost { native_ms: number; mount_ms: number }
export interface TypeBaseline { fixture?: string; entries?: Record<string, TypeCost & {type?: string}> }
export interface PerfBaseline extends MeasurementCosts { about?: string; acceptedGrowth?: PerfAcceptance[]; allTypes?: TypeBaseline }
export interface PerfAcceptance { id: string; authority: string; words: string; date: string; commit: string }
export interface PerfSource { commit: string; receipt: string; build?: {commit: string; dirty: boolean} }
interface Delta { key: string; was: number; now: number; margin: string }
interface ChartBlock { index: number; type: string; block: string }
interface TypeEntry extends TypeCost { index: number; type: string; key?: string }

export const BUDGETS: Budgets = {
	// PRF-30: Daniel, 2026-10-29, "Rebase + quiet + 2 MB": decimal bytes, entire initial payload.
	"site.payload": { limit: 2_000_000, unit: "B", why: "PRF-30" },
	// PRF-31: Daniel's answer of 2026-10-28 raises the report runtime to 2 MiB after gzip -9 ("the budget is artificial").
	"report-runtime.payload": { limit: 2 * MIB, unit: "B", why: "PRF-31" },
	// PRF-32: raw exported HTML in each fixture's pinned style, without runtime or media data.
	"html.small.payload": { limit: 224 * KIB, unit: "B", why: "PRF-32" },
	"html.flagship.payload": { limit: 768 * KIB, unit: "B", why: "PRF-32" },
	"html.dense.payload": { limit: 2.5 * MIB, unit: "B", why: "PRF-32" },
	// PRF-40: peak physical footprint of one complete offline CLI render.
	"cli.small.footprint": { limit: 320 * MIB, unit: "B", why: "PRF-40" },
	"cli.flagship.footprint": { limit: 480 * MIB, unit: "B", why: "PRF-40" },
	"cli.dense.footprint": { limit: 1280 * MIB, unit: "B", why: "PRF-40" },
	// docs/PERFORMANCE.md: complete offline CLI p95.
	"cli.small.p95": { limit: 300, unit: "ms", why: "PRF-11", timing: true },
	"cli.flagship.p95": { limit: 500, unit: "ms", why: "PRF-11", timing: true },
	"cli.dense.p95": { limit: 800, unit: "ms", why: "PRF-11", timing: true },
	// PRF-21: the probe times the whole frame (script, style, layout, paint commit), not script alone,
	// so it holds the frame to the grid fixture's 8 ms p95 at 1x, a limit script time must also meet.
	"webkit.frame.p95": { limit: 8, unit: "ms", why: "PRF-21", timing: true },
	"webkit.frame.max": { limit: 16.7, unit: "ms", why: "PRF-21", timing: true },
};

/** Comparison used only by the optional per-type disclosure; Daniel29Oct removed its gate. */
export const TYPE_TOLERANCE = 0.2;

/** PRF-05, Daniel29Oct decision2: relative margins against the accepted baseline. */
export const MARGINS = {
	wasm: 0.1,
	html: 0.1,
	binary: 0.1,
	instructions: 0.2,
	footprint: 0.25,
};

/** Apply an explicit accepted cost once, using only the named run's own clean build measurements. */
export function acceptSizesAndMemory(baseline: PerfBaseline | undefined, measured: MeasurementCosts, { acceptance, source }: {acceptance?: PerfAcceptance; source?: PerfSource} = {}) {
	if (!acceptance || baseline?.acceptedGrowth?.some(record => record.id === acceptance.id)) return { baseline };
	const commit = source?.commit;
	if (!/^[a-zA-Z0-9.-]+$/.test(acceptance.id ?? "") || acceptance.authority !== "Daniel" || !acceptance.words?.trim() || !acceptance.date) throw new Error("Accepted perf growth needs Daniel's dated authorization");
	const receiptKind = /^(full|affected)-\d{8}T\d{6}Z-[0-9a-f]{10}$/.exec(source?.receipt ?? "")?.[1];
	if (!/^[0-9a-f]{40}$/.test(commit ?? "") || acceptance.commit !== commit || !receiptKind || !source!.receipt.endsWith(commit!.slice(0, 10)) || source!.build?.commit !== commit || source!.build!.dirty !== false) throw new Error("Accepted perf growth must name this run's exact clean build");
	const positive = (value: number | undefined) => Number.isFinite(value) && value! > 0;
	const sizeKeys = ["wasm.site", "wasm.runtime", "binary.richview", "html.small", "html.flagship", "html.dense"];
	const fixtures = ["small", "flagship", "dense"];
	if (!baseline?.sizes || sizeKeys.some(key => !positive(measured.sizes?.[key])) || fixtures.some(name => !positive(measured.cli?.[name]?.footprint) || !positive(baseline.cli?.[name]?.instructions))) throw new Error("Accepted perf growth requires complete sizes and memory measurements");
	// Daniel's 9 November ruling relaxes size gates; it does not authorize rebasing sizes or memory.
	if (acceptance.id === "daniel-2026-11-09-size-relax") return {
		baseline, advisory: true,
		acceptance: { ...acceptance, receipt: source!.receipt, receiptKind, scope: ["size-budgets"], effect: "advisory" },
	};
	const record = {
		...acceptance, receipt: source!.receipt, receiptKind, scope: ["sizes", "cli.*.footprint"],
		previous: { sizes: baseline.sizes, footprint: Object.fromEntries(fixtures.map(name => [name, baseline.cli![name].footprint])) },
	};
	const next = {
		...baseline, sizes: { ...baseline.sizes, ...measured.sizes },
		cli: { ...baseline.cli, ...Object.fromEntries(fixtures.map(name => [name, { ...baseline.cli![name], footprint: measured.cli![name].footprint }])) },
		acceptedGrowth: [...(baseline.acceptedGrowth ?? []), record],
	};
	return { baseline: next, acceptance: record };
}

/** PRF-32's raw UTF-8 HTML: embedded fonts and chart data count; runtime and media payloads do not. */
export function reportHtmlBytes(html: string) {
	const withoutRuntime = html.replace(/<script\b[^>]*\btype=["']module["'][^>]*>[\s\S]*?<\/script>/gi, (script: string) =>
		/\bawait (?:richview_runtime|__wbg_init)\(\{module_or_path:/.test(script) ? "" : script,
	);
	return Buffer.byteLength(withoutRuntime.replace(/\bdata:(?:image|audio|video)\/[^\s"'<>)]*/gi, ""));
}

export const median = (values: readonly number[]) => {
	const sorted = [...values].sort((a, b) => a - b);
	return sorted.length % 2 ? sorted[sorted.length >> 1] : (sorted[sorted.length / 2 - 1] + sorted[sorted.length / 2]) / 2;
};

/** The nearest-rank quantile, as the harness has always computed p95. */
export const quantile = (values: readonly number[], q: number) => {
	const sorted = [...values].sort((a, b) => a - b);
	return sorted[Math.max(0, Math.ceil(sorted.length * q) - 1)];
};

/**
 * What macOS `/usr/bin/time -l` reports: `{ seconds, instructions, footprint, rss }`, or undefined
 * when the output is not that report (Linux prints different lines).
 */
export function parseTimeL(stderr: string) {
	const number = (pattern: RegExp) => {
		const match = pattern.exec(stderr);
		return match ? Number(match[1]) : undefined;
	};
	const seconds = number(/^\s*([0-9.]+)\s+real\b/m);
	const instructions = number(/^\s*(\d+)\s+instructions retired\b/m);
	const footprint = number(/^\s*(\d+)\s+peak memory footprint\b/m);
	const rss = number(/^\s*(\d+)\s+maximum resident set size\b/m);
	if (seconds === undefined || instructions === undefined || footprint === undefined) return undefined;
	return { seconds, instructions, footprint, rss };
}

/**
 * Compares a capture with the accepted baseline (PRF-05). Returns `{ regressions, improvements }`,
 * each `{ key, was, now, margin }`. A key the baseline lacks is not compared: the first capture of
 * a new measurement becomes its baseline.
 */
export function compareBaseline(baseline: MeasurementCosts, current: MeasurementCosts) {
	const regressions: Delta[] = [];
	const improvements: Delta[] = [];
	const check = (key: string, was: number | undefined, now: number | undefined, margin: number) => {
		if (was === undefined || now === undefined) return;
		const allowed = was * margin;
		if (now - was > allowed) regressions.push({ key, was, now, margin: `${margin * 100}%` });
		else if (was - now > allowed) improvements.push({ key, was, now, margin: `${margin * 100}%` });
	};
	for (const [name, bytes] of Object.entries(current.sizes ?? {})) {
		const margin = name.startsWith("binary.") ? MARGINS.binary : name.startsWith("html.") ? MARGINS.html : MARGINS.wasm;
		check(`sizes.${name}`, baseline.sizes?.[name], bytes, margin);
	}
	for (const [fixture, cost] of Object.entries(current.cli ?? {})) {
		check(`cli.${fixture}.instructions`, baseline.cli?.[fixture]?.instructions, cost.instructions, MARGINS.instructions);
		check(`cli.${fixture}.footprint`, baseline.cli?.[fixture]?.footprint, cost.footprint, MARGINS.footprint);
	}
	return { regressions, improvements };
}

/**
 * The chart fences of a report, in order, each `{ index, type, block }`. The all-types fixture is
 * measured one chart at a time, because the runtime exposes no per-chart timing (PRF-07).
 */
export function chartBlocks(source: string) {
	const blocks: ChartBlock[] = [];
	for (const match of source.matchAll(/^```chart\n([\s\S]*?)^```$/gm)) {
		const type = /^type:\s*(\S+)/m.exec(match[1])?.[1];
		if (!type) throw new Error(`chart ${blocks.length} of the all-types fixture names no type`);
		blocks.push({ index: blocks.length, type, block: match[1] });
	}
	return blocks;
}

/** A report of one chart; the version is valid so that the only diagnostics are the chart's own. */
export const oneChartReport = ({ index, type, block }: ChartBlock) => `---\ntitle: All types ${index} ${type}\nversion: 0.0.1\n---\n\n\`\`\`chart\n${block}\`\`\`\n`;

/**
 * The registry's definition of one chart type: the text of its `ChartType { name: "<type>" ... }` entry up to
 * the next entry. This is the build's identity of the type (fields, defaults and checks it accepts), so a
 * change to the type moves its charts' baselines and a change to another type does not. A registry that names
 * no such type gives the type's name alone.
 */
export function typeIdentity(registryText: string|undefined, type: string) {
	const text = registryText ?? "";
	const start = text.search(new RegExp(`ChartType\\s*\\{\\s*name:\\s*"${type.replace(/[^\w-]/g, "\\$&")}"`));
	if (start < 0) return type;
	const rest = text.slice(start + 1);
	const next = rest.search(/ChartType\s*\{\s*name:/);
	return text.slice(start, next < 0 ? undefined : start + 1 + next);
}

/**
 * A chart's baseline key (PRF-05, PRF-07): its type and a hash of its own block source, the data files that
 * block names (`dataFiles` maps a file name to its bytes) and the build's identity of the type. An edit to
 * another block of the fixture leaves the key, and so the baseline, alone; an edit to this chart, to its data
 * or to its type's definition gives it a new key with no baseline.
 */
export function chartKey({ type, block }: Pick<ChartBlock, "type" | "block">, identity: string | NodeJS.ArrayBufferView, dataFiles: ReadonlyMap<string, string | NodeJS.ArrayBufferView> = new Map()) {
	const hash = createHash("sha256").update(type).update("\0").update(block).update("\0").update(identity);
	for (const [name, bytes] of [...dataFiles].filter(([name]) => block.includes(name)).sort(([a], [b]) => a.localeCompare(b))) hash.update("\0").update(name).update(bytes);
	return `${type}:${hash.digest("hex").slice(0, 16)}`;
}

/** A chart is named by its baseline key; a chart without one (an old record) by position and type. */
export const typeKey = ({ index, type, key }: Pick<TypeEntry, "index" | "type" | "key">) => key ?? `${index}:${type}`;

/**
 * Reports each chart's native and mount time against that chart's own baseline, found by its key
 * (PRF-05, PRF-07). A chart without a baseline is "no baseline yet", neither a pass nor a failure, and is
 * counted as a gap: `gap.text` reads "57 charts without a baseline" and lists the charts, so a run that
 * judged no time says so. One above its baseline plus TYPE_TOLERANCE is a disclosure, reported like
 * compareBaseline's. Returns `{ entries, regressions, gap }`; each entry gains `limit_native_ms`,
 * `limit_mount_ms` and `status`. These records never fail the perf gate (Daniel29Oct).
 */
export function judgeTypes(baseline: TypeBaseline | undefined, measured: readonly TypeEntry[]) {
	const regressions: Delta[] = [];
	const known = baseline?.entries;
	const entries = measured.map((entry) => {
		const was = known?.[typeKey(entry)];
		if (!was) return { ...entry, limit_native_ms: null, limit_mount_ms: null, status: "no baseline yet" };
		const limit_native_ms = was.native_ms * (1 + TYPE_TOLERANCE);
		const limit_mount_ms = was.mount_ms * (1 + TYPE_TOLERANCE);
		let status = "pass";
		for (const [metric, limit] of [["native_ms", limit_native_ms], ["mount_ms", limit_mount_ms]] as const) {
			if (entry[metric] <= limit) continue;
			status = "fail";
			regressions.push({ key: `allTypes.${typeKey(entry)}.${metric}`, was: was[metric], now: entry[metric], margin: `${TYPE_TOLERANCE * 100}%` });
		}
		return { ...entry, limit_native_ms, limit_mount_ms, status };
	});
	const missing = entries.filter((entry: { status: string; }) => entry.status === "no baseline yet");
	const gap = missing.length ? { withoutBaseline: missing.length, of: entries.length, text: `${missing.length} chart${missing.length === 1 ? "" : "s"} without a baseline (of ${entries.length}): no time was judged for ${missing.length === entries.length ? "any of them" : "them"}`, charts: missing.map((entry) => ({ index: entry.index, type: entry.type })) } : undefined;
	return { entries, regressions, gap };
}

/**
 * The processes that keep the host from being quiet: real Cargo, rustc and rustdoc runs, and
 * headless or Playwright browsers. `ps` lines are `<pid> <command>`; a shim waiting on a hold runs
 * as zsh, so it never counts, and a person's own browser counts only when it runs headless.
 */
export function busyProcesses(psOutput: string, self = process.pid) {
	return psOutput
		.split("\n")
		.map((line: string) => /^\s*(\d+)\s+(.*)$/.exec(line))
		.filter((match): match is RegExpExecArray => match !== null)
		.map(([, pid, command]) => ({ pid: Number(pid), command }))
		.filter(({ pid, command }) => {
			if (pid === self) return false;
			const program = basename(command.trim().split(/\s+/)[0]);
			if (/^(?:cargo|rustc|rustdoc)$/.test(program)) return true;
			return /ms-playwright|chrome-headless-shell|\/cache\/vscode\/vscode-/.test(command) || (/chrom/i.test(command) && /\s--headless\b/.test(command));
		});
}

/**
 * The repeat after a timing failure on a heavily loaded host (lead, 3 Nov): when a capture fails a
 * timing budget while the 5-minute load is over `over`, the next capture first waits up to
 * `maxWaitMs` for the 5-minute load to fall under `under`, polling every `pollMs`, and decides.
 * The budgets are unchanged.
 */
export const LOAD_RETRY = { over: 8, under: 6, maxWaitMs: 15 * 60_000, pollMs: 15_000 };

/** Waits up to `maxWaitMs` for `load5()` to fall under `under`; the wait's record for the receipt. */
interface HostClock { sleep: (ms: number) => unknown; now: () => number; load5: () => number | undefined }
type LoadRetry = typeof LOAD_RETRY;
async function waitForLoad({ load5, sleep, now, under, maxWaitMs, pollMs }: HostClock & Omit<LoadRetry, "over">) {
	const started = now();
	const samples = [];
	for (;;) {
		const load = load5();
		samples.push({ at: now() - started, load5: load });
		if (load! < under || now() - started + pollMs > maxWaitMs) break;
		await sleep(pollMs);
	}
	const last = samples.at(-1)!.load5;
	return { fell: last! < under, load5: last, waitedMs: now() - started, samples };
}

/**
 * Makes the quiet the timing needs, since with every stream building the host is never quiet on its
 * own (lead, 28 Oct): `hold()` makes the Cargo shim hold new builds, then this waits up to
 * `maxWaitMs` for in-flight builds and headless browsers to finish, and after they have, up to
 * `settleMs` more for the load to fall to `limit`. It measures once either way and records the
 * 1-minute load (`loadavg`), the 5-minute load (`load5`) and anything still running as context.
 * A timing limit exceeded under load repeats the whole timing capture once after a new quiet wait.
 * A timing limit exceeded while the 5-minute load is over `loadRetry.over` instead waits for that
 * load to fall (`LOAD_RETRY`) before the repeat, and once after a repeat that failed so. Only the
 * final attempt decides the timing verdict; every attempt stays in evidence, and the hold spans them.
 */
type LoadWait = Awaited<ReturnType<typeof waitForLoad>> & Pick<LoadRetry, "under" | "over" | "maxWaitMs"> & {after: number; trigger: number};
export type PerfMeasured<Data extends object = object> = {values: Metrics} & Partial<Data>;
interface QuietOptions<Data extends object> extends Omit<HostClock, "load5"> {
	hold: () => () => void;
	inFlight: () => {command: string}[];
	loadavg: () => number;
	load5?: HostClock["load5"];
	capture: (attempt: number) => PerfMeasured<Data> | Promise<PerfMeasured<Data>>;
	budgets?: Budgets;
	limit?: number;
	maxWaitMs?: number;
	settleMs?: number;
	pollMs?: number;
	loadRetry?: LoadRetry;
}
interface TimingAttempt<Data extends object = object> {
	n: number;
	result: string;
	wait: {quiet: boolean; waitedMs: number; samples: {at: number; load: number; inFlight: number}[]};
	loadBefore: number;
	load5Before?: number;
	inFlight: string[];
	loadWait?: LoadWait;
	reason?: string;
	loadAfter?: number;
	load5After?: number;
	measured?: PerfMeasured<Data>;
	failedTiming?: string[];
}
export async function quietCapture<Data extends object>({ hold, inFlight, loadavg, load5 = () => undefined, sleep, now, capture, budgets = BUDGETS, limit = 2, maxWaitMs = 2 * 60_000, settleMs = 30_000, pollMs = 5_000, loadRetry = LOAD_RETRY }: QuietOptions<Data>) {
	maxWaitMs = Math.min(maxWaitMs, 2 * 60_000);
	settleMs = Math.min(settleMs, 30_000);
	const release = hold();
	try {
		const attempts: TimingAttempt<Data>[] = [];
		let measured: PerfMeasured<Data> | undefined;
		// Set when a failure under a heavy 5-minute load makes the next capture wait for it to fall.
		let trigger: {n: number; load5: number} | undefined;
		for (let n = 1; ; n++) {
			const loadWait: LoadWait | undefined = trigger && { after: trigger.n, trigger: trigger.load5, over: loadRetry.over, under: loadRetry.under, maxWaitMs: loadRetry.maxWaitMs, ...(await waitForLoad({ load5, sleep, now, under: loadRetry.under, maxWaitMs: loadRetry.maxWaitMs, pollMs: loadRetry.pollMs })) };
			const started = now();
			const samples = [];
			let drainedAt;
			let busy;
			let load;
			for (;;) {
				busy = inFlight();
				load = loadavg();
				samples.push({ at: now() - started, load, inFlight: busy.length });
				if (!busy.length && drainedAt === undefined) drainedAt = now();
				if (!busy.length && load <= limit) break;
				if (drainedAt !== undefined && now() - drainedAt + pollMs > settleMs) break;
				if (now() - started + pollMs > maxWaitMs) break;
				await sleep(pollMs);
			}
			const quiet = !busy.length && load <= limit;
			const wait = { quiet, waitedMs: now() - started, samples };
			const attempt: TimingAttempt<Data> = { n, result: quiet ? "measured" : "measured under load", wait, loadBefore: load, load5Before: load5(), inFlight: busy.slice(0, 10).map((p) => p.command.slice(0, 120)), ...(loadWait && { loadWait }) };
			if (!quiet) attempt.reason = `${busy.length ? `${busy.length} build or browser process(es) still running` : `load ${load} above ${limit}`} after ${Math.round(wait.waitedMs / 1000)} s; measured anyway`;
			try { measured = await capture(n); }
			catch (error) {
				if (n === 1) throw error;
				attempt.result = "measurement failed";
				attempt.reason = String(error);
				measured = { values: {} } as PerfMeasured<Data>;
			}
			attempt.loadAfter = loadavg();
			attempt.load5After = load5();
			attempt.measured = measured;
			attempts.push(attempt);
			attempt.failedTiming = Object.entries(budgets).filter(([key, budget]) => budget.timing && budget.gate !== false && measured?.values?.[key]! > budget.limit).map(([key]) => key);
			// The capture after the load wait decides; nothing repeats it.
			if (!attempt.failedTiming.length || loadWait) break;
			if (attempt.load5After! > loadRetry.over) {
				trigger = { n, load5: attempt.load5After! };
				continue;
			}
			if (n > 1 || quiet) break;
		}
		return { measured, attempts, decidingAttempt: attempts.at(-1)!.n };
	} finally {
		release();
	}
}

/**
 * The step's verdict. `values` maps a budget name to its measurement (absent when not measured),
 * `regressions` is compareBaseline's list and `timing` is `quietCapture`'s record (undefined when
 * the timing stages were not asked for). `fail` beats `inconclusive`, which beats `pass`.
 */
interface JudgementOptions {
	values: Metrics;
	regressions?: Delta[];
	timing?: {measured?: PerfMeasured};
	budgets?: Budgets;
	stages?: ReadonlySet<string>;
	relaxSizeBudgets?: boolean;
}
interface Failure {name: string; reason?: string; limit?: number; value?: number; unit?: string; why?: string}
interface Disclosure {name: string; value: number; limit: number; unit: string; why: string; note?: string; was?: number; margin?: string}
export function judge({ values, regressions = [], timing, budgets = BUDGETS, stages = new Set(["sizes", "cli", "timing"]), relaxSizeBudgets = RELAX_SIZE_BUDGETS_UNTIL_LAUNCH }: JudgementOptions) {
	const failures: Failure[] = [];
	const disclosures: Disclosure[] = [];
	const inconclusive: string[] = [];
	for (const [name, budget] of Object.entries(budgets)) {
		// A stage nobody asked for (a partial run by hand) is not a failure to measure.
		if (!stages.has(stageOf(name, budget))) continue;
		const value = values[name];
		if (value === undefined) {
			if (budget.timing && timing && !timing.measured) inconclusive.push(`${name}: no quiet capture`);
			else if (budget.gate !== false) failures.push({ name, reason: "not measured", limit: budget.limit });
			continue;
		}
		if (!Number.isFinite(value)) {
			if (budget.gate !== false) failures.push({ name, reason: "not a finite measurement", limit: budget.limit });
			continue;
		}
		if (value > budget.limit) {
			const row = { name, value, limit: budget.limit, unit: budget.unit, why: budget.why };
			if (relaxSizeBudgets && isSizeBudget(name)) disclosures.push({ ...row, note: "relaxed until launch" });
			else if (budget.gate === false) disclosures.push({ ...row, note: budget.note });
			else failures.push(row);
		}
	}
	for (const regression of regressions) {
		if (relaxSizeBudgets && regression.key.startsWith("sizes.")) disclosures.push({ name: regression.key, value: regression.now, limit: regression.was + regression.was * (Number.parseFloat(regression.margin) / 100), unit: "B", why: "PRF-05", was: regression.was, margin: regression.margin, note: "relaxed until launch" });
		else failures.push({ name: regression.key, reason: `grew from ${regression.was} to ${regression.now}, more than ${regression.margin}`, why: "PRF-05" });
	}
	if (failures.length) return { verdict: "fail" as const, failures, disclosures, inconclusive };
	if (timing && !timing.measured) return { verdict: "inconclusive" as const, failures, disclosures, inconclusive: [...new Set(inconclusive)] };
	return { verdict: "pass" as const, failures, disclosures, inconclusive };
}

/** The stage that measures a budget: sizes, the CLI's counts and footprints, or the quiet-host timing. */
export const stageOf = (name: string, budget: Budget) => (budget.timing ? "timing" : /^(?:cli|html)\./.test(name) ? "cli" : "sizes");

/** The PRF-21 probe's per-frame costs reduced to what the budgets read. */
export function frameStats(costs: readonly number[]) {
	if (!costs.length) return undefined;
	return { frames: costs.length, p50: median(costs), p95: quantile(costs, 0.95), max: Math.max(...costs) };
}

/** The load wait's sentence for the step reason, the printed receipt and the go card ("" without one). */
export function loadRetryLine(retry?: Omit<LoadWait, "samples"> & {attempt: number}) {
	if (!retry) return "";
	const waited = `${Math.round(retry.waitedMs / 1000)} s`;
	return `timing failed in attempt ${retry.after} with the 5-minute load at ${retry.trigger}, over ${retry.over}; attempt ${retry.attempt} waited ${waited} for it to fall under ${retry.under} (${retry.fell ? `it fell to ${retry.load5}` : `still ${retry.load5}, measured anyway`}) and decides`;
}

/**
 * What a capture "measured under load" or a load wait tells the release run: the load before and
 * after, the processes still running and the wait, as a step reason and receipt details, with every
 * attempt's values and loads. The verdict stays pass or fail on the budgets (review 1f should-fix 17).
 * Undefined when every capture was quiet and nothing waited, or there was no timing.
 */
export function loadNote(summary?: {timing?: {attempts: TimingAttempt[]; decidingAttempt: number}}) {
	const attempts = summary?.timing?.attempts ?? [];
	const attempt = attempts.filter((a: { result: string; }) => a.result === "measured under load").at(-1);
	const waited = attempts.find((a) => a.loadWait);
	if (!attempt && !waited) return undefined;
	const { samples: _samples, ...wait } = waited?.loadWait ?? {};
	const loadRetry = waited && { attempt: waited.n, ...wait } as Omit<LoadWait, "samples"> & {attempt: number};
	const running = attempt?.inFlight ?? [];
	const reason = [
		loadRetry && loadRetryLine(loadRetry),
		attempt && `timing attempt ${attempt.n} measured under load: load ${attempt.loadBefore} before, ${attempt.loadAfter} after; ${running.length ? `still running: ${running.join("; ")}` : "no build or browser still running"}`,
	].filter(Boolean).join("; ");
	return {
		reason,
		details: {
			...(attempt && { measuredUnderLoad: true, loadBefore: attempt.loadBefore, loadAfter: attempt.loadAfter, stillRunning: running, wait: attempt.reason }),
			...(loadRetry && { loadRetry }),
			timingAttempts: summary!.timing!.attempts,
			decidingTimingAttempt: summary!.timing!.decidingAttempt,
		},
	};
}
