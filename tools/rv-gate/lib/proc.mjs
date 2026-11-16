// Run commands for gate steps: each step's output goes to its own log file, and the result
// records the command, exit status and duration (DEL-20).

import { spawn, spawnSync } from "node:child_process";
import { createWriteStream, existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { browserEventJournal } from "./browser-events.mjs";
import { createFirefoxProtocolSink } from "./firefox-protocol.mjs";

const OWNER_WRAPPER = `${process.env.HOME}/tmp/richview-rewrite/rustc-owner.sh`;
// On Claw Mini a Cargo shim runs one build at a time in the shared target and stamps the
// building checkout when another compiled last (DEL-81); the gate's Cargo goes through it too.
const CARGO_SHIM_DIR = `${process.env.HOME}/tmp/richview-rewrite/bin`;

// Detached command groups do not receive the gate's terminal signals. Relay them to every
// group we started, while leaving existing gate handlers in charge of their own cleanup.
const commandStops = new Set();
let defaultSignal;
const commandSignal = name => {
	if (process.listenerCount(name) === 1) defaultSignal ??= name;
	for (const stop of commandStops) stop(name);
};
const onInt = () => commandSignal("SIGINT"), onTerm = () => commandSignal("SIGTERM");
export function registerCommandStop(stop) {
	// Run before existing one-shot handlers remove themselves, so they remain in charge of exit.
	if (!commandStops.size) { process.prependListener("SIGINT", onInt); process.prependListener("SIGTERM", onTerm); }
	commandStops.add(stop);
	if (defaultSignal) stop(defaultSignal);
	return () => {
		commandStops.delete(stop);
		if (commandStops.size) return;
		process.off("SIGINT", onInt); process.off("SIGTERM", onTerm);
		if (defaultSignal) {
			// Give callers' promise continuations a turn to release their own holds.
			setImmediate(() => {
				if (commandStops.size || !defaultSignal) return;
				const name = defaultSignal; defaultSignal = undefined;
				if (process.listenerCount(name) === 0) process.kill(process.pid, name);
			});
		}
	};
}

/**
 * Daniel's disk floor: no build starts while less than this many GiB are free. It was 40 GiB on
 * 2026-10-27; on 2026-10-29 he lowered it to 10 ("there is no real floor, the only thing I'm at is
 * to prevent we hit disk issues in general, I'm happy for it to drop to 10gb before raising alarms").
 * On 2026-11-03 he made Claw Mini's disk the lead's to manage, with at least 15 GB always free for
 * his other sessions, and the Cargo shim pauses below 15 GiB.
 */
export const DISK_FLOOR_GIB = 15;

/** Free GiB on the root volume, or undefined when `df` fails. */
export function freeGib() {
	const out = spawnSync("df", ["-g", "/"], { encoding: "utf8" }).stdout ?? "";
	const free = Number(out.trim().split("\n")[1]?.trim().split(/\s+/)[3]);
	return Number.isFinite(free) ? free : undefined;
}

/** Throw when the disk is below the floor: pause and tell Daniel rather than fill it. */
export function requireDiskFloor() {
	const free = freeGib();
	if (free !== undefined && free < DISK_FLOOR_GIB)
		throw new Error(`Only ${free} GiB free on /, under the ${DISK_FLOOR_GIB} GiB floor (Daniel, 2026-11-03): no build starts. Free space or tell the lead.`);
}

/** The environment every gated command sees: the shared Cargo target and the pinned toolchain. */
export function gateEnv(extra = {}) {
	const home = process.env.HOME;
	const shim = existsSync(join(CARGO_SHIM_DIR, "cargo")) ? [CARGO_SHIM_DIR] : [];
	// The gate's own Node runs every node, npm and npx command, so the suites run the version the pins
	// step checks (.node-version) rather than the first node on the host's PATH: a Homebrew upgrade moved
	// ~/.local/bin's node to 26.13 on 3 Nov, and a full's suites ran on it while the gate ran 26.5.
	const path = [...shim, dirname(process.execPath), `/opt/homebrew/opt/rustup/bin`, `${home}/.cargo/bin`, `${home}/.local/bin`, process.env.PATH].join(":");
	// A gate run never rewrites expected outputs: a blessed spec_docs or updated golden run passes by definition.
	// Only the dispatcher may select a partial cohort, with completeness checked before merging it.
	const { RICHVIEW_BLESS: _bless, RICHVIEW_DEV_PREVIEW: _preview, RICHVIEW_UPDATE_GOLDEN: _golden, UPDATE_SNIPPETS: _snippets, RICHVIEW_VERIFY_SHARD: _shard, RICHVIEW_VERIFY_PARENT: _parent, RICHVIEW_VERIFY_CASES: _cases, RICHVIEW_BROWSER_EVENT_LOG: _journal, RV_GATE_FIREFOX_PROTOCOL_LOG: _firefoxProtocol, ...inherited } = process.env;
	return {
		...inherited,
		PATH: path,
		// The shim waits while a gate run holds the target, except for this run's own commands.
		RV_GATE_HOLDER: String(process.pid),
		// Records which checkout compiled each workspace crate (rv-gate fast, DEL-81).
		...(existsSync(OWNER_WRAPPER) && !process.env.RUSTC_WRAPPER ? { RUSTC_WRAPPER: OWNER_WRAPPER } : {}),
		CARGO_TERM_COLOR: "never",
		// Dedicated targets bypass the shim's job default. Keep their builds at four too.
		CARGO_BUILD_JOBS: process.env.CARGO_BUILD_JOBS ?? "4",
		NO_COLOR: "1",
		...extra,
	};
}

/**
 * The Rust tests a cargo test log reports as ignored: `test <name> ... ignored` with or without
 * `, <reason>`. A test that a `--ignored` run executes is not listed, since it did not stay ignored.
 */
export function ignoredTests(log) {
	const found = [];
	for (const match of log.matchAll(/^test (.+?) \.\.\. ignored(?:, (.*))?$/gm)) found.push({ name: match[1], reason: match[2]?.trim() ?? "" });
	return found;
}

/**
 * The inputs a gate script reports it did not run: each `UNRUN <what>: <reason>` line of its log
 * (golden's and contract's connector examples). They go into the receipt's Not run list (DEL-21).
 */
export function unrunLines(log) {
	return [...log.matchAll(/^UNRUN (\S.*?): (.+)$/gm)].map((match) => ({ what: match[1], reason: match[2].trim() }));
}

/** Every part a step did not run, from its details: skipped Rust tests and unrun inputs. */
export function notRunParts(step) {
	const details = step.details ?? {};
	return [
		...(details.unrunTests ?? []).map((t) => ({ what: t.name, reason: t.reason })),
		...(details.unrunInputs ?? []),
	];
}

/** Run a command synchronously and return trimmed stdout; throws on a non-zero exit. */
export function sh(cmd, args, { cwd, env, allowFail = false, input } = {}) {
	const result = spawnSync(cmd, args, {
		cwd,
		env: env ?? gateEnv(),
		encoding: "utf8",
		input,
		maxBuffer: 256 * 1024 * 1024,
	});
	if (result.error) throw result.error;
	if (result.status !== 0 && !allowFail) {
		const detail = (result.stderr || result.stdout || "").trim().split("\n").slice(-20).join("\n");
		throw new Error(`${cmd} ${args.join(" ")} exited ${result.status}\n${detail}`);
	}
	return allowFail ? { status: result.status, stdout: result.stdout, stderr: result.stderr } : result.stdout.trim();
}

// On macOS kill(-pgid, 0) can return EPERM for a group containing only zombies. Inspect
// the actual members; an unavailable or malformed process table is an unknown, not a pass.
function liveGroup(pgid) {
	try {
		const result = spawnSync("ps", ["-A", "-o", "pid=,pgid=,stat="], { encoding: "utf8", timeout: 2000 });
		if (result.error || result.status !== 0) return undefined;
		const lines = result.stdout.trim().split("\n");
		const rows = lines.map(line => /^\s*(\d+)\s+(\d+)\s+(\S+)\s*$/.exec(line));
		if (!rows.length || rows.some(row => !row)) return undefined;
		return rows.some(row => Number(row[2]) === pgid && !row[3].startsWith("Z"));
	} catch { return undefined; }
}

/** Inspect live group members with their process start identity; unknown is never an empty group. */
export function snapshotOwnedGroup(pgid) {
	if (!Number.isSafeInteger(pgid) || pgid <= 0) throw new Error("Invalid process group");
	try {
		const ps = process.platform === "darwin" ? "/bin/ps" : "/usr/bin/ps";
		const result = spawnSync(ps, ["-A", "-o", "pid=,pgid=,lstart=,stat="], {encoding: "utf8", timeout: 2000, env: {...process.env, LC_ALL: "C"}});
		if (result.error || result.status !== 0) return {status: "unknown", members: []};
		const rows = result.stdout.trim().split("\n").map(line => /^\s*(\d+)\s+(\d+)\s+(.+?)\s+(\S+)\s*$/.exec(line));
		if (!rows.length || rows.some(row => !row)) return {status: "unknown", members: []};
		return {status: "known", members: rows.filter(row => Number(row[2]) === pgid && Number(row[1]) !== result.pid && !row[4].startsWith("Z")).map(row => ({pid: Number(row[1]), start: row[3]}))};
	} catch { return {status: "unknown", members: []}; }
}

/** Confirm an owned member is still in the group; an unrelated group is not ours to signal. */
export function classifyOwnedGroup(snapshot, identities) {
	if (snapshot?.status !== "known" || !Array.isArray(snapshot.members) || !Array.isArray(identities)) return "unknown";
	if (!snapshot.members.length) return "empty";
	return snapshot.members.some(member => identities.some(known => member.pid === known.pid && member.start === known.start)) ? "owned" : "unknown";
}

/** Recheck identity immediately before signaling; never signal an unknown or recycled group. */
export function signalOwnedGroup(pgid, identities, signal = "SIGKILL") {
	const state = classifyOwnedGroup(snapshotOwnedGroup(pgid), identities);
	if (state !== "owned") return state;
	try { process.kill(-pgid, signal); return "signalled"; }
	catch (error) { if (error.code === "ESRCH") return "empty"; return "unknown"; }
}

/**
 * Run a command, streaming its output into `logPath`. Resolves with
 * `{status: "pass"|"fail", exitCode, durationMs, command}`; never rejects for a failing command.
 */
export function runLogged(cmd, args, { cwd, env, logPath, timeoutMs = 3600_000, signal, onOutput } = {}) {
	const started = Date.now();
	const command = [cmd, ...args].join(" ");
	return new Promise((accept) => {
		if (signal?.aborted) { accept({status: "unrun", reason: "Gate cancelled before command start.", durationMs: 0, command}); return; }
		let journal;
		try { journal = browserEventJournal(logPath, env ?? gateEnv()); }
		catch (error) { accept({ status: "fail", exitCode: 1, durationMs: Date.now() - started, command, reason: `Could not prepare browser relaunch journal: ${error.message}`, browserRelaunches: [] }); return; }
		const log = createWriteStream(logPath, { flags: "a" });
		log.write(`$ ${command}\n`);
		// `signal` stops the command when another step of a release run has failed.
		// Own a process group so cancellation cannot leave a test or Cargo grandchild using the
		// target after the caller releases its hold. Never signal any other command's group.
		// A command that deliberately creates a separate group owns that group's cleanup.
		const child = spawn(cmd, args, { cwd, env: journal.env, stdio: ["ignore", "pipe", "pipe"], detached: process.platform !== "win32" });
		let stopped = false, stoppedByAbort = false, stoppedAt, forceTimer, drainTimer;
		const signalGroup = name => {
			if (!child.pid) return;
			try { if (process.platform === "win32") child.kill(name); else process.kill(-child.pid, name); }
			catch (error) { if (error.code !== "ESRCH") log.write(`rv-gate: cannot signal command group: ${error.message}\n`); }
		};
		const boundDrain = () => {
			if (drainTimer) return;
			drainTimer = setTimeout(() => {
				if (finished) return;
				signalGroup("SIGKILL");
				log.write("rv-gate: command group did not drain or could not be inspected after SIGKILL\n");
				finish(stoppedByAbort ? 127 : 1);
			}, 8000);
		};
		const stop = (name = "SIGTERM") => {
			if (stopped) return;
			stopped = true;
			stoppedAt = Date.now();
			signalGroup(name);
			forceTimer = setTimeout(() => signalGroup("SIGKILL"), 5000);
			boundDrain();
		};
		const unregister = registerCommandStop(stop);
		const abort = () => { stoppedByAbort = true; stop(); };
		signal?.addEventListener("abort", abort, { once: true });
		if (signal?.aborted) abort();
		if (onOutput) child.stdout.on("data", onOutput);
		child.stdout.pipe(log, { end: false });
		let finished = false, stderrPaused = false;
		const resumeStderr = () => { stderrPaused = false; if (!finished) child.stderr.resume(); };
		const protocol = journal.env.RV_GATE_FIREFOX_PROTOCOL_LOG && journal.env.RICHVIEW_VERIFY_BROWSER === "firefox"
			&& (cmd === "node" || cmd === process.execPath) && args[0] === "scripts/verify-browser.mjs"
			? createFirefoxProtocolSink({ logPath: journal.env.RV_GATE_FIREFOX_PROTOCOL_LOG, forward: chunk => {
				if (log.destroyed || log.errored) return;
				onOutput?.(chunk);
				if (!log.write(chunk) && !finished && !stderrPaused) {
					stderrPaused = true; child.stderr.pause(); log.once("drain", resumeStderr);
				}
			} }) : undefined;
		const protocolData = chunk => protocol.write(chunk);
		if (protocol) { child.stderr.on("data", protocolData); log.on("error", resumeStderr); }
		else { child.stderr.pipe(log, { end: false }); if (onOutput) child.stderr.on("data", onOutput); }
		const timer = setTimeout(() => {
			log.write(`\nrv-gate: timed out after ${timeoutMs} ms\n`);
			stopped = true;
			stoppedAt ??= Date.now();
			signalGroup("SIGKILL");
			boundDrain();
		}, timeoutMs);
		// An aborted child emits both `error` and `close`; the log ends and the promise settles once.
		// A stopped command's output can still arrive after the log ended (its children hold the
		// pipes), so the pipes are cut first.
		log.on("error", () => {});
		const finish = (exitCode) => {
			if (finished) return;
			finished = true;
			clearTimeout(timer);
			clearTimeout(forceTimer);
			clearTimeout(drainTimer);
			signal?.removeEventListener("abort", abort);
			if (protocol) { child.stderr.off("data", protocolData); log.off("drain", resumeStderr); log.off("error", resumeStderr); }
			for (const stream of [child.stdout, child.stderr]) {
				if (onOutput) stream.off("data", onOutput);
				stream.unpipe(log);
				stream.resume();
			}
			const durationMs = Date.now() - started;
			const firefoxProtocol = protocol?.end();
			if (firefoxProtocol && !firefoxProtocol.critical_complete)
				log.write("\nrv-gate: Firefox protocol diagnostics incomplete " + JSON.stringify(firefoxProtocol) + "\n");
			log.end(`\nrv-gate: exit ${exitCode} after ${durationMs} ms\n`, (error) => {
				const logError = error ?? log.errored;
				const code = logError && exitCode === 0 ? 1 : exitCode;
				accept(journal.collect({ status: code === 0 ? "pass" : "fail", exitCode: code, durationMs, command, pid: child.pid, ...(firefoxProtocol && { firefoxProtocol }), ...(logError && { reason: `Could not finish command log: ${logError.message}` }) }));
				unregister();
			});
		};
		child.on("error", (error) => {
			log.write(`rv-gate: ${error.message}\n`);
			finish(127);
		});
		child.on("close", async (code) => {
			if (finished) return;
			if (stopped && process.platform !== "win32" && child.pid) {
				// A child can close its pipes before a grandchild exits. The hold is safe to
				// release after the live members drain. SIGKILL is sent after five seconds;
				// allow a few more seconds, then report an undrainable or unknown group.
				const deadline = stoppedAt + 8000;
				for (;;) {
					if (finished) return;
					if (liveGroup(child.pid) === false) break;
					if (Date.now() >= deadline) {
						signalGroup("SIGKILL");
						log.write("rv-gate: command group did not drain or could not be inspected after SIGKILL\n");
						break;
					}
					await new Promise(resolve => setTimeout(resolve, 100));
				}
			}
			finish(stoppedByAbort ? 127 : stopped ? 1 : code ?? 1);
		});
	});
}
