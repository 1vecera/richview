// The Publish journey of the full run (DEL-76, FEA-BR-26, FEA-BR-29): a source change, a protected
// preview, approval by a second identity, the release and a public read, an urgent hide and a
// rollback that keeps the access floors.
//
// It drives the rewrite's Publish server (crates/richview-publish-server, compiled to Wasm) through
// the real Cloudflare Worker adapter (cloudflare/publish/adapter.ts), the way
// cloudflare/publish/test does, on loopback and in memory. Only the edges are doubles: the Durable
// Object's storage, the R2 bucket and the Git host (a small GitHub REST double, ported from the
// Rust desk tests). No Cloudflare account, no network.
//
// It needs the server's Wasm. The gate takes it from `--publish-wasm`, RICHVIEW_PUBLISH_WASM or the
// shared Cargo target (`$CARGO_TARGET_DIR/wasm32-unknown-unknown/release/richview_publish_server.wasm`)
// and turns it into the adapter's glue with `wasm-bindgen --target web` (a code generator over the
// built artifact; nothing is compiled). Without the Wasm, the glue tool or the adapter's `jose`
// package the journey fails and says which is missing.

import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { homedir } from "node:os";
import { npmCi } from "./lib/npm.mts";
import { dirname, join } from "node:path";
import { pathToFileURL } from "node:url";

import type { Journey } from "./lib/journey.mts";

type Item = {id: string; state: string; audience: string; body: string; title?: string; summary?: string};
type ObjectRecord = {sha256: string; bytes: number; content_type: string};
type GitAuthor = {login: string};
type Pull = {number: number; state: string; draft: boolean; changed_files: number; user: GitAuthor; html_url: string; base: {ref: string; repo: {full_name: string}}; head: {sha: string; ref: string; repo: {full_name: string}}; files: string[]; commits: {author: GitAuthor; committer: GitAuthor}[]; reviews: unknown[]};
type GitNeed = {method: string; path: string; body?: string};
type FindOptions = {flag?: string; env?: NodeJS.ProcessEnv; home?: string; cwd?: string};
type PrepareOptions = {cwd: string; out: string; wasm?: string};
type HostOptions = {cwd: string; pkg: string; bootstrapCommit: string; git: Git};
type Principal = {key: string; name: string; email?: string; service?: boolean} | null;
type PublishReply = {status: number; text: string; json?: {generation?: number; state?: string; release?: string; paused?: boolean; commit?: string; previews?: {number: number; head: string}[]}; headers: Headers};

export const HOST = "news.example.test";
export const BRANCH = "publish/news";
export const ROOT = "publications/news";
export const SOURCE = "items/story/report.rmd";
const PUBLISHER = { key: "service:publisher", name: "Publisher", service: true };
const person = (email: string, name: string) => ({ key: `email:${email}`, name, email });
export const PEOPLE = {
	owner: person("owner@example.test", "Owner"),
	ed: person("ed@example.test", "Ed One"),
	ed2: person("ed2@example.test", "Ed Two"),
	reader: person("reader@example.test", "Reader"),
};
const sha256 = (bytes: string|Buffer<ArrayBuffer>|NodeJS.ArrayBufferView<ArrayBufferLike>) => createHash("sha256").update(bytes).digest("hex");
const PNG = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==", "base64");

// ---------------------------------------------------------------------------------------------
// Pure parts: the release contract as the server hashes it.
// ---------------------------------------------------------------------------------------------

/**
 * A release manifest the server accepts, and its objects. `items` are `{id, state, audience, body}`.
 * The manifest id is the SHA-256 of the manifest serialized by the server's own field order with
 * an empty `id` (crates/richview-publish-server/src/model.rs), so the key order below is part of
 * the contract; serde fills the defaults (`theme`, `accent`, `aliases`, `corrections`, `cover`,
 * `cover_alt`, `stat`) and drops `cover_credit`, `figure` and `snapshot` when empty.
 */
