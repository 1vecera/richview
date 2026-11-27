// One Cargo target serves the gate and every checkout on the host. Cargo names a
// workspace crate's artifacts by its path inside the workspace, so a build in another worktree
// writes the same files as this run, and Cargo trusts them whenever this checkout's sources are
// older. A run uses the time Cargo last recorded a workspace build to notice that happening.

import { spawnSync } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import { closeSync, constants, cpSync, existsSync, fstatSync, linkSync, lstatSync, mkdirSync, openSync, readFileSync, readdirSync, readlinkSync, realpathSync, renameSync, rmSync, rmdirSync, statSync, symlinkSync, unlinkSync, utimesSync, writeFileSync } from "node:fs";
import { dirname, isAbsolute, join, relative, resolve } from "node:path";
import {setTimeout as delay} from "node:timers/promises";
import { BUILD_MANIFEST } from "./build-manifest.mts";
import type { CargoMetadata } from "./impact.mts";

interface Inode { ino: number | bigint; dev: number | bigint }
interface CargoWait { waiting?: (pid?: number) => void; every?: number; signal?: AbortSignal }
interface IdleWait<T> { waiting?: (found: T[]) => void; every?: number; limit?: number; signal?: AbortSignal }
interface KeepBuildOptions { target: string; cwd: string; commit: string; dir: string; keep?: number }
interface SnapshotOptions { target: string; cwd: string; dir: string; outputs?: readonly string[]; files?: CopiedOutput[] }
export interface CopiedOutput { path: string; size: number; mtimeMs: number; copiedSize: number }
type BuildIdentity = ReturnType<typeof keptBuildIdentity>;
interface KeptLease extends BuildIdentity { version: number; id: string; pid: number; started: string; commit: string }

const errno = (error: unknown) => (error as NodeJS.ErrnoException).code;

/** Workspace members are all named `richview-*`; registry dependencies never are. */
const MEMBER = /^richview-/;

function entries(dir: string) {
	try {
		return readdirSync(dir);
	} catch {
		return [];
	}
}

/** The `.fingerprint` folders of a target: `debug` and `release`, and those under each cross target. */
function fingerprintFolders(target: string) {
	const folders = [];
	for (const name of entries(target)) {
		const own = join(target, name, ".fingerprint");
		if (existsSync(own)) {
			folders.push({ profile: name, path: own });
			continue;
		}
		for (const sub of entries(join(target, name))) {
			const nested = join(target, name, sub, ".fingerprint");
			if (existsSync(nested)) folders.push({ profile: sub, path: nested });
		}
	}
	return folders;
}

/**
 * The latest time, in milliseconds, at which Cargo recorded building a workspace crate in
 * `target`, or 0 when it never did. With `release`, only release builds count: the steps after
 * the build step read release binaries alone.
 */
export function workspaceBuiltAt(target: string, { release = false } = {}) {
	let latest = 0;
	for (const folder of fingerprintFolders(target)) {
		if (release && folder.profile !== "release") continue;
		for (const unit of entries(folder.path)) {
			if (!MEMBER.test(unit)) continue;
			for (const file of entries(join(folder.path, unit))) {
				try {
					latest = Math.max(latest, statSync(join(folder.path, unit, file)).mtimeMs);
				} catch {
					// Replaced while we read; the next look sees the new file.
				}
			}
		}
	}
	return latest;
}

const SOURCE_OWNER = ".rv-gate-sources.json";

// Include every fingerprint, not only the latest time: another checkout can replace one crate
// in an older profile. Custom Wasm profiles count independently too.
function workspaceGeneration(target: string) {
	const hash = createHash("sha256");
	for (const folder of fingerprintFolders(target)) {
		for (const unit of entries(folder.path).filter(name => MEMBER.test(name))) {
			for (const file of entries(join(folder.path, unit))) {
				const path = join(folder.path, unit, file);
				try {
					const stat = statSync(path, { bigint: true });
					hash.update(`${path}\0${stat.ino}\0${stat.size}\0${stat.mtimeNs}\n`);
				} catch (error) {
					if (errno(error) !== "ENOENT") throw error;
					hash.update(`disappeared:${path}\n`);
				}
			}
		}
	}
	return hash.digest("hex");
}

function writeSourceOwner(target: string, cwd: string, generation: string) {
	const file = join(target, SOURCE_OWNER);
	const temporary = `${file}.${process.pid}.tmp`;
	writeFileSync(temporary, JSON.stringify({ cwd: realpathSync(cwd), generation }) + "\n");
	renameSync(temporary, file);
}

