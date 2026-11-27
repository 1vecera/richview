// Pure parts of the fresh-agent gate step. These never start an agent.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { parseArgs } from "../gate.mts";
import { aggregate, excerptOf, exitDecision, failedRecord, findings, issueCandidates, renderAppendix, runRecord, taskPlan, trendByBuild } from "../lib/gate.mts";
import { parseCheck, parseJsonl, readTranscript } from "../lib/transcript.mts";
import { series } from "../report.mts";

import type { RunRecord, GateScore } from "../lib/gate.mts";
import type { RunMetadata } from "../history.mts";

const here = dirname(fileURLToPath(import.meta.url));
const TASKS = ["a", "b", "c", "d", "e"];

const meta = (extra: Partial<RunMetadata> = {}) => ({ runId: "r1", product: "claude", model: "sonnet", task: "a", exitCode: 0, timedOut: false, startedAt: "2026-10-28T10:00:00Z", finishedAt: "2026-10-28T10:05:00Z", runDir: "/tmp/r1", ...extra });
const scored = (extra: Partial<GateScore> = {}): GateScore => ({ score: 90, pass: true, clean: { attempts: 1, firstAttempt: 1 }, toolCalls: 9, diagnosticsMet: {}, misread: [], ignored: [], unresolved: [], facts: { items: [{ id: "f", ok: true }] }, ...extra });
const record = (product: string, task: string, extra: Partial<RunRecord> = {}) => ({ ...runRecord(meta({ product, task, runId: `${product}-${task}` }), scored()), ...extra });

test("rotation: consecutive runs differ, the next gate run continues where this stopped", () => {
	const plan = taskPlan(TASKS, ["claude", "codex"], 3, 0);
	assert.deepEqual(plan.map((p) => p.task), ["a", "b", "c", "d", "e", "a"]);
	assert.deepEqual(plan.map((p) => p.product), ["claude", "claude", "claude", "codex", "codex", "codex"]);
	const next = taskPlan(TASKS, ["claude", "codex"], 3, 6);
	assert.equal(next[0].task, "b");
	for (const offset of [0, 3, 4, 9, 13]) {
		const both = taskPlan(TASKS, ["claude", "codex"], 3, offset);
		both.slice(1).forEach((p, i) => assert.notEqual(p.task, both[i].task));
		assert.notEqual(taskPlan(TASKS, ["claude", "codex"], 3, offset + 6)[0].task, both.at(-1)!.task);
	}
	assert.throws(() => taskPlan([], ["claude"], 1));
});

test("a run record keeps misread, ignored and unresolved codes with their messages", () => {
	const r = runRecord(
		meta(),
		scored({
			pass: false,
			diagnosticsMet: { "chart.a": { level: "error", message: "bad a" }, "chart.b": { level: "warning", message: "bad b" }, "chart.c": { level: "warning", message: "bad c" } },
			misread: [{ code: "chart.a", level: "error" }],
			ignored: [{ code: "chart.b", level: "warning" }],
			unresolved: ["chart.c"],
			facts: { items: [{ id: "figure:x", ok: false }] },
		}),
	);
	assert.deepEqual(r.confusions.map((c) => [c.kind, c.code, c.message]), [["misread", "chart.a", "bad a"], ["ignored", "chart.b", "bad b"], ["unresolved", "chart.c", "bad c"]]);
	assert.deepEqual(r.factsFailed, ["figure:x"]);
	assert.equal(r.durationMs, 300_000);
	assert.equal(r.executed, true);
});

test("a run record names the clauth profile the run signed in under", () => {
	assert.equal(runRecord(meta({ profile: "work" }), scored()).profile, "work");
	assert.equal(runRecord(meta({ product: "codex" }), scored()).profile, null);
});

test("a run that timed out or crashed before any tool call did not execute", () => {
	assert.equal(runRecord(meta({ timedOut: true }), scored()).executed, false);
	assert.equal(runRecord(meta({ exitCode: 1 }), scored({ toolCalls: 0 })).executed, false);
	assert.equal(runRecord(meta({ exitCode: 1 }), scored({ toolCalls: 4 })).executed, true);
});