export function makeRelease(commit: string, items: Item[], toolchain = "journeys") {
	const blobs = new Map<string, Uint8Array<ArrayBuffer>>();
	const objects: Record<string, ObjectRecord> = {};
	const put = (bytes: Uint8Array<ArrayBuffer>, content_type: string) => {
		const hash = sha256(bytes);
		blobs.set(hash, bytes);
		objects[hash] = { sha256: hash, bytes: bytes.length, content_type };
		return hash;
	};
	const rows = items.map((item) => {
		const page = `<!doctype html><html lang="en"><head><meta charset="utf-8"><title>${item.id}</title><!--PUBLISH:SOCIAL--></head><body><main class="story"><h1>${item.title ?? item.id}</h1><p>${item.body}</p><!--PUBLISH:MORE--></main></body></html>`;
		const article = put(Buffer.from(page), "text/html; charset=utf-8");
		const content = put(Buffer.from(`<p>${item.body}</p>`), "text/html; charset=utf-8");
		const assets = item.state === "published" && item.audience === "public" ? { "og.png": put(PNG, "image/png") } : {};
		return {
			id: item.id,
			slug: item.id,
			summary: item.summary ?? `About ${item.id}`,
			byline: "Ed One",
			section: "news",
			tags: [],
			published: "2026-10-14T09:00:00Z",
			updated: "2026-10-18T09:00:00Z",
			state: item.state,
			audience: item.audience,
			kind: "report",
			aliases: [],
			corrections: [],
			cover: null,
			cover_alt: null,
			title: item.title ?? item.id,
			source: `items/${item.id}/report.rmd`,
			route: `/stories/${item.id}`,
			article,
			content,
			card: `<a class="card" href="/stories/${item.id}">${item.title ?? item.id}</a>`,
			lead: "",
			stat: "",
			assets,
		};
	});
	const release = {
		schema: 1,
		id: "",
		commit,
		toolchain,
		site: {
			title: "News",
			description: "Journey publication",
			base_url: `https://${HOST}`,
			layout: "broadsheet",
			mode: "external",
			theme: "paper",
			sections: [{ id: "news", title: "News", description: "News" }],
			navigation: [],
			lead: [],
			accent: "#1a6dac",
		},
		items: rows,
		templates: {},
		runtime: { "richview_runtime.js": put(Buffer.from("/* rt */"), "text/javascript; charset=utf-8") },
		objects: {} as Record<string, ObjectRecord>,
	};
	release.objects = Object.fromEntries(Object.keys(objects).sort().map((k) => [k, objects[k]]));
	release.id = sha256(Buffer.from(JSON.stringify(release)));
	return { release, blobs };
}

/** A story source as the desk stores it: Rich Markdown with its publish block. */
export function storySource(state: string, body: string) {
	return `---\ntitle: "Story"\ndescription: "A story."\nversion: 1.0.0\npublish:\n  id: "story"\n  slug: "story"\n  summary: "s"\n  byline: "Ed One"\n  section: "news"\n  tags: []\n  published: "2026-10-14T09:00:00Z"\n  updated: "2026-10-18T09:00:00Z"\n  state: "${state}"\n  audience: "public"\n  kind: "report"\n---\n\n${body}\n`;
}

/** The link-preview tags of a page's head: `{title, description, image, card, type}`, absent keys omitted. */
export function previewTags(html: string) {
	const tags: Record<string, string> = {};
	for (const m of html.matchAll(/<meta\s+(?:property|name)="((?:og|twitter):[a-z:_]+)"\s+content="([^"]*)"/g)) tags[m[1]] = m[2];
	return tags;
}

// ---------------------------------------------------------------------------------------------
// Doubles: storage, the bucket and the Git host.
// ---------------------------------------------------------------------------------------------

class Storage {
	data = new Map<string, unknown>();
	async get(k: string) {
		return structuredClone(this.data.get(k));
	}
	async put(k: string, v: unknown) {
		this.data.set(k, structuredClone(v));
	}
	async list({ prefix = "", reverse = false, limit = 1000 } = {}) {
		const entries = [...this.data].filter(([k]) => k.startsWith(prefix)).sort(([a], [b]) => a.localeCompare(b));
		if (reverse) entries.reverse();
		return new Map(entries.slice(0, limit));
	}
	async transaction<Value>(fn: (storage: Storage) => Promise<Value>) {
		return fn(this);
	}
}

