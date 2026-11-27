// Host reason (ARC-03): build the full native Rust cohort once and pool whole test binaries.
import { spawn } from "node:child_process";
import type { ChildProcess } from "node:child_process";
import type { PathLike } from "node:fs";
import type { Cohort } from "./affected-cohort.mts";
import type { CommandOutcome } from "./proc.mts";
import type { GateStep } from "./steps.mts";
import { appendFileSync, createWriteStream, existsSync, mkdirSync, readFileSync, realpathSync, writeFileSync } from "node:fs";
import { basename, delimiter, dirname, isAbsolute, join, relative, resolve } from "node:path";
import { homedir } from "node:os";
import { fileURLToPath } from "node:url";
import { freeGib, registerCommandStop } from "./proc.mts";
import { KEEP_TEST_BINARIES_GIB, acquireFastCargo, cargoHolder, claimSources, dropTestBinaries, recordSourceOwner, runningCargo, waitUntilIdle } from "./target.mts";

interface CargoTarget { name: string; kind: string[]; test?: boolean; doctest?: boolean; "required-features"?: string[] }
interface CargoPackage {
	id: string;
	name: string;
	targets: CargoTarget[];
	version: string;
	manifest_path: string;
	authors?: string[];
	description?: string;
	homepage?: string;
	repository?: string;
	license?: string;
	license_file?: string;
	rust_version?: string;
	readme?: string;
}
export interface CohortMetadata { target_directory: string; workspace_members: string[]; packages: CargoPackage[] }
export interface CargoEvent {
	reason: string;
	success?: boolean;
	profile?: { test?: boolean };
	executable?: string;
	package_id?: string;
	target?: CargoTarget;
	linked_paths?: string[];
	env?: [string, string][];
}
interface CargoArtifact extends CargoEvent { executable: string; package_id: string; target: CargoTarget }
interface CargoBinary extends CargoArtifact { pkg: CargoPackage; key: string }
interface ExecuteOptions { cwd: string; env: NodeJS.ProcessEnv; logPath: string; signal?: AbortSignal; capture?: boolean }
type ExecuteResult = CommandOutcome & { exitCode: number; stdout: string };

const identity = (pkg: string, target: CargoTarget) => `${pkg}\0${target.kind.slice().sort().join(",")}\0${target.name}`;

// Only the two full-workspace commands currently registered by the gate are pooled. Other
// Cargo selections retain Cargo's own runner rather than guessing which doctests they select.
export function selection(args: readonly string[]) {
	const separator = args.indexOf("--"), cargo = separator < 0 ? args : args.slice(0, separator);
	const harness = separator < 0 ? [] : args.slice(separator + 1);
	if (cargo[0] !== "test" || !cargo.includes("--workspace") || !cargo.includes("--no-fail-fast")) return undefined;
	for (let i = 1; i < cargo.length; i++) {
		if (["--workspace", "--locked", "--no-fail-fast"].includes(cargo[i])) continue;
		if (cargo[i] === "--features" && cargo[i + 1]) { i++; continue; }
		if (cargo[i].startsWith("--features=")) continue;
		return undefined;
	}
	return { cargo, harness, compile: [...cargo, "--no-run", "--message-format=json"], docs: [...cargo, "--doc", ...(harness.length ? ["--", ...harness] : [])] };
}