/** Stamp tracked Rust/build inputs only when this checkout no longer owns this target generation. */
export function claimSources(target: string, cwd: string) {
	const generation = workspaceGeneration(target);
	try {
		const owner: { cwd?: string; generation?: string } = JSON.parse(readFileSync(join(target, SOURCE_OWNER), "utf8"));
		if (owner.cwd === realpathSync(cwd) && owner.generation === generation) return false;
	} catch { /* Missing or malformed ownership is cold. */ }
	const result = spawnSync("git", ["ls-files", "-z", "--", "*.rs", "*Cargo.toml", "*.wgsl", "*.glsl"], { cwd, encoding: "utf8" });
	if (result.error) throw result.error;
	if (result.status !== 0) throw new Error(`Cannot claim Cargo sources: ${result.stderr.trim()}`);
	const stamp = new Date();
	for (const file of result.stdout.split("\0").filter(Boolean)) {
		try { utimesSync(join(cwd, file), stamp, stamp); }
		catch (error) { if (errno(error) !== "ENOENT") throw error; }
	}
	// Record the generation observed BEFORE stamping. A concurrent build during preparation then
	// invalidates this record; the command claims again after acquiring the Cargo hold.
	writeSourceOwner(target, cwd, generation);
	return true;
}

/** Remember our completed command's generation while the gate still excludes other Cargo writers. */
export function recordSourceOwner(target: string, cwd: string) {
	if (cargoHolder(target) !== process.pid) throw new Error("Recording source ownership requires the Cargo hold");
	writeSourceOwner(target, cwd, workspaceGeneration(target));
}

// A gate run also needs the shared target to itself while each of its Cargo commands runs:
// every checkout's binaries share `target/debug/richview` and its siblings, so a build elsewhere
// during the gate's `cargo test` can swap the binary its tests run. The gate holds a
// file naming its process; other rv-gate runs and the `cargo` wrapper in the rewrite's `env.sh`
// wait while a live process holds it.

/** The hold file inside the shared target. */
export const HOLD = ".rv-gate-cargo";

// A full or prod run tests its own release build for its whole run, so it also names itself in a
// second file for that long: the `cargo` wrapper keeps release builds waiting while it lives, and
// debug builds go ahead between the run's own Cargo commands (lead, 3 Nov). It is a file of its own
// because `HOLD` admits one process at a time, and a fast run must not wait for a whole full.

/** The release hold file inside the shared target. */
export const RELEASE_HOLD = ".rv-gate-release";

/** The live process holding `target`'s hold file (`file`, by default `HOLD`), or 0 when none does. */
export function cargoHolder(target: string, file = HOLD) {
	let pid;
	try {
		pid = Number.parseInt(readFileSync(join(target, file), "utf8"), 10);
	} catch {
		return 0;
	}
	if (!Number.isInteger(pid) || pid <= 0) return 0;
	try {
		process.kill(pid, 0);
		return pid;
	} catch (error) {
		// EPERM: the process exists but belongs to someone else, which still holds the target.
		return errno(error) === "EPERM" ? pid : 0;
	}
}

function deadPid(pid: number) {
	if (!Number.isInteger(pid) || pid <= 0) return false;
	try { process.kill(pid, 0); return false; } catch (error) {
		if (errno(error) === "ESRCH") return true;
		if (errno(error) === "EPERM") return false;
		throw error;
	}
}

const sameFile = (a: Inode, b: Inode) => a.ino === b.ino && a.dev === b.dev;

/** Serialize dead-holder reclamation so a second waiter cannot unlink a freshly acquired hold. */
function reclaimDeadCargo(file: string) {
	const guard = `${file}.reclaim`;
	let fd;
	let owned: Inode | undefined;
	let reclaimed = false;
	try {
		fd = openSync(guard, "wx", 0o600);
		owned = fstatSync(fd);
		writeFileSync(fd, `${process.pid}\n`);
		try {
			const observed = statSync(file);
			if (deadPid(Number(readFileSync(file, "utf8").trim())) && sameFile(observed, statSync(file))) { rmSync(file); reclaimed = true; }
		} catch (error) { if (errno(error) !== "ENOENT") throw error; }
	} catch (error) {
		if (errno(error) !== "EEXIST") throw error;
		// A killed reclaimer can leave its tiny guard, including an empty file before the PID write.
		// Wait ten seconds for that write; a guard naming a live process is never stolen.
		try {
			const observed = statSync(guard);
			const pid = Number(readFileSync(guard, "utf8").trim());
			const abandoned = !Number.isInteger(pid) || pid <= 0 || deadPid(pid);
			if (Date.now() - observed.mtimeMs > 10_000 && abandoned && sameFile(observed, statSync(guard))) rmSync(guard);
		} catch (error) { if (errno(error) !== "ENOENT") throw error; }
	} finally {
		if (fd !== undefined) {
			closeSync(fd);
			try { if (sameFile(owned!, statSync(guard))) rmSync(guard); } catch (error) { if (errno(error) !== "ENOENT") throw error; }
		}
	}
	return reclaimed;
}