class Bucket {
	objects = new Map<string, {bytes: Buffer; meta?: Record<string, string>}>();
	async get(key: string) {
		const found = this.objects.get(key);
		return found ? { arrayBuffer: async () => found.bytes.buffer.slice(found.bytes.byteOffset, found.bytes.byteOffset + found.bytes.length) } : null;
	}
	async head(key: string) {
		const found = this.objects.get(key);
		return found ? { size: found.bytes.length, customMetadata: found.meta } : null;
	}
	async put(key: string, bytes: Uint8Array, options: {onlyIf?: unknown; customMetadata?: Record<string, string>} = {}) {
		if (options.onlyIf && this.objects.has(key)) return;
		this.objects.set(key, { bytes: Buffer.from(bytes), meta: options.customMetadata });
	}
}

/** A Git host with just the calls the bridge makes: one branch, open pull requests, merges and the hide's reconciling commit. */
export class Git {
	declare head: string;
	declare sources: Map<string, string>;
	declare pulls: Map<number, Pull>;
	declare parents: Map<string, string>;
	declare commits: Map<string, [string, string]>;
	declare blobs: Map<string, string>;
	declare counter: number;
	declare writes: string[];
	declare merges: Map<number, string>;

	constructor(head: string) {
		this.head = head;
		this.sources = new Map();
		this.pulls = new Map();
		this.parents = new Map();
		this.commits = new Map();
		this.blobs = new Map();
		this.counter = 0;
		this.writes = [];
		this.merges = new Map();
	}
	/** Apply the merges the host answered: the branch moves to each merge commit and its pull request closes. */
	settle() {
		for (const [number, sha] of this.merges) {
			const pull = this.pulls.get(number)!;
			for (const [key, text] of [...this.sources]) if (key.startsWith(`${pull.head.sha}|`)) this.sources.set(`${sha}|${key.split("|")[1]}`, text);
			Object.assign(pull, { state: "closed", merged_at: "2026-10-27T10:00:00Z", merge_commit_sha: sha });
			this.head = sha;
		}
		this.merges.clear();
	}
	sha() {
		this.counter += 1;
		return (0xc0ffee00 + this.counter).toString(16).padStart(40, "0");
	}
	openPull(number: number, head: string, file: string, source: string, login: string) {
		this.sources.set(`${head}|${file}`, source);
		this.pulls.set(number, {
			number, state: "open", draft: false, changed_files: 1, user: { login },
			html_url: `https://git.example.test/pull/${number}`,
			base: { ref: BRANCH, repo: { full_name: "o/r" } },
			head: { sha: head, ref: `publish-edit/${number}`, repo: { full_name: "o/r" } },
			files: [file], commits: [{ author: { login }, committer: { login } }], reviews: [],
		});
	}
	async answer(need: GitNeed) {
		const ok = (body: unknown) => ({ status: 200, body });
		const miss = { status: 404, body: null };
		const { method, path } = need;
		const body: unknown = need.body ? JSON.parse(need.body) : null;
		if (method !== "GET") this.writes.push(`${method} ${path}`);
		if (method === "GET" && path === "") return ok({ private: true });
		if (method === "GET" && path.startsWith("/git/ref/heads/")) return ok({ object: { sha: this.head } });
		let m;
		if (method === "GET" && path.startsWith("/contents/")) {
			const [file, rev] = path.slice(`/contents/${ROOT}/`.length).split("?ref=");
			const text = this.sources.get(`${rev}|${file}`) ?? this.sources.get(`${this.head}|${file}`);
			return text === undefined ? miss : ok({ type: "file", size: text.length, encoding: "base64", sha: "blob1", content: Buffer.from(text).toString("base64") });
		}
		if (method === "GET" && path.startsWith("/pulls?state=open")) return ok([...this.pulls.values()].filter((p) => p.state === "open").map((p) => ({ number: p.number })));
		if (method === "GET" && (m = path.match(/^\/pulls\/(\d+)\/files/))) return this.pulls.has(+m[1]) ? ok(this.pulls.get(+m[1])!.files.map((f) => ({ filename: `${ROOT}/${f}` }))) : miss;
		if (method === "GET" && (m = path.match(/^\/pulls\/(\d+)\/commits/))) return this.pulls.has(+m[1]) ? ok(this.pulls.get(+m[1])!.commits) : miss;
		if (method === "GET" && (m = path.match(/^\/pulls\/(\d+)\/reviews/))) return this.pulls.has(+m[1]) ? ok(this.pulls.get(+m[1])!.reviews) : miss;
		if (method === "GET" && (m = path.match(/^\/pulls\/(\d+)$/))) return this.pulls.has(+m[1]) ? ok(this.pulls.get(+m[1])) : miss;
		if (method === "GET" && (m = path.match(/^\/git\/commits\/([0-9a-f]+)$/))) return ok({ tree: { sha: "tree0" }, parents: [{ sha: this.parents.get(m[1]) ?? this.head }] });
		if (method === "PUT" && (m = path.match(/^\/pulls\/(\d+)\/merge$/))) {
			// The host answers the merge; the branch itself moves when `settle` runs, between the desk's calls,
			// so a replayed request sees the same Git it saw the first time.
			const pull = this.pulls.get(+m[1]);
			if (!pull || pull.state !== "open") return { status: 405, body: null };
			const merged = this.merges.get(+m[1]) ?? this.sha();
			this.merges.set(+m[1], merged);
			this.parents.set(merged, this.head);
			return ok({ merged: true, sha: merged });
		}
		if (method === "POST" && path === "/git/blobs") {
			const sha = this.sha();
			this.blobs.set(sha, (body as {content?: string}).content ?? "");
			return ok({ sha });
		}
		if (method === "POST" && path === "/git/trees") {
			const entry = (body as {tree: {path: string; sha: string}[]}).tree[0];
			const sha = this.sha();
			this.commits.set(sha, [entry.path.replace(`${ROOT}/`, ""), this.blobs.get(entry.sha) ?? ""]);
			return ok({ sha });
		}
		if (method === "POST" && path === "/git/commits") {
			const sha = this.sha();
			this.commits.set(sha, this.commits.get((body as {tree: string}).tree)!);
			return ok({ sha });
		}
		if (method === "POST" && path === "/git/refs") return ok({});
		if (method === "PATCH" && path.startsWith("/git/refs/heads/")) return (body as {force?: boolean}).force === false ? ok({}) : { status: 422, body: null };
		if (method === "POST" && path === "/pulls") {
			const number = this.pulls.size + 1;
			const head = [...this.commits.keys()].sort().at(-1)!;
			const [file, text] = this.commits.get(head)!;
			this.openPull(number, head, file, text, "publisher");
			return ok({ number, html_url: `https://git.example.test/pull/${number}` });
		}
		if (method === "PATCH" && (m = path.match(/^\/pulls\/(\d+)$/))) {
			if (this.pulls.has(+m[1])) this.pulls.get(+m[1])!.state = (body as {state: string}).state;
			return ok({});
		}
		return miss;
	}
}

