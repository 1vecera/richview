// Tests for the perf step's pure parts (DEL-51, PRF-02, PRF-05): the baseline comparison, the quiet-host
// wait and its two-attempt rule, `time -l` parsing and the verdict. Nothing here measures the host.
import assert from "node:assert/strict";
import test from "node:test";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { BUDGETS, MARGINS, TYPE_TOLERANCE, acceptSizesAndMemory, busyProcesses, chartBlocks, chartKey, judgeTypes, typeIdentity, oneChartReport, compareBaseline, frameStats, judge, median, parseTimeL, quantile, quietCapture, reportHtmlBytes, stageOf, loadNote, LOAD_RETRY } from "../lib/perf.mjs";
import { perfBudgetsFor, DEFAULT_STAGES, PERF_SAMPLING, gzipBytes, holdStreams, hostGzip, PERF_BUDGETS } from "../perf.mjs";
import { writeGoCard } from "../lib/report.mjs";
import { CONTROL_RESPONSE_BUDGETS, CONTROL_SAMPLING } from "../control-response.mjs";

test("Daniel29Oct decision2 keeps the accepted limits and bounds measurement work", () => {
	assert.deepEqual(MARGINS, { wasm: 0.1, html: 0.1, binary: 0.1, instructions: 0.2, footprint: 0.25 });
	for (const [name, time, memory] of [["small", 300, 320], ["flagship", 500, 480], ["dense", 800, 1280]]) {
		assert.equal(BUDGETS[`cli.${name}.p95`].limit, time);
		assert.equal(BUDGETS[`cli.${name}.footprint`].limit, memory * 1024 * 1024);
	}
	assert.ok(Object.values(CONTROL_RESPONSE_BUDGETS).every(budget => budget.limit === 100));
	assert.deepEqual(DEFAULT_STAGES, ["sizes", "cli", "timing"], "full does not measure per-type timing");
	assert.ok([PERF_SAMPLING.cli, PERF_SAMPLING.timing, PERF_SAMPLING.frame, CONTROL_SAMPLING.samples].every(samples => samples <= 10));
	assert.equal(PERF_SAMPLING.warmup, 1);
	assert.equal(CONTROL_SAMPLING.warmup, 1);
});

test("release timing requires every control measurement and rejects each exceeded budget", () => {
	const stages = new Set(["timing"]);
	const timing = { measured: {}, attempts: [{ result: "measured" }] };
	const values = Object.fromEntries(Object.entries(PERF_BUDGETS).filter(([, b]) => b.timing).map(([name, b]) => [name, b.limit]));
	assert.equal(Object.keys(CONTROL_RESPONSE_BUDGETS).length, 36);
	assert.equal(judge({ values, timing, stages, budgets: PERF_BUDGETS }).verdict, "pass");
	for (const [name, budget] of Object.entries(CONTROL_RESPONSE_BUDGETS)) {
		const over = judge({ values: { ...values, [name]: budget.limit + 0.01 }, timing, stages, budgets: PERF_BUDGETS });
		assert.deepEqual(over.failures.map(f => f.name), [name]);
		const missing = { ...values };
		delete missing[name];
		assert.deepEqual(judge({ values: missing, timing, stages, budgets: PERF_BUDGETS }).failures.map(f => f.name), [name]);
	}
});

test("control timing stays inconclusive without a quiet capture and is omitted in size-only runs", () => {
	const budgets = PERF_BUDGETS;
	const timing = { attempts: [{ result: "inconclusive" }] };
	const result = judge({ values: {}, budgets, timing, stages: new Set(["timing"]) });
	assert.equal(result.verdict, "inconclusive");
	for (const name of Object.keys(CONTROL_RESPONSE_BUDGETS)) assert.ok(result.inconclusive.includes(`${name}: no quiet capture`));
	const values = Object.fromEntries(Object.entries(budgets).filter(([name, b]) => stageOf(name, b) === "sizes").map(([name]) => [name, 0]));
	assert.equal(judge({ values, budgets, stages: new Set(["sizes"]) }).verdict, "pass");
});

const TIME_L = `        0.05 real         0.03 user         0.01 sys
            35962880  maximum resident set size
                   0  average shared memory size
           208787745  instructions retired
            63801089  cycles elapsed
            10666416  peak memory footprint
`;

test("`time -l` output gives seconds, instructions retired and peak footprint", () => {
	assert.deepEqual(parseTimeL(TIME_L), { seconds: 0.05, instructions: 208787745, footprint: 10666416, rss: 35962880 });
	assert.equal(parseTimeL("Command exited with non-zero status 1\n0.05user 0.01system"), undefined, "a Linux report is not parsed as one");
});