/** Spawn one member and close its log before consumers read it; abort only this process group. */
function execute(cmd: string, args: readonly string[], { cwd, env, logPath, signal, capture = false }: ExecuteOptions): Promise<ExecuteResult> {
	return new Promise(accept => {
		const start = Date.now(), output: Buffer[] = [], log = createWriteStream(logPath);
		log.write(`$ ${[cmd, ...args].join(" ")}\n`);
		let child: ChildProcess | undefined, spawnError: Error | undefined, killTimer: NodeJS.Timeout | undefined;
		let finished = false, bytes = 0;
		const finish = (code: number) => {
			if (finished) return;
			finished = true; clearTimeout(killTimer); signal?.removeEventListener("abort", abort);
			const durationMs = Date.now() - start;
			log.end(`\nrv-gate: exit ${code} after ${durationMs} ms\n`, () => accept({ status: code === 0 ? "pass" : "fail", exitCode: code, durationMs, command: [cmd, ...args].join(" "), stdout: Buffer.concat(output).toString("utf8") }));
		};
		const abort = () => {
			if (!child?.pid) return;
			const owned = child;
			const kill = (signalName: NodeJS.Signals) => {
				try { process.platform === "win32" ? owned.kill(signalName) : process.kill(-owned.pid!, signalName); }
				catch (error) { if ((error as NodeJS.ErrnoException).code !== "ESRCH") log.write(`${(error as Error).message}\n`); }
			};
			kill("SIGTERM"); killTimer = setTimeout(() => kill("SIGKILL"), 5000);
		};
		try {
			if (signal?.aborted) { log.write("Rust cohort aborted before spawn\n"); finish(1); return; }
			child = spawn(cmd, args, { cwd, env, detached: process.platform !== "win32", stdio: ["ignore", "pipe", "pipe"] });
			child.stdout!.on("data", (chunk: Buffer) => {
				log.write(chunk);
				if (capture) {
					bytes += chunk.length;
					if (bytes <= 256 * 1024 * 1024) output.push(chunk);
					else { spawnError = new Error("Rust command output exceeded 256 MiB"); abort(); }
				}
			});
			child.stderr!.on("data", (chunk: Buffer) => log.write(chunk));
			child.once("error", error => { spawnError = error; log.write(`${error.message}\n`); });
			child.once("close", code => finish(spawnError ? 127 : (code ?? 1)));
			signal?.addEventListener("abort", abort, { once: true });
			if (signal?.aborted) abort();
		} catch (error) { log.write(`${(error as Error).message}\n`); finish(127); }
	});
}