// ---------------------------------------------------------------------------------------------
// The host: the real adapter over the Wasm.
// ---------------------------------------------------------------------------------------------

/**
 * Where the Wasm is: the flag or the environment when either names a path, else the checkout's own
 * target when it has one, else the shared Cargo target. Each is the only place tried: a gate run
 * links the checkout's target to its private copy of the build, and a Wasm missing there must not
 * be borrowed from another checkout's build. `undefined` when it is not there.
 */
export function findWasm({ flag, env = process.env, home = homedir(), cwd = process.cwd() }: FindOptions = {}) {
	const name = "wasm32-unknown-unknown/release/richview_publish_server.wasm";
	const given = flag || env.RICHVIEW_PUBLISH_WASM;
	if (given) return existsSync(given) ? given : undefined;
	if (existsSync(join(cwd, "target"))) return existsSync(join(cwd, "target", name)) ? join(cwd, "target", name) : undefined;
	return [env.CARGO_TARGET_DIR && join(env.CARGO_TARGET_DIR, name), join(home, "tmp/richview-rewrite/target", name)].filter((p): p is string => Boolean(p)).find((p) => existsSync(p));
}

/** The adapter imports `jose`; install the lockfile's copy the way the publish-tests step does. */
function ensureJose(cwd: string) {
	if (existsSync(join(cwd, "cloudflare/publish/node_modules/jose"))) return { ok: true };
	try {
		npmCi(cwd, { prefix: "cloudflare/publish", env: process.env });
		return { ok: true };
	} catch (error) {
		return { ok: false, error: `the adapter's jose package (npm ci in cloudflare/publish failed: ${(error as Error).message})` };
	}
}