test("median and nearest-rank quantiles", () => {
	assert.equal(median([3, 1, 2]), 2);
	assert.equal(median([4, 1, 2, 3]), 2.5);
	assert.equal(quantile([1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 0.95), 10);
	assert.equal(quantile(Array.from({ length: 100 }, (_, i) => i + 1), 0.95), 95);
	assert.equal(frameStats([]), undefined);
	assert.deepEqual(frameStats([1, 2, 3]), { frames: 3, p50: 2, p95: 3, max: 3 });
});

test("gzip -9 sizes are measured on the bytes", (t) => {
	// The host's gzip, without the file's name or time: two names for the same bytes measure the same (S27-855).
	const dir = mkdtempSync(join(tmpdir(), "rv-perf-gzip-"));
	t.after(() => rmSync(dir, { recursive: true, force: true }));
	const bytes = Buffer.from(Array.from({ length: 100_000 }, (_, i) => (i * 7919) % 251));
	writeFileSync(join(dir, "a"), bytes);
	writeFileSync(join(dir, "richview_runtime_bg.wasm"), bytes);
	writeFileSync(join(dir, "zeros"), Buffer.alloc(100_000));
	assert.equal(gzipBytes(join(dir, "a")), gzipBytes(join(dir, "richview_runtime_bg.wasm")));
	assert.ok(gzipBytes(join(dir, "zeros")) < 200);
	assert.throws(() => gzipBytes(join(dir, "missing")), /gzip -9 -n .*missing failed/);
});

test("the sizes name the host gzip that measured them: the first on PATH and its version", (t) => {
	const dir = mkdtempSync(join(tmpdir(), "rv-perf-gzip-path-"));
	t.after(() => rmSync(dir, { recursive: true, force: true }));
	const fake = join(dir, "bin");
	mkdirSync(fake);
	writeFileSync(join(fake, "gzip"), "#!/bin/sh\necho 'Test gzip 1.0'\necho second line\n", { mode: 0o755 });
	writeFileSync(join(dir, "gzip"), "not executable", { mode: 0o644 });
	assert.deepEqual(hostGzip([dir, fake].join(":")), { path: join(fake, "gzip"), version: "Test gzip 1.0" });
	assert.throws(() => hostGzip(dir), /No gzip on PATH/);
	const real = hostGzip();
	assert.ok(real.path.endsWith("/gzip") && real.version.length > 0, JSON.stringify(real));
});

test("PRF-32 counts UTF-8 report HTML and fonts, excluding only runtime and media data", () => {
	const content = '<p>Čísla</p><style>@font-face{src:url(data:font/woff2;base64,Zm9udA==)}</style><script type="application/json">{"value":42}</script>';
	const media = '<img src="data:image/png;base64,aW1hZ2U="><video src="data:video/mp4;base64,dmlkZW8="></video><audio src="data:audio/ogg;base64,YXVkaW8="></audio>';
	const shells = '<img src=""><video src=""></video><audio src=""></audio>';
	const runtime = '<script type="module">bindings();\nawait richview_runtime({module_or_path:atob("d2FzbQ==")});\n</script>';
	assert.equal(reportHtmlBytes(content + media + runtime), Buffer.byteLength(content + shells));
	assert.equal(reportHtmlBytes(content + runtime.replace("richview_runtime", "__wbg_init")), Buffer.byteLength(content));
	assert.equal(reportHtmlBytes(content + '<script type="module">other();</script>'), Buffer.byteLength(content + '<script type="module">other();</script>'));
});

test("PRF-32 fixture HTML budgets allow the boundary and fail one byte over", () => {
	for (const [fixture, limit] of [["small", 224 * 1024], ["flagship", 768 * 1024], ["dense", 2.5 * 1024 * 1024]]) {
		const name = `html.${fixture}.payload`;
		assert.equal(BUDGETS[name]?.limit, limit);
		assert.equal(stageOf(name, BUDGETS[name]), "cli");
		assert.equal(judge({ values: { ...ok, [name]: limit }, timing: measured }).verdict, "pass");
		const over = judge({ values: { ...ok, [name]: limit + 1 }, timing: measured, relaxSizeBudgets: false });
		assert.deepEqual(over.failures.map((failure) => [failure.name, failure.why]), [[name, "PRF-32"]]);
	}
});

const baseline = {
	sizes: { "wasm.site": 1_000_000, "binary.richview": 10_000_000, "html.small": 900_000 },
	cli: { small: { instructions: 200_000_000, footprint: 10_000_000 } },
};
const same = structuredClone(baseline);

function acceptedGrowthFixture() {
	const commit = "a".repeat(40), receipt = `full-20261029T200000Z-${commit.slice(0, 10)}`;
	const before = {
		sizes: { "wasm.site": 100, "wasm.runtime": 200, "binary.richview": 300, "html.small": 400, "html.flagship": 500, "html.dense": 600 },
		cli: Object.fromEntries(["small", "flagship", "dense"].map(name => [name, { footprint: 1000, instructions: 10000 }])),
		allTypes: { entries: { "line:stable": { native_ms: 12, mount_ms: 20 } } },
	};
	const measured = { sizes: Object.fromEntries(Object.keys(before.sizes).map(key => [key, 1000000])), cli: Object.fromEntries(Object.keys(before.cli).map(name => [name, { footprint: 2000, instructions: 10000 }])) };
	const acceptance = { id: "daniel-2026-10-29", authority: "Daniel", date: "2026-10-29", words: "Rebase + quiet + 2 MB", commit };
	return { before, measured, context: { acceptance, source: { commit, receipt, build: { commit, dirty: false } } } };
}

test("accepted sizes and memory use the exact full's measurements once, leaving every other regression gate intact", () => {
	const { before, measured, context } = acceptedGrowthFixture();
	assert.ok(compareBaseline(before, measured).regressions.length, "old baseline rejects the accepted growth");
	const accepted = acceptSizesAndMemory(before, measured, context);
	assert.deepEqual(compareBaseline(accepted.baseline, measured).regressions, []);
	assert.deepEqual(accepted.baseline.allTypes, before.allTypes);
	assert.deepEqual(accepted.acceptance.previous.sizes, before.sizes);
	assert.equal(accepted.acceptance.receipt, context.source.receipt);
	assert.equal(accepted.acceptance.receiptKind, "full");
	const later = structuredClone(measured);
	later.sizes["wasm.site"] += measured.sizes["wasm.site"] * MARGINS.wasm + 1;
	later.cli.small.footprint *= 1.26;
	later.cli.flagship.instructions *= 1.21;
	const retry = acceptSizesAndMemory(accepted.baseline, later, context);
	assert.equal(retry.acceptance, undefined, "the same authorization cannot ratchet later growth");
	assert.deepEqual(compareBaseline(retry.baseline, later).regressions.map(r => r.key), ["sizes.wasm.site", "cli.small.footprint", "cli.flagship.instructions"]);
	assert.equal(before.cli.small.footprint, 1000, "the previous record remains unchanged");
});

test("accepted growth records an affected receipt for the same clean commit", () => {
	const { before, measured, context } = acceptedGrowthFixture();
	context.source.receipt = context.source.receipt.replace("full-", "affected-");
	const accepted = acceptSizesAndMemory(before, measured, context);
	assert.equal(accepted.acceptance.receipt, context.source.receipt);
	assert.equal(accepted.acceptance.receiptKind, "affected");
});

test("the 9 November size relaxation records either receipt and leaves the baseline unchanged", () => {
	for (const kind of ["full", "affected"]) {
		const { before, measured, context } = acceptedGrowthFixture();
		context.acceptance.id = "daniel-2026-11-09-size-relax";
		context.source.receipt = context.source.receipt.replace("full-", `${kind}-`);
		const saved = structuredClone(before);
		const accepted = acceptSizesAndMemory(before, measured, context);
		assert.equal(accepted.advisory, true);
		assert.deepEqual(accepted.baseline, saved);
		assert.equal(accepted.acceptance.receiptKind, kind);
		assert.equal(accepted.acceptance.effect, "advisory");
		const regressions = compareBaseline(accepted.baseline, measured).regressions;
		assert.ok(regressions.some(row => row.key === "sizes.binary.richview"));
		const verdict = judge({ values: {}, regressions, stages: new Set() });
		assert.ok(verdict.disclosures.some(row => row.name === "sizes.binary.richview" && row.note === "relaxed until launch"));
	}
});

test("accepted growth refuses a different or dirty build, partial measurements and other receipt kinds", () => {
	for (const mutate of [
		f => delete f.context.acceptance.id,
		f => f.context.acceptance.authority = "other",
		f => f.context.acceptance.words = " ",
		f => delete f.context.acceptance.date,
		f => f.context.acceptance.commit = "b".repeat(40),
		f => f.context.source.build.commit = "b".repeat(40),
		f => f.context.source.build.dirty = true,
		f => f.context.source.receipt = f.context.source.receipt.replace("full-", "prod-"),
		f => f.context.source.receipt = f.context.source.receipt.replace(/a{10}$/, "b".repeat(10)),
		f => f.context.source.receipt = f.context.source.receipt.replace("full-", "affected-").replace(/a{10}$/, "b".repeat(10)),
		f => delete f.measured.cli.dense,
		f => f.measured.sizes["wasm.site"] = NaN,
	]) {
		const f = acceptedGrowthFixture(); mutate(f);
		assert.throws(() => acceptSizesAndMemory(f.before, f.measured, f.context), /Accepted perf growth/);
		assert.equal(f.before.acceptedGrowth, undefined);
	}
	const f = acceptedGrowthFixture();
	assert.equal(acceptSizesAndMemory(f.before, f.measured).baseline, f.before, "normal captures do not accept costs");
});

test("growth inside PRF-05's margins passes and growth beyond them regresses", () => {
	const near = structuredClone(baseline);
	near.sizes["wasm.site"] += baseline.sizes["wasm.site"] * MARGINS.wasm;
	near.sizes["html.small"] += baseline.sizes["html.small"] * MARGINS.html;
	near.sizes["binary.richview"] += baseline.sizes["binary.richview"] * MARGINS.binary;
	near.cli.small.instructions *= 1.2;
	near.cli.small.footprint *= 1.25;
	assert.deepEqual(compareBaseline(baseline, near).regressions, []);
	const over = structuredClone(baseline);
	over.sizes["wasm.site"] += baseline.sizes["wasm.site"] * MARGINS.wasm + 1;
	over.sizes["binary.richview"] += baseline.sizes["binary.richview"] * MARGINS.binary + 1;
	over.cli.small.instructions *= 1.21;
	over.cli.small.footprint *= 1.26;
	assert.deepEqual(
		compareBaseline(baseline, over).regressions.map((r) => r.key).sort(),
		["cli.small.footprint", "cli.small.instructions", "sizes.binary.richview", "sizes.wasm.site"],
	);
});

test("a shrink beyond the margin is an improvement, and a measurement with no baseline is not compared", () => {
	const better = structuredClone(same);
	better.cli.small.instructions *= 0.79;
	assert.deepEqual(compareBaseline(baseline, better).improvements.map((r) => r.key), ["cli.small.instructions"]);
	const extra = { ...structuredClone(same), cli: { ...same.cli, flagship: { instructions: 9e9, footprint: 9e9 } } };
	assert.deepEqual(compareBaseline(baseline, extra), { regressions: [], improvements: [] });
});

const ps = [
	"  101 /opt/rust/.rustup/toolchains/1.99.0-aarch64-apple-darwin/bin/rustc --crate-name foo --edition=2024",
	"  102 /opt/rust/.rustup/toolchains/1.99.0-aarch64-apple-darwin/bin/cargo test -p richview-core",
	"  103 /bin/zsh /opt/rv/bin/cargo build",
	"  104 /opt/cache/ms-playwright/chromium-1200/chrome-mac/Chromium.app/Contents/MacOS/Chromium --no-sandbox",
	"  105 /Applications/Google Chrome.app/Contents/MacOS/Google Chrome --profile-directory=Default",
	"  106 /Applications/Google Chrome.app/Contents/MacOS/Google Chrome --headless=new --print-to-pdf",
	"  107 node scripts/verify-site.mjs",
	"  108 /opt/rust/.rustup/toolchains/1.99.0-aarch64-apple-darwin/bin/rustc -vV",
	"  109 /gate/cache/vscode/vscode-darwin-arm64-1.136.1/Visual Studio Code.app/Contents/MacOS/Electron",
].join("\n");

test("busy processes are real builds and headless browsers; a waiting shim and a person's browser are not", () => {
	assert.deepEqual(busyProcesses(ps, 108).map((p) => p.pid), [101, 102, 104, 106, 109]);
});

function quietClock({ busy, loads }) {
	let t = 0;
	let i = 0;
	return {
		now: () => t,
		sleep: async (ms) => {
			t += ms;
			i++;
		},
		inFlight: () => (i < busy ? [{ pid: 1, command: "rustc --crate-name x" }] : []),
		loadavg: () => loads[Math.min(i, loads.length - 1)],
	};
}

test("the quiet capture holds the streams, waits for in-flight builds, measures once and releases the hold", async () => {
	const events = [];
	const capture = async () => {
		events.push("capture");
		return { values: { "cli.small.p95": 100 } };
	};
	const hold = () => {
		events.push("hold");
		return () => events.push("release");
	};
	const quiet = await quietCapture({ ...quietClock({ busy: 3, loads: [9, 6, 4, 2, 1.5] }), hold, capture, pollMs: 5_000 });
	assert.deepEqual(events, ["hold", "capture", "release"]);
	assert.equal(quiet.attempts[0].result, "measured");
	assert.equal(quiet.attempts[0].wait.waitedMs, 15_000);
	assert.deepEqual(quiet.measured.values, { "cli.small.p95": 100 });
});

test("work that never drains is measured within two minutes, even when a caller asks for ten", async () => {
	const result = await quietCapture({ ...quietClock({ busy: Infinity, loads: [8] }), hold: () => () => {}, capture: async () => ({ values: {} }), maxWaitMs: 10 * 60_000, pollMs: 5_000 });
	const [attempt] = result.attempts;
	assert.equal(attempt.result, "measured under load");
	assert.ok(attempt.wait.waitedMs <= 2 * 60_000 && attempt.wait.waitedMs >= 2 * 60_000 - 5_000);
	assert.equal(attempt.loadBefore, 8);
	assert.deepEqual(attempt.inFlight, ["rustc --crate-name x"]);
	assert.match(attempt.reason, /1 build or browser process\(es\) still running after 120 s; measured anyway/);
});

test("after the builds drain, a load that stays high delays the capture by the settle time only", async () => {
	const result = await quietCapture({ ...quietClock({ busy: 2, loads: [7] }), hold: () => () => {}, capture: async () => ({ values: {} }), settleMs: 60_000, pollMs: 5_000 });
	assert.equal(result.attempts[0].result, "measured under load");
	assert.ok(result.attempts[0].wait.waitedMs <= 10_000 + 30_000);
	assert.match(result.attempts[0].reason, /load 7 above 2/);
});

test("a capture that throws still releases the hold", async () => {
	let released = false;
	await assert.rejects(quietCapture({ ...quietClock({ busy: 0, loads: [1] }), hold: () => () => (released = true), capture: async () => { throw new Error("browser died"); } }), /browser died/);
	assert.equal(released, true);
});

const repeatBudgets = {
	"webkit.frame.max": BUDGETS["webkit.frame.max"],
	"cli.small.p95": BUDGETS["cli.small.p95"],
};
const timingVerdict = timing => judge({ values: timing.measured.values, timing, budgets: repeatBudgets, stages: new Set(["timing"]) });

test("a timing failure under load repeats the whole capture after a new quiet wait and records both attempts", async () => {
	const events = [], calls = [];
	const timing = await quietCapture({
		...quietClock({ busy: Infinity, loads: [8] }), maxWaitMs: 10_000,
		hold: () => { events.push("hold"); return () => events.push("release"); },
		capture: async attempt => {
			calls.push(attempt); events.push("capture");
			return { values: { "webkit.frame.max": attempt === 1 ? 22 : 16, "cli.small.p95": 100 } };
		},
	});
	assert.deepEqual(calls, [1, 2]);
	assert.deepEqual(events, ["hold", "capture", "capture", "release"]);
	assert.deepEqual(timing.attempts.map(attempt => [attempt.n, attempt.wait.waitedMs]), [[1, 10_000], [2, 10_000]]);
	assert.equal(timing.attempts[0].measured.values["webkit.frame.max"], 22);
	assert.equal(timing.attempts[1].measured.values["webkit.frame.max"], 16);
	assert.equal(timing.decidingAttempt, 2);
	assert.equal(timingVerdict(timing).verdict, "pass");
	const receipt = loadNote({ timing }).details;
	assert.deepEqual(receipt.timingAttempts, timing.attempts);
	assert.equal(receipt.decidingTimingAttempt, 2);
});

test("a second timing failure fails at the unchanged limit without a third capture", async () => {
	let calls = 0;
	const timing = await quietCapture({ ...quietClock({ busy: 0, loads: [8] }), settleMs: 0, hold: () => () => {}, capture: async () => { calls++; return { values: { "webkit.frame.max": 22, "cli.small.p95": 100 } }; } });
	assert.equal(calls, 2);
	assert.equal(timingVerdict(timing).verdict, "fail");
	assert.equal(timingVerdict(timing).failures[0].limit, 16.7);
});

test("a quiet timing failure, a loaded pass and non-timing failures never trigger a repeat", async () => {
	for (const [load, values] of [[1, { "webkit.frame.max": 22 }], [8, { "webkit.frame.max": 16 }], [8, { "site.payload": 3e6, "cli.small.footprint": 2e9 }]]) {
		let calls = 0;
		const timing = await quietCapture({ ...quietClock({ busy: 0, loads: [load] }), settleMs: 0, hold: () => () => {}, capture: async () => { calls++; return { values }; } });
		assert.equal(calls, 1, JSON.stringify({ load, values }));
		assert.equal(timing.attempts.length, 1);
	}
});

test("attempt two alone decides every timing budget, including one that passed in attempt one", async () => {
	const timing = await quietCapture({ ...quietClock({ busy: 0, loads: [8] }), settleMs: 0, hold: () => () => {}, capture: async attempt => ({ values: attempt === 1
		? { "webkit.frame.max": 22, "cli.small.p95": 100 }
		: { "webkit.frame.max": 16, "cli.small.p95": 301 } }) });
	assert.deepEqual(timing.measured, timing.attempts[1].measured);
	assert.deepEqual(timingVerdict(timing).failures.map(failure => failure.name), ["cli.small.p95"]);
	const external = { "site.payload": 3e6, "cli.small.footprint": 2e9 };
	const verdict = judge({ values: { ...external, ...timing.measured.values }, timing, budgets: { ...repeatBudgets, "site.payload": BUDGETS["site.payload"], "cli.small.footprint": BUDGETS["cli.small.footprint"] }, regressions: [{ key: "cli.small.instructions", was: 1, now: 2, margin: 0.2 }] });
	assert.ok(verdict.disclosures.some(note => note.name === "site.payload" && note.note === "relaxed until launch"));
	assert.ok(verdict.failures.some(failure => failure.name === "cli.small.footprint"));
	assert.ok(verdict.failures.some(failure => failure.name === "cli.small.instructions"));
});

test("a missing, non-finite or failed second capture cannot reuse passing values from the first", async () => {
	for (const repeat of [() => ({ values: { "webkit.frame.max": 16 } }), () => ({ values: { "webkit.frame.max": NaN, "cli.small.p95": 100 } }), () => { throw new Error("browser died"); }]) {
		let releases = 0;
		const timing = await quietCapture({ ...quietClock({ busy: 0, loads: [8] }), settleMs: 0, hold: () => () => releases++, capture: async attempt => attempt === 1 ? { values: { "webkit.frame.max": 22, "cli.small.p95": 100 } } : repeat() });
		assert.equal(releases, 1);
		assert.equal(timing.attempts.length, 2);
		assert.equal(timingVerdict(timing).verdict, "fail");
	}
});

test("the perf hold names this process and is removed only while it is still ours", () => {
	const dir = mkdtempSync(join(tmpdir(), "rv-perf-hold-"));
	const file = join(dir, "target", ".rv-perf-quiet");
	const release = holdStreams(file, 4242);
	assert.equal(readFileSync(file, "utf8").trim(), "4242");
	release();
	assert.equal(existsSync(file), false);
	const mine = holdStreams(file, 4242);
	writeFileSync(file, "777\n");
	mine();
	assert.equal(readFileSync(file, "utf8").trim(), "777", "another run's hold stays");
	rmSync(dir, { recursive: true, force: true });
});

const ok = {
	"site.payload": 1_900_000,
	"report-runtime.payload": 500_000,
	"html.small.payload": 200_000,
	"html.flagship.payload": 700_000,
	"html.dense.payload": 2_000_000,
	"cli.small.footprint": 10e6,
	"cli.flagship.footprint": 30e6,
	"cli.dense.footprint": 20e6,
	"cli.small.p95": 30,
	"cli.flagship.p95": 90,
	"cli.dense.p95": 40,
	"webkit.frame.p95": 5,
	"webkit.frame.max": 12,
};
const measured = { measured: { values: {} }, attempts: [] };

test("every budget within its limit passes", () => {
	assert.equal(judge({ values: ok, timing: measured }).verdict, "pass");
});

test("a budget over its limit fails the step and names the card", () => {
	assert.equal(BUDGETS["site.payload"].limit, 2_000_000, "Daniel29Oct: PRF-30 is exactly decimal2MB");
	assert.equal(judge({ values: { ...ok, "site.payload": 2_000_000 }, timing: measured }).verdict, "pass");
	const verdict = judge({ values: { ...ok, "site.payload": 2_000_001 }, timing: measured, relaxSizeBudgets: false });
	assert.equal(verdict.verdict, "fail");
	assert.deepEqual(verdict.failures.map((f) => [f.name, f.why]), [["site.payload", "PRF-30"]]);
});

test("the report runtime gates at PRF-31's answered 2 MiB", () => {
	assert.equal(judge({ values: { ...ok, "report-runtime.payload": 896_582 }, timing: measured }).verdict, "pass");
	const over = judge({ values: { ...ok, "report-runtime.payload": 2_200_000 }, timing: measured, relaxSizeBudgets: false });
	assert.equal(over.verdict, "fail");
	assert.deepEqual(over.failures.map((f) => [f.name, f.why]), [["report-runtime.payload", "PRF-31"]]);
});

test("a regression against the baseline fails the step", () => {
	const verdict = judge({ values: ok, timing: measured, regressions: [{ key: "cli.small.instructions", was: 1, now: 2, margin: "5%" }] });
	assert.equal(verdict.verdict, "fail");
	assert.equal(verdict.failures[0].why, "PRF-05");
});

test("an unmeasured budget fails, except timing budgets with no quiet capture, which are inconclusive", () => {
	const { "cli.small.footprint": _drop, ...missing } = ok;
	assert.deepEqual(judge({ values: missing, timing: measured }).failures.map((f) => f.name), ["cli.small.footprint"]);
	const timing = Object.fromEntries(Object.keys(ok).filter((k) => BUDGETS[k].timing).map((k) => [k, undefined]));
	const { "cli.small.p95": _a, "cli.flagship.p95": _b, "cli.dense.p95": _c, "webkit.frame.p95": _d, "webkit.frame.max": _e, ...sizes } = ok;
	const verdict = judge({ values: sizes, timing: { measured: undefined, attempts: [{ result: "inconclusive" }, { result: "inconclusive" }] } });
	assert.equal(verdict.verdict, "inconclusive");
	assert.equal(verdict.inconclusive.length, Object.keys(timing).length);
	// A failure still outranks inconclusive timing.
	assert.equal(judge({ values: { ...sizes, "cli.small.footprint": 3e9 }, timing: { measured: undefined, attempts: [] } }).verdict, "fail");
});

test("a stage nobody asked for is skipped, not failed", () => {
	const sizesOnly = { "site.payload": 1_900_000, "report-runtime.payload": 500_000 };
	assert.equal(judge({ values: sizesOnly, stages: new Set(["sizes"]) }).verdict, "pass");
	assert.equal(stageOf("cli.small.p95", BUDGETS["cli.small.p95"]), "timing");
	assert.equal(stageOf("cli.small.footprint", BUDGETS["cli.small.footprint"]), "cli");
	assert.equal(stageOf("site.payload", BUDGETS["site.payload"]), "sizes");
});

test("a capture measured under load puts the load and the running processes in the step reason and details", async () => {
	const result = await quietCapture({ ...quietClock({ busy: Infinity, loads: [8] }), hold: () => () => {}, capture: async () => ({ values: {} }), maxWaitMs: 60_000, pollMs: 5_000 });
	const note = loadNote({ timing: { attempts: result.attempts } });
	assert.match(note.reason, /measured under load: load 8 before, 8 after; still running: rustc --crate-name x/);
	assert.equal(note.details.measuredUnderLoad, true);
	assert.deepEqual(note.details.stillRunning, ["rustc --crate-name x"]);
	assert.equal(loadNote({ timing: { attempts: [{ result: "measured" }] } }), undefined, "a quiet capture adds nothing");
	assert.equal(loadNote({}), undefined);
});

// A host with nothing in flight whose 1-minute and 5-minute loads follow the clock.
function loadedHost({ load1, load5 }) {
	let t = 0;
	return { now: () => t, sleep: async (ms) => { t += ms; }, inFlight: () => [], loadavg: () => load1(t), load5: () => load5(t) };
}
const failingFrame = { "webkit.frame.max": 22, "cli.small.p95": 100 };
const passingFrame = { "webkit.frame.max": 16, "cli.small.p95": 100 };
// Under load until 60 s; the 5-minute load is 9 when the first capture fails, then falls 1.5 per 15 s poll.
const fallingLoad = { load1: (t) => (t < 60_000 ? 8 : 1), load5: (t) => Math.max(5.5, 12 - t / 10_000) };

test("a timing failure while the 5-minute load is over 8 waits for it to fall under 6, measures once more, and that capture decides", async () => {
	const events = [], calls = [];
	const timing = await quietCapture({
		...loadedHost(fallingLoad),
		hold: () => { events.push("hold"); return () => events.push("release"); },
		capture: async (attempt) => { calls.push(attempt); events.push("capture"); return { values: attempt === 1 ? failingFrame : passingFrame }; },
	});
	assert.deepEqual(LOAD_RETRY, { over: 8, under: 6, maxWaitMs: 15 * 60_000, pollMs: 15_000 });
	assert.deepEqual(calls, [1, 2], "the load wait replaces the short repeat");
	assert.deepEqual(events, ["hold", "capture", "capture", "release"], "the hold spans the load wait");
	const [first, second] = timing.attempts;
	assert.deepEqual([first.result, first.loadBefore, first.load5Before, first.load5After, first.loadWait], ["measured under load", 8, 9, 9, undefined]);
	assert.deepEqual(second.loadWait, { after: 1, trigger: 9, over: 8, under: 6, maxWaitMs: 15 * 60_000, fell: true, load5: 5.5, waitedMs: 45_000, samples: [{ at: 0, load5: 9 }, { at: 15_000, load5: 7.5 }, { at: 30_000, load5: 6 }, { at: 45_000, load5: 5.5 }] });
	assert.deepEqual([second.result, second.loadBefore, second.load5Before, second.load5After], ["measured", 1, 5.5, 5.5]);
	assert.equal(timing.decidingAttempt, 2);
	assert.deepEqual(timing.measured.values, passingFrame);
	assert.equal(timingVerdict(timing).verdict, "pass");
});

test("the load wait stops at 15 minutes and measures anyway; that capture decides and fails at the unchanged limit", async () => {
	let calls = 0;
	const timing = await quietCapture({ ...loadedHost({ load1: () => 8, load5: () => 11 }), hold: () => () => {}, capture: async () => { calls++; return { values: failingFrame }; } });
	assert.equal(calls, 2, "no capture after the one that waited");
	const wait = timing.attempts[1].loadWait;
	assert.deepEqual([wait.fell, wait.load5, wait.waitedMs, wait.samples.length], [false, 11, 15 * 60_000, 61]);
	assert.equal(timingVerdict(timing).verdict, "fail");
	assert.equal(timingVerdict(timing).failures[0].limit, 16.7);
	assert.match(loadNote({ timing }).reason, /attempt 2 waited 900 s for it to fall under 6 \(still 11, measured anyway\) and decides/);
});

test("a failure at a 5-minute load of 8 or less keeps the short repeat; a repeat failing over 8 waits for the load once", async () => {
	const calls = [];
	const timing = await quietCapture({
		...loadedHost({ load1: () => 8, load5: (t) => (t <= 30_000 ? 8 : t <= 60_000 ? 9 : 5) }),
		hold: () => () => {},
		capture: async (attempt) => { calls.push(attempt); return { values: attempt < 3 ? failingFrame : passingFrame }; },
	});
	assert.deepEqual(calls, [1, 2, 3]);
	assert.deepEqual(timing.attempts.map((a) => [a.n, a.load5After, a.loadWait?.after]), [[1, 8, undefined], [2, 9, undefined], [3, 5, 2]]);
	assert.equal(timing.decidingAttempt, 3);
	assert.equal(timingVerdict(timing).verdict, "pass");
	let quiet = 0;
	await quietCapture({ ...loadedHost({ load1: () => 1, load5: () => 7 }), hold: () => () => {}, capture: async () => { quiet++; return { values: failingFrame }; } });
	assert.equal(quiet, 1, "a quiet failure at a 5-minute load of 8 or less still fails without a repeat");
});

test("the receipt keeps both measurements, their loads and the load wait, and the go card says which capture decided", async (t) => {
	const timing = await quietCapture({ ...loadedHost(fallingLoad), hold: () => () => {}, capture: async (attempt) => ({ values: attempt === 1 ? failingFrame : passingFrame }) });
	const note = loadNote({ timing });
	assert.equal(note.reason, "timing failed in attempt 1 with the 5-minute load at 9, over 8; attempt 2 waited 45 s for it to fall under 6 (it fell to 5.5) and decides; timing attempt 1 measured under load: load 8 before, 8 after; no build or browser still running");
	assert.deepEqual(note.details.loadRetry, { attempt: 2, after: 1, trigger: 9, over: 8, under: 6, maxWaitMs: 15 * 60_000, fell: true, load5: 5.5, waitedMs: 45_000 });
	assert.deepEqual(note.details.timingAttempts.map((a) => [a.measured.values["webkit.frame.max"], a.loadBefore, a.loadAfter, a.load5Before, a.load5After]), [[22, 8, 8, 9, 9], [16, 1, 1, 5.5, 5.5]]);
	assert.equal(note.details.decidingTimingAttempt, 2);
	const home = mkdtempSync(join(tmpdir(), "rv-perf-go-"));
	const before = process.env.RV_GATE_HOME;
	t.after(() => {
		if (before === undefined) delete process.env.RV_GATE_HOME;
		else process.env.RV_GATE_HOME = before;
		rmSync(home, { recursive: true, force: true });
	});
	process.env.RV_GATE_HOME = home;
	const card = readFileSync(writeGoCard({ receipt: { id: "prod-1", candidate: "candidate/1", commit: "c", tree: "t", prod: "p", result: "pass", steps: [{ id: "perf", status: "pass", reason: note.reason, details: note.details }] } }), "utf8");
	assert.match(card, /^Perf: timing failed in attempt 1 with the 5-minute load at 9, over 8; attempt 2 waited 45 s for it to fall under 6 \(it fell to 5\.5\) and decides; timing was measured under load \(load 8 before, 8 after; nothing of ours still running\)\. The verdict is on the budgets alone\.$/m);
});

test("a chart's limit is its baseline plus PRF-05's tolerance: no baseline, within, over", () => {
	const baseline = { entries: { "line:a": { native_ms: 50, mount_ms: 100 }, "bar:b": { native_ms: 50, mount_ms: 100 } } };
	const measured = [
		{ index: 0, type: "line", key: "line:a", native_ms: 50 * (1 + TYPE_TOLERANCE), mount_ms: 100 },
		{ index: 1, type: "bar", key: "bar:b", native_ms: 50, mount_ms: 100 * (1 + TYPE_TOLERANCE) + 1 },
		{ index: 2, type: "pie", key: "pie:c", native_ms: 40, mount_ms: 90 },
	];
	const { entries, regressions, gap } = judgeTypes(baseline, measured);
	assert.deepEqual(entries.map((e) => e.status), ["pass", "fail", "no baseline yet"]);
	assert.deepEqual([entries[0].limit_native_ms, entries[0].limit_mount_ms], [60, 120]);
	assert.deepEqual([entries[2].limit_native_ms, entries[2].limit_mount_ms], [null, null], "a chart with no baseline has no limit");
	assert.deepEqual(regressions.map((r) => r.key), ["allTypes.bar:b.mount_ms"], "only the chart over its limit regresses; no baseline is not a failure");
	assert.equal(gap.withoutBaseline, 1);
	assert.deepEqual(gap.charts, [{ index: 2, type: "pie" }]);
});

test("a run with no baseline reports every chart as a gap with the count, never silently (review 1l should-fix 5)", () => {
	const measured = Array.from({ length: 57 }, (_, index) => ({ index, type: "bar", key: `bar:${index}`, native_ms: 5, mount_ms: 50 }));
	for (const was of [undefined, { entries: {} }]) {
		const { entries, regressions, gap } = judgeTypes(was, measured);
		assert.equal(entries.every((e) => e.status === "no baseline yet"), true);
		assert.deepEqual(regressions, []);
		assert.match(gap.text, /^57 charts without a baseline /);
		assert.equal(gap.charts.length, 57);
	}
	assert.equal(judgeTypes({ entries: { "bar:0": { native_ms: 5, mount_ms: 50 } } }, [{ index: 0, type: "bar", key: "bar:0", native_ms: 5, mount_ms: 50 }]).gap, undefined);
});

test("a chart's baseline key follows its own block, data and type definition, not the rest of the fixture", () => {
	const registry = 'ChartType { name: "bar", fields: 1 },\n    ChartType { name: "line", fields: 2 },\n    ChartType { name: "pie", fields: 3 },';
	const bar = { type: "bar", block: "type: bar\ndata: {file: data/a.csv}\n" };
	const key = (chart, text = registry, files = new Map([["a.csv", "1"], ["b.csv", "2"]])) => chartKey(chart, typeIdentity(text, chart.type), files);
	assert.match(key(bar), /^bar:[0-9a-f]{16}$/);
	assert.equal(key(bar), key({ ...bar }), "the same block keeps its key whatever else the fixture holds");
	assert.notEqual(key(bar), key({ ...bar, block: `${bar.block}title: x\n` }), "an edit of the chart is a new chart");
	assert.notEqual(key(bar), key(bar, registry, new Map([["a.csv", "changed"]])), "so is an edit of its data file");
	assert.equal(key(bar), key(bar, registry, new Map([["a.csv", "1"], ["b.csv", "changed"]])), "another chart's data file is not its data");
	assert.equal(key(bar), key(bar, registry.replace("fields: 2", "fields: 9")), "another type's registry entry is not its identity");
	assert.notEqual(key(bar), key(bar, registry.replace("fields: 1", "fields: 9")), "its own registry entry is");
	assert.equal(typeIdentity("", "bar"), "bar", "a registry without the type gives the name alone");
});

test("the all-types fixture splits into one report per chart fence, with its type", () => {
	const source = "---\ntitle: x\n---\n\n```chart\ntype: line\ntitle: A\n```\n\n```callout\ntype: note\n```\n\n```chart\ntype: pie\ntitle: B\n```\n";
	const charts = chartBlocks(source);
	assert.deepEqual(charts.map((c) => [c.index, c.type]), [[0, "line"], [1, "pie"]]);
	assert.match(oneChartReport(charts[1]), /^---\ntitle: All types 1 pie\nversion: 0\.0\.1\n---\n\n```chart\ntype: pie\ntitle: B\n```\n$/);
	assert.throws(() => chartBlocks("```chart\ntitle: A\n```\n"), /names no type/);
});

test("the staged all-types fixture gives every chart its own key, found again by a second staging", async () => {
	const { stageAllTypes } = await import("../perf.mjs");
	const { mkdtempSync, rmSync } = await import("node:fs");
	const { tmpdir } = await import("node:os");
	const { join, resolve } = await import("node:path");
	const out = mkdtempSync(join(tmpdir(), "perf-stage-"));
	try {
		const cwd = resolve(import.meta.dirname, "../../..");
		const first = stageAllTypes({ cwd, out });
		const second = stageAllTypes({ cwd, out });
		assert.equal(new Set(first.charts.map((chart) => chart.key)).size, first.charts.length);
		assert.deepEqual(second.charts.map((chart) => chart.key), first.charts.map((chart) => chart.key));
	} finally {
		rmSync(out, { recursive: true, force: true });
	}
});

test("size overages are advisory until launch, with their original limits recorded", () => {
	const regression = { key: "sizes.wasm.site", was: 1000, now: 1101, margin: "10%" };
	const result = judge({ values: { ...ok, "site.payload": 2_000_001 }, timing: measured, regressions: [regression] });
	assert.equal(result.verdict, "pass");
	assert.deepEqual(result.failures, []);
	assert.deepEqual(result.disclosures.map(({ name, value, limit, note }) => ({ name, value, limit, note })), [
		{ name: "site.payload", value: 2_000_001, limit: 2_000_000, note: "relaxed until launch" },
		{ name: "sizes.wasm.site", value: 1101, limit: 1100, note: "relaxed until launch" },
	]);
});

test("all payload overages are notes but their measurements remain required", () => {
	for (const name of ["site.payload", "report-runtime.payload", "html.small.payload", "html.flagship.payload", "html.dense.payload"]) {
		const budget = BUDGETS[name];
		const boundary = judge({ values: { ...ok, [name]: budget.limit }, timing: measured });
		assert.equal(boundary.verdict, "pass");
		assert.deepEqual(boundary.disclosures, []);
		const over = judge({ values: { ...ok, [name]: budget.limit + 1 }, timing: measured });
		assert.equal(over.verdict, "pass");
		assert.deepEqual(over.disclosures, [{ name, value: budget.limit + 1, limit: budget.limit, unit: "B", why: budget.why, note: "relaxed until launch" }]);
		for (const value of [undefined, NaN, Infinity]) {
			const invalid = judge({ values: { ...ok, [name]: value }, timing: measured });
			assert.equal(invalid.verdict, "fail");
			assert.equal(invalid.failures[0].name, name);
		}
	}
});

test("relative size notes keep the baseline and limit while memory, instructions and timing fail", () => {
	const names = ["wasm.site", "wasm.runtime", "binary.richview", "html.small", "html.flagship", "html.dense"];
	const before = { sizes: Object.fromEntries(names.map(name => [name, 1000])) };
	const after = { sizes: Object.fromEntries(names.map(name => [name, 1101])) };
	const regressions = compareBaseline(before, after).regressions;
	const result = judge({ values: { ...ok, "cli.small.footprint": BUDGETS["cli.small.footprint"].limit + 1, "cli.small.p95": BUDGETS["cli.small.p95"].limit + 1 }, timing: measured, regressions: [...regressions, { key: "cli.small.instructions", was: 1000, now: 1201, margin: "20%" }, { key: "cli.small.footprint", was: 1000, now: 1251, margin: "25%" }] });
	assert.equal(result.verdict, "fail");
	assert.deepEqual(result.failures.map(f => f.name), ["cli.small.footprint", "cli.small.p95", "cli.small.instructions", "cli.small.footprint"]);
	assert.deepEqual(result.disclosures.map(d => [d.name, d.was, d.margin, d.limit, d.value, d.note]), names.map(name => [`sizes.${name}`, 1000, "10%", 1100, 1101, "relaxed until launch"]));
	const strict = judge({ values: ok, timing: measured, regressions, relaxSizeBudgets: false });
	assert.equal(strict.verdict, "fail");
	assert.deepEqual(strict.failures.map(f => f.name), names.map(name => `sizes.${name}`));
	assert.deepEqual(strict.disclosures, []);
});

test("prelaunch skips only WebKit timing budgets while launch keeps every budget", () => {
	const full=perfBudgetsFor(false), prelaunch=perfBudgetsFor(true);
	assert.deepEqual(full,PERF_BUDGETS);
	assert.deepEqual(Object.keys(PERF_BUDGETS).filter(key=>!(key in prelaunch)),["webkit.frame.p95","webkit.frame.max"]);
	const values=Object.fromEntries(Object.entries(prelaunch).filter(([,budget])=>budget.timing).map(([name,budget])=>[name,budget.limit]));
	assert.equal(judge({values,timing:{measured:{}},stages:new Set(["timing"]),budgets:prelaunch}).verdict,"pass");
	assert.equal(judge({values,timing:{measured:{}},stages:new Set(["timing"]),budgets:full}).verdict,"fail");
});
