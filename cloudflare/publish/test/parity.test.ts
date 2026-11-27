// Parity proof: the Rust Publish Worker, after converting PR #70's stored ledger, answers the
// reader routes like PR #70's JavaScript Worker did on the same stored state.
//
// What it does, offline (no Cloudflare, no network: `fetch` is replaced by a thrower while it runs):
//  1. Extracts PR #70's Worker sources with `git show e8aa9e1:cloudflare/publish/<file>` into
//     `<cwd>/tmp/parity-old-*` and imports them. `jose` is symlinked from node_modules; `yaml` too when it is
//     installed, otherwise a stub stands in (it is imported only by the old desk editor, which no replayed route reaches).
//  2. Takes a release made by the real builder (a News build) and derives three releases from it: the live one
//     (an editors-only item, a former slug on a story and on a page), an older one that was once live, and one
//     that was never activated. Both Workers share one fake R2 holding the builder's objects and manifests.
//  3. Seeds identical PR #70 ledgers (camelCase `auditSequence`/`accessRevision`, `activated:<id>`, `runtime:<id>`,
//     flat `audit:` entries) into two fake Durable Object storages: hidden items, a role mapping, a deleted-policy
//     item, a narrowed-audience policy and an old release that was once live.
//  4. Replays the same GETs as anonymous, an unmapped signed-in user, a member, an editor and the bootstrap
//     super editor, and compares status, Location, content-type and the body bytes, plus the security headers.
//  5. Asserts the migration itself: the stored `state` gains `revision`, keeps its hides, roles and policies,
//     and `state-pr70` holds the old record unchanged.
//
// Release input, one of:
//  - env PARITY_RELEASE_DIR=<dir> holding `release.json` and `objects/<hash>` as `richview-publish build` writes them:
//      mkdir -p tmp/parity/runtime && printf '/* rt */' > tmp/parity/runtime/richview_runtime.js &&
//      printf '\0asm\1\0\0\0' > tmp/parity/runtime/richview_runtime_bg.wasm &&
//      cargo run -p richview-publish -- build --site publications/news/site.yaml --output tmp/parity/out \
//        --runtime tmp/parity/runtime --source-commit abcdefabcdefabcdefabcdefabcdefabcdefabcd
//      PARITY_RELEASE_DIR=tmp/parity/out node --test cloudflare/publish/test/parity.test.ts
//  - env PARITY_BUILD=1 builds it with cargo into `<cwd>/tmp/parity-build-*` (slow; not for the gate, which holds Cargo).
// Without either, or without the Wasm build, git history or `jose`, the tests skip with the reason.
//
// Accepted differences (each is asserted to still occur, so the list cannot go stale):
//  A1 Dashboards were removed by FEA-BR-30: PR #70 counted `kind: "dashboard"` items in the dateline and routed them
//     under /dashboards/. A freshly built release has none, so no dashboard route is replayed; a legacy stored one is served by PR #70 and answers 410 in Rust.
//  A2 Runtime names: PR #70's Worker served only richview_web.js/_bg.wasm. The builder now writes richview_runtime.*
//     and PR #70's list was never updated, so `/_runtime/<current id>/richview_runtime.js` is 404 on PR #70 and 200 on
//     Rust. (This test found that the Rust server refused a kept richview_web.* runtime that PR #70 served; the
//     server now serves both pairs, and that pair is compared and equal.)
//  A4 Private items (OPN-54) exist only in Rust and the migrated ledger has none, so nothing is hidden by them here.
//  A5 /_auth/login and /_auth/session, /_editor/* and the desk are not reader routes; /_auth/* is compared on status.
//  A6 A redirect has no body; PR #70 sent it with no content-type and Rust labels it text/plain. Nobody reads that type.
//  A7 JSON key order: serde_json writes keys sorted (/_status, /search.json); PR #70 wrote insertion order. The parsed
//     documents are compared and are equal.
//  A8 The converted ledger spells out `deleted: false` on every policy floor, where PR #70 left the field absent.
//  A9 Link-preview tags (FEA-SF-55, FEA-RP-51): the release has a `<!--PUBLISH:SOCIAL-->` slot that PR #70 leaves in the page
//     and the Rust Worker fills per reader with og:type, og:image, og:image:alt and twitter:card. Both are cut from the page
//     before comparing; the rest of the page must still be byte-equal.
//  A10 robots.txt (FEA-SF-55): the Rust Worker adds two `Allow` lines so chat and social crawlers may fetch a public story's
//     link-preview image (`/_content/*/og.png` and its versioned `/_releases/*/_content/*/og.png`); PR #70 has neither.
//     Only those two lines are cut; every other line must be byte-equal.
//  A11 The `?embed=1` style of a content frame (PR #70's `embedStyle`) styles the metric block as `.rv-big` (`.rv-big__value`, `.rv-big__label`);
//     the renderer renamed that class to `.rv-metric__value` (8c5e4f9, STY components.metric) and the Rust Worker follows
//     it, so a frame is served with the class its HTML carries. Only those selectors are renamed in PR #70's body.
//  A12 The `?embed=1` style (PR #70's `embedStyle`) keeps a chart at the embed's width through `.rv-report__body>:is(.rv-chart, ...)`;
//     the renderer now boxes every chart and its source note in `.rv-keep` (7d14fe5b), so the Rust Worker adds
//     `.rv-report__body>.rv-keep>:is(.rv-chart,.rv-image,.rv-embed)` to that rule, and PR #70 has not. Only that
//     added selector is cut from the Rust body.
//  The A9 tags are not just dropped: the tag values of every story page are asserted against the release's own item in
//  "link-preview tags carry the story's own values" below.
//  Every normalization is in `normalize` below; there are no others. HEAD, writes, fonts and the desk are covered by
//  adapter.test.ts.
import test, { after } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import type { Principal } from "../auth.ts";
import { PublicationBase, type PublicationBindings, type PublishStorage } from "../adapter.ts";

