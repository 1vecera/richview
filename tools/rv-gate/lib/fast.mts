// The fast gate (DEL-81): before every merge into dev, run affected unit tests and changed crates'
// own integration tests. Browser, visual and end-to-end suites run at release.
//
// One Cargo target serves every checkout on the host, and Cargo names a workspace
// crate's artifacts by its path inside the workspace, so another checkout's build of a crate
// looks fresh to this one whenever this checkout's sources are older. The fast gate records which
// checkout built each crate last, and stamps a crate's sources before building when another
// checkout built it since; Cargo then rebuilds that crate and its dependents from this checkout.

import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, readdirSync, statSync, utimesSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { JS_SUITES, MERGED_TESTS, affected, affectsTypeScript, includeInputs, readerLiterals, testTargets, unitTargets, withDependencies, workspaceGraph } from "./impact.mts";
import type { CargoMetadata, IncludeInput, WorkspaceCrate } from "./impact.mts";
import { coverage, publicTree, retiredTests, secrets } from "./checks.mts";
import { parsePrBody } from "./pr.mts";
import { batchCommits, batchFields } from "./release.mts";
import { FRESHNESS_TEST, snippetCheck } from "./snippets.mts";
import { recordFastPass } from "./fastpass.mts";
import { npmCi, npmStale } from "./npm.mts";
import { freeGib, gateEnv, requireDiskFloor, sh } from "./proc.mts";
import { WASM_CRATES } from "./steps.mts";
import { KEEP_TEST_BINARIES_GIB, acquireFastCargo, dropTestBinaries } from "./target.mts";

const lines = (text: string, sep = "\n") => text.split(sep).filter(Boolean);

/**
 * Integration tests that run whenever their crate changes: the registry and the generated spec, and
 * for an engine change core's table of chart types, which reads the engine's registry rows.
 */
/**
 * Core's small payload contracts: what a chart's embedded data keeps (source rows, typing, ranges).
 * They run in a fraction of a second, and two of them first failed at release 1zd after a payload
 * change that passed fast (337c01c5).
 */
const PAYLOAD_CONTRACTS = ["boxplot", "histogram", "interval", "numeric_range"].map((name) => `crates/richview-core/tests/${name}_contract.rs`);

export const CONTRACT_TESTS = {
	"richview-core": ["crates/richview-core/tests/registry.rs", "crates/richview-core/tests/spec_docs.rs", ...PAYLOAD_CONTRACTS],
	"richview-chart": ["crates/richview-core/tests/chart_types.rs", "crates/richview-core/tests/spec_docs.rs", "crates/richview-core/tests/type_contracts.rs", ...PAYLOAD_CONTRACTS],
};

/** Paths that differ between the base and the working tree, untracked ones included. */
export function changedFiles(cwd: string, base: string) {
	const tracked = lines(sh("git", ["diff", "--name-only", "--no-renames", "-z", base], { cwd }), "\0");
	const untracked = lines(sh("git", ["ls-files", "--others", "--exclude-standard", "-z"], { cwd }), "\0");
	return [...new Set([...tracked, ...untracked])];
}

/** `cargo metadata --no-deps`, cached per checkout until a manifest changes. */
function metadata(cwd: string, cache: string, env: NodeJS.ProcessEnv): CargoMetadata {
	const manifests = lines(sh("git", ["ls-files", "-z", "--cached", "--others", "--exclude-standard", "--", "Cargo.toml", "*/Cargo.toml"], { cwd }), "\0");
	const key = createHash("sha1");
	key.update(cwd);
	for (const file of manifests) {
		try {
			key.update(`${file}:${statSync(join(cwd, file)).mtimeMs}`);
		} catch {
			key.update(`${file}:gone`);
		}
	}
	const path = join(cache, `metadata-${key.digest("hex").slice(0, 16)}.json`);
	if (existsSync(path)) return JSON.parse(readFileSync(path, "utf8"));
	const text = sh("cargo", ["metadata", "--format-version", "1", "--no-deps", "--offline"], { cwd, env });
	writeFileSync(path, text);
	return JSON.parse(text);
}

