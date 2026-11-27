// Host reason (ARC-03): prepare and verify a single-commit production hotfix on the gate host.
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { buildManifestProblems } from "./build-manifest.mts";
import { releaseBuildModeArgs } from "./build-mode.mts";
import { hasRecord, patchId } from "./fastpass.mts";
import { hotfixSteps } from "./hotfix.mts";
import { gateEnv, requireDiskFloor, runLogged, sh, unrunLines } from "./proc.mts";
import { acquireLock, gateHome, newReceiptId, runDir, sha256File, writeReceipt } from "./store.mts";
import { holdCargo, holdRelease, keepReleaseBuild, linkTarget } from "./target.mts";
import {SOURCE_OVERRIDES} from "./source-env.mts";
import { typeScriptPaths } from "./source-paths.mts";
import type { StepOutcome } from "./results.mts";
import type { CommandOutcome } from "./proc.mts";

const git = (cwd: string, ...args: string[]) => sh("git", args, { cwd });
const success = (cwd: string, ...args: string[]) => sh("git", args, { cwd, allowFail: true }).status === 0;

/** Bind hotfix checks to the candidate while retaining its explicitly budgeted Chromium engine. */
export function hotfixEnvironment(target: string) {
	const env: NodeJS.ProcessEnv = gateEnv({ CARGO_TARGET_DIR: target });
	for (const name of SOURCE_OVERRIDES) delete env[name];
	return {...env, RICHVIEW_VERIFY_BROWSER:"chromium", RICHVIEW_VERIFY_WORKERS:"2"};
}

/** A fast stamp covers the exact source tree and this commit's patch, not a different stack tip. */
export function requireSourcePass(cwd: string, source: string) {
	const tree = git(cwd, "rev-parse", `${source}^{tree}`);
	const patch = patchId(cwd, source);
	if (!patch || !hasRecord(patch) || !hasRecord(`tree-${tree}`)) throw new Error("No fast pass recorded for this commit's patch and exact tree.");
}

function cleanAt(cwd: string, commit: string, tree: string) {
	if (git(cwd, "rev-parse", "HEAD") !== commit || git(cwd, "rev-parse", "HEAD^{tree}") !== tree || git(cwd, "status", "--porcelain", "--untracked-files=normal")) throw new Error("The candidate checkout changed during hotfix verification.");
}

function allocateCandidate(cwd: string, commit: string) {
	const release = acquireLock("candidate", { wait: true });
	try {
		const lines = git(cwd, "ls-remote", "--heads", "origin", "candidate/*").split("\n");
		const numbers = lines.map(line => Number(line.match(/candidate\/(\d+)$/)?.[1])).filter(Number.isFinite);
		const candidate = `candidate/${Math.max(0, ...numbers) + 1}`;
		// The empty lease makes creation atomic even against a candidate cutter outside this lock.
		sh("git", ["push", "-q", `--force-with-lease=refs/heads/${candidate}:`, "origin", `${commit}:refs/heads/${candidate}`], { cwd, env: gateEnv({ RV_GATE_PUSH: "candidate" }) });
		return candidate;
	} finally { release(); }
}

/** Create a single-commit candidate only after the source and cherry-pick have been checked. */
export function prepareHotfixCandidate({ source, cwd }: {source?: string; cwd: string}) {
	if (!/^[0-9a-f]{7,40}$/.test(source ?? "")) throw new Error("Usage: rv-gate hotfix <dev-commit>");
	git(cwd, "fetch", "-q", "origin", "--prune");
	const sourceCommit = git(cwd, "rev-parse", "--verify", `${source}^{commit}`);
	if (!success(cwd, "merge-base", "--is-ancestor", sourceCommit, "origin/dev")) throw new Error("The source commit is not on origin/dev.");
	if (git(cwd, "rev-list", "--parents", "-n", "1", sourceCommit).split(/\s+/).length !== 2) throw new Error("A hotfix must be one non-merge commit.");
	requireSourcePass(cwd, sourceCommit);
	const prod = git(cwd, "rev-parse", "origin/prod");
	if (git(cwd, "status", "--porcelain", "--untracked-files=normal")) throw new Error("The hotfix checkout is not clean.");
	git(cwd, "checkout", "-q", "--detach", prod);
	const pick = sh("git", ["cherry-pick", "--no-edit", sourceCommit], { cwd, allowFail: true });
	if (pick.status !== 0) {
		sh("git", ["cherry-pick", "--abort"], { cwd, allowFail: true });
		throw new Error(`The commit does not cherry-pick cleanly onto prod: ${pick.stderr || pick.stdout}`);
	}
	const commit = git(cwd, "rev-parse", "HEAD");
	const tree = git(cwd, "rev-parse", "HEAD^{tree}");
	cleanAt(cwd, commit, tree);
	const paths = git(cwd, "diff", "--name-only", "--no-renames", "-z", prod, commit, "--").split("\0").filter(Boolean);
	if (!paths.length) throw new Error("The hotfix changes no files.");
	const candidate = allocateCandidate(cwd, commit);
	return { sourceCommit, prod, commit, tree, paths, candidate };
}