/** Atomically hold one fast run's Cargo sequence; only a dead process's file may be reclaimed. */
export async function acquireFastCargo(target: string, { waiting = () => {}, every = 1000, signal, profile: profileName = "debug" }: CargoWait & { profile?: string } = {}) {
	mkdirSync(target, { recursive: true });
	const file = join(target, HOLD);
	const profile = `${file}.profile`;
	const started = Date.now();
	let told = false;
	for (;;) {
		signal?.throwIfAborted();
		let fd;
		try {
			fd = openSync(file, "wx", 0o600);
			writeFileSync(fd, `${process.pid}\n`);
			closeSync(fd);
			fd = undefined;
			const owned = statSync(file);
			let ownedProfile: Inode | undefined;
			const owns = () => {
				try { return sameFile(owned, statSync(file)) && readFileSync(file, "utf8").trim() === String(process.pid); }
				catch (error) { if (errno(error) === "ENOENT") return false; throw error; }
			};
			const release = () => {
				try {
					// Each fast owner installs a new profile inode; a displaced owner clears only its own.
					if (ownedProfile && sameFile(ownedProfile, statSync(profile))) rmSync(profile);
				} catch (error) {
					if (errno(error) !== "ENOENT") throw error;
				}
				if (owns()) rmSync(file, { force: true });
			};
			const temporary = `${profile}.${process.pid}.tmp`;
			try {
				writeFileSync(temporary, `${profileName}\n`, { mode: 0o600 });
				renameSync(temporary, profile);
				ownedProfile = statSync(profile);
			} catch (error) { rmSync(temporary, { force: true }); release(); throw error; }
			return {
				release,
				waitedMs: Date.now() - started,
				assertOwned: () => { if (!owns()) throw new Error("Lost the fast Cargo hold; refusing another Cargo command."); },
			};
		} catch (error) {
			if (fd !== undefined) { closeSync(fd); rmSync(file, { force: true }); }
			if (errno(error) !== "EEXIST") throw error;
		}
		let pid;
		try {
			pid = Number(readFileSync(file, "utf8").trim());
			if (deadPid(pid) && reclaimDeadCargo(file)) continue;
		} catch (error) {
			if (errno(error) === "ENOENT") continue;
			throw error;
		}
		if (!told) { waiting(pid); told = true; }
		await new Promise((done) => setTimeout(done, every));
	}
}

const holds = new Map<string, { count: number; pending: ReturnType<typeof acquireFastCargo> }>();

/**
 * Atomically take a gate command's hold, waiting for another owner. Nested commands share one
 * acquisition per target; the last release removes both files. A whole build may start commands
 * of either profile, so the marker explicitly names all profiles.
 */
export async function holdCargo(target: string, options: CargoWait = {}) {
	mkdirSync(target, { recursive: true });
	const key = realpathSync(target);
	let entry = holds.get(key);
	if (!entry) {
		entry = { count: 0, pending: acquireFastCargo(key, { ...options, profile: "all" }) };
		holds.set(key, entry);
	}
	entry.count++;
	let hold;
	try { hold = await entry.pending; hold.assertOwned(); }
	catch (error) {
		if (--entry.count === 0) hold?.release();
		if (holds.get(key) === entry) holds.delete(key);
		throw error;
	}
	let released = false;
	return () => {
		if (released) return;
		released = true;
		if (--entry.count === 0) {
			hold.release();
			if (holds.get(key) === entry) holds.delete(key);
		}
	};
}

/**
 * Wrap a run's command budget (`scheduleCommand`) so that each command it admits holds the target
 * from its turn to its end: `hold()` resolves to a release function once the command's permits are
 * granted. A command queued behind a full's browser suites then holds nothing (lead, 3 Nov).
 */
export function heldOnTurn<T>(scheduleCommand: (run: () => Promise<T>, permits?: number) => Promise<T>, hold: () => Promise<() => void>) {
	return (run: () => T | PromiseLike<T>, permits?: number) => scheduleCommand(async () => {
		const release = await hold();
		try {
			return await run();
		} finally {
			release();
		}
	}, permits);
}

/**
 * Hold `target`'s release builds for a whole full or prod run, waiting while another live process
 * holds them; only a dead process's file is reclaimed. Returns the function that releases it.
 */