test("pass rate aggregation", () => {
	const runs = [record("claude", "a", { score: 100, attempts: 1, toolCalls: 8, diagnosticCodes: ["x"] }), record("claude", "b", { pass: false, score: 60, attempts: 3, toolCalls: 20, diagnosticCodes: ["x", "y"], misread: ["y"], unresolved: ["y"] }), record("claude", "c", { score: 80, attempts: 2, toolCalls: 11 })];
	const p = aggregate("claude", runs);
	assert.equal(p.passed, 2);
	assert.equal(p.passRate, 0.667);
	assert.deepEqual(p.scores, [100, 60, 80]);
	assert.equal(p.meanScore, 80);
	assert.deepEqual(p.attempts, [1, 3, 2]);
	assert.deepEqual(p.diagnosticCodes, ["x", "y"]);
	assert.deepEqual(p.misread, ["y"]);
	assert.deepEqual(p.unresolved, ["y"]);
	assert.equal(aggregate("codex", []).passRate, null);
});

test("exit rule: only a run that could not execute fails the gate; low pass rates are findings", () => {
	const none = aggregate("claude", [0, 1, 2].map((i) => record("claude", `t${i}`, { pass: false, score: 20 })));
	assert.equal(none.passRate, 0);
	assert.equal(exitDecision([none], 3).status, "pass");
	assert.ok(findings([none], []).some((f) => /pass rate 0\/3 is under 67%/.test(f)));

	const crashed = aggregate("codex", [record("codex", "a"), failedRecord("codex", "b", "codex not found"), record("codex", "c")]);
	const d = exitDecision([none, crashed], 3);
	assert.equal(d.status, "fail");
	assert.match(d.reasons.join(), /codex b: could not execute \(codex not found\)/);

	const short = aggregate("claude", [record("claude", "a")]);
	assert.match(exitDecision([short], 3).reasons[0], /1 of 3 runs finished/);
	assert.equal(exitDecision([short], 1).status, "pass");
});

test("findings flag a score drop of this gate run only", () => {
	const entry = (at: string, score: number, runId: string) => ({ at, runId, task: "a", product: "claude", build: { commit: "x" }, score, pass: true });
	const history = [entry("2026-10-01", 95, "o1"), entry("2026-10-02", 95, "o2"), entry("2026-10-03", 50, "r9")];
	const p = aggregate("claude", [record("claude", "a", { runId: "r9", score: 50, pass: true })]);
	assert.ok(findings([p], series(history)).some((f) => /claude \/ a|a \/ claude/.test(f) && /45 points below/.test(f)));
	const other = aggregate("claude", [record("claude", "a", { runId: "elsewhere" })]);
	assert.deepEqual(findings([other], series(history)), []);
});

test("issue candidates group by code with each transcript and excerpt", () => {
	const mk = (task: string, kind: string) => record("claude", task, { confusions: [{ kind, code: "chart.a", level: "warning", message: "bad a" }], runDir: `/tmp/${task}`, transcript: `/out/${task}.jsonl` });
	const c = issueCandidates([aggregate("claude", [mk("a", "misread"), mk("b", "unresolved"), record("claude", "c")])], { excerptOf: (r) => `/out/ex-${r.task}.md` });
	assert.equal(c.length, 1);
	assert.equal(c[0].code, "chart.a");
	assert.equal(c[0].message, "bad a");
	assert.deepEqual(c[0].occurrences.map((o) => [o.task, o.kind, o.transcript, o.excerpt]), [["a", "misread", "/out/a.jsonl", "/out/ex-a.md"], ["b", "unresolved", "/out/b.jsonl", "/out/ex-b.md"]]);
});