/** `[file, literal]` for every `include_str!` and `include_bytes!` in the checkout's Rust sources. */
export function includes(cwd: string) {
	const found = sh("git", ["grep", "-n", "--untracked", "-A1", "-E", "include_(str|bytes)!\\(", "--", "*.rs"], { cwd, allowFail: true }).stdout;
	const pairs: IncludeInput[] = [];
	let pending: string | undefined;
	for (const line of lines(found)) {
		const match = /^(.+?)[:-]\d+[:-](.*)$/.exec(line);
		if (!match) continue;
		const [, file, text] = match;
		const macro = /include_(?:str|bytes)!\(\s*"([^"]+)"/.exec(text);
		if (macro) pairs.push([file, macro[1]]);
		else if (/include_(?:str|bytes)!\(\s*$/.test(text)) pending = file;
		else if (pending === file) {
			const literal = /^\s*"([^"]+)"/.exec(text);
			if (literal) pairs.push([file, literal[1]]);
			pending = undefined;
		}
	}
	return pairs;
}

/** The latest time Cargo started building `crate` in `profiles` (its `invoked.timestamp`), or 0. */
function invokedAt(target: string, crate: string, profiles: readonly string[]) {
	const unit = new RegExp(`^${crate.replaceAll("-", "\\-")}-[0-9a-f]{16}$`);
	let latest = 0;
	for (const profile of profiles) {
		const dir = join(target, profile, ".fingerprint");
		let units: string[];
		try {
			units = readdirSync(dir).filter((name) => unit.test(name));
		} catch {
			continue;
		}
		for (const name of units) {
			try {
				latest = Math.max(latest, statSync(join(dir, name, "invoked.timestamp")).mtimeMs);
			} catch {
				// A unit Cargo has not started yet.
			}
		}
	}
	return latest;
}

/**
 * The checkout that last compiled `crate` into `target`, as `rustc-owner.sh` (the RUSTC_WRAPPER
 * that `env.sh` and the gate set) recorded it for each of the crate's targets, or undefined when
 * the targets disagree, a record is missing, or Cargo started a build after the newest record:
 * that build ran without the wrapper.
 */
function ownerOf(target: string, crate: WorkspaceCrate, profiles: readonly string[]) {
	const owners = new Set<string>();
	let recorded = 0;
	for (const name of crate.targets.map((t) => t.name.replaceAll("-", "_"))) {
		const path = join(target, ".rv-fast", "owner", name);
		try {
			owners.add(readFileSync(path, "utf8").trim());
			recorded = Math.max(recorded, statSync(path).mtimeMs);
		} catch {
			// Never compiled through the wrapper; that is fine while another target is.
		}
	}
	if (owners.size !== 1) return undefined;
	if (invokedAt(target, crate.name, profiles) > recorded + 2000) return undefined;
	return [...owners][0];
}

/**
 * Stamp the sources of every crate in `closure` that this checkout did not compile last into
 * `target`, in any of `profiles`, and return their names. Cargo then rebuilds them, and their
 * dependents, from this checkout.
 */
export function claim(cwd: string, target: string, closure: ReadonlySet<string>, graph: readonly WorkspaceCrate[], profiles = ["debug"]) {
	const stamped = [];
	const now = new Date();
	for (const crate of graph.filter((c) => closure.has(c.name))) {
		if (ownerOf(target, crate, profiles) === cwd) continue;
		for (const file of lines(sh("git", ["ls-files", "-z", "--", crate.dir], { cwd }), "\0")) {
			try {
				utimesSync(join(cwd, file), now, now);
			} catch {
				// Deleted in the working tree.
			}
		}
		stamped.push(crate.name);
	}
	return stamped;
}

