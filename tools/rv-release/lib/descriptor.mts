// Host reason (ARC-03): reads the per-release descriptor that names a candidate's pins; no network or clock access.
// A release descriptor (`release.json`, kept in the release's working folder) holds every value that release 31's kit hard-coded:
// the candidate, its full receipt, what it replaces, the Windows ceiling ask and the previous release's evidence.
import { readFileSync } from "node:fs";
import { dirname, isAbsolute, join, resolve } from "node:path";

export type DescriptorInput = {
	schema: number; name: string; tag: string; previousTag: string; binaryVersion: string;
	final: string; old: string; publicBefore: string; candidate: string; oldCandidate: string; full: string; oldWorker: string;
	route: string; windowsScope: string; dir?: string; notes: string; assets: string;
	windows: {ask: string; ceilingUsd: number; yes: string; no: string};
	previous: {release: string; journal: string; retainedFull: string; siteDeploy: string; updateSnapshot?: string};
	worker: {baselineDeployment: string; firstWorker: string; firstDeployment: string; manifestSha256: string};
	hosted: {stack: string; volume: string}; hosts: {lenovoHome: string};
	pins: {dockerSha256: string; controllerHashes: string};
};
export type ReleaseDescriptor = ReturnType<typeof loadDescriptor>;

const SHA40 = /^[0-9a-f]{40}$/;
const UUID = /^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/;
const FULL = /^full-\d{8}T\d{6}Z-[0-9a-f]{10}$/;
const CANDIDATE = /^candidate\/[1-9]\d*$/;
const TAG = /^v\d+\.\d+\.\d+$/;
const NAME = /^[a-z0-9][a-z0-9-]*$/;

function need(ok: unknown, message: string): asserts ok {
	if (!ok) throw new Error(`Release descriptor: ${message}`);
}

/** Validate a parsed descriptor and return it with resolved paths. */
export function parseDescriptor(input: unknown, file = "release.json") {
	const raw = input as DescriptorInput; // Validate descriptor JSON before any release step reads it.
	need(raw && typeof raw === "object" && raw.schema === 1, "schema must be 1");
	need(NAME.test(raw.name ?? ""), "name must look like release-31");
	need(TAG.test(raw.tag ?? "") && TAG.test(raw.previousTag ?? ""), "tag and previousTag must look like v3.1.0");
	need(/^\d+\.\d+\.\d+$/.test(raw.binaryVersion ?? ""), "binaryVersion must look like 3.0.0");
	need(SHA40.test(raw.final ?? "") && SHA40.test(raw.old ?? "") && SHA40.test(raw.publicBefore ?? ""), "final, old and publicBefore must be 40-hex commits");
	need(CANDIDATE.test(raw.candidate ?? "") && CANDIDATE.test(raw.oldCandidate ?? ""), "candidate and oldCandidate must look like candidate/33");
	need(FULL.test(raw.full ?? "") && raw.full.endsWith(`-${raw.final.slice(0, 10)}`), "full must be the full receipt id of final");
	need(UUID.test(raw.oldWorker ?? ""), "oldWorker must be a Worker version id");
	need(["A", "B"].includes(raw.route), "route must be A or B");
	need(["site", "combined"].includes(raw.windowsScope), "windowsScope must be site or combined");
	const w = raw.windows;
	need(w && NAME.test(w.ask ?? "") && Number.isFinite(w.ceilingUsd) && w.ceilingUsd > 0 && typeof w.yes === "string" && typeof w.no === "string", "windows needs ask, ceilingUsd, yes and no");
	need(typeof raw.notes === "string" && typeof raw.assets === "string", "notes and assets name the release notes and the asset list");
	need(raw.previous && NAME.test(raw.previous.release ?? "") && /^go-record-[0-9a-f]{8}$/.test(raw.previous.journal ?? "") && typeof raw.previous.retainedFull === "string" && typeof raw.previous.siteDeploy === "string", "previous needs release, journal, retainedFull and siteDeploy");
	const worker = raw.worker;
	need(worker && [worker.baselineDeployment, worker.firstDeployment].every((id) => UUID.test(id ?? "")) && UUID.test(worker.firstWorker ?? "") && /^[0-9a-f]{64}$/.test(worker.manifestSha256 ?? ""), "worker needs baselineDeployment, firstWorker, firstDeployment and manifestSha256");
	need(raw.hosted && typeof raw.hosted.stack === "string" && /^vol-[0-9a-f]+$/.test(raw.hosted.volume ?? ""), "hosted needs stack and volume");
	need(raw.hosts && /^\/[\w.\/-]+$/.test(raw.hosts.lenovoHome ?? ""), "hosts needs lenovoHome, the absolute home folder of the build host");
	need(raw.pins && /^[0-9a-f]{64}$/.test(raw.pins.dockerSha256 ?? "") && typeof raw.pins.controllerHashes === "string", "pins needs dockerSha256 and controllerHashes");
	return { ...raw, number: Number(raw.candidate.split("/")[1]), file };
}

/** Read a descriptor from disk; `dir` (the release folder) defaults to the descriptor's own folder. */
export function loadDescriptor(file: string) {
	const path = resolve(file);
	const desc = parseDescriptor(JSON.parse(readFileSync(path, "utf8")), path);
	const dir = desc.dir ? (isAbsolute(desc.dir) ? desc.dir : resolve(dirname(path), desc.dir)) : dirname(path);
	return { ...desc, dir, journal: join(dir, `go-record-${desc.final.slice(0, 8)}`) };
}