test("an excerpt shows the check that showed the code and what the agent did next", () => {
	const { calls } = readTranscript(parseJsonl(readFileSync(join(here, "fixtures", "misread.jsonl"), "utf8")), "claude");
	const code = "chart.failed";
	assert.ok(calls.some((c) => c.kind === "check" && parseCheck(c).diagnostics.some((d) => d.code === code)), "fixture shows the code");
	const text = excerptOf(calls, code);
	assert.ok(text);
	assert.match(text, /^# chart\.failed/);
	assert.match(text, /The check that showed it/);
	assert.match(text, /Next call 1/);
	assert.equal(excerptOf(calls, "no.such-code"), null);
});

test("trend by build counts passes and mean score in build order", () => {
	const e = (at: string, commit: string, pass: boolean, score: number) => ({ at, product: "claude", build: { commit }, pass, score });
	const t = trendByBuild([e("2", "bbb", true, 90), e("1", "aaa", false, 50), e("3", "bbb", false, 70), { ...e("1", "zzz", true, 1), product: "codex" }], "claude");
	assert.deepEqual(t.map((b) => [b.commit, b.runs, b.passed, b.meanScore]), [["aaa", 1, 0, 50], ["bbb", 2, 1, 80]]);
});

test("appendix renders pass rates, trend, confusing diagnostics and issue candidates from fixture history", () => {
	const history = [
		{ at: "2026-10-20T10:00:00Z", runId: "h1", task: "b", product: "claude", model: "sonnet", build: { commit: "aaaaaaaaaa" }, score: 95, pass: true, firstCleanAttempt: 1, toolCalls: 8, facts: { passed: 3, total: 3, failed: [] }, misread: [] },
		{ at: "2026-10-27T10:00:00Z", runId: "claude-b", task: "b", product: "claude", model: "sonnet", build: { commit: "bbbbbbbbbb" }, score: 55, pass: false, firstCleanAttempt: 3, toolCalls: 20, facts: { passed: 1, total: 3, failed: ["f"] }, misread: ["chart.a"] },
	];
	const bad = record("claude", "b", { runId: "claude-b", pass: false, score: 55, attempts: 3, firstCleanAttempt: 3, toolCalls: 20, factsFailed: ["f"], confusions: [{ kind: "misread", code: "chart.a", level: "warning", message: "bad a" }], transcript: "/out/transcripts/claude-b.jsonl", excerpts: { "chart.a": "/out/excerpts/claude-b-chart.a.md" } });
	const products = [aggregate("claude", [record("claude", "a", { runId: "h1", score: 95 }), { ...bad, profile: "work" }])];
	const candidates = issueCandidates(products, { excerptOf: (r, code) => r.excerpts?.[code] ?? null });
	const decision = exitDecision(products, 2);
	const md = renderAppendix({ at: "2026-10-28T09:00:00Z", richview: { binary: "/x/richview", version: "richview 3 (commit bbbbbbbbbb)" }, products, decision, findings: findings(products, series(history)), candidates, history, requestedRuns: 2 });
	assert.match(md, /Result: \*\*pass\*\*/);
	assert.match(md, /\| claude \| sonnet \(clauth work\) \| 50% \(1\/2\) \| a, b \| 95, 55 \| 1, 3 \| 9, 20 \|/);
	assert.match(md, /### claude by build/);
	assert.match(md, /\| bbbbbbbbbb \| 1 \| 0 \| 55 \|/);
	assert.match(md, /DROP/);
	assert.match(md, /`chart\.a` \(warning\): bad a/);
	assert.match(md, /transcript \/out\/transcripts\/claude-b\.jsonl, excerpt \/out\/excerpts\/claude-b-chart\.a\.md/);
	assert.match(md, /## Issue candidates[\s\S]*Code `chart\.a`, message "bad a"/);
	assert.match(md, /pass rate 1\/2/);
	const empty = renderAppendix({ at: "t", richview: { binary: "b", version: null }, products, decision, findings: [], candidates: [], history: [], requestedRuns: 2 });
	assert.match(empty, /None: no code came back/);
});

test("a failed gate run lists its reasons in the appendix", () => {
	const products = [aggregate("codex", [failedRecord("codex", "a", "codex missing")])];
	const md = renderAppendix({ at: "t", richview: { binary: "b", version: null }, products, decision: exitDecision(products, 1), findings: [], candidates: [], history: [], requestedRuns: 1 });
	assert.match(md, /Result: \*\*fail\*\*/);
	assert.match(md, /codex a: could not execute \(codex missing\)/);
});

test("the agent's environment is an allowlist and never carries a secret", async () => {
	const { agentEnv, SECRET_NAME, claudeArgv } = await import("../run.mts");
	const secrets = ["DAS_ITEM_LINEAR", "LINEAR_TOKEN", "LINEAR_API_KEY", "GITHUB_TOKEN", "GH_TOKEN", "AWS_SECRET_ACCESS_KEY", "AWS_PROFILE", "OP_SERVICE_ACCOUNT_TOKEN", "ANTHROPIC_API_KEY", "OPENAI_API_KEY", "MY_PASSWORD"];
	const parent = { PATH: "/usr/bin", HOME: "/Users/x", USER: "x", LOGNAME: "x", LANG: "en_US.UTF-8", LC_ALL: "C", TERM: "xterm-256color", CLAUDE_CONFIG_DIR: "/cfg", CODEX_HOME: "/x/.codex", EDITOR: "vim", ...Object.fromEntries(secrets.map((n) => [n, "s3cret"])) };
	for (const product of ["claude", "codex"]) {
		const env = agentEnv({ product, parent, sandbox: "/run/sandbox", runDir: "/run", codexHome: "/run/home/.codex" });
		for (const name of secrets) assert.ok(!(name in env), `${product} env has ${name}`);
		assert.ok(!Object.values(env).includes("s3cret"));
		assert.ok(!("EDITOR" in env));
		assert.equal(env.PATH, "/run/sandbox/bin:/usr/bin");
		assert.equal(env.TMPDIR, "/run/tmp");
		assert.equal(env.LC_ALL, "C");
		assert.ok(!Object.keys(env).some((n) => SECRET_NAME.test(n)));
	}
	// clauth signs claude in from the Keychain: the user's HOME, USER and LOGNAME, and no session's
	// CLAUDE_CONFIG_DIR, which gives a new claude no sign-in.
	const claude = agentEnv({ product: "claude", parent, sandbox: "/run/sandbox", runDir: "/run" });
	assert.deepEqual(Object.keys(claude).sort(), ["HOME", "LANG", "LC_ALL", "LOGNAME", "PATH", "TERM", "TMPDIR", "USER"]);
	assert.deepEqual([claude.HOME, claude.USER, claude.LOGNAME], ["/Users/x", "x", "x"]);
	const codex = agentEnv({ product: "codex", parent, sandbox: "/run/sandbox", runDir: "/run", codexHome: "/run/home/.codex" });
	assert.equal(codex.HOME, "/run/home");
	assert.equal(codex.CODEX_HOME, "/run/home/.codex");
	assert.ok(!("CLAUDE_CONFIG_DIR" in codex));
	const argv = claudeArgv({ mcpConfig: "m", model: null, sandbox: "/run/sandbox" }).join(" ");
	assert.match(argv, /Read\(\/\/run\/sandbox\/\*\*\),Write\(\/\/run\/sandbox\/\*\*\),Edit\(\/\/run\/sandbox\/\*\*\),Bash\(richview:\*\)/);
});

test("a Claude run starts through clauth under RV_AGENT_PROFILE, else work, with claude's own arguments after --", async () => {
	const { claudeArgv, claudeCommand, claudeProfile } = await import("../run.mts");
	assert.equal(claudeProfile({}), "work");
	assert.equal(claudeProfile({ RV_AGENT_PROFILE: "" }), "work");
	assert.equal(claudeProfile({ RV_AGENT_PROFILE: "personal" }), "personal");
	const options = { mcpConfig: "/run/mcp.json", model: "claude-sonnet-5-5", sandbox: "/run/sandbox" };
	const [cmd, argv] = claudeCommand({ profile: "work", ...options });
	assert.equal(cmd, "clauth");
	assert.deepEqual(argv.slice(0, 3), ["start", "work", "--"]);
	assert.deepEqual(argv.slice(3), claudeArgv(options));
	assert.deepEqual(argv.slice(3, 5), ["-p", "--mcp-config"]);
	assert.deepEqual(argv.slice(-2), ["--model", "claude-sonnet-5-5"]);
});

test("a product the run leaves out can be named unrun with its reason, and the appendix says so", () => {
	const args = parseArgs(["--richview", "r", "--out", "o", "--products", "claude", "--unrun", "codex:Daniel: no longer used"]);
	assert.deepEqual(args.unrun, [{ product: "codex", reason: "Daniel: no longer used" }]);
	assert.throws(() => parseArgs(["--richview", "r", "--out", "o", "--unrun", "codex:reason"]), /--unrun takes/);
	assert.throws(() => parseArgs(["--richview", "r", "--out", "o", "--products", "claude", "--unrun", "codex"]), /--unrun takes/);
	const products = [aggregate("claude", [record("claude", "a", { runId: "c1", score: 95 })])];
	const md = renderAppendix({ at: "t", richview: { binary: "b", version: null }, products, decision: exitDecision(products, 1), findings: [], candidates: [], history: [], requestedRuns: 1, unrunProducts: args.unrun });
	assert.match(md, /\*\*codex did not run:\*\* Daniel: no longer used\./);
});