interface FastResult { label: string; status: "pass" | "fail"; output: string; ms: number; blockedMs: number }

/** Run one command, keeping its output; resolves with status, output and duration. */
function run(label: string, cmd: string, args: readonly string[], { cwd, env, signal }: {
	cwd: string; env: NodeJS.ProcessEnv; signal?: AbortSignal;
}): Promise<FastResult> {
	const started = Date.now();
	if (signal?.aborted) return Promise.resolve({ label, status: "fail", output: "Interrupted", ms: 0, blockedMs: 0 });
	return new Promise((done) => {
		// Cargo owns a process group so a signal stops its compiler children before the hold goes.
		const group = cmd === "cargo" && process.platform !== "win32";
		const child = spawn(cmd, args, { cwd, env, detached: group, stdio: ["ignore", "pipe", "pipe"] });
		const stop = () => {
			if (!child.pid) return;
			try { process.kill(group ? -child.pid : child.pid, "SIGTERM"); } catch (error) { if ((error as NodeJS.ErrnoException).code !== "ESRCH") child.kill("SIGTERM"); }
		};
		signal?.addEventListener("abort", stop, { once: true });
		let output = "";
		// Cargo says once that it waits for another build's lock on the target; the wait lasts
		// until its next line.
		let blockedAt: number | undefined;
		let blockedMs = 0;
		const take = (chunk: Buffer) => {
			const text = String(chunk);
			if (blockedAt !== undefined) {
				blockedMs += Date.now() - blockedAt;
				blockedAt = undefined;
			}
			if (/Blocking waiting for file lock/.test(text) && !/\n.+\S/.test(text.slice(text.lastIndexOf("Blocking")))) blockedAt = Date.now();
			output += text;
		};
		child.stdout.on("data", take);
		child.stderr.on("data", take);
		child.on("error", (error) => { signal?.removeEventListener("abort", stop); done({ label, status: "fail", output: error.message, ms: Date.now() - started, blockedMs }); });
		child.on("close", (code) => { signal?.removeEventListener("abort", stop); done({ label, status: code === 0 ? "pass" : "fail", output, ms: Date.now() - started, blockedMs }); });
	});
}

const seconds = (ms: number) => `${(ms / 1000).toFixed(1)} s`;

/** The test binaries a `cargo test` run names in its "Running" lines. */
export function testBinaries(output: string) {
	return [...output.matchAll(/^\s+Running (?:unittests )?\S+ \((\/[^)\n]+)\)$/gm)].map((match) => match[1]);
}

/** Check every card declared by this committed stack against its prospective working coverage map. */
export function committedCoverage(cwd: string, base: string) {
	const log = sh("git", ["log", "--format=%H%x1f%B%x1e", `${base}..HEAD`], { cwd });
	const fields = batchFields(batchCommits(log));
	return fields.items.length ? coverage({ cwd, pr: fields }) : { status: "pass" as const, details: { items: 0 } };
}

/**
 * The linted crates that ship as Wasm, which fast also lints for wasm32: their browser code compiles
 * only for that target, so the host lint never sees it (11e5318b broke release 1zc's Wasm build).
 */
export const wasmLinted = (linted: readonly string[]) => WASM_CRATES.filter((name) => linted.includes(name));

