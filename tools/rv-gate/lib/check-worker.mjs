// Host reason (ARC-03): run a failed built-in gate check in a fresh Node process and browser.
import {readFileSync, renameSync, writeFileSync} from "node:fs";
import {deserialize, serialize} from "node:v8";
import childProcess from "node:child_process";
import {syncBuiltinESMExports} from "node:module";
import * as checks from "./checks.mjs";
import {specialChecks} from "./step-functions.mjs";
import {snapshotOwnedGroup, classifyOwnedGroup, signalOwnedGroup} from "./proc.mjs";

const [input, output, groupsFile] = process.argv.slice(2);
const {step, ctx, target} = deserialize(readFileSync(input));
// Keep a durable chain of observed member identities. A closed leader does not prove its
// process group is empty; only an overlapping live identity can extend ownership to peers.
const groups = new Map(), spawn = childProcess.spawn;
const persist = () => {
	if (!groupsFile) return;
	const temp = `${groupsFile}.tmp`;
	writeFileSync(temp, JSON.stringify([...groups.values()].map(({pgid, identities}) => ({pgid, identities}))));
	renameSync(temp, groupsFile);
};
if (groupsFile) persist();
const refresh = record => {
	const snapshot = snapshotOwnedGroup(record.pgid), state = classifyOwnedGroup(snapshot, record.identities);
	if (state === "empty") groups.delete(record.pgid);
	else if (state === "owned") for (const member of snapshot.members) {
		if (!record.identities.some(identity => identity.pid === member.pid && identity.start === member.start)) record.identities.push(member);
	}
	persist();
	return state;
};
if (groupsFile) childProcess.spawn = (...args) => {
	const child = spawn(...args), options = args.at(-1);
	if (!options?.detached || !Number.isSafeInteger(child.pid) || child.pid <= 0) return child;
	const record = {pgid: child.pid, identities: []}; groups.set(record.pgid, record); persist();
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
};
if (groupsFile) syncBuiltinESMExports();
const stop = signal => {
	for (const {pgid, identities} of groups.values()) signalOwnedGroup(pgid, identities);
	persist();
	process.exit(signal === "SIGINT" ? 130 : 143);
};
if (groupsFile) {
	process.once("SIGINT", () => stop("SIGINT"));
	process.once("SIGTERM", () => stop("SIGTERM"));
}
const special = specialChecks(target);
let result;
try {
	const check = Object.hasOwn(special, step.fn) ? special[step.fn] : checks[step.fn];
	if (typeof check !== "function") throw new Error(`Unknown gate check ${step.fn}`);
	result = await check(ctx);
} catch (error) { result = {status: "fail", reason: String(error?.message ?? error)}; }
const updates = Object.fromEntries(["captures", "captureDiffs", "surfaceChanges"].filter(key => Object.hasOwn(ctx, key)).map(key => [key, ctx[key]]));
writeFileSync(output, serialize({result, updates, pid: process.pid}), {flag: "wx"});
process.exitCode = result.status === "fail" ? 1 : 0;