const here = dirname(fileURLToPath(import.meta.url));
const publishDir = resolve(here, "..");
type ParityItem = {
	id: string;
	slug: string;
	section: string;
	state: string;
	audience: string;
	kind: string;
	aliases: string[];
	title: string;
	source: string;
	route: string;
	card: string;
	lead: string;
	stat: string;
	assets: Record<string, unknown>;
	summary: string;
	byline?: string;
};
type ParityRelease = {
	id: string;
	commit: string;
	items: ParityItem[];
	site: { title: string; sections: { id: string }[]; base_url: string };
	runtime: Record<string, string>;
};
type Policy = Pick<ParityItem, "id" | "section" | "state" | "audience"> & { deleted?: boolean };
type HiddenItem = { revision: string; actor: string; reason: string; at: string; reconciliation: string };
type ConvertedLedger = {
	revision?: number;
	generation: number;
	active: string | null;
	audit_sequence: number;
	access_revision: number;
	hidden: Record<string, HiddenItem>;
	roles: Record<string, unknown>;
	policies: Record<string, Policy>;
	activated: Record<string, string>;
	runtimes: Record<string, Record<string, string>>;
};
type Reader = { fetch(request: Request): Promise<Response> };

const repoRoot = resolve(publishDir, "../..");
const OLD_COMMIT = "e8aa9e1";
const OLD_FILES = ["worker", "policy", "repository", "editor", "editor-ui", "auth"];
const HOST = "news.example.test";
const scratch: string[] = [];

