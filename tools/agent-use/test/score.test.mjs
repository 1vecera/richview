// Scorer tests on fixture transcripts. No live agent runs.

import assert from "node:assert/strict";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { readReport } from "../lib/rmd.mjs";
import { parseCheck, parseJsonl, readTranscript } from "../lib/transcript.mjs";
import { scoreRun } from "../score.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const taskDir = join(here, "..", "tasks", "complaint-themes");
const task = JSON.parse(readFileSync(join(taskDir, "task.json"), "utf8"));

function run(transcript, rmd, finalCheck = null) {
	const dir = mkdtempSync(join(tmpdir(), "rv-agent-use-test-"));
	try {
		mkdirSync(join(dir, "report"));
		cpSync(join(taskDir, "data"), join(dir, "report", "data"), { recursive: true });
		writeFileSync(join(dir, "report", "report.rmd"), readFileSync(join(here, "fixtures", `${rmd}.rmd`), "utf8"));
		const { calls, result } = readTranscript(parseJsonl(readFileSync(join(here, "fixtures", `${transcript}.jsonl`), "utf8")), "claude");
		return scoreRun({ calls, result, report: readReport(join(dir, "report")), finalCheck, task, taskDir });
	} finally {
		rmSync(dir, { recursive: true, force: true });
	}
}

const CLEAN = { output: '{"charts":[],"diagnostics":[],"ok":true,"output":null,"status":"clean"}' };

test("clean on the first attempt passes the bar with a full score", () => {
	const s = run("clean-first", "good", CLEAN);
	assert.equal(s.clean.firstAttempt, 1);
	assert.equal(s.clean.final, true);
	assert.equal(s.pass, true);
	assert.equal(s.score, 100);
	assert.equal(s.invisibleDiagnostics, 1, "the MCP summary showed a count only");
	assert.deepEqual(s.misread, []);
});

test("clean after a fix counts the second attempt and the code met", () => {
	const s = run("clean-after-fix", "good", CLEAN);
	assert.equal(s.clean.firstAttempt, 2);
	assert.equal(s.clean.attempts, 2);
	assert.equal(s.pass, true);
	assert.equal(s.points.attempt, 10);
	assert.ok(s.diagnosticsMet["chart.source-error"]);
	assert.deepEqual(s.misread, [], "the code did not come back after the fix");
	assert.deepEqual(s.unresolved, []);
});

test("a code that returns after an edit is misread, and an unfixed one is unresolved", () => {
	const unclean = { output: "report.rmd:38 error chart.failed: Cannot draw this lollipop chart\nChecked 3 charts: issues" };
	const s = run("misread", "repeated", unclean);
	assert.deepEqual(s.misread.map((m) => m.code).sort(), ["chart.failed", "chart.source-error"]);
	assert.deepEqual(s.ignored, []);
	assert.equal(s.clean.final, false);
	assert.equal(s.clean.firstAttempt, null);
	assert.equal(s.pass, false);
	assert.ok(s.unresolved.includes("chart.failed"));
	assert.equal(s.points.clean + s.points.attempt, 0);
});

test("a wrong figure fails its fact and the pass, but not the clean check", () => {
	const s = run("clean-first", "wrong-figure", CLEAN);
	assert.equal(s.clean.final, true);
	const failed = s.facts.items.filter((i) => !i.ok).map((i) => i.id);
	assert.deepEqual(failed.sort(), ["figure:review-count-gap", "figure:review-gap"]);
	assert.equal(s.pass, false);
	assert.ok(s.score < 100 && s.score > 60);
});

test("a missing chart type fails the distinct-types fact and the bar", () => {
	const s = run("clean-first", "two-types", CLEAN);
	assert.deepEqual(s.facts.chartTypes.sort(), ["bar", "lollipop"]);
	assert.equal(s.facts.items.find((i) => i.id === "chart:distinct").ok, false);
	assert.equal(s.bar, false);
	assert.equal(s.pass, false);
});

test("a chart without a source note fails the sources fact", () => {
	const dir = mkdtempSync(join(tmpdir(), "rv-agent-use-test-"));
	try {
		mkdirSync(join(dir, "report"));
		cpSync(join(taskDir, "data"), join(dir, "report", "data"), { recursive: true });
		const rmd = readFileSync(join(here, "fixtures", "good.rmd"), "utf8").replace("*Source: Hacker News negative-comment sample (RichView flagship example).*", "Hacker News gaps are shown above.");
		writeFileSync(join(dir, "report", "report.rmd"), rmd);
		const report = readReport(join(dir, "report"));
		assert.equal(report.charts.filter((c) => !c.source).length, 1);
		const s = scoreRun({ calls: [], report, finalCheck: CLEAN, task, taskDir });
		assert.equal(s.facts.items.find((i) => i.id === "sources").ok, false);
	} finally {
		rmSync(dir, { recursive: true, force: true });
	}
});

test("report length does not change the score", () => {
	const dir = mkdtempSync(join(tmpdir(), "rv-agent-use-test-"));
	try {
		mkdirSync(join(dir, "report"));
		cpSync(join(taskDir, "data"), join(dir, "report", "data"), { recursive: true });
		const base = readFileSync(join(here, "fixtures", "good.rmd"), "utf8");
		const score = (text) => {
			writeFileSync(join(dir, "report", "report.rmd"), text);
			return scoreRun({ calls: [], report: readReport(join(dir, "report")), finalCheck: CLEAN, task, taskDir }).facts.fraction;
		};
		assert.equal(score(base), score(`${base}\n${"A long paragraph of prose. ".repeat(500)}\n`));
	} finally {
		rmSync(dir, { recursive: true, force: true });
	}
});

test("check results are read from JSON, pretty text and the MCP summary", () => {
	assert.equal(parseCheck({ output: '{"diagnostics":[{"code":"x.y","level":"warning","line":3}],"ok":true}' }).clean, false);
	const pretty = parseCheck({ output: "report.rmd:3 warning frontmatter.type: must be a string\nChecked 1 chart: issues" });
	assert.deepEqual(pretty.diagnostics.map((d) => d.code), ["frontmatter.type"]);
	assert.equal(pretty.ok, true);
	assert.equal(parseCheck({ output: "Checked 1 chart: clean" }).clean, true);
	const summary = parseCheck({ output: "check: has errors, 5 diagnostics." });
	assert.deepEqual([summary.ok, summary.visible, summary.count], [false, false, 5]);
});