export async function holdRelease(target: string, { waiting = () => {}, every = 1000, signal }: CargoWait = {}) {
	mkdirSync(target, { recursive: true });
	const file = join(target, RELEASE_HOLD);
	let told = false;
	for (;;) {
		signal?.throwIfAborted();
		let fd;
		try {
			fd = openSync(file, "wx", 0o600);
			writeFileSync(fd, `${process.pid}\n`);
			closeSync(fd);
			fd = undefined;
			const owned = statSync(file);
			let released = false;
			return () => {
				if (released) return;
				released = true;
				try {
					if (sameFile(owned, statSync(file)) && readFileSync(file, "utf8").trim() === String(process.pid)) rmSync(file);
				} catch (error) { if (errno(error) !== "ENOENT") throw error; }
			};
		} catch (error) {
			if (fd !== undefined) { closeSync(fd); rmSync(file, { force: true }); }
			if (errno(error) !== "EEXIST") throw error;
		}
		let pid;
		try {
			pid = Number(readFileSync(file, "utf8").trim());
			if (deadPid(pid) && reclaimDeadCargo(file)) continue;
		} catch (error) {
			if (errno(error) === "ENOENT") continue;
			throw error;
		}
		if (!told) { waiting(pid); told = true; }
		await new Promise((done) => setTimeout(done, every));
	}
}

/** Resolve once no other live process holds `target`, calling `waiting(pid)` the first time it waits. */
export async function waitForCargo(target: string, { waiting = () => {}, every = 5000 }: CargoWait = {}) {
	let told = false;
	for (;;) {
		const pid = cargoHolder(target);
		if (!pid || pid === process.pid) return;
		if (!told) waiting(pid);
		told = true;
		await new Promise((done) => setTimeout(done, every));
	}
}

/**
 * Cargo processes that build into `target`, other than this run's own. A gate run holding the
 * target waits until there are none, since one that started before the hold may still be writing
 * it. Since DEL-81 each stream builds in its own target, so only a Cargo whose environment names
 * `target`, or names none, counts; `ps -E` shows the environment of this user's processes.
 */
export function runningCargo(target?: string) {
	const pids = (spawnSync("pgrep", ["-x", "cargo"], { encoding: "utf8" }).stdout ?? "").split("\n").filter(Boolean).map(Number);
	// Parents of every process, to leave out this run's own Cargo, which rustup and cargo-clippy
	// start as grandchildren.
	const parents = new Map<number, number>(
		(spawnSync("ps", ["-A", "-o", "pid=,ppid="], { encoding: "utf8" }).stdout ?? "")
			.split("\n")
			.map((row) => row.trim().split(/\s+/).map(Number))
			.filter((pair): pair is [number, number] => pair.length === 2),
	);
	const ours = (pid: number) => {
		let at: number | undefined = pid;
		for (let hops = 0; at !== undefined && at > 1 && hops < 32; at = parents.get(at), hops++) if (at === process.pid) return true;
		return false;
	};
	return pids.filter((pid) => {
		if (ours(pid)) return false;
		const row = spawnSync("ps", ["-E", "-o", "command=", "-p", String(pid)], { encoding: "utf8" }).stdout ?? "";
		if (!row.trim()) return false;
		const named = / CARGO_TARGET_DIR=(\S+)/.exec(row)?.[1];
		return !target || !named || named === target;
	});
}

/**
 * Resolve once `busy()` returns an empty list, calling `waiting(list)` the first time it waits.
 * Resolve to false if it is still busy after `limit` milliseconds.
 */
export async function waitUntilIdle<T>(busy: () => T[], { waiting = () => {}, every = 5000, limit = 30 * 60 * 1000, signal }: IdleWait<T> = {}) {
	const start = Date.now();
	let told = false;
	for (;;) {
		signal?.throwIfAborted();
		const found = busy();
		if (!found.length) return true;
		if (Date.now() - start >= limit) return false;
		if (!told) waiting(found);
		told = true;
		await delay(every, undefined, {signal});
	}
}

/**
 * Below this many GiB free on /, each Rust test binary goes once it has run and its result is
 * recorded (lead, 3 Nov): every fast and full run left 4 to 8 GB of them, which the janitor may take
 * only after 30 minutes. A later run relinks a missing one.
 */
export const KEEP_TEST_BINARIES_GIB = 25;

/**
 * Delete the test binaries `executables`, files inside `target`, when `free` (GiB free on /) is
 * under KEEP_TEST_BINARIES_GIB. Returns how many it deleted and their bytes.
 */