/** Cut a candidate from prod and run an independently recorded fast and affected browser checks. */
export async function runHotfix({ source, cwd, target, prepare }: {source?: string; cwd: string; target: string; prepare: (cwd: string) => void}) {
	requireDiskFloor();
	const unlock = acquireLock("hotfix", { wait: true });
	try {
		const { sourceCommit, prod, commit, tree, paths, candidate } = prepareHotfixCandidate({ source, cwd });
		const id = newReceiptId("hotfix", commit);
		const run = runDir(id);
		const required = hotfixSteps(paths);
		const env = hotfixEnvironment(target);
		const steps: (StepOutcome & {id: string; commands?: CommandOutcome[]})[] = [];
		console.log(`Hotfix ${candidate}: ${sourceCommit.slice(0, 10)} on prod ${prod.slice(0, 10)}; receipt ${id}`);
		const command = async (name: string, exe: string, args: readonly string[]) => {
			const logPath = join(run, `${name.replace(/[^A-Za-z0-9_-]/g, "_")}.log`);
			let result: CommandOutcome & {details?: {unrunInputs: ReturnType<typeof unrunLines>}} = await runLogged(exe, args, { cwd, env, logPath, onOutput: chunk => process.stdout.write(chunk) });
			const unrunInputs = existsSync(logPath) ? unrunLines(readFileSync(logPath, "utf8")) : [];
			if (unrunInputs.length) result = { ...result, status: "fail", reason: "The command reported inputs it did not run.", details: { unrunInputs } };
			console.log(`  ${name}: ${result.status} ${(result.durationMs / 1000).toFixed(1)}s`);
			return result;
		};
		const input = join(run, "fast-options.json");
		writeFileSync(input, JSON.stringify({ cwd, target, base: prod, recordPass: false }));
		prepare(cwd);
		const fast = await command("fast", process.execPath, [fileURLToPath(new URL("./fast-worker.mts", import.meta.url)), input]);
		steps.push({ id: "fast", ...fast });
		let build;
		let releaseWait;
		if (fast.status === "pass") {
			const waitStarted = Date.now();
			let waitingFor: number | undefined;
			const releaseHold = await holdRelease(target, { waiting: pid => {
				waitingFor = pid;
				console.log(`  waiting for release build hold from process ${pid}`);
			} });
			releaseWait = { durationMs: Date.now() - waitStarted, ...(waitingFor && { waitingFor }) };
			console.log(`  release build hold: ${(releaseWait.durationMs / 1000).toFixed(1)}s`);
			try {
				const cargoHold = await holdCargo(target);
				let buildResult;
				try {
					const started = Date.now();
					const commands: CommandOutcome[] = [];
					const buildCommands: [name: string, args: string[]][] = [["build-app", ["run", "--release", "--locked", "-p", "richview-build", ...releaseBuildModeArgs(cwd)]], ["build-fixtures", ["run", "--release", "--locked", "-p", "richview-bench", "--", "--write-fixtures", "--fixtures-only"]]];
					for (const [name, args] of buildCommands) {
						const result = await command(name, "cargo", args); commands.push(result);
						if (result.status !== "pass") break;
					}
					const problems = commands.length === 2 && commands.every(c => c.status === "pass") ? buildManifestProblems({ cwd, target, head: commit, rust: "1.99.0", wasmBindgen: "0.2.129" }) : ["Build command failed"];
					if (!problems.length) {
						const dir = join(gateHome(), "hotfix-builds", id);
						mkdirSync(dir, { recursive: true });
						const snapshot = keepReleaseBuild({ target, cwd, commit, dir });
						build = { snapshot, manifestSha256: sha256File(join(snapshot, "build-manifest.json")) };
					}
					buildResult = { id: "build", status: problems.length ? "fail" as const : "pass" as const, durationMs: Date.now() - started, commands, ...(problems.length && { reason: problems.join("; ") }) };
				} finally { cargoHold(); }
				steps.push(buildResult);
				console.log(`  build: ${buildResult.status} ${(buildResult.durationMs / 1000).toFixed(1)}s`);
				if (buildResult.status === "pass") {
					const scripts: Record<string, string[]> = {
						"report-runtime": ["scripts/verify-report-runtime.mjs", "--out", join(run, "report-runtime")],
						"report-visual": ["scripts/verify-chart-review.mjs", "--out", join(run, "chart-review"), "--phase", "after"],
						"verify-site": ["scripts/verify-site.mjs"],
						"site-browser": ["scripts/verify-browser.mjs"],
					};
					for (const name of required.slice(2)) {
						const script = name.startsWith("script:") ? name.slice("script:".length) : undefined;
						const args = script ? [script, ...(script === "scripts/verify-hostile-browser.mjs" ? ["--out", join(run, "hostile-browser")] : [])] : scripts[name];
						// Candidates may be cut from source trees on either side of the migration.
						const entry = [args[0], ...typeScriptPaths(args[0])].find(path => existsSync(join(cwd, path))) ?? args[0];
						const result = await command(name, process.execPath, [entry, ...args.slice(1)]);
						steps.push({ id: name, ...result });
						if (result.status !== "pass") break;
					}
				}
			} finally { releaseHold(); }
		}
		cleanAt(cwd, commit, tree);
		const pass = required.every(id => steps.some(step => step.id === id && step.status === "pass")) && steps.every(step => step.status === "pass");
		const receipt = { id, kind: "hotfix", candidate, commit, tree, prod, sourceCommit, paths, required, steps, build, releaseWait, result: pass ? "pass" : "fail", at: new Date().toISOString() };
		writeReceipt(receipt);
		console.log(`Hotfix receipt ${id}: ${receipt.result}. Full release checks were not run.`);
		return pass ? 0 : 1;
	} finally { linkTarget(cwd, target); unlock(); }
}