const git = (...args: string[]) => execFileSync("git", args, { cwd: repoRoot, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
const sha = (data: string) => createHash("sha256").update(data).digest("hex");
const enc = new TextEncoder();
const dec = new TextDecoder();

function preflight() {
	if (!existsSync(join(publishDir, "pkg/richview_publish_server_bg.wasm"))) return "build the Wasm first: npm run build:wasm";
	if (!existsSync(join(publishDir, "node_modules/jose"))) return "jose is not installed: npm ci --ignore-scripts in cloudflare/publish";
	try {
		for (const file of OLD_FILES) git("cat-file", "-e", `${OLD_COMMIT}:cloudflare/publish/${file}.mjs`);
	} catch {
		return `git history does not hold ${OLD_COMMIT} (PR #70's Worker); fetch full history`;
	}
	if (!process.env.PARITY_RELEASE_DIR && process.env.PARITY_BUILD !== "1")
		return "no release: set PARITY_RELEASE_DIR to a `richview-publish build` output (see the file header) or PARITY_BUILD=1";
	if (process.env.PARITY_RELEASE_DIR && !existsSync(join(resolve(process.env.PARITY_RELEASE_DIR), "release.json")))
		return `PARITY_RELEASE_DIR has no release.json: ${process.env.PARITY_RELEASE_DIR}`;
	return false;
}
const skip = preflight();
// The release run sets PARITY_REQUIRED: a missing prerequisite there is a failure, not a skip.
if (skip && process.env.PARITY_REQUIRED === "1") throw new Error(`parity cannot run: ${skip}`);

function tempDir(name: string) {
	mkdirSync(join(process.cwd(), "tmp"), { recursive: true });
	const dir = mkdtempSync(join(process.cwd(), "tmp", `${name}-`));
	scratch.push(dir);
	return dir;
}
after(() => {
	for (const dir of scratch) rmSync(dir, { recursive: true, force: true });
});

/** PR #70's Worker, imported from a temp copy of its sources. */
async function oldWorker(): Promise<{ Publication: new (ctx: { storage: Storage }, env: PublicationBindings) => Reader }> {
	const dir = tempDir("parity-old");
	for (const file of OLD_FILES) writeFileSync(join(dir, `${file}.mjs`), git("show", `${OLD_COMMIT}:cloudflare/publish/${file}.mjs`));
	const modules = join(dir, "node_modules");
	mkdirSync(modules);
	symlinkSync(join(publishDir, "node_modules/jose"), join(modules, "jose"));
	if (existsSync(join(publishDir, "node_modules/yaml"))) symlinkSync(join(publishDir, "node_modules/yaml"), join(modules, "yaml"));
	else {
		mkdirSync(join(modules, "yaml"));
		writeFileSync(join(modules, "yaml/package.json"), '{"name":"yaml","type":"module","main":"index.js"}');
		writeFileSync(join(modules, "yaml/index.js"), 'export const parseDocument = () => { throw new Error("yaml stub: the desk editor is not replayed"); };\n');
	}
	return import(pathToFileURL(join(dir, "worker.mjs")).href);
}

function releaseInput() {
	if (process.env.PARITY_RELEASE_DIR) return resolve(process.env.PARITY_RELEASE_DIR);
	const root = tempDir("parity-build");
	mkdirSync(join(root, "runtime"));
	writeFileSync(join(root, "runtime/richview_runtime.js"), "/* fixture runtime */");
	writeFileSync(join(root, "runtime/richview_runtime_bg.wasm"), Buffer.from("\0asm\x01\0\0\0", "latin1"));
	execFileSync(
		"cargo",
		["run", "-p", "richview-publish", "--", "build", "--site", "publications/news/site.yaml", "--output", join(root, "out"), "--runtime", join(root, "runtime"), "--source-commit", "abcdefabcdefabcdefabcdefabcdefabcdefabcd"],
		{ cwd: repoRoot, stdio: "inherit", timeout: 1_800_000 },
	);
	return join(root, "out");
}

class Storage implements PublishStorage {
	data = new Map<string, unknown>();
	tail: Promise<unknown> = Promise.resolve();
	async get<T>(k: string): Promise<T | undefined> { return structuredClone(this.data.get(k)) as T | undefined; }
	async put(k: string, v: unknown) { this.data.set(k, structuredClone(v)); }
	async list<T>({ prefix = "", reverse = false, limit = 1000 } = {}): Promise<Map<string, T>> {
		const entries = [...this.data].filter(([k]) => k.startsWith(prefix)).sort(([a], [b]) => a.localeCompare(b));
		if (reverse) entries.reverse();
		return new Map(entries.slice(0, limit)) as Map<string, T>;
	}
	async transaction<T>(fn: (tx: Storage) => Promise<T>) {
		const next = this.tail.then(() => fn(this));
		this.tail = next.catch(() => {});
		return next;
	}
}

const sealed = (release: ParityRelease) => {
	release.id = "";
	release.id = sha(JSON.stringify(release));
	return release;
};

const principals = {
	anonymous: null,
	stranger: { key: "email:stranger@example.test", name: "Stranger", email: "stranger@example.test" },
	member: { key: "email:member@example.test", name: "Member", email: "member@example.test" },
	editor: { key: "email:editor@example.test", name: "Editor", email: "editor@example.test" },
	super: { key: "email:super@example.test", name: "Super", email: "super@example.test" },
};

type Actor = keyof typeof principals;
let worldPromise: Promise<Awaited<ReturnType<typeof buildWorld>>> | undefined;
const world = () => (worldPromise ??= buildWorld());

async function buildWorld() {
	const dir = releaseInput();
	const base: ParityRelease = JSON.parse(readFileSync(join(dir, "release.json"), "utf8"));
	const blobs = new Map<string, Uint8Array>();
	for (const hash of readdirSync(join(dir, "objects"))) blobs.set(`objects/${hash}`, readFileSync(join(dir, "objects", hash)));

	// The live release: the builder's output plus an editors-only item and two former slugs.
	const live = structuredClone(base);
	const story = live.items.find((i) => i.kind === "report" && i.audience === "public" && i.state === "published")!;
	const page = live.items.find((i) => i.kind === "page" && i.audience === "public")!;
	story.aliases = ["former-slug", "older-slug"];
	page.aliases = ["former-page"];
	live.items.push({
		...structuredClone(story),
		id: "editors-note", slug: "editors-note", route: "/stories/editors-note", audience: "editors", aliases: [],
		title: "Editors only note", source: "items/editors-note/report.rmd",
		card: '<article class="row">Editors only note</article>', lead: '<article class="lead">Editors only note</article>', stat: "",
	});
	sealed(live);
	// An older release that was live once: one story fewer, another title.
	const older = structuredClone(base);
	older.items = older.items.filter((i) => i.id !== "small-samples");
	older.site.title = "RichView News (earlier edition)";
	older.commit = "1".repeat(40);
	sealed(older);
	// A release that was stored but never activated.
	const staged = structuredClone(base);
	staged.commit = "2".repeat(40);
	staged.site.title = "RichView News (staged)";
	sealed(staged);
	for (const r of [live, older, staged]) blobs.set(`releases/${r.id}.json`, enc.encode(JSON.stringify(r)));

	// A kept runtime from before the builder's rename; the objects exist in R2.
	const jsHash = sha("old runtime js");
	const wasmHash = sha("old runtime wasm");
	blobs.set(`objects/${jsHash}`, enc.encode("old runtime js"));
	blobs.set(`objects/${wasmHash}`, enc.encode("old runtime wasm"));
	const oldRuntimeId = "d".repeat(64);
	const liveRuntimeId = sha(JSON.stringify(live.runtime));

	const hidden: Record<string, HiddenItem> = {};
	const hide = (id: string, reason: string) => (hidden[id] = { revision: `rev-${id}`, actor: "email:super@example.test", reason, at: "2026-10-20T10:00:00.000Z", reconciliation: "pending" });
	const publicStories = live.items.filter((i) => i.kind === "report" && i.audience === "public" && i.state === "published" && i.id !== story.id);
	hide(publicStories[0].id, "legal");
	hide(publicStories[1].id, "factual error");
	const policies: Record<string, Policy> = Object.fromEntries(live.items.map((i) => [i.id, { id: i.id, section: i.section, state: i.state, audience: i.audience }]));
	policies[publicStories[2].id].deleted = true; // a deleted-policy item that is still in the manifest
	policies[publicStories[3].id].audience = "members"; // narrowed after publication
	policies["removed-story"] = { id: "removed-story", section: "research", state: "published", audience: "public", deleted: true };
	const roles = {
		[principals.member.key]: { name: "reader", sections: [], audiences: ["members"] },
		[principals.editor.key]: { name: "editor", sections: ["*"], audiences: ["members", "editors"], github_login: "editor-gh" },
	};
	const ledger = () => ({
		generation: 7, active: live.id, paused: false, hidden, policies, roles, edits: {}, approvals: {},
		authorized: { [live.commit]: { actor: "email:boss@example.test", pull: 3, head: live.commit, base: older.commit } },
		builds: { [live.commit]: { state: "live", release: live.id }, [older.commit]: { state: "live", release: older.id } },
		auditSequence: 3, accessRevision: 5,
	});
	const seed = () => {
		const storage = new Storage();
		storage.data.set("state", structuredClone(ledger()));
		storage.data.set(`activated:${older.id}`, { commit: older.commit });
		storage.data.set(`activated:${live.id}`, { commit: live.commit });
		storage.data.set(`runtime:${liveRuntimeId}`, structuredClone(live.runtime));
		storage.data.set(`runtime:${oldRuntimeId}`, { "richview_web.js": jsHash, "richview_web_bg.wasm": wasmHash });
		for (const [n, action] of [[1, "release.activated"], [2, "item.hidden"], [3, "item.hidden"]])
			storage.data.set(`audit:${String(n).padStart(10, "0")}`, { at: "2026-10-20T10:00:00.000Z", actor: "email:super@example.test", action, id: "x" });
		return storage;
	};
	const bucket = {
		get: async (k: string) => {
			const bytes = blobs.get(k);
			if (!bytes) return null;
			return {
				body: bytes,
				text: async () => dec.decode(bytes),
				json: async () => JSON.parse(dec.decode(bytes)),
				arrayBuffer: async () => bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength),
			};
		},
		head: async (k: string) => (blobs.has(k) ? { size: blobs.get(k)!.byteLength, customMetadata: { sha256: k.slice(8) } } : null),
		put: async () => { throw new Error("a reader route wrote to R2"); },
	};
	const env = {
		PUBLICATION_HOST: HOST, BOOTSTRAP_SUPER_EDITOR: principals.super.key, PUBLISHER_IDENTITY: "service:publisher",
		GITHUB_REPOSITORY: "o/r", PUBLICATION_BRANCH: "publish/news", CONTENT_ROOT: "publications/news", GITHUB_TOKEN: "unused",
		RELEASES: bucket,
	};
	const oldStorage = seed();
	const newStorage = seed();
	const { Publication } = await oldWorker();
	const wasm = await import("../pkg/richview_publish_server.js");
	wasm.initSync({ module: readFileSync(join(publishDir, "pkg/richview_publish_server_bg.wasm")) });
	const old = new Publication({ storage: oldStorage }, env);
	const rust = new PublicationBase({ storage: newStorage }, env, wasm);
	return { live, older, staged, oldRuntimeId, liveRuntimeId, old, rust, oldStorage, newStorage, seedState: ledger(), hidden, policies, roles, blobs };
}

/** Runs `fn` with the network cut: a reader route must not leave the process. */
async function offline<T>(fn: () => Promise<T>): Promise<T> {
	const real = globalThis.fetch;
	globalThis.fetch = () => { throw new Error("network use during an offline parity run"); };
	try {
		return await fn();
	} finally {
		globalThis.fetch = real;
	}
}

const pull = async (object: Reader, path: string, principal: Principal | null) => {
	const response = await object.fetch(new Request(`https://${HOST}${path}`, { headers: { "x-publish-identity": JSON.stringify(principal) } }));
	return {
		status: response.status,
		location: response.headers.get("location"),
		type: response.headers.get("content-type"),
		csp: response.headers.get("content-security-policy"),
		robots: response.headers.get("x-robots-tag"),
		disposition: response.headers.get("content-disposition"),
		sniff: response.headers.get("x-content-type-options"),
		body: Buffer.from(await response.arrayBuffer()),
	};
};

const accepted = new Map<string, string>();
const accept = (code: string, path: string, who: Actor, detail: string) => accepted.set(`${code} ${path} ${who}`, detail);

/** The one place where a difference is excused; returns the pair to compare. */
const canonical = (value: unknown): unknown =>
	Array.isArray(value) ? value.map(canonical) : value && typeof value === "object" ? Object.fromEntries(Object.keys(value).sort().map((k) => [k, canonical((value as Record<string, unknown>)[k])])) : value;
type ReadResult = Awaited<ReturnType<typeof pull>>;
type NormalizedRead = Omit<ReadResult, "body"> & { body: Buffer | string };
function normalize(path: string, who: Actor, a: ReadResult, b: ReadResult): [NormalizedRead, NormalizedRead] {
	if (path === "/robots.txt" && /^text\/plain/.test(a.type ?? "") && /^text\/plain/.test(b.type ?? "") && a.status === b.status) {
		const y = dec.decode(b.body).replace(/^Allow: \/_(?:content|releases)\/\*[^\n]*og\.png\n/gm, "");
		if (y !== dec.decode(b.body)) accept("A10", path, who, "og.png Allow lines");
		return [a, { ...b, body: Buffer.from(y) }];
	}
	if (a.status >= 300 && a.status < 400 && a.body.length === 0 && b.body.length === 0) {
		if (a.type !== b.type) accept("A6", path, who, "redirect content-type");
		return [{ ...a, type: null }, { ...b, type: null }];
	}
	if (/^application\/json/.test(a.type ?? "") && /^application\/json/.test(b.type ?? "") && a.status === b.status) {
		const [x, y] = [a, b].map((r) => JSON.stringify(canonical(JSON.parse(dec.decode(r.body)))));
		if (a.body.compare(b.body) !== 0 && x === y) accept("A7", path, who, "json key order");
		return [{ ...a, body: x }, { ...b, body: y }];
	}
	if (/^text\/html/.test(a.type ?? "") && /^text\/html/.test(b.type ?? "") && a.status === b.status) {
		const slot = "<!--PUBLISH:SOCIAL-->";
		const tags = /<meta property="og:type"[^>]*>(?:<meta (?:property|name)="(?:og|twitter):[a-z:]+"[^>]*>)*?(?=<meta property="og:url")/;
		const cut = (body: Buffer) => dec.decode(body).replace(slot, "").replace(tags, "");
		let [x, y] = [cut(a.body), cut(b.body)];
		if (dec.decode(a.body) !== x || dec.decode(b.body) !== y) accept("A9", path, who, "link-preview tags");
		// A11: the embed style names the renderer's metric class, which 8c5e4f9 renamed from rv-big.
		const renamed = x.replace(/\.rv-big(__[a-z]+)?\{/g, ".rv-metric$1{");
		if (renamed !== x) accept("A11", path, who, "embed style metric class");
		x = renamed;
		// A12: the embed width rule also names a chart inside its `.rv-keep` box.
		const boxed = y.replace(",.rv-report__body>.rv-keep>:is(.rv-chart,.rv-image,.rv-embed)", "");
		if (boxed !== y) accept("A12", path, who, "embed width rule for boxed charts");
		y = boxed;
		return [{ ...a, body: Buffer.from(x) }, { ...b, body: Buffer.from(y) }];
	}
	return [a, b];
}

async function compare(w: Awaited<ReturnType<typeof buildWorld>>, path: string, who: Actor, extra = {}) {
	const principal = principals[who];
	const [a, b] = await Promise.all([pull(w.old, path, principal), pull(w.rust, path, principal)]);
	const [x, y] = normalize(path, who, a, b);
	const problems = [];
	for (const field of ["status", "location", "type", "csp", "robots", "disposition", "sniff"] as const)
		if (x[field] !== y[field]) problems.push(`${field}: PR70 ${JSON.stringify(x[field])} vs Rust ${JSON.stringify(y[field])}`);
	const same = Buffer.isBuffer(x.body) ? x.body.compare(y.body as Buffer) === 0 : x.body === y.body;
	if (!same) problems.push(`body differs (PR70 ${a.body.length} bytes, Rust ${b.body.length} bytes): ${firstDifference(Buffer.from(x.body), Buffer.from(y.body))}`);
	return { path, who, a, b, problems, ...extra };
}
function firstDifference(a: Buffer, b: Buffer) {
	const x = a.toString("utf8");
	const y = b.toString("utf8");
	let i = 0;
	while (i < x.length && x[i] === y[i]) i++;
	return `at ${i}: PR70 …${JSON.stringify(x.slice(Math.max(0, i - 30), i + 60))} Rust …${JSON.stringify(y.slice(Math.max(0, i - 30), i + 60))}`;
}

async function replay(paths: string[], who = Object.keys(principals) as Actor[]) {
	const w = await world();
	const failures: string[] = [];
	let count = 0;
	const tally: Record<number, number> = {};
	await offline(async () => {
		for (const path of paths)
			for (const name of who) {
				const result = await compare(w, path, name);
				count++;
				tally[result.a.status] = (tally[result.a.status] ?? 0) + 1;
				if (result.problems.length) failures.push(`${name} GET ${path}\n    ${result.problems.join("\n    ")}`);
			}
	});
	assert.deepEqual(failures, [], `${failures.length} of ${count} replays differ:\n${failures.join("\n")}`);
	return tally;
}

const itemPaths = (release: ParityRelease) =>
	release.items.flatMap((i) => [
		i.route,
		`/r/${i.id}`,
		...i.aliases.map((a) => (i.kind === "page" ? `/${a}` : `/stories/${a}`)),
		`/_content/${i.id}/report.html`,
		`/_content/${i.id}/report.html?embed=1`,
		...Object.keys(i.assets).map((name) => `/_content/${i.id}/${name}`),
		`/_content/${i.id}/missing.png`,
	]);

test("PR #70's ledger is converted on the first request and nothing is lost", { skip }, async () => {
	const w = await world();
	assert.equal((await w.newStorage.get<ConvertedLedger>("state"))!.revision, undefined, "seeded in PR #70's format");
	assert.equal(await w.newStorage.get("state-pr70"), undefined);
	const response = await pull(w.rust, "/_status", null);
	assert.equal(response.status, 200);
	const state = (await w.newStorage.get<ConvertedLedger>("state"))!;
	assert.equal(state.revision, 0);
	assert.equal(state.generation, 7);
	assert.equal(state.active, w.live.id);
	assert.equal(state.audit_sequence, 3);
	assert.equal(state.access_revision, 5);
	assert.deepEqual(state.hidden, w.hidden, "the hide ledger is intact");
	assert.deepEqual(state.roles, w.roles, "the role mapping is intact");
	// A8: the converted floors spell out `deleted: false`; nothing else about them changes.
	assert.deepEqual(state.policies, Object.fromEntries(Object.entries(w.policies).map(([id, p]) => [id, { deleted: false, ...p }])), "floors, including the deleted and narrowed ones, are intact");
	assert.deepEqual(Object.keys(state.activated).sort(), [w.live.id, w.older.id].sort());
	assert.equal(state.activated[w.older.id], w.older.commit);
	assert.deepEqual(state.runtimes[w.oldRuntimeId], { "richview_web.js": sha("old runtime js"), "richview_web_bg.wasm": sha("old runtime wasm") });
	assert.deepEqual(await w.newStorage.get("state-pr70"), w.seedState, "the old record is kept byte for byte");
	assert.deepEqual(await w.oldStorage.get("state"), w.seedState, "PR #70's own storage is untouched by reads");
	// Once converted, a second request does not convert again.
	await pull(w.rust, "/_status", null);
	assert.deepEqual(await w.newStorage.get("state-pr70"), w.seedState);
	assert.equal((await w.newStorage.get<ConvertedLedger>("state"))!.revision, 0);
});

test("pages: home, archive, search, sections and paging answer alike", { skip }, async () => {
	const w = await world();
	const word = w.live.items.find((i) => i.id === "measurement-gap")!.title.split(" ")[0];
	const paths = [
		"/", "/?page=2", "/?page=0", "/?page=x", "/archive", "/archive?page=2", "/search", "/search?q=", `/search?q=${encodeURIComponent(word)}`,
		"/search?q=zzzz-no-match", "/search?q=%3Cb%3E%22x%22", "/search?q=" + "a".repeat(260), "/search?q=Th%C3%A9&page=2",
		...w.live.site.sections.map((s) => `/sections/${s.id}`), "/sections/nothing", "/sections/", "/_reading.js", "/robots.txt",
	];
	const tally = await replay(paths);
	assert.ok(tally[200] > 20 && tally[404] > 0, JSON.stringify(tally));
});

test("items: routes, short links, former slugs, content frames and assets answer alike", { skip }, async () => {
	const w = await world();
	const paths = [...itemPaths(w.live), "/stories/none", "/stories/", "/r/none", "/_content/none/report.html", "/about/", "/about"];
	assert.ok(paths.length > 40);
	const tally = await replay(paths);
	assert.ok(tally[200] > 50 && tally[308] > 20 && tally[404] > 20, JSON.stringify(tally));
});

test("feeds, status, sitemap and search index answer alike", { skip }, async () => {
	await replay(["/feed.xml", "/sitemap.xml", "/search.json", "/robots.txt", "/_status"]);
	// Identity routes: status and the member sign-in redirect only (A5).
	const w = await world();
	await offline(async () => {
		for (const [path, who] of [["/_auth/session", "anonymous"], ["/_auth/session", "member"], ["/_auth/login", "anonymous"], ["/_auth/login", "member"]] as const) {
			const [a, b] = await Promise.all([pull(w.old, path, principals[who]), pull(w.rust, path, principals[who])]);
			assert.equal(a.status, b.status, `${who} ${path}`);
			assert.equal(a.location, b.location, `${who} ${path}`);
		}
	});
});

test("hidden, floor-narrowed, deleted-floor, members-only and editors-only items are withheld alike", { skip }, async () => {
	const w = await world();
	const hiddenIds = Object.keys(w.hidden);
	const floors = Object.values(w.policies).filter((p) => p.deleted || p.audience !== w.live.items.find((i) => i.id === p.id)?.audience).map((p) => p.id);
	const ids = [...new Set([...hiddenIds, ...floors, "member-notebook", "editors-note", "desk-draft", "withdrawn-example"])];
	assert.ok(ids.length >= 7);
	const routes = ids.flatMap((id) => {
		const item = w.live.items.find((i) => i.id === id);
		return item ? [item.route, `/r/${id}`, `/_content/${id}/report.html`] : [];
	});
	await replay(routes);
	// Sanity: the replay is meaningful, because the editor sees what the anonymous reader does not.
	const [anon, editor] = await offline(() => Promise.all([pull(w.rust, "/stories/editors-note", null), pull(w.rust, "/stories/editors-note", principals.editor)]));
	assert.equal(anon.status, 404);
	assert.equal(editor.status, 200);
	const [hiddenForEditor] = await offline(() => Promise.all([pull(w.rust, `/stories/${hiddenIds[0]}`, principals.editor)]));
	assert.equal(hiddenForEditor.status, 404, "an urgent hide withdraws an item from every reader, editors included");
});

test("old releases, previews, the member edition and kept runtimes answer alike", { skip }, async () => {
	const w = await world();
	const olderRoutes = ["/", "/archive", "/feed.xml", "/search.json", "/_status", ...itemPaths(w.older).slice(0, 12)].map((p) => `/_releases/${w.older.id}${p === "/" ? "/" : p}`);
	const paths = [
		...olderRoutes,
		`/_releases/${w.live.id}/`, `/_releases/${w.live.id}/stories/measurement-gap`,
		`/_releases/${w.staged.id}/`, `/_releases/${w.staged.id}/stories/measurement-gap`, // stored, never live: 404
		`/_releases/${"0".repeat(64)}/`, "/_releases/short/", "/_releases/",
		`/_preview/${w.staged.id}/`, `/_preview/${w.staged.id}/stories/measurement-gap`, `/_preview/${w.live.id}/stories/editors-note`,
		`/_preview/${w.live.id}/_content/measurement-gap/report.html`, "/_preview/short/",
		"/internal", "/internal/", "/internal/stories/measurement-gap", "/internal/archive", "/internal/stories/member-notebook",
		"/internal/stories/editors-note", "/internal/feed.xml", "/internal/r/measurement-gap",
		`/_runtime/${w.oldRuntimeId}/richview_web.js`, `/_runtime/${w.oldRuntimeId}/richview_web_bg.wasm`, `/_runtime/${w.oldRuntimeId}/other.js`,
		`/_runtime/${w.oldRuntimeId}/richview_web.js/extra`, "/_runtime/unknown/richview_web.js",
	];
	const tally = await replay(paths);
	assert.ok(tally[200] > 20 && tally[401] > 0 && tally[404] > 20, JSON.stringify(tally));
});

test("accepted difference A2: the builder's runtime names are served by Rust only", { skip }, async () => {
	const w = await world();
	const path = `/_runtime/${w.liveRuntimeId}/richview_runtime.js`;
	const [a, b] = await offline(() => Promise.all([pull(w.old, path, null), pull(w.rust, path, null)]));
	assert.equal(a.status, 404, "PR #70 allowed only richview_web.*");
	assert.equal(b.status, 200);
	assert.equal(b.type, "text/javascript; charset=utf-8");
	const wasm = await offline(() => pull(w.rust, `/_runtime/${w.liveRuntimeId}/richview_runtime_bg.wasm`, null));
	assert.equal(wasm.type, "application/wasm");
	// PR #70's answer for the same file under its own names is the kept runtime above; nothing else is accepted.
	accept("A2", path, "anonymous", "runtime name");
});

test("every accepted difference that was listed still occurs", { skip }, async () => {
	// A1, A4 and A5 are scope notes; A8 is asserted in the migration test; failure pages are compared byte for byte.
	const codes = new Set([...accepted.keys()].map((k) => k.split(" ")[0]));
	for (const code of ["A2", "A6", "A7", "A9", "A10", "A11", "A12"]) assert.ok(codes.has(code), `${code} is listed but no longer occurs`);
});

test("link-preview tags carry the story's own values", { skip }, async () => {
	const w = await world();
	const escape = (v: string) => v.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
	const stories = w.live.items.filter((i) => i.kind === "report" && i.audience === "public" && i.state === "published");
	assert.ok(stories.length > 0);
	let checked = 0;
	await offline(async () => {
		for (const item of stories) {
			const page = await pull(w.rust, item.route, null);
			if (page.status !== 200) continue;
			const html = dec.decode(page.body);
			const tag = (key: string) => html.match(new RegExp(`<meta (?:property|name)="${key}" content="([^"]*)">`))?.[1];
			if (tag("og:image") === undefined) continue; // no stored card for this story
			const expected = {
				"og:type": "article",
				"og:title": escape(item.title),
				"og:description": escape(item.summary),
				"og:image": `${w.live.site.base_url.replace(/\/$/, "")}/_releases/${w.live.id}/_content/${item.id}/og.png`,
				"og:image:type": "image/png",
				"og:image:width": "1200",
				"og:image:height": "630",
				"og:image:alt": escape(item.byline ? `${item.title}, by ${item.byline}` : item.title),
				"twitter:card": "summary_large_image",
			};
			for (const [key, value] of Object.entries(expected)) assert.equal(tag(key), value, `${item.route} ${key}`);
			// The advertised image is served, as a PNG, at the address the tag names.
			const image = await pull(w.rust, `/_releases/${w.live.id}/_content/${item.id}/og.png`, null);
			assert.equal(image.status, 200, item.route);
			assert.equal(image.type, "image/png");
			checked++;
		}
	});
	assert.ok(checked > 0, "no story page carried link-preview tags");
});