export function dropTestBinaries(target: string, executables: readonly string[], free: number | undefined) {
	const dropped = { count: 0, bytes: 0 };
	if (free === undefined || free >= KEEP_TEST_BINARIES_GIB) return dropped;
	const root = realpathSync(target);
	for (const executable of new Set(executables)) {
		let path;
		try {
			path = realpathSync(executable);
		} catch (error) {
			if (errno(error) === "ENOENT") continue;
			throw error;
		}
		const inside = relative(root, path);
		if (!inside || inside.startsWith("..") || isAbsolute(inside)) continue;
		const stat = lstatSync(path);
		if (!stat.isFile()) continue;
		rmSync(path);
		dropped.count++;
		dropped.bytes += stat.size;
	}
	return dropped;
}

/**
 * Delete the workspace's own outputs (rlibs, test and binary executables, objects) from every
 * profile folder of `target`, keeping the dependencies' outputs. The release run does this to
 * its own target after each run: the workspace rebuilds for every batch anyway, and its test
 * executables were most of the folder (DEL-81, Claw Mini's disk floor). Returns bytes freed.
 */
export function pruneWorkspaceOutputs(target: string, metadata: CargoMetadata) {
	const names = new Set(metadata.packages.flatMap((pkg) => pkg.targets.map((t) => t.name.replaceAll("-", "_"))));
	const profiles = [];
	for (const entry of existsSync(target) ? readdirSync(target) : []) {
		if (["debug", "release"].includes(entry)) profiles.push(join(target, entry));
		else if (entry.includes("-") && existsSync(join(target, entry, "release"))) profiles.push(join(target, entry, "release"));
		if (entry.includes("-") && existsSync(join(target, entry, "debug"))) profiles.push(join(target, entry, "debug"));
		for (const wasm of ["wasm-release", "wasm-site"]) {
			if (entry.includes("-") && existsSync(join(target, entry, wasm))) profiles.push(join(target, entry, wasm));
		}
	}
	let freed = 0;
	for (const profile of profiles) {
		for (const folder of ["deps", "."]) {
			const dir = join(profile, folder);
			if (!existsSync(dir)) continue;
			for (const file of readdirSync(dir)) {
				const stem = file.replace(/^lib/, "").replace(/-[0-9a-f]{16}(\..*)?$/, "").replace(/\.(d|rlib|rmeta|wasm|dylib)$/, "");
				if (!names.has(stem.replaceAll("-", "_"))) continue;
				const path = join(dir, file);
				try {
					const stat = statSync(path);
					if (!stat.isFile()) continue;
					freed += stat.size;
					rmSync(path, { force: true });
				} catch {
					// A file another process removed is already gone.
				}
			}
		}
	}
	return freed;
}

// Lease acquisition and pruning share one mutation guard: a live reader can never enter
// between the pruner's lease check and its delete. Contention refuses without touching builds.
function withKeptBuildLock<T>(dir: string, run: () => T) {
	mkdirSync(dir, { recursive: true });
	const file = join(dir, ".rv-kept-build.lock");
	const temporary = join(dir, `.rv-kept-build.${process.pid}.${randomUUID()}.tmp`);
	let owned: Inode | undefined;
	let published = false;
	try {
		writeFileSync(temporary, `${process.pid}\n`, { flag: "wx", mode: 0o600 });
		owned = statSync(temporary, { bigint: true });
		for (;;) {
			try { linkSync(temporary, file); published = true; break; }
			catch (error) {
				if (errno(error) !== "EEXIST") throw error;
				if (reclaimDeadCargo(file)) continue;
				throw new Error("Kept-build retention is busy; no build or lease was changed");
			}
		}
		return run();
	} finally {
		rmSync(temporary, { force: true });
		if (published) {
			if (!sameFile(owned!, statSync(file, { bigint: true })) || readFileSync(file, "utf8").trim() !== String(process.pid)) throw new Error("Lost the kept-build retention guard");
			unlinkSync(file);
		}
	}
}

function keptBuildIdentity(folder: string, commit: string) {
	const stat = lstatSync(folder, { bigint: true });
	if (!stat.isDirectory() || stat.isSymbolicLink()) throw new Error("A leased build must be a real directory");
	const manifestPath = join(folder, "build-manifest.json");
	if (!lstatSync(manifestPath).isFile()) throw new Error("A leased build needs a regular manifest");
	const bytes = readFileSync(manifestPath);
	const manifest: { inputs?: { commit?: string } } = JSON.parse(bytes.toString("utf8"));
	if (manifest.inputs?.commit !== commit) throw new Error("Kept-build manifest does not match the expected full commit");
	return { dev: String(stat.dev), ino: String(stat.ino), manifestSha256: createHash("sha256").update(bytes).digest("hex") };
}