// Cargo owns config-provided environment/runner/target semantics. Defer instead of partially
// reproducing them; an over-conservative match only costs pooling, never coverage.
export function cargoContextNeedsOriginal(cwd: string, env: NodeJS.ProcessEnv) {
	if (env.CARGO_BUILD_TARGET || Object.keys(env).some(key => /^CARGO_TARGET_.+_RUNNER$/.test(key) && env[key])) return true;
	const dirs = new Set([resolve(cwd, env.CARGO_HOME ?? join(env.HOME ?? homedir(), ".cargo"))]);
	for (let at = resolve(cwd);;) {
		dirs.add(join(at, ".cargo")); const parent = dirname(at);
		if (parent === at) break; at = parent;
	}
	for (const directory of dirs) for (const name of ["config", "config.toml"]) {
		const path = join(directory, name); if (!existsSync(path)) continue;
		const text = readFileSync(path, "utf8");
		if (/^\s*\[\s*(?:env|"env"|'env')\s*(?:\]|\.)/m.test(text) || /^\s*(?:env|"env"|'env')\s*=/m.test(text) || /^\s*(?!#)[^=\n]*\b(?:runner|target|include)["']?\s*=/m.test(text)) return true;
	}
	return false;
}

export function inventory(events: readonly CargoEvent[], metadata: CohortMetadata, target: PathLike) {
	if (!events.some(event => event.reason === "build-finished" && event.success === true)) throw new Error("Cargo did not finish the complete test build");
	if (realpathSync(metadata.target_directory) !== realpathSync(target)) throw new Error("Cargo metadata names another target directory");
	const packages = metadata.packages.filter(pkg => metadata.workspace_members.includes(pkg.id));
	const expected = new Set(packages.flatMap(pkg => pkg.targets.filter(target => target.test).map(target => identity(pkg.id, target))));
	const actual = new Map<string, CargoBinary>();
	for (const artifact of events.filter((event): event is CargoArtifact => Boolean(event.reason === "compiler-artifact" && event.profile?.test && event.executable))) {
		const key = identity(artifact.package_id, artifact.target), pkg = packages.find(pkg => pkg.id === artifact.package_id);
		if (!expected.has(key) || !pkg) throw new Error(`Unexpected Cargo test target ${artifact.target.name}`);
		if (!isAbsolute(artifact.executable) || !existsSync(artifact.executable)) throw new Error(`Missing Cargo test executable ${artifact.executable}`);
		const path = relative(realpathSync(target), realpathSync(artifact.executable));
		if (path.startsWith("..") || isAbsolute(path)) throw new Error(`Cargo executable is outside the held target: ${artifact.target.name}`);
		if (actual.has(key)) throw new Error(`Duplicate Cargo test target ${artifact.target.name}`);
		actual.set(key, { ...artifact, pkg, key });
	}
	const missing = [...expected].filter(key => !actual.has(key));
	if (!expected.size || missing.length) throw new Error(`Incomplete Cargo test cohort: ${actual.size}/${expected.size} targets, ${missing.length} missing`);
	return [...actual.values()].sort((a, b) => a.key.localeCompare(b.key));
}

export function packageEnv(env: NodeJS.ProcessEnv, pkg: CargoPackage, artifact: { executable: string }, events: readonly CargoEvent[], stdlib: string, target: string) {
	const version = /^(\d+)\.(\d+)\.(\d+)(?:-([^+]+))?/.exec(pkg.version);
	if (!version) throw new Error(`Invalid Cargo package version ${pkg.version}`);
	const inherited = process.platform === "darwin" ? "DYLD_FALLBACK_LIBRARY_PATH" : process.platform === "win32" ? "PATH" : "LD_LIBRARY_PATH";
	const insideTarget = (path: string) => { const rel = relative(target, path); return !rel.startsWith("..") && !isAbsolute(rel); };
	const native = events.filter((event: { reason: string; }) => event.reason === "build-script-executed").flatMap(event => event.linked_paths ?? []).map((path: string) => path.replace(/^[^=]+=+/, "")).filter(insideTarget);
	const executableDir = dirname(artifact.executable);
	const output = ["deps", "examples"].includes(basename(executableDir)) ? dirname(executableDir) : executableDir;
	const deps = join(output, "deps");
	const fallback = env[inherited] || (process.platform === "darwin" ? `${env.HOME}/lib:/usr/local/lib:/usr/lib` : "");
	const buildEnv = events.filter(event => event.reason === "build-script-executed" && event.package_id === pkg.id).flatMap(event => event.env ?? []);
	const values = {
		...Object.fromEntries(buildEnv),
		CARGO: env.CARGO ?? env.PATH!.split(delimiter).map((path: string) => join(path, process.platform === "win32" ? "cargo.exe" : "cargo")).find((path: PathLike) => existsSync(path)) ?? "cargo",
		CARGO_MANIFEST_DIR: dirname(pkg.manifest_path), CARGO_MANIFEST_PATH: pkg.manifest_path,
		CARGO_PKG_NAME: pkg.name, CARGO_PKG_VERSION: pkg.version,
		CARGO_PKG_VERSION_MAJOR: version[1], CARGO_PKG_VERSION_MINOR: version[2], CARGO_PKG_VERSION_PATCH: version[3], CARGO_PKG_VERSION_PRE: version[4] ?? "",
		CARGO_PKG_AUTHORS: (pkg.authors ?? []).join(":"), CARGO_PKG_DESCRIPTION: pkg.description ?? "", CARGO_PKG_HOMEPAGE: pkg.homepage ?? "",
		CARGO_PKG_REPOSITORY: pkg.repository ?? "", CARGO_PKG_LICENSE: pkg.license ?? "", CARGO_PKG_LICENSE_FILE: pkg.license_file ?? "",
		CARGO_PKG_RUST_VERSION: pkg.rust_version ?? "", CARGO_PKG_README: pkg.readme ?? "",
	};
	return { ...env, ...values, [inherited]: [...new Set([...native, output, deps, stdlib, ...fallback.split(delimiter).filter(Boolean)])].join(delimiter) };
}

/**
 * Preserve a full Cargo test command while running its whole binaries in a bounded pool.
 * The caller holds Cargo/source ownership through this promise, including every child and doc,
 * and excludes its own other Cargo writers to the target as well as other processes.
 */
export async function runRustTests({ cwd, env, logPath, args, signal, workers = 2 }: Omit<ExecuteOptions, "capture"> & { args: readonly string[]; workers?: number }): Promise<CommandOutcome> {
	const started = Date.now(), command = ["cargo", ...args].join(" "), logs: string[] = [], details: {name: string; status: string; exitCode: number; durationMs: number}[] = [];
	mkdirSync(dirname(logPath), { recursive: true }); writeFileSync(logPath, `$ ${command}\n`);
	const directory = `${logPath}.rust`; mkdirSync(directory, { recursive: true });
	const controller = new AbortController(), abort = () => controller.abort(signal?.reason);
	const timer = setTimeout(() => controller.abort(new Error("Rust test cohort timed out after one hour")), 3600_000);
	signal?.addEventListener("abort", abort, { once: true }); if (signal?.aborted) abort();
	// Share signal ownership with ordinary commands so concurrent pools all drain together.
	const unregister = registerCommandStop(name => controller.abort(new Error(`Rust cohort interrupted by ${name}`)));
	const run = async (name: string, cmd: string, argv: readonly string[], options: Partial<ExecuteOptions> = {}) => {
		if (cargoHolder(env.CARGO_TARGET_DIR!) !== process.pid) { controller.abort(); throw new Error("Lost the caller's Cargo/source hold"); }
		const path = join(directory, `${name}.log`); logs.push(path);
		const result = await execute(cmd, argv, { cwd, env, logPath: path, signal: controller.signal, capture: ["compile", "metadata", "stdlib"].includes(name), ...options });
		details.push({ name, status: result.status, exitCode: result.exitCode, durationMs: result.durationMs }); return result;
	};
	let exitCode = 0;
	try {
		if (!Number.isInteger(workers) || workers < 1 || workers > 2) throw new Error("Rust binary workers must be 1 or 2");
		if (!env.CARGO_TARGET_DIR || cargoHolder(env.CARGO_TARGET_DIR) !== process.pid) throw new Error("Rust execution requires the caller's complete Cargo/source hold");
		const plan = selection(args);
		if (!plan || cargoContextNeedsOriginal(cwd, env)) { exitCode = (await run("cargo-original", "cargo", args)).exitCode; }
		else {
			const built = await run("compile", "cargo", plan.compile);
			if (built.status !== "pass") exitCode = built.exitCode;
			else {
				const events: CargoEvent[] = built.stdout.trim().split("\n").filter(Boolean).map((line: string) => JSON.parse(line));
				const meta = await run("metadata", "cargo", ["metadata", "--locked", "--no-deps", "--format-version=1"]);
				if (meta.status !== "pass") throw new Error("Cargo metadata failed");
				const metadata: CohortMetadata = JSON.parse(meta.stdout);
				if (metadata.packages.some(pkg => pkg.targets.some(target => target.test && target["required-features"]?.length))) {
					exitCode = (await run("cargo-original", "cargo", args)).exitCode;
				} else {
					const binaries = inventory(events, metadata, env.CARGO_TARGET_DIR);
					const compiler = await run("stdlib", env.RUSTC || "rustc", ["--print", "target-libdir"]);
					if (compiler.status !== "pass" || !isAbsolute(compiler.stdout.trim())) throw new Error("Cannot determine the pinned Rust library directory");
					const outcomes = new Array<ExecuteResult>(binaries.length), dropped = { count: 0, bytes: 0 }; let next = 0;
					const drained = await Promise.allSettled(Array.from({ length: workers }, async () => {
						while (next < binaries.length) {
							const index = next++, artifact = binaries[index];
							outcomes[index] = await run(`binary-${String(index).padStart(3, "0")}`, artifact.executable, plan.harness, { cwd: dirname(artifact.pkg.manifest_path), env: packageEnv(env, artifact.pkg, artifact, events, compiler.stdout.trim(), metadata.target_directory) });
							// Its outcome and log are recorded; on a short disk the binary goes, and a later run relinks it.
							const gone = dropTestBinaries(metadata.target_directory, [artifact.executable], freeGib());
							dropped.count += gone.count; dropped.bytes += gone.bytes;
						}
					}));
					if (dropped.count) appendFileSync(logPath, `rv-gate Rust runner: deleted ${dropped.count} test binaries after they ran (${(dropped.bytes / 2 ** 30).toFixed(1)} GiB), with / under ${KEEP_TEST_BINARIES_GIB} GiB free\n`);
					const rejected = drained.find(result => result.status === "rejected");
					if (rejected) throw rejected.reason;
					if (outcomes.some(result => result.status !== "pass")) exitCode = 1;
					// --no-run excludes doctests. Cargo still owns their compilation, environment and runner.
					const docs = await run("doctests", "cargo", plan.docs);
					if (docs.status !== "pass") exitCode = docs.exitCode || 1;
					const docLog = readFileSync(join(directory, "doctests.log"), "utf8");
					const docNames = new Set([...docLog.matchAll(/^\s*Doc-tests (\S+)\s*$/gm)].map(match => match[1]));
					for (const pkg of metadata.packages.filter(pkg => metadata.workspace_members.includes(pkg.id))) {
						for (const target of pkg.targets.filter(target => target.doctest)) {
							if (!docNames.has(target.name.replaceAll("-", "_"))) throw new Error(`Missing doctest cohort ${target.name}`);
						}
					}
					writeFileSync(`${logPath}.rust.json`, JSON.stringify({ binaries: binaries.map(artifact => ({ key: artifact.key, executable: artifact.executable })), completed: outcomes.length, dropped, commands: details }, null, 2) + "\n");
				}
			}
		}
	} catch (error) { exitCode = 1; appendFileSync(logPath, `rv-gate Rust runner: ${(error as Error).message}\n`); }
	finally {
		clearTimeout(timer); signal?.removeEventListener("abort", abort);
		unregister();
		for (const path of logs) appendFileSync(logPath, readFileSync(path));
		if (controller.signal.aborted) { exitCode = 1; appendFileSync(logPath, "rv-gate Rust cohort aborted\n"); }
	}
	const durationMs = Date.now() - started;
	appendFileSync(logPath, `\nrv-gate: exit ${exitCode} after ${durationMs} ms\n`);
	return { status: exitCode === 0 ? "pass" : "fail", exitCode, durationMs, command };
}


const worker = fileURLToPath(new URL('./rust-cohort-worker.mts', import.meta.url));

/** Return stable shard commands; the preparation discovers their complete binary inventory. */
export function expandCohort(step: GateStep, ctx: { runDir: string }): Cohort {
 if (step.cohort !== 'rust-debug' || step.run?.length !== 1 || step.run[0].cmd !== 'cargo' || !selection(step.run[0].args)) throw new Error('Unsupported Rust cohort');
 if (!ctx.runDir || !/^[a-z][a-z0-9-]*$/.test(step.id)) throw new Error('Rust cohort needs a run directory and a stable step id');
 const original = step.run[0], state = join(ctx.runDir, `${step.id}-cohort.json`), encoded = JSON.stringify(original.args);
 const command = (action: string, index?: number) => ({cmd:process.execPath, args:[worker, action, state, encoded, ...(index === undefined ? [] : [String(index), '4'])], id:index === undefined ? 'prepare' : `shard-${index}-of-4`, ...(original.env ? {env:original.env} : {}), ...(original.whenEnvMissing ? {whenEnvMissing:original.whenEnvMissing} : {})});
 return {prepare:command('prepare'), shards:Array.from({length:4},(_,i)=>command('shard',i)), maxConcurrent:2};
}

/** Hold the debug generation until all shard attempts have drained. */
export async function withCohortHold<T>(cohort: string, ctx: {cwd: string; target?: string; signal?: AbortSignal}, fn: () => Promise<T>) {
 if (cohort !== 'rust-debug') throw new Error(`Unknown Rust cohort ${cohort}`);
 const target = ctx.target ?? process.env.CARGO_TARGET_DIR;
 if (!target || !ctx.cwd) throw new Error('Rust cohort needs its target and checkout');
 const hold = await acquireFastCargo(target, {signal:ctx.signal, profile:'debug'});
 try {
  if (!await waitUntilIdle(() => runningCargo(target))) throw new Error('Other Cargo processes did not drain before the Rust cohort');
  hold.assertOwned(); claimSources(target,ctx.cwd);
  try { return await fn(); }
  finally { hold.assertOwned(); recordSourceOwner(target,ctx.cwd); }
 } finally { hold.release(); }
}