/** The sources the Publish Wasm is built from, below the checkout. */
const PUBLISH_SOURCES = ["crates/richview-publish-server", "crates/richview-core", "richview-chart", "Cargo.lock", "Cargo.toml", "rust-toolchain.toml"];

/** The file beside a built Publish Wasm that records the digest of the sources it was built from. */
export const PKG_STAMP = ".built-from";

/**
 * One digest of the Publish Wasm's sources in `cwd`: each file's path and bytes, in path order.
 * Dot files are left out; editors and Finder write them and nothing compiles them.
 */
export function sourcesDigest(cwd: string) {
	const hash = createHash("sha256");
	const walk = (rel: string) => {
		const path = join(cwd, rel);
		if (!existsSync(path)) return;
		if (statSync(path).isDirectory()) {
			for (const name of readdirSync(path).sort()) if (!name.startsWith(".")) walk(join(rel, name));
			return;
		}
		const bytes = readFileSync(path);
		hash.update(`${rel}\0${bytes.length}\0`).update(bytes);
	};
	for (const source of PUBLISH_SOURCES) walk(source);
	return hash.digest("hex");
}

/** Records beside the checkout's glue which sources it was built from (publish-tests, after build:wasm). */
export function stampPkg(cwd: string, pkg = join(cwd, "cloudflare/publish/pkg")) {
	writeFileSync(join(pkg, PKG_STAMP), `${sourcesDigest(cwd)}\n`);
}

/**
 * Whether a built Publish Wasm was built from the checkout's sources. The gate's work checkout is
 * reused across runs, so a glue folder left by an earlier publish-tests run would otherwise stand in
 * for today's sources (review 1d note 10). The digest publish-tests recorded decides, not file times:
 * the Cargo shim touches every source of a checkout when another checkout built in the shared target
 * since, so during a full the times say nothing about content. Without a record, as after a
 * hand-run `npm run build:wasm`, the Wasm must be at least as new as every source.
 */
export function pkgFresh(cwd: string, wasm: string) {
	if (!existsSync(wasm)) return false;
	const stamp = join(dirname(wasm), PKG_STAMP);
	if (existsSync(stamp)) return readFileSync(stamp, "utf8").trim() === sourcesDigest(cwd);
	const built = statSync(wasm).mtimeMs;
	const newest = (path: string): number => {
		if (!existsSync(path)) return 0;
		const info = statSync(path);
		if (!info.isDirectory()) return info.mtimeMs;
		return Math.max(0, ...readdirSync(path).map((name) => newest(join(path, name))));
	};
	return PUBLISH_SOURCES.every((source) => newest(join(cwd, source)) <= built);
}

/** Prepare the adapter's glue and its `jose` package; returns `{error}` naming what is missing. */
export function prepareHost({ cwd, out, wasm: flag }: PrepareOptions): {pkg: string; source: string; error?: never} | {error: string; pkg?: never; source?: never} {
	// The publish-tests step builds the checkout's own glue; a gate run uses it unless a Wasm is named.
	const built = join(cwd, "cloudflare/publish/pkg");
	if (!flag && existsSync(join(built, "richview_publish_server.js")) && pkgFresh(cwd, join(built, "richview_publish_server_bg.wasm")) && ensureJose(cwd).ok) return { pkg: built, source: "cloudflare/publish/pkg (built from this checkout by publish-tests)" };
	const wasm = flag ?? findWasm({ cwd });
	if (!wasm) return { error: "the Publish server's Wasm (pass --publish-wasm, set RICHVIEW_PUBLISH_WASM, or build richview-publish-server for wasm32-unknown-unknown)" };
	const jose = ensureJose(cwd);
	if (!jose.ok) return { error: jose.error! };
	const pkg = join(out, "publish-pkg");
	mkdirSync(pkg, { recursive: true });
	const tool = [process.env.WASM_BINDGEN, join(homedir(), ".cargo/bin/wasm-bindgen"), "wasm-bindgen"].filter((t): t is string => Boolean(t)).find((t) => spawnSync(t, ["--version"], { encoding: "utf8" }).status === 0);
	if (!tool) return { error: "wasm-bindgen (0.2.129) to generate the adapter's glue from the Wasm" };
	const glue = spawnSync(tool, ["--target", "web", "--out-dir", pkg, wasm], { encoding: "utf8" });
	if (glue.status !== 0) return { error: `the adapter's glue (wasm-bindgen failed: ${glue.stderr.trim().slice(0, 300)})` };
	return { pkg, source: `${wasm} through wasm-bindgen` };
}