function ownerStart(pid: number) {
	const result = spawnSync("ps", ["-p", String(pid), "-o", "lstart="], { encoding: "utf8", timeout: 5000, env: { ...process.env, LC_ALL: "C", TZ: "UTC" } });
	return result.status === 0 && !result.error ? result.stdout.trim() || undefined : undefined;
}

const leaseRoot = (dir: string) => join(dir, ".rv-build-leases");
const matchesBuild = (a: BuildIdentity, b: BuildIdentity) => (["dev", "ino", "manifestSha256"] as const).every(key => a[key] === b[key]);
function leaseEntries(dir: string) {
	try { return readdirSync(dir); }
	catch (error) { if (errno(error) === "ENOENT") return []; throw error; }
}

/** Collect live leases under the mutation guard; uncertain or corrupt ownership never permits deletion. */
function leasedBuilds(dir: string) {
	const protectedNames = new Set();
	for (const name of leaseEntries(leaseRoot(dir))) {
		if (!/^[0-9a-f]{10}$/.test(name)) throw new Error("Invalid kept-build lease folder");
		const leases = join(leaseRoot(dir), name);
		for (const entry of leaseEntries(leases).filter(value => value.endsWith(".json"))) {
			const file = join(leases, entry);
			if (!lstatSync(file).isFile()) throw new Error("Invalid kept-build lease record");
			const record: KeptLease = JSON.parse(readFileSync(file, "utf8"));
			if (record.version !== 1 || !Number.isSafeInteger(record.pid) || record.pid <= 1 || typeof record.started !== "string" || !record.started
				|| !/^[0-9a-f]{40}$/.test(record.commit) || record.commit.slice(0, 10) !== name || entry !== `${record.id}.json`
				|| !/^[0-9a-f]{64}$/.test(record.manifestSha256) || !/^\d+$/.test(record.dev) || !/^\d+$/.test(record.ino)) throw new Error("Invalid kept-build lease identity");
			if (deadPid(record.pid)) { unlinkSync(file); continue; }
			const started = ownerStart(record.pid);
			if (started && started !== record.started) { unlinkSync(file); continue; }
			if (!matchesBuild(record, keptBuildIdentity(join(dir, name), record.commit))) throw new Error("A live leased build changed identity; refusing retention");
			protectedNames.add(name);
		}
	}
	return protectedNames;
}

/** Protect a kept folder bound to its expected full commit until release or owner exit. */
export function leaseReleaseBuild({ dir, commit }: { dir: string; commit: string }) {
	if (!/^[0-9a-f]{40}$/.test(commit)) throw new Error("A kept-build lease requires the expected full commit");
	dir = resolve(dir);
	return withKeptBuildLock(dir, () => {
		const folder = join(dir, commit.slice(0, 10));
		const identity = keptBuildIdentity(folder, commit), started = ownerStart(process.pid);
		if (!started) throw new Error("Cannot identify the kept-build lease owner");
		const id = randomUUID(), leases = join(leaseRoot(dir), commit.slice(0, 10));
		mkdirSync(leases, { recursive: true });
		const file = join(leases, `${id}.json`), temporary = `${file}.tmp`;
		const data = JSON.stringify({ version: 1, id, pid: process.pid, started, commit, ...identity }) + "\n";
		try { writeFileSync(temporary, data, { flag: "wx", mode: 0o600 }); renameSync(temporary, file); }
		finally { rmSync(temporary, { force: true }); }
		const owned = statSync(file, { bigint: true });
		let released = false;
		return { folder, release: () => {
			if (released) return;
			withKeptBuildLock(dir, () => {
				if (!sameFile(owned, lstatSync(file, { bigint: true })) || readFileSync(file, "utf8") !== data) throw new Error("Lost the kept-build lease; refusing to remove its replacement");
				try {
					if (!matchesBuild(identity, keptBuildIdentity(folder, commit))) throw new Error("The leased build changed identity");
				} finally {
					unlinkSync(file); released = true;
					try { rmdirSync(leases); } catch (error) { if (errno(error) !== "ENOTEMPTY" && errno(error) !== "ENOENT") throw error; }
				}
			});
		} };
	});
}

/** Keep the newest builds plus every live leased build; a leased folder is never replaced. */
export function keepReleaseBuild(options: KeepBuildOptions) {
	return withKeptBuildLock(options.dir, () => keepBuildUnderLock(options));
}