function testCount(output: string) {
	let passed = 0;
	for (const match of output.matchAll(/test result: \w+\. (\d+) passed/g)) passed += Number(match[1]);
	for (const match of output.matchAll(/^(?:# |ℹ )pass (\d+)/gm)) passed += Number(match[1]);
	return passed;
}

export interface FastOptions {
	cwd: string;
	target: string;
	base?: string;
	all?: boolean;
	plan?: boolean;
	dependentTests?: boolean;
	recordPass?: boolean;
}

/**
 * Run the fast gate in `cwd` against `base`: changed crates' unit and integration tests, and a `cargo check`
 * of their dependents, whose tests run at release (the lead, 2026-10-27). With `dependentTests`,
 * the dependents' unit and integration tests run too (the lead, 2026-11-03); with `all`, every
 * crate and tool counts as changed; with
 * `plan`, print what would run and stop. Returns the exit code.
 */
export async function fastGate({ cwd, target, base = "origin/dev", all = false, plan = false, dependentTests = false, recordPass = true }: FastOptions) {
	const started = Date.now();
	const env = gateEnv({ CARGO_TARGET_DIR: target });
	const cache = join(target, ".rv-fast");
	mkdirSync(cache, { recursive: true });

	const baseSha = sh("git", ["merge-base", "HEAD", base], { cwd });
	const workspace = metadata(cwd, cache, env);
	const graph = workspaceGraph(workspace, cwd);
	const changed = all ? ["Cargo.toml", ...JS_SUITES.map((suite) => suite.paths[0])] : changedFiles(cwd, baseSha);
	const typecheck = all || affectsTypeScript(changed) && (existsSync(join(cwd, "tsconfig.json")) || changed.includes("tsconfig.json"));
	const impact = affected(changed, graph, includeInputs(includes(cwd), graph));
	const testFiles = new Set(lines(sh("git", ["grep", "-l", "--untracked", "#\\[test\\]", "--", "*.rs"], { cwd, allowFail: true }).stdout));
	const units = unitTargets(dependentTests ? impact.crates : impact.direct, graph, testFiles);
	// Integration tests that name an untraced file by path read it at run time: they run too.
	const literals = readerLiterals(impact.untraced);
	const readerFiles = literals.length
		? lines(grepFiles(["-l", "-F", ...literals.flatMap((literal) => ["-e", literal]), "--", "*/tests/*.rs"], cwd))
		: [];
	// A crate's contract tests (its catalogue against the spec and the authoring set) run with
	// any change to it: they take seconds and drift there reached dev twice (batch 1 part c).
	const contracts = Object.entries(CONTRACT_TESTS).flatMap(([crate, files]) => (impact.direct.has(crate) ? files : []));
	// A changed integration test runs itself: two print tests whose expectation was edited reached
	// release unrun (release 1z). A deleted one has nothing to run.
	const changedTests = changed.filter((file) => existsSync(join(cwd, file)));
	// A changed support file under tests/ (common/mod.rs, fixtures) may feed any of them, so all run.
	const listTests = (crate: WorkspaceCrate) =>
		readdirSync(join(cwd, crate.dir, "tests"), { withFileTypes: true })
			.filter((entry) => entry.isFile() && entry.name.endsWith(".rs"))
			.map((entry) => entry.name.slice(0, -3));
	// A crate whose integration tests are modules of one `integration` binary is selected by name filters inside it.
	const merged = new Set(workspace.packages.filter((pkg) => pkg.targets.some((target) => target.kind.includes("test") && target.name === MERGED_TESTS)).map((pkg) => pkg.name));
	const readers = testTargets([...new Set([...readerFiles, ...contracts, ...changedTests])], graph, { listTests, merged });
	// Cargo's metadata also names explicit paths and tests/<name>/main.rs targets. Every changed
	// crate runs its own integrations, including unchanged ones, under the existing default
	// features and reasoned #[ignore] attributes. With `dependentTests` every dependent runs its
	// integrations too: a dependent's contract test sees a change its unit tests miss (the editor's
	// source ranges caught a core finding's new path only at release).
	const integrated = dependentTests ? impact.crates : impact.direct;
	for (const pkg of workspace.packages.filter(pkg => integrated.has(pkg.name))) {
		for (const target of pkg.targets.filter(target => target.kind.includes("test") && target.test !== false)) {
			const reader = readers.find(reader => reader.crate === pkg.name && reader.test === target.name);
			if (!reader) readers.push({ crate: pkg.name, test: target.name });
			else delete reader.filters;
		}
	}
	// The committed snippet file is generated from the chart pages; a stale one fails every release build.
	const freshness = join(cwd, FRESHNESS_TEST.file);
	const hasFreshnessTest = existsSync(freshness) && readFileSync(freshness, "utf8").includes(FRESHNESS_TEST.name);
	// `--all` runs the freshness test whenever it exists (review 1g note 8).
	const snippets = all
		? { status: hasFreshnessTest ? "test" : "pass", reason: "--all: the freshness test compares the committed snippet file with the generator" }
		: snippetCheck({
				changed,
				before: (file) => {
					const shown = sh("git", ["show", `${baseSha}:${file}`], { cwd, allowFail: true });
					return shown.status === 0 ? shown.stdout : undefined;
				},
				after: (file) => (existsSync(join(cwd, file)) ? readFileSync(join(cwd, file), "utf8") : undefined),
				mentionsSnippets: (file) => /snippet/i.test(sh("git", ["diff", "-U0", baseSha, "--", file], { cwd })),
				hasFreshnessTest,
			});
	if (snippets.status === "test" && !readers.some((reader) => reader.crate === FRESHNESS_TEST.crate && reader.test === FRESHNESS_TEST.test))
		readers.push({ crate: FRESHNESS_TEST.crate, test: FRESHNESS_TEST.test });
	// Every affected crate compiles: dependents, and changed crates that hold no unit test.
	const tested = new Set([...units.libs, ...units.bins.map((bin) => bin.crate), ...readers.map((reader) => reader.crate)]);
	const checked = [...impact.crates].filter((name) => !tested.has(name) && (!dependentTests || impact.direct.has(name)));
	const rustFiles = changed.filter((file) => file.endsWith(".rs") && existsSync(join(cwd, file)));

	console.log(`rv-gate fast: ${changed.length} changed file(s) since ${baseSha.slice(0, 10)} (${base})`);
	if (impact.direct.size) {
		const dependents = [...impact.crates].filter((name) => !impact.direct.has(name));
		console.log(`  crates changed: ${[...impact.direct].join(", ")}${dependents.length ? `; dependents: ${dependents.join(", ")}` : ""}`);
	}
	const targets = [...units.libs.map((name) => `${name} (lib)`), ...units.bins.map((bin) => `${bin.name} (bin)`), ...readers.map((reader) => `${reader.crate} --test ${reader.test}${reader.filters ? ` -- ${reader.filters.map((filter) => `${filter}::`).join(" ")}` : ""} (integration)`)];
	console.log(`  unit tests: ${targets.length ? targets.join(", ") : "none"}`);
	if (checked.length) console.log(`  lint only (no unit tests here; dependents' tests run at release): ${checked.join(", ")}`);
	const wasm = wasmLinted([...tested, ...checked]);
	if (wasm.length) console.log(`  wasm32 lint: ${wasm.join(", ")}`);
	if (impact.suites.size) console.log(`  tools: ${[...impact.suites].join(", ")}`);
	if (typecheck) console.log("  TypeScript: check the root project and its dependencies");
	if (snippets.status === "test") console.log(`  snippets: ${snippets.reason}`);
	if (impact.untraced.length && !readers.length)
		console.log(`  no unit test traces ${impact.untraced.length} file(s); the release run covers them: ${impact.untraced.slice(0, 6).join(", ")}${impact.untraced.length > 6 ? ", …" : ""}`);
	if (plan) return 0;

	const sensitive = secrets({ cwd, base: baseSha });
	if (sensitive.status === "fail") {
		console.log(`  fail  secrets: ${sensitive.reason}`);
		return 1;
	}

	const tree = publicTree({ cwd });
	if (tree.status === "fail") {
		console.log(`  fail  public tree: ${tree.reason}`);
		return 1;
	}

	const declared = committedCoverage(cwd, baseSha);
	if (declared.status === "fail") {
		console.log(`  fail  declared coverage: ${declared.reason}`);
		return 1;
	}
	if (declared.details.items) console.log(`  pass  declared coverage: ${declared.details.items} item(s)`);

	// Paperwork the release run checks, here in a second: every test the branch deletes or skips
	// has its reason in a commit message or in tools/rv-gate/retired-tests.json (DEL-36).
	const messages = sh("git", ["log", "--format=%B", `${baseSha}..HEAD`], { cwd });
	const retired = retiredTests({ cwd, base: baseSha, head: "HEAD", pr: { retired: parsePrBody(messages).retired } });
	if (retired.status === "fail") {
		console.log(`  fail  retired tests: ${retired.reason}\n  Add each to tools/rv-gate/retired-tests.json with its reason, or list it under "## Retired tests" in the commit message.`);
		return 1;
	}

	if (snippets.status === "fail") {
		console.log(`  fail  snippets: ${snippets.reason}`);
		return 1;
	}

	const jobs: Promise<FastResult | FastResult[]>[] = [];
	if (typecheck) {
		if (npmStale(cwd)) npmCi(cwd, { env });
		jobs.push(run("typecheck", "node", ["node_modules/typescript/bin/tsc", "--noEmit"], { cwd, env }));
	}
	if (rustFiles.length) jobs.push(run("fmt", "rustfmt", ["--check", "--edition", "2024", ...rustFiles], { cwd, env }));
	for (const suite of JS_SUITES.filter((s) => impact.suites.has(s.id))) {
		const prefix = suite.needs?.replace(/\/?node_modules$/, "") || ".";
		if (suite.needs && (prefix === "." ? npmStale(cwd) : !existsSync(join(cwd, suite.needs))))
			npmCi(cwd, { prefix, env });
		jobs.push(run(`tools:${suite.id}`, suite.run.cmd, suite.run.args, { cwd, env }));
	}
	let waited = 0;
	let stamped: string[] = [];
	const dropped = { count: 0, bytes: 0 };
	if (targets.length || checked.length) {
		requireDiskFloor();
		// One Cargo at a time: both would wait for the target's build lock anyway.
		const cargo = async (): Promise<FastResult[]> => {
			const controller = new AbortController();
			const interrupt = () => controller.abort();
			process.on("SIGINT", interrupt);
			process.on("SIGTERM", interrupt);
			let hold;
			try {
				const admission = { signal: controller.signal, waiting: (pid?: number) => console.log(`  waiting for the gate's Cargo sequence (pid ${pid ?? "pending"})`) };
				const cargoHold = hold = await acquireFastCargo(target, admission);
				waited = hold.waitedMs;
				// A run may have queued while the host still had room; check again before building.
				requireDiskFloor();
				hold.assertOwned();
				stamped = claim(cwd, target, withDependencies([...tested, ...checked], graph), graph);
				const cargoEnv = { cwd, env, signal: controller.signal };
				const cargoRun = (label: string, args: string[]) => { cargoHold.assertOwned(); return run(label, "cargo", args, cargoEnv); };
				// Once a test command's binaries have run and its output holds their results, they go on a
				// short disk, still under the hold; a later run relinks them (lead, 3 Nov).
				const testRun = async (label: string, args: string[]) => {
					const result = await cargoRun(label, args);
					const gone = dropTestBinaries(target, testBinaries(result.output), freeGib());
					dropped.count += gone.count;
					dropped.bytes += gone.bytes;
					return result;
				};
				const results = [];
				if (units.libs.length || units.bins.length) {
					const unitCrates = new Set([...units.libs, ...units.bins.map((bin) => bin.crate)]);
					const args = ["test", "--locked", ...[...unitCrates].flatMap((name) => ["-p", name])];
					if (units.libs.length) args.push("--lib");
					for (const bin of units.bins) args.push("--bin", bin.name);
					results.push(await testRun("rust", args));
				}
				// `--test` applies to every `-p`, so each crate's readers run in their own command.
				for (const crate of new Set(readers.map((reader) => reader.crate))) {
					if (results.some((result) => result.status === "fail")) break;
					const own = readers.filter((reader) => reader.crate === crate);
					const tests = own.flatMap((reader) => ["--test", reader.test]);
					// A merged crate's one binary runs only the modules named: a filter is a substring of the test path.
					const filters = own.flatMap((reader) => reader.filters ?? []);
					results.push(await testRun(`rust:${crate} readers`, ["test", "--locked", "-p", crate, ...tests, ...(filters.length ? ["--", ...filters.map((filter) => `${filter}::`)] : [])]));
				}
				// Lint every affected crate, its tests included, as the release run does: an unused
				// variable in a test file failed release batch 1 (clippy runs -D warnings).
				const linted = [...new Set([...tested, ...checked])];
				if (linted.length && !results.some((result) => result.status === "fail"))
					results.push(await cargoRun("clippy", ["clippy", "--locked", ...linted.flatMap((name) => ["-p", name]), "--all-targets", "--", "-D", "warnings"]));
				if (wasm.length && !results.some((result) => result.status === "fail"))
					results.push(await cargoRun("clippy:wasm32", ["clippy", "--locked", ...wasm.flatMap((name) => ["-p", name]), "--target", "wasm32-unknown-unknown", "--", "-D", "warnings"]));
				return results;
			} catch (error) {
				if (!controller.signal.aborted) throw error;
				return [{ label: "cargo", status: "fail", output: "Interrupted", ms: 0, blockedMs: 0 }];
			} finally {
				hold?.release();
				process.off("SIGINT", interrupt);
				process.off("SIGTERM", interrupt);
			}
		};
		jobs.push(cargo());
	}
	const results = (await Promise.all(jobs)).flat();
	for (const result of results) {
		const count = testCount(result.output);
		const blocked = result.blockedMs > 500 ? `, ${seconds(result.blockedMs)} of it waiting for another checkout's build` : "";
		console.log(`  ${result.status.padEnd(4)}  ${result.label.padEnd(22)} ${seconds(result.ms).padStart(7)}${count ? `  ${count} tests` : ""}${blocked}`);
	}
	if (stamped.length) console.log(`  rebuilt from this checkout (another checkout built them last): ${stamped.join(", ")}`);
	if (dropped.count) console.log(`  deleted ${dropped.count} test binaries after they ran (${(dropped.bytes / 2 ** 30).toFixed(1)} GiB), with / under ${KEEP_TEST_BINARIES_GIB} GiB free`);
	if (waited > 1000) console.log(`  waited ${seconds(waited)} for a gate run's Cargo sequence`);
	const failed = results.filter((result) => result.status === "fail");
	for (const result of failed) {
		const tail = result.output.trimEnd().split("\n").slice(-80).join("\n");
		console.log(`\n--- ${result.label}: last 80 lines ---\n${tail}`);
	}
	console.log(`\nfast: ${failed.length ? `${failed.length} failed` : "pass"} in ${seconds(Date.now() - started)}`);
	if (!failed.length && recordPass) {
		// The pre-push hook lets a stack reach `dev` only when a passing run at its tip recorded it
		// (lib/fastpass.mts): one run here covers every commit since origin/dev.
		const recorded = recordFastPass(cwd);
		console.log(recorded.refused !== undefined ? `  not recorded for the pre-push hook: ${recorded.refused}` : `  recorded for the pre-push hook: ${recorded.commits} commit(s) up to ${recorded.head.slice(0, 10)}, which is the tip to push`);
	}
	return failed.length ? 1 : 0;
}

/** `git grep` output; exit 1 means no match, and anything above it is an error, never an empty selection. */
function grepFiles(args: string[], cwd: string) {
	const result = sh("git", ["grep", ...args], { cwd, allowFail: true });
	if (result.status !== null && result.status > 1) throw new Error(`git grep ${args.join(" ")} exited ${result.status}: ${(result.stderr ?? "").trim()}`);
	return result.stdout;
}