async function makeHost({ cwd, pkg, bootstrapCommit, git }: HostOptions) {
	const adapter = join(cwd, "cloudflare/publish/adapter.ts");
	const { PublicationBase } = await import(pathToFileURL(existsSync(adapter) ? adapter : join(cwd, "cloudflare/publish/adapter.mjs")).href);
	const wasm = await import(pathToFileURL(join(pkg, "richview_publish_server.js")).href);
	wasm.initSync({ module: readFileSync(join(pkg, "richview_publish_server_bg.wasm")) });
	const bucket = new Bucket();
	const env = {
		PUBLICATION_HOST: HOST,
		BOOTSTRAP_SUPER_EDITOR: PEOPLE.owner.key,
		PUBLISHER_IDENTITY: PUBLISHER.key,
		GITHUB_REPOSITORY: "o/r",
		PUBLICATION_BRANCH: BRANCH,
		CONTENT_ROOT: ROOT,
		BOOTSTRAP_COMMIT: bootstrapCommit,
		RELEASES: bucket,
	};
	const storage = new Storage();
	const object = new PublicationBase({ storage }, env, wasm);
	object.forge = (need: GitNeed) => git.answer(need);
	const call = async (principal: Principal, method: string, path: string, body?: unknown): Promise<PublishReply> => {
		const raw = body === undefined ? undefined : body instanceof Uint8Array ? body as Uint8Array<ArrayBuffer> : JSON.stringify(body);
		const headers: Record<string, string> = { "x-publish-identity": JSON.stringify(principal) };
		if (raw !== undefined) {
			headers["content-type"] = "application/json";
			headers["content-length"] = String(raw.length);
		}
		const response = await object.fetch(new Request(`https://${HOST}${path}`, { method, headers, body: raw, redirect: "manual" }));
		const text = await response.text();
		let json: PublishReply["json"];
		try {
			json = JSON.parse(text);
		} catch {
			json = undefined;
		}
		return { status: response.status, text, json, headers: response.headers };
	};
	return { call, bucket, storage };
}

// ---------------------------------------------------------------------------------------------
// The journey.
// ---------------------------------------------------------------------------------------------