function keepBuildUnderLock({ target, cwd, commit, dir, keep = 2 }: KeepBuildOptions) {
	const protectedNames = leasedBuilds(dir);
	const folder = join(dir, commit.slice(0, 10));
	if (protectedNames.has(commit.slice(0, 10))) throw new Error("Cannot replace a live leased build");
	const install = installOutputPaths(target, commit);
	rmSync(folder, { recursive: true, force: true });
	mkdirSync(folder, { recursive: true });
	for (const name of ["richview", "richview-editor", "richview-sql-worker"]) {
		const binary = join(target, "release", name);
		if (existsSync(binary)) cpSync(binary, join(folder, name));
	}
	if (existsSync(join(target, BUILD_MANIFEST))) cpSync(join(target, BUILD_MANIFEST), join(folder, "build-manifest.json"));
	for (const path of install) {
		const to = join(folder, path.slice("release/".length));
		mkdirSync(dirname(to), { recursive: true });
		cpSync(join(target, path), to);
	}
	const web = join(cwd, "dist/web");
	if (existsSync(web)) cpSync(web, join(folder, "dist/web"), { recursive: true });
	const kept = readdirSync(dir)
		.filter((name) => /^[0-9a-f]{7,12}$/.test(name) && existsSync(join(dir, name, "richview")))
		.map((name) => ({ name, at: statSync(join(dir, name)).mtimeMs }))
		.sort((a, b) => b.at - a.at);
	for (const old of kept.slice(keep)) if (!protectedNames.has(old.name)) rmSync(join(dir, old.name), { recursive: true, force: true });
	return folder;
}


/** Finished build outputs: native executables, manifest, font licences and declared installer files. */
export const BUILD_OUTPUTS = ["release/richview", "release/richview-editor", "release/richview-sql-worker", BUILD_MANIFEST, "release/examples/seed_duckdb", "release/font-licenses", "release/install"];

/**
 * The Publish server's Wasm. The publish-tests step can still be building it while the build step's
 * outputs are copied, so it is copied when the parallel pool has finished (note 11, review 1f).
 */
export const LATE_OUTPUTS = ["wasm32-unknown-unknown/release/richview_publish_server.wasm"];

/** What the steps after the build read from the target. */
export const RELEASE_OUTPUTS = [...BUILD_OUTPUTS, ...LATE_OUTPUTS];

/** Select only this build's declared installer; older archives may remain in the shared target. */
function installOutputPaths(target: string, expectedCommit?: string) {
	if (!existsSync(join(target, BUILD_MANIFEST))) {
		if (existsSync(join(target, "release/install"))) throw new Error("Installer output has no build manifest");
		return [];
	}
	const manifest: { inputs?: { commit?: string; dirty?: boolean }; outputs?: Record<string, { bytes?: number; sha256?: string }> } = JSON.parse(readFileSync(join(target, BUILD_MANIFEST), "utf8"));
	const outputs = manifest.outputs ?? {};
	const paths = Object.keys(outputs).filter(key => key.startsWith("target:release/install/")).map(key => key.slice("target:".length));
	if (!paths.length) return []; // Historical builds did not produce an installer.
	const commit = manifest.inputs?.commit;
	if (!commit || !/^[0-9a-f]{40}$/.test(commit) || manifest.inputs?.dirty !== false || (expectedCommit && commit !== expectedCommit)) throw new Error("Installer build source mismatch");
	// The Windows preview (ARC-67) adds its own archive and its PowerShell installer, always together.
	const archive = new RegExp(`^release/install/${commit}/richview-(?:(?:aarch64|x86_64)-(?:apple-darwin|unknown-linux-gnu)|x86_64-pc-windows-msvc)\\.tar\\.gz(?:\\.sha256)?$`);
	const installers = ["release/install/install.sh", "release/install/install.ps1"];
	const windows = paths.some(path => path.includes("-pc-windows-msvc."));
	if (!paths.includes(installers[0]) || paths.includes(installers[1]) !== windows || paths.length < 3 || paths.some(path => !installers.includes(path) && !archive.test(path))) throw new Error("Invalid installer output paths");
	for (const path of paths.filter(path => !installers.includes(path))) {
		const pair = path.endsWith(".sha256") ? path.slice(0, -7) : `${path}.sha256`;
		if (!paths.includes(pair)) throw new Error("Unpaired installer archive/checksum");
	}
	for (const path of paths) {
		if (!lstatSync(join(target, path)).isFile()) throw new Error(`Installer output is not a regular file: ${path}`);
		const bytes = readFileSync(join(target, path)), record = outputs[`target:${path}`];
		if (record?.bytes !== bytes.length || record?.sha256 !== createHash("sha256").update(bytes).digest("hex")) throw new Error(`Installer output changed: ${path}`);
	}
	return paths.sort();
}

