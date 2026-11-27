#!/usr/bin/env node
// Runs one fresh agent on one task (FEA-SF-65, DEL-78) and scores it.
//
//   node tools/agent-use/run.mjs --product claude|codex --task <id> --richview <binary> [--model <m>]
//
// The agent gets an empty sandbox under $TMPDIR/rv-agent-use/<run-id>, outside every checkout,
// holding only the task's data, and the installed binary as `richview` CLI and MCP server.

import { spawn, spawnSync } from "node:child_process";
import { chmodSync, copyFileSync, cpSync, existsSync, mkdirSync, readFileSync, realpathSync, symlinkSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { appendHistory, historyEntry } from "./history.mts";
import { scoreRunFolder } from "./score.mts";

import type { RunMetadata } from "./history.mts";
import type { Task } from "./lib/facts.mts";

type RunArgs = {product: string | null; task: string | null; richview: string | null; model: string | null; timeoutMin: number; history: boolean; build: string | null};
type ClaudeOptions = {mcpConfig: string; model?: string | null; sandbox: string};
type CodexOptions = {binary: string; model?: string | null; sandbox: string};

const here = dirname(fileURLToPath(import.meta.url));

export function parseArgs(argv: string[]) {
	const args: RunArgs = { product: null, task: null, richview: null, model: null, timeoutMin: 25, history: true, build: null };
	for (let i = 0; i < argv.length; i++) {
		const a = argv[i];
		const take = () => argv[++i];
		if (a === "--product") args.product = take();
		else if (a === "--task") args.task = take();
		else if (a === "--richview") args.richview = take();
		else if (a === "--model") args.model = take();
		else if (a === "--timeout-min") args.timeoutMin = Number(take());
		else if (a === "--build") args.build = take();
		else if (a === "--no-history") args.history = false;
		else throw new Error(`unknown argument ${a}`);
	}
	if (args.product !== "claude" && args.product !== "codex") throw new Error("--product claude|codex is required");
	if (!args.task || !args.richview) throw new Error("--task and --richview are required");
	return args as RunArgs & {product: "claude" | "codex"; task: string; richview: string};
}

/** What the agent is told besides the brief: where things are, nothing about how to use RichView. */
export function promptFor(brief: string | undefined, sandbox: string) {
	return `${brief}

Your working folder is ${sandbox}. The data is in ${sandbox}/report/data/. The report belongs in its own folder: save it as ${sandbox}/report/report.rmd and finish with a report that renders. You have the RichView tools and the \`richview\` command; nothing else is installed, and you have no other source of information about RichView.`;
}

export function claudeArgv({ mcpConfig, model, sandbox }: ClaudeOptions) {
	return [
		"-p",
		"--mcp-config", mcpConfig,
		"--strict-mcp-config",
		"--output-format", "stream-json",
		"--verbose",
		"--tools", "Read,Write,Edit,Bash",
		"--allowedTools", `mcp__richview,Read(/${sandbox}/**),Write(/${sandbox}/**),Edit(/${sandbox}/**),Bash(richview:*)`,
		"--disallowedTools", "mcp__richview__richview_publish,Bash(richview publish:*),Bash(richview image:*),Bash(richview server:*)",
		"--permission-mode", "dontAsk",
		"--disable-slash-commands",
		"--setting-sources", "project",
		"--no-session-persistence",
		...(model ? ["--model", model] : []),
	];
}

/** The clauth profile a Claude run signs in under: RV_AGENT_PROFILE, else `work`. */
export function claudeProfile(parent: NodeJS.ProcessEnv) {
	return parent.RV_AGENT_PROFILE || "work";
}

/**
 * The command that starts a Claude run. clauth starts claude under the profile, in a runtime of
 * its own: a parent session's CLAUDE_CONFIG_DIR gives a new claude no sign-in.
 */
export function claudeCommand({ profile, mcpConfig, model, sandbox }: ClaudeOptions & {profile: string}): [string, string[]] {
	return ["clauth", ["start", profile, "--", ...claudeArgv({ mcpConfig, model, sandbox })]];
}

export function codexArgv({ binary, sandbox, model }: CodexOptions) {
	return [
		"exec",
		"--json",
		"--skip-git-repo-check",
		"--ephemeral",
		"-C", sandbox,
		"-s", "workspace-write",
		"-c", `mcp_servers.richview.command=${JSON.stringify(binary)}`,
		"-c", 'mcp_servers.richview.args=["mcp"]',
		"-c", 'approval_policy="never"',
		...(model ? ["-m", model] : []),
		"-",
	];
}

/** Names that are never passed on, whatever else matches (the allowlist below is the real gate). */
export const SECRET_NAME = /^(DAS_ITEM_|GITHUB|GH_|LINEAR|AWS|OP_)|TOKEN|SECRET|PASSWORD|CREDENTIAL|_KEY$|^ANTHROPIC_API|^OPENAI_API/i;

/**
 * The agent's whole environment, from an allowlist: PATH with the sandbox's bin first, locale,
 * TERM, a TMPDIR inside the run folder, and what each CLI needs to sign in. Claude starts through
 * clauth, which reads the profile's sign-in from the Keychain and so needs the user's HOME, USER
 * and LOGNAME; the parent's CLAUDE_CONFIG_DIR stays out. Codex gets a HOME and CODEX_HOME inside
 * the run folder that hold a copy of auth.json only, so it reads none of Daniel's config.
 */
export function agentEnv({ product, parent, sandbox, runDir, codexHome }: {product: string; parent: NodeJS.ProcessEnv; sandbox: string; runDir: string; codexHome?: string | null}) {
	const env: NodeJS.ProcessEnv = { PATH: `${join(sandbox, "bin")}:${parent.PATH ?? "/usr/bin:/bin"}`, TERM: parent.TERM || "xterm", TMPDIR: join(runDir, "tmp") };
	for (const [name, value] of Object.entries(parent)) if (name === "LANG" || name.startsWith("LC_")) env[name] = value;
	if (product === "claude") {
		for (const name of ["HOME", "USER", "LOGNAME"]) if (parent[name]) env[name] = parent[name];
	} else {
		env.HOME = dirname(codexHome!);
		env.CODEX_HOME = codexHome!;
	}
	for (const name of Object.keys(env)) if (SECRET_NAME.test(name)) delete env[name];
	return env;
}

/** A Codex home with only the sign-in file; no config.toml, so none of the user's MCP servers load. */
function makeCodexHome(runDir: string, parent: NodeJS.ProcessEnv) {
	const home = join(runDir, "home", ".codex");
	mkdirSync(home, { recursive: true });
	const source = join(parent.CODEX_HOME || join(parent.HOME || homedir(), ".codex"), "auth.json");
	if (!existsSync(source)) throw new Error(`no Codex sign-in at ${source}`);
	copyFileSync(source, join(home, "auth.json"));
	chmodSync(join(home, "auth.json"), 0o600);
	return home;
}

function inGitCheckout(dir: string) {
	return spawnSync("git", ["-C", dir, "rev-parse", "--show-toplevel"], { encoding: "utf8" }).status === 0;
}

function buildOf(binary: string, override: string | null) {
	const out = spawnSync(binary, ["--version"], { encoding: "utf8" }).stdout?.trim() ?? "";
	const commit = override ?? out.match(/commit ([0-9a-f]{7,40})/)?.[1] ?? null;
	return { version: out || null, commit };
}

async function main() {
	const args = parseArgs(process.argv.slice(2));
	const binary = realpathSync(resolve(args.richview));
	const taskDir = join(here, "tasks", args.task);
	if (!existsSync(join(taskDir, "task.json"))) throw new Error(`no task ${args.task}`);
	const task = JSON.parse(readFileSync(join(taskDir, "task.json"), "utf8")) as Task;

	const startedAt = new Date().toISOString();
	const runId = `${args.task}-${args.product}-${startedAt.replace(/[-:]/g, "").replace(/\..*/, "")}`;
	const runDir = join(realpathSync(process.env.TMPDIR || tmpdir()), "rv-agent-use", runId);
	const sandbox = join(runDir, "sandbox");
	mkdirSync(join(sandbox, "report"), { recursive: true });
	mkdirSync(join(sandbox, "bin"), { recursive: true });
	if (inGitCheckout(sandbox)) throw new Error(`${sandbox} is inside a git checkout; set TMPDIR elsewhere`);
	cpSync(join(taskDir, "data"), join(sandbox, "report", "data"), { recursive: true });
	symlinkSync(binary, join(sandbox, "bin", "richview"));

	const mcpConfig = join(runDir, "mcp.json");
	writeFileSync(mcpConfig, JSON.stringify({ mcpServers: { richview: { command: binary, args: ["mcp"] } } }, null, 2));
	const prompt = promptFor(task.brief, sandbox);
	writeFileSync(join(runDir, "prompt.txt"), prompt);

	mkdirSync(join(runDir, "tmp"), { recursive: true });
	const codexHome = args.product === "codex" ? makeCodexHome(runDir, process.env) : null;
	const env = agentEnv({ product: args.product, parent: process.env, sandbox, runDir, codexHome });
	const profile = args.product === "claude" ? claudeProfile(process.env) : null;
	const [cmd, argv] = args.product === "claude" ? claudeCommand({ profile: profile!, mcpConfig, model: args.model, sandbox }) : ["codex", codexArgv({ binary, sandbox, model: args.model })] as const;

	const meta: RunMetadata = { runId, product: args.product, profile, model: args.model, task: args.task, build: buildOf(binary, args.build), binary, startedAt, runDir, reportDir: join(sandbox, "report"), command: [cmd, ...argv], timedOut: false };
	writeFileSync(join(runDir, "run.json"), JSON.stringify(meta, null, 2));
	console.error(`run ${runId}: ${cmd} in ${sandbox}`);

	const child = spawn(cmd, argv, { cwd: sandbox, env, stdio: ["pipe", "pipe", "pipe"] });
	const transcript: Buffer[] = [];
	const stderr: Buffer[] = [];
	child.stdout.on("data", (d) => transcript.push(d));
	child.stderr.on("data", (d) => stderr.push(d));
	child.stdin.end(prompt);
	const timer = setTimeout(() => {
		meta.timedOut = true;
		child.kill("SIGTERM");
		setTimeout(() => child.kill("SIGKILL"), 10_000);
	}, args.timeoutMin * 60_000);
	// A CLI that is not installed emits `error` (and then `close`); say so instead of crashing.
	const spawnFailure: {error?: Error} = {};
	child.on("error", (e) => {
		spawnFailure.error = e;
		stderr.push(Buffer.from(`${e.message}\n`));
	});
	const code = await new Promise<number | null>((done) => child.on("close", done));
	if (spawnFailure.error) throw new Error(`could not start ${cmd}: ${spawnFailure.error.message}`);
	clearTimeout(timer);
	writeFileSync(join(runDir, "transcript.jsonl"), Buffer.concat(transcript));
	writeFileSync(join(runDir, "stderr.txt"), Buffer.concat(stderr));

	// The scorer's own check of what the agent left behind, with the same binary.
	const rmd = join(sandbox, "report", "report.rmd");
	if (existsSync(rmd)) {
		const check = spawnSync(binary, ["check", rmd, "--format", "json"], { encoding: "utf8", cwd: sandbox });
		writeFileSync(join(runDir, "final-check.txt"), check.stdout ?? "");
	}
	meta.exitCode = code;
	meta.finishedAt = new Date().toISOString();
	writeFileSync(join(runDir, "run.json"), JSON.stringify(meta, null, 2));

	const scored = scoreRunFolder(runDir);
	writeFileSync(join(runDir, "score.json"), `${JSON.stringify(scored, null, 2)}\n`);
	if (args.history) appendHistory(historyEntry(meta, scored));
	console.log(JSON.stringify({ runId, score: scored.score, pass: scored.pass, runDir, clean: scored.clean, toolCalls: scored.toolCalls, misread: scored.misread, facts: scored.facts.items.map((i) => `${i.ok ? "ok  " : "FAIL"} ${i.detail}`) }, null, 2));
}

if (process.argv[1] && (resolve(process.argv[1]) === fileURLToPath(import.meta.url) ||
	resolve(process.argv[1]) === fileURLToPath(import.meta.url.replace(/\.mts$/, ".mjs")))) {
	main().catch((e) => {
		console.error(e.message);
		process.exit(1);
	});
}
