// Host reason (ARC-03): run a failed built-in gate check in a fresh Node process and browser.
import {readFileSync, renameSync, writeFileSync} from "node:fs";
import {deserialize, serialize} from "node:v8";
import childProcess from "node:child_process";
import type {ChildProcess} from "node:child_process";
import {syncBuiltinESMExports} from "node:module";
import * as checks from "./checks.mts";
import {specialChecks} from "./step-functions.mts";
import {snapshotOwnedGroup, classifyOwnedGroup, signalOwnedGroup} from "./proc.mts";
import type {WorkerContext, WorkerGroup, WorkerResult} from "./check-process.mts";
import type {GateStep} from "./steps.mts";
import type {StepOutcome} from "./results.mts";

const [input, output, groupsFile] = process.argv.slice(2);
const {step, ctx, target}: {step: GateStep & {fn: string}; ctx: WorkerContext; target: string} = deserialize(readFileSync(input));
// Keep a durable chain of observed member identities. A closed leader does not prove its
// process group is empty; only an overlapping live identity can extend ownership to peers.
const groups = new Map<number, WorkerGroup>(), spawn = childProcess.spawn;
const persist = () => {
	if (!groupsFile) return;
	const temp = `${groupsFile}.tmp`;
	writeFileSync(temp, JSON.stringify([...groups.values()].map(({pgid, identities}) => ({pgid, identities}))));
	renameSync(temp, groupsFile);
};
if (groupsFile) persist();
const refresh = (record: WorkerGroup) => {
	const snapshot = snapshotOwnedGroup(record.pgid), state = classifyOwnedGroup(snapshot, record.identities);
	if (state === "empty") groups.delete(record.pgid);
	else if (state === "owned") for (const member of snapshot.members) {
		if (!record.identities.some(identity => identity.pid === member.pid && identity.start === member.start)) record.identities.push(member);
	}
	persist();
	return state;
};
// The wrapper forwards every Node spawn overload with its original arguments and return value.
if (groupsFile) childProcess.spawn = ((...args: unknown[]) => {
	const child = Reflect.apply(spawn, undefined, args) as ChildProcess;
	const options = args.at(-1);
	if (typeof options !== "object" || options === null || !("detached" in options) || !options.detached
		|| child.pid === undefined || !Number.isSafeInteger(child.pid) || child.pid <= 0) return child;
	const record: WorkerGroup = {pgid: child.pid, identities: []}; groups.set(record.pgid, record); persist();
	const initial = snapshotOwnedGroup(record.pgid);
	if (initial.status === "known") record.identities = initial.members.filter(member => member.pid === child.pid);
	if (record.identities.length) refresh(record); else persist();
	const started = Date.now();
	const tick = () => {
		if (!groups.has(record.pgid)) return;
		refresh(record);
		if (groups.has(record.pgid)) { const timer = setTimeout(tick, Date.now() - started < 250 ? 10 : 1000); timer.unref(); }
	};
	const timer = setTimeout(tick, 10); timer.unref();
	child.once("close", () => { if (groups.has(record.pgid)) refresh(record); });
	return child;
}) as typeof spawn;
if (groupsFile) syncBuiltinESMExports();
const stop = (signal: NodeJS.Signals) => {
	for (const {pgid, identities} of groups.values()) signalOwnedGroup(pgid, identities);
	persist();
	process.exit(signal === "SIGINT" ? 130 : 143);
};
if (groupsFile) {
	process.once("SIGINT", () => stop("SIGINT"));
	process.once("SIGTERM", () => stop("SIGTERM"));
}
const special = specialChecks(target);
let result: StepOutcome;
try {
	// The checked-in registry names either a built-in or target-specific check.
	const check: unknown = Reflect.get(Object.hasOwn(special, step.fn) ? special : checks, step.fn);
	if (typeof check !== "function") throw new Error(`Unknown gate check ${step.fn}`);
	result = await check(ctx);
} catch (error) { result = {status: "fail", reason: String((error as Error | null)?.message ?? error)}; }
const updates = Object.fromEntries(["captures", "captureDiffs", "surfaceChanges"].filter(key => Object.hasOwn(ctx, key)).map(key => [key, ctx[key]]));
const evidence: WorkerResult = {result, updates, pid: process.pid};
writeFileSync(output, serialize(evidence), {flag: "wx"});
process.exitCode = result.status === "fail" ? 1 : 0;