/** Every file under `path` (the file itself when it is one), as paths relative to `root`. */
function filesUnder(root: string, rel: string): string[] {
	const path = join(root, rel);
	const stat = statSync(path);
	if (!stat.isDirectory()) return [rel];
	return readdirSync(path).sort().flatMap((name) => filesUnder(root, join(rel, name)));
}

/**
 * Copy `outputs` from `target` into `dir` (cloned on APFS, so it costs no space until something
 * writes). Returns the provenance of every file copied: its path, and the size and mtime it had in
 * the shared target when it was copied. Missing outputs are skipped.
 */
export function copyOutputs({ target, dir, outputs }: { target: string; dir: string; outputs: readonly string[] }): CopiedOutput[] {
	const copied = [];
	for (const rel of outputs.flatMap(path => path === "release/install" ? installOutputPaths(target) : [path])) {
		const from = join(target, rel);
		if (!existsSync(from)) continue;
		mkdirSync(dirname(join(dir, rel)), { recursive: true });
		const before = filesUnder(target, rel).map((file) => ({ file, stat: statSync(join(target, file)) }));
		cpSync(from, join(dir, rel), { recursive: true, mode: constants.COPYFILE_FICLONE, preserveTimestamps: true });
		for (const { file, stat } of before) copied.push({ path: file, size: stat.size, mtimeMs: Math.round(stat.mtimeMs), copiedSize: statSync(join(dir, file)).size });
	}
	return copied;
}

/**
 * What in a snapshot's provenance (`copyOutputs`) is not the run's own build: a file written more
 * than a second after `notAfter`, when the step that built it ended, came from another checkout's
 * build; a copy whose size differs from the source's was written during the copy. Returns one
 * sentence per such file; an empty list means every file is the run's.
 */
export function provenanceProblems(files: readonly CopiedOutput[], notAfter: number) {
	const problems = [];
	for (const file of files) {
		if (file.size !== file.copiedSize) problems.push(`${file.path} changed while it was copied`);
		else if (file.mtimeMs > notAfter + 1000) problems.push(`${file.path} was written ${Math.round((file.mtimeMs - notAfter) / 1000)} s after its build ended`);
	}
	return problems;
}

/**
 * Copies this run's release outputs out of the shared target into `dir` and points the checkout's
 * `target` link at the copy. Another checkout's release build in the shared target then cannot
 * change what the remaining steps test; release 1z lost 17 steps to one (2026-10-28). Returns
 * undefined, changing nothing, when the checkout's `target` is its own folder rather than a link.
 * `outputs` defaults to the build step's outputs; `copyOutputs` adds the late ones. The run's end
 * (`linkTarget`) points the link back at the shared target.
 */
export function snapshotRelease({ target, cwd, dir, outputs = BUILD_OUTPUTS, files = [] }: SnapshotOptions) {
	const link = join(cwd, "target");
	let stat;
	try {
		stat = lstatSync(link);
	} catch {
		stat = undefined;
	}
	if (stat && !stat.isSymbolicLink()) return undefined;
	rmSync(dir, { recursive: true, force: true });
	mkdirSync(dir, { recursive: true });
	files.push(...copyOutputs({ target, dir, outputs }));
	if (stat) unlinkSync(link);
	symlinkSync(dir, link);
	return dir;
}

/**
 * The directory the after-build steps read release binaries from: the checkout's `target` link
 * (the run's private copy) when there is a snapshot, otherwise the shared target itself, where the
 * overwrite guard must stay on.
 */
export function releaseBinDir({ snapshot, cwd, target }: { snapshot?: string; cwd: string; target: string }) {
	return snapshot ? join(cwd, "target", "release") : join(target, "release");
}

/**
 * Point the checkout's `target/` at `shared`: the scripts read `target/release/...`. A link to
 * anything else, including a deleted run folder, is replaced. A real folder is left alone.
 */
export function linkTarget(cwd: string, shared: string) {
	const target = join(cwd, "target");
	// `cargo clean` removes the shared directory itself, which leaves the link dangling.
	mkdirSync(shared, { recursive: true });
	let link;
	try {
		link = lstatSync(target);
	} catch {
		link = undefined;
	}
	if (link?.isSymbolicLink() && (!existsSync(target) || resolve(cwd, readlinkSync(target)) !== shared)) {
		unlinkSync(target);
		link = undefined;
	}
	if (!link) symlinkSync(shared, target);
	else if (!link.isSymbolicLink()) console.log(`  ${target} is its own folder, not a link to the shared target ${shared}; delete it to keep one build on the host`);
}