export async function publishJourney(j: Journey, { cwd, out, wasm }: PrepareOptions) {
	const prepared = prepareHost({ cwd, out, wasm });
	if (!prepared.pkg) j.abort("the Publish server runs outside its test harness", "gate", "Wasm, glue and adapter present", `missing: ${prepared.error}`);
	j.info(`Publish server Wasm: ${prepared.source}`);

	const R1 = makeRelease("1".repeat(40), [
		{ id: "story", state: "published", audience: "public", title: "Story", body: "Story body v1" },
		{ id: "note", state: "published", audience: "public", title: "Note", body: "Note body v1" },
	]);
	const git = new Git(R1.release.commit);
	git.sources.set(`${R1.release.commit}|${SOURCE}`, storySource("published", "Story body v1"));
	const { call } = await makeHost({ cwd, pkg: prepared.pkg, bootstrapCommit: R1.release.commit, git });

	const upload = async (built: ReturnType<typeof makeRelease>): Promise<PublishReply | {status: 200}> => {
		for (const [hash, bytes] of built.blobs) {
			const reply = await call(PUBLISHER, "PUT", `/_publish/objects/${hash}`, bytes);
			if (reply.status !== 200) return reply;
		}
		return { status: 200 };
	};
	const stage = async (built: ReturnType<typeof makeRelease>): Promise<PublishReply> => {
		const uploaded = await upload(built);
		return uploaded.status === 200 ? call(PUBLISHER, "POST", "/_publish/stage", built.release) : uploaded as PublishReply;
	};
	const generation = async () => (await call(PUBLISHER, "GET", "/_publish/state")).json?.generation;
	const read = (who: Principal, path: string) => call(who, "GET", path);
	const said = (r: PublishReply) => `${r.status} ${r.text.replace(/<[^>]*>/g, " ").replace(/\s+/g, " ").trim().slice(0, 160)}`;
	const refused = (reply: PublishReply) => [401, 403, 404, 410].includes(reply.status);

	// 1. The first release goes live by the bootstrap commit.
	let reply = await stage(R1);
	j.need("stage the first release", "publish", reply.status === 200, "200", said(reply));
	reply = await call(PUBLISHER, "POST", "/_publish/activate", { release: R1.release.id, generation: await generation() });
	j.need("activate the first release", "publish", reply.status === 200, "200", said(reply));
	reply = await read(null, "/_status");
	j.equal("status is live on the first release", "publish", [reply.json?.state, reply.json?.release], ["live", R1.release.id]);

	// 2. A public read, with the story's link preview.
	reply = await read(null, "/stories/story");
	j.need("public read of the story", "publish", reply.status === 200 && reply.text.includes("Story body v1"), "200 with the story body", said(reply));
	let tags = previewTags(reply.text);
	j.equal("a public story advertises its title and image", "publish", [tags["og:title"], tags["og:image"], tags["twitter:card"]], ["Story", `https://${HOST}/_releases/${R1.release.id}/_content/story/og.png`, "summary_large_image"]);
	const advertisedImage = new URL(tags["og:image"]);
	reply = await read(null, advertisedImage.pathname + advertisedImage.search);
	j.assert("the advertised immutable preview image is served to an anonymous crawler", "publish", reply.status === 200 && reply.headers.get("content-type") === "image/png", "200 image/png", `${reply.status} ${reply.headers.get("content-type")}`);
	reply = await read(null, "/_content/story/og.png");
	j.assert("the preview image is served to an anonymous crawler", "publish", reply.status === 200 && reply.headers.get("content-type") === "image/png", "200 image/png", `${reply.status} ${reply.headers.get("content-type")}`);

	// 3. Roles: an editor, and a second super editor mapped to a Git login.
	// A change imported from Git names no item path, so only a super editor can review it.
	const grants: [Principal & {key: string; name: string}, string, string[], string[], string | null][] = [
		[PEOPLE.ed, "editor", ["news"], ["public"], null],
		[PEOPLE.ed2, "super editor", ["*"], ["*"], "ed2-gh"],
	];
	for (const [who, role, sections, audiences, login] of grants) {
		reply = await call(PEOPLE.owner, "POST", "/_editor/api/roles", { identity: who.key, role, sections, audiences, ...(login ? { github_login: login } : {}) });
		j.need(`the owner gives ${who.name} the ${role} role`, "publish", reply.status === 200, "200", said(reply));
	}

	// 4. A source change from outside the desk, and its protected preview.
	const H1 = "2".repeat(40);
	git.openPull(7, H1, SOURCE, storySource("published", "Story body v2"), "contributor");
	reply = await call(PUBLISHER, "POST", "/_publish/sync", {});
	j.need("the publisher observes the change", "publish", reply.status === 200 && reply.json?.previews?.some((p: { number: number; head: string; }) => p.number === 7 && p.head === H1), "a pending preview for pull 7", said(reply));
	const R2 = makeRelease(H1, [
		{ id: "story", state: "published", audience: "public", title: "Story", body: "Story body v2" },
		{ id: "note", state: "published", audience: "public", title: "Note", body: "Note body v1" },
	]);
	reply = await stage(R2);
	j.need("the preview build is staged", "publish", reply.status === 200, "200", said(reply));
	reply = await read(PEOPLE.ed2, `/_preview/${R2.release.id}/stories/story`);
	j.assert("an editor reads the protected preview", "publish", reply.status === 200 && reply.text.includes("Story body v2") && reply.text.includes("Protected preview"), "200 with v2 and the preview notice", said(reply));
	for (const [who, name] of [[null, "an anonymous reader"], [PEOPLE.reader, "a signed-in reader"]] as const) {
		reply = await read(who, `/_preview/${R2.release.id}/stories/story`);
		j.assert(`${name} is refused the preview`, "publish", refused(reply) && !reply.text.includes("Story body v2"), "refused without the draft", said(reply));
	}
	reply = await read(null, "/stories/story");
	j.assert("the live story is still v1 while the change waits", "publish", reply.text.includes("Story body v1"), "v1", reply.text.slice(0, 80));

	// 5. Approval by a second identity, bound to the exact head.
	reply = await call(PEOPLE.ed, "POST", "/_editor/api/approve", { number: 7, head: H1 });
	j.assert("an editor without a mapped Git login cannot approve an imported change", "publish", reply.status === 403, "403", said(reply));
	reply = await call(PEOPLE.ed2, "POST", "/_editor/api/approve", { number: 7, head: "3".repeat(40) });
	j.assert("an approval of a stale head is refused", "publish", reply.status === 409, "409", said(reply));
	reply = await call(PEOPLE.ed2, "POST", "/_editor/api/approve", { number: 7, head: H1 });
	git.settle();
	j.need("the second identity approves the exact head", "publish", reply.status === 200 && reply.json?.state === "building", "200 building", said(reply));
	const merged = reply.json!.commit!;
	j.equal("approving merges the change into the publication branch", "publish", git.head, merged);

	// 6. The release and the public read. The note is narrowed to members in this release.
	const R3 = makeRelease(merged, [
		{ id: "story", state: "published", audience: "public", title: "Story", body: "Story body v2" },
		{ id: "note", state: "published", audience: "members", title: "Note", body: "Note body v1" },
	]);
	reply = await stage(R3);
	j.need("the merged commit is built", "publish", reply.status === 200, "200", said(reply));
	reply = await call(PUBLISHER, "POST", "/_publish/activate", { release: R3.release.id, generation: await generation() });
	j.need("the approved commit goes live", "publish", reply.status === 200, "200", said(reply));
	reply = await read(null, "/stories/story");
	j.assert("the public read shows the approved change", "publish", reply.status === 200 && reply.text.includes("Story body v2"), "200 with v2", said(reply));
	reply = await read(null, "/stories/note");
	j.assert("the narrowed note is refused to the public", "publish", refused(reply), "refused", reply.status);

	// 7. An urgent hide.
	reply = await call(PEOPLE.ed, "POST", "/_editor/api/hide", { id: "story", reason: "legal" });
	j.assert("an editor hides the story urgently", "publish", [200, 202].includes(reply.status), "200 or 202", said(reply));
	reply = await read(null, "/stories/story");
	j.assert("the hidden story is gone for the public, with no preview tags", "publish", refused(reply) && !reply.text.includes("og:title") && !reply.text.includes("Story body"), "refused, nothing of the story", said(reply));
	reply = await read(null, "/_content/story/og.png");
	j.assert("the hidden story's preview image is gone", "publish", refused(reply), "refused", reply.status);
	for (const release of [R1.release.id, R3.release.id]) {
		reply = await read(null, `/_releases/${release}/_content/story/og.png`);
		j.assert(`the hidden story's immutable preview is gone in ${release}`, "publish", refused(reply), "refused", reply.status);
	}

	// 8. Rollback keeps the access floors.
	reply = await call(PEOPLE.owner, "POST", "/_publish/rollback", { release: R1.release.id, generation: await generation(), reason: "journey drill" });
	j.need("a super editor rolls back to the first release", "publish", reply.status === 200, "200", said(reply));
	reply = await read(null, "/_status");
	j.equal("the first release is live again and publication is paused", "publish", [reply.json?.release, reply.json?.paused], [R1.release.id, true]);
	reply = await read(null, "/stories/story");
	j.assert("the urgent hide still applies to the old release", "publish", refused(reply) && !reply.text.includes("Story body"), "refused", said(reply));
	reply = await read(null, "/stories/note");
	j.assert("the narrower audience still applies to the old release", "publish", refused(reply) && !reply.text.includes("Note body"), "refused", said(reply));
	reply = await read(null, advertisedImage.pathname + advertisedImage.search);
	j.assert("rollback does not restore the hidden immutable preview", "publish", refused(reply), "refused", reply.status);
	reply = await call(PUBLISHER, "POST", "/_publish/activate", { release: R3.release.id, generation: await generation() });
	j.assert("automatic activation waits for an explicit resume", "publish", reply.status === 409, "409", said(reply));
}
