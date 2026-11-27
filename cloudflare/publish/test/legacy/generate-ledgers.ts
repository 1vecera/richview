// Generates the PR #70 ledger snapshots that crates/richview-publish-server/tests/legacy_ledgers.rs migrates.
//
// The snapshots come from the real PR #70 Worker, editor, repository and policy modules, run against a fake
// GitHub and in-memory Durable Object storage, so no shape is written by hand (news 503 of 30 Oct 2026).
// One run per Worker version that changed what it stores; a snapshot is kept when it adds a record shape.
//
//   node generate-ledgers.ts prepare <dir> <commit>...    export the old modules of each commit and install
//                                                          their pinned dependencies into <dir>/<commit>
//   node generate-ledgers.ts generate <dir> <out.json>    run every prepared version, write the snapshots
//
// The old modules were deleted from the tree by 5a84d5c3 and live on in Git history only.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

import type { Principal } from "../../auth.ts";
import type { PublicationBindings } from "../../adapter.ts";

type ForgeEnv = { GITHUB_REPOSITORY: string; CONTENT_ROOT: string; PUBLICATION_BRANCH: string };
type GitRef = { ref: string; repo: { full_name: string } };
type Review = { state: string; commit_id: string; user: { login: string } };
type Pull = {
	number: number;
	html_url: string;
	user: { login: string };
	state: string;
	draft: boolean;
	changed_files: number;
	head: GitRef & { sha: string };
	base: GitRef;
	merged_at: string | null;
	merge_commit_sha: string | null;
	files: string[];
	reviews: Review[];
};
type GitTreeChange = { path: string; sha: string };
type GitTree = { base: string; changes: GitTreeChange[] };
type GitCommit = { tree: { sha: string }; parents: { sha: string }[] };
type ForgeBody = {
	content: string;
	base_tree: string;
	tree: string | GitTreeChange[];
	parents: string[];
	ref?: string;
	sha: string;
	head: string;
};
type GitFixture = {
	head: string;
	nextPull: number;
	trace: string[];
	reviewer: string | null;
	chain: Record<string, string>;
	branches: Record<string, string>;
	errors: string[];
	advance(sha: string, parent: string): void;
	checkout(sha: string): void;
	addPull(number: number, fields?: Partial<Pull>): Pull;
	pull(number: number): Pull;
	fetch(raw: string | URL | Request, init?: RequestInit): Promise<Response>;
	dispatch(raw: string | URL | Request, init?: RequestInit): Promise<Response>;
};
type LegacySnapshot = {
	version: string;
	step: string;
	state: Record<string, unknown>;
	activated: Record<string, unknown>;
	runtimes: Record<string, unknown>;
};
type LegacyPublication = {
	Publication: new (ctx: { storage: Storage }, env: PublicationBindings) => {
		fetch(request: Request): Promise<Response>;
	};
};

const MODULES = ["worker", "editor", "repository", "policy", "auth", "editor-ui"];
const NOW = "2026-10-18T09:00:00.000Z";
/** The ledger fields that map an id to a record. */
const RECORD_MAPS = ["hidden", "policies", "roles", "edits", "approvals", "authorized", "builds"];

function prepare(dir: string, commits: string[]) {
	for (const commit of commits) {
		const target = path.join(dir, commit);
		fs.rmSync(target, { recursive: true, force: true });
		fs.mkdirSync(target, { recursive: true });
		const show = (file: string) => execFileSync("git", ["show", `${commit}:cloudflare/publish/${file}`], { encoding: "buffer", maxBuffer: 1 << 26 });
		for (const name of MODULES) fs.writeFileSync(path.join(target, `${name}.mjs`), show(`${name}.mjs`));
		fs.writeFileSync(path.join(target, "package.json"), show("package.json"));
		fs.writeFileSync(path.join(target, "package-lock.json"), show("package-lock.json"));
		execFileSync("npm", ["ci", "--ignore-scripts", "--no-audit", "--no-fund"], { cwd: target, stdio: "inherit" });
	}
}

const sha = (value: string | Buffer) => crypto.createHash("sha256").update(value).digest("hex");
const commit = (n: number) => n.toString(16).repeat(40);
const head = (label: string) => sha(`head-${label}`).slice(0, 40);

const frozenClock = () => {
	const RealDate: new (...args: (string | number | Date)[]) => Date = Date;
	globalThis.Date = class extends RealDate {
		constructor(...args: (string | number | Date)[]) {
			super(...(args.length ? args : [NOW]));
		}
		static now() {
			return new RealDate(NOW).valueOf();
		}
	} as DateConstructor;
	let counter = 0;
	crypto.webcrypto.randomUUID = () => `00000000-0000-4000-8000-${String(++counter).padStart(12, "0")}`;
};

class Storage {
	data = new Map<string, unknown>();
	async get(key: string) {
		return structuredClone(this.data.get(key));
	}
	async put(key: string, value: unknown) {
		this.data.set(key, structuredClone(value));
	}
	async delete(key: string) {
		return this.data.delete(key);
	}
	async list({ prefix = "" }) {
		return new Map([...this.data].filter(([key]) => key.startsWith(prefix)));
	}
	async transaction<T>(fn: (tx: Storage) => Promise<T>) {
		// Like a Durable Object transaction, an exception leaves nothing written.
		const before = structuredClone([...this.data]);
		try {
			return await fn(this);
		} catch (e) {
			this.data = new Map(before);
			throw e;
		}
	}
}

/** The few GitHub endpoints the old repository module uses, with real branch, commit, pull and merge bookkeeping. */
function fakeGit(env: ForgeEnv) {
	const prefix = `/repos/${env.GITHUB_REPOSITORY}`;
	const root = env.CONTENT_ROOT;
	const snapshots = new Map<string, Map<string, string>>(); // commit sha -> Map(path -> text)
	const blobs = new Map<string, string>();
	const trees = new Map<string, GitTree>(); // tree sha -> base commit + changes
	const commits = new Map<string, GitCommit>(); // commit sha -> { tree, parents }
	const pulls = new Map<number, Pull>();
	const git: GitFixture = {
		head: commit(1), nextPull: 100, trace: [], reviewer: null, chain: {}, branches: {}, errors: [],
		advance, checkout, addPull, pull, fetch, dispatch,
	};
	const source = (id: string, state: string) =>
		`---\ntitle: Fixture ${id}\npublish:\n  id: ${id}\n  slug: ${id}\n  section: research\n  audience: public\n  state: ${state}\n---\nBody of ${id}.\n`;
	snapshots.set(
		git.head,
		new Map([
			[`${root}/items/story/report.rmd`, source("story", "published")],
			[`${root}/items/other/report.rmd`, source("other", "published")],
		]),
	);
	/** Moves the branch to `sha`, which keeps the files of the commit it replaces and has `parent` as its parent. */
	function advance(sha: string, parent: string) {
		snapshots.set(sha, new Map(snapshots.get(git.head)));
		git.chain[sha] = parent;
		git.head = sha;
	}
	function checkout(sha: string) {
		git.head = sha;
	}
	function addPull(number: number, fields: Partial<Pull> = {}) {
		pulls.set(number, {
			number,
			html_url: `https://example.test/pull/${number}`,
			user: { login: "author" },
			state: "open",
			draft: false,
			changed_files: 1,
			head: { sha: head(`pull-${number}`), ref: `publish-edit/${number}`, repo: { full_name: env.GITHUB_REPOSITORY } },
			base: { ref: env.PUBLICATION_BRANCH, repo: { full_name: env.GITHUB_REPOSITORY } },
			merged_at: null,
			merge_commit_sha: null,
			files: [`${root}/items/story/report.rmd`],
			reviews: [],
			...fields,
		});
		return pulls.get(number)!;
	}
	function pull(number: number) { return pulls.get(number)!; }
	async function fetch(raw: string | URL | Request, init: RequestInit = {}) {
		try {
			return await git.dispatch(raw, init);
		} catch (e) {
			git.errors.push(String((e as Error).stack ?? e).split("\n").slice(0, 3).join(" | "));
			throw e;
		}
	}
	async function dispatch(raw: string | URL | Request, init: RequestInit = {}) {
		const url = new URL(raw as string);
		assert.equal(url.hostname, "api.github.com");
		assert.ok(url.pathname.startsWith(prefix), url.pathname);
		const route = url.pathname.slice(prefix.length);
		const method = init.method ?? "GET";
		const body: ForgeBody = init.body ? JSON.parse(init.body as string) : undefined;
		git.trace.push(`${method} ${route}`);
		let value;
		let status = 200;
		const publicPull = (pull: Pull) => {
			const { files, reviews, ...rest } = pull;
			return { ...rest, changed_files: files.length };
		};
		let match: RegExpMatchArray | null;
		if (route === "") value = { private: true };
		else if (route === `/git/ref/heads/${env.PUBLICATION_BRANCH}`) value = { object: { sha: git.head } };
		else if (method === "GET" && (match = route.match(/^\/contents\/(.+)$/))) {
			const ref = url.searchParams.get("ref");
			const text = snapshots.get(ref!)?.get(decodeURIComponent(match[1]));
			if (text === undefined) {
				status = 404;
				value = { message: "Not Found" };
			} else {
				value = { type: "file", size: text.length, encoding: "base64", content: Buffer.from(text).toString("base64"), sha: sha(text).slice(0, 40) };
			}
		} else if (method === "GET" && (match = route.match(/^\/git\/commits\/([a-f0-9]{40})$/))) {
			// The numbered fixture commits form a chain; any other commit is one this fake created.
			const numbered = Object.entries(git.chain).find(([sha]) => sha === match![1]);
			value = commits.get(match[1]) ?? { tree: { sha: match[1] }, parents: numbered ? [{ sha: numbered[1] }] : [] };
		} else if (method === "POST" && route === "/git/blobs") {
			const blobSha = sha(`blob-${body.content}`).slice(0, 40);
			blobs.set(blobSha, body.content);
			value = { sha: blobSha };
		} else if (method === "POST" && route === "/git/trees") {
			const treeSha = sha(`tree-${JSON.stringify(body)}`).slice(0, 40);
			trees.set(treeSha, { base: body.base_tree, changes: body.tree as GitTreeChange[] });
			value = { sha: treeSha };
		} else if (method === "POST" && route === "/git/commits") {
			const commitSha = sha(`commit-${JSON.stringify(body)}`).slice(0, 40);
			const tree = trees.get(body.tree as string)!;
			const files = new Map(snapshots.get(body.parents[0]) ?? snapshots.get(tree.base) ?? []);
			for (const change of tree.changes) files.set(change.path, blobs.get(change.sha)!);
			snapshots.set(commitSha, files);
			commits.set(commitSha, { tree: { sha: body.tree as string }, parents: body.parents.map((sha) => ({ sha })) });
			value = { sha: commitSha };
		} else if (route === "/git/refs" || route.startsWith("/git/refs/heads/")) {
			const branch = (body.ref ?? route).replace(/^refs\/heads\//, "").replace(/^\/git\/refs\/heads\//, "");
			const number = [...pulls.values()].find((p) => p.head.ref === branch)?.number;
			if (number) pulls.get(number)!.head.sha = body.sha;
			else git.branches = { ...git.branches, [branch]: body.sha };
			value = { ref: branch, object: { sha: body.sha } };
		} else if (method === "POST" && route === "/pulls") {
			const number = git.nextPull++;
			const created = git.addPull(number, {
				head: { sha: git.branches[body.head], ref: body.head, repo: { full_name: env.GITHUB_REPOSITORY } },
				files: [...snapshots.get(git.branches[body.head])!].filter(([file, text]) => snapshots.get(git.head)!.get(file) !== text).map(([file]) => file),
			});
			value = publicPull(created);
		} else if (method === "GET" && route === "/pulls") {
			value = [...pulls.values()].filter((p) => p.state === "open").map(publicPull);
		} else if (method === "GET" && (match = route.match(/^\/pulls\/(\d+)\/files$/))) {
			value = pulls.get(Number(match[1]))!.files.map((filename) => ({ filename }));
		} else if (method === "GET" && (match = route.match(/^\/pulls\/(\d+)\/reviews$/))) {
			value = pulls.get(Number(match[1]))!.reviews;
		} else if (method === "PUT" && (match = route.match(/^\/pulls\/(\d+)\/merge$/))) {
			const pull = pulls.get(Number(match[1]))!;
			const mergeSha = sha(`merge-${pull.number}-${pull.head.sha}`).slice(0, 40);
			snapshots.set(mergeSha, snapshots.get(pull.head.sha)!);
			commits.set(mergeSha, { tree: { sha: mergeSha }, parents: [{ sha: git.head }, { sha: pull.head.sha }] });
			Object.assign(pull, { state: "closed", merged_at: NOW, merge_commit_sha: mergeSha });
			git.head = mergeSha;
			value = { merged: true, sha: mergeSha };
		} else if (method === "PATCH" && (match = route.match(/^\/pulls\/(\d+)$/))) {
			Object.assign(pulls.get(Number(match[1]))!, body);
			value = publicPull(pulls.get(Number(match[1]))!);
		} else if (method === "GET" && (match = route.match(/^\/pulls\/(\d+)$/))) {
			value = publicPull(pulls.get(Number(match[1]))!);
		} else if (method === "GET" && (match = route.match(/^\/commits\/([a-f0-9]{40})\/pulls$/))) {
			value = [...pulls.values()].filter((p) => p.merge_commit_sha === match![1]).map(publicPull);
		} else throw Error(`Unmodelled fixture Git request ${method} ${route}`);
		return Response.json(value, { status });
	}
	return git;
}

/** Records the ledger's shape: which keys each record kind has, by type, so equal shapes are not repeated. */
function shapes(state: Record<string, unknown>, storage: Storage) {
	const kind = (value: unknown) => (value === null ? "null" : Array.isArray(value) ? "array" : typeof value);
	const out = new Set<string>();
	for (const [field, value] of Object.entries(state)) {
		if (RECORD_MAPS.includes(field)) {
			for (const record of Object.values(value as Record<string, Record<string, unknown>>)) {
				const keys = Object.entries(record).map(([key, inner]) => `${key}:${kind(inner)}${key === "state" ? `=${inner}` : ""}${key === "name" ? `=${inner}` : ""}`);
				out.add(`${field}{${keys.sort().join(",")}}`);
			}
		} else out.add(`${field}:${kind(value)}${field === "paused" ? `=${value}` : ""}`);
	}
	for (const key of storage.data.keys()) out.add(`key:${key.replace(/[0-9a-f]{64}|[0-9]{10}/g, "#")}`);
	return out;
}

async function run(dir: string, version: string, snapshots: LegacySnapshot[], known: Set<string>, last: boolean) {
	const here = path.join(dir, version);
	const { Publication }: LegacyPublication = await import(pathToFileURL(path.join(here, "worker.mjs")).href);
	const owner = { key: "email:owner@example.test", name: "Owner", email: "owner@example.test" };
	const second = { key: "email:second@example.test", name: "Second", email: "second@example.test" };
	const writer = { key: "email:writer@example.test", name: "Writer", email: "writer@example.test" };
	const publisher = { key: "service:publisher", name: "Publisher", service: true };
	const objects = new Map<string, { bytes: Buffer; metadata?: Record<string, string> }>();
	const env: PublicationBindings & ForgeEnv = {
		PUBLICATION_HOST: "news.example.test",
		GITHUB_REPOSITORY: "fixture/publication",
		PUBLICATION_BRANCH: "publish/news",
		CONTENT_ROOT: "publications/news",
		PUBLISHER_IDENTITY: publisher.key,
		BOOTSTRAP_SUPER_EDITOR: owner.key,
		BOOTSTRAP_GITHUB_LOGIN: "reviewer",
		BOOTSTRAP_COMMIT: commit(1),
		GITHUB_TOKEN: "fixture-not-a-credential",
		RELEASES: {
			async get(key: string) {
				const bytes = objects.get(key)?.bytes;
				return bytes ? { json: async () => JSON.parse(bytes.toString()), arrayBuffer: async () => Uint8Array.from(bytes).buffer, text: async () => bytes.toString() } : null;
			},
			async put(key, value, options) {
				objects.set(key, { bytes: Buffer.from(value), metadata: options?.customMetadata });
			},
			async head(key: string) {
				const found = objects.get(key);
				return found ? { size: found.bytes.length, customMetadata: found.metadata } : null;
			},
		},
	};
	const git = fakeGit(env);
	globalThis.fetch = git.fetch;
	const storage = new Storage();
	const app = new Publication({ storage }, env);
	const stored: string[] = [];
	const skipped: string[] = [];

	const snapshot = (label: string, force = false) => {
		const state = storage.data.get("state") as Record<string, unknown> | undefined;
		if (!state) return;
		const seen = shapes(state, storage);
		if (!force && ![...seen].some((shape) => !known.has(shape))) return;
		for (const shape of seen) known.add(shape);
		const withPrefix = (prefix: string) => Object.fromEntries([...storage.data].filter(([key]) => key.startsWith(prefix)).map(([key, value]) => [key.slice(prefix.length), structuredClone(value)]));
		snapshots.push({ version, step: label, state: structuredClone(state), activated: withPrefix("activated:"), runtimes: withPrefix("runtime:") });
		stored.push(label);
	};
	const post = async <T = unknown>(route: string, who: Principal, body: unknown, label?: string): Promise<T | null> => {
		const json = JSON.stringify(body ?? {});
		const response = await app.fetch(
			new Request(`https://news.example.test${route}`, {
				method: "POST",
				headers: { "x-publish-identity": JSON.stringify(who), "content-type": "application/json", "content-length": String(Buffer.byteLength(json)) },
				body: json,
			}),
		);
		const text = await response.text();
		if (response.status >= 400) {
			// An early version may not have this route or rule yet; the run records what it could not do.
			skipped.push(`${label ?? route}: ${response.status} ${text.replace(/<[^>]+>/g, " ").replace(/\s+/g, " ").slice(0, 160)}`);
			return null;
		}
		snapshot(label ?? route);
		return JSON.parse(text);
	};

	const bytes = Buffer.from("<html><body>Fixture report</body></html>");
	const objectHash = sha(bytes);
	objects.set(`objects/${objectHash}`, { bytes, metadata: { sha256: objectHash } });
	const runtimeBytes = Buffer.from("/* fixture old runtime */");
	const runtimeHash = sha(runtimeBytes);
	objects.set(`objects/${runtimeHash}`, { bytes: runtimeBytes, metadata: { sha256: runtimeHash } });
	const runtimeName = fs.readFileSync(path.join(here, "worker.mjs"), "utf8").includes("richview_runtime.js") ? "richview_runtime.js" : "richview_web.js";
	const item = (id: string, extra: Record<string, unknown> = {}) => ({
		id, slug: id, summary: "Fixture", byline: "Writer", section: "research", tags: [], published: NOW, updated: NOW, state: "published",
		audience: "public", kind: "report", aliases: [], title: `Fixture ${id}`, source: `items/${id}/report.rmd`, route: `/stories/${id}`,
		article: objectHash, content: objectHash, card: "<article>Fixture</article>", lead: "<article>Fixture</article>", assets: {}, ...extra,
	});
	const stage = async (at: string, items: ReturnType<typeof item>[], label?: string) => {
		const release = {
			schema: 1, id: "", commit: at, toolchain: "pinned-fixture",
			site: { title: "News", description: "Fixture", base_url: "https://news.example.test", layout: "broadsheet", mode: "external", theme: "paper", sections: [{ id: "research", title: "Research", description: "Fixture" }], navigation: [], lead: [] },
			items, templates: { home: objectHash }, runtime: { [runtimeName]: runtimeHash },
			objects: {
				[objectHash]: { sha256: objectHash, bytes: bytes.length, content_type: "text/html; charset=utf-8" },
				[runtimeHash]: { sha256: runtimeHash, bytes: runtimeBytes.length, content_type: "text/javascript; charset=utf-8" },
			},
		};
		release.id = sha(JSON.stringify(release));
		await post("/_publish/stage", publisher, release, label ?? `stage ${at.slice(0, 7)}`);
		return release;
	};
	const activate = async (release: { id: string }, generation: number, label: string) => post("/_publish/activate", publisher, { release: release.id, generation }, label);
	const catalogue = (...extra: ReturnType<typeof item>[]) => [item("story"), item("other"), ...extra];

	// Five activations: the first through the bootstrap path, the rest through GitHub-review or editor approvals.
	const releases = [];
	let generation = 0;
	for (let n = 1; n <= 5; n++) {
		const at = commit(n);
		if (n > 1) {
			const pull = git.addPull(n, { state: "closed", merged_at: NOW, merge_commit_sha: at, head: { sha: head(`activation-${n}`), ref: `publish-edit/${n}`, repo: { full_name: env.GITHUB_REPOSITORY } } });
			pull.reviews = [{ state: "APPROVED", commit_id: pull.head.sha, user: { login: "reviewer" } }];
			git.advance(at, commit(n - 1));
		}
		const release = await stage(at, n === 5 ? catalogue() : catalogue(item("retired")));
		releases.push(release);
		if (await activate(release, generation, `activate ${n}`)) generation++;
		if (n === 1) {
			await post("/_editor/api/roles", owner, { identity: "email:reader@example.test", role: "reader", sections: ["research"], audiences: ["members"] }, "role reader");
			await post("/_editor/api/roles", owner, { identity: writer.key, role: "editor", sections: ["research"], audiences: ["*"] }, "role editor");
			await post("/_editor/api/roles", owner, { identity: second.key, role: "super editor", sections: ["*"], audiences: ["*"], github_login: "second" }, "role super editor");
			// The retired item has no source in Git, so reconciliation stays pending; the story's reconciles into a pull.
			await post("/_editor/api/hide", owner, { id: "retired", reason: "Fixture urgent withdrawal" }, "hide pending");
		}
	}
	await post("/_editor/api/hide", owner, { id: "story", reason: "Fixture legal hold" }, "hide reconciled");
	// An editor drafts a new item in the browser, then revises it on the same branch.
	const draft = (title: string) => `---\ntitle: ${title}\npublish:\n  id: draftone\n  slug: draftone\n  section: research\n  audience: public\n  state: draft\n---\nBody.\n`;
	const created = await post<{ head: string; number: number }>("/_editor/api/create", writer, { path: "items/draftone/report.rmd", revision: git.head, source: draft("Draft one") }, "editor create");
	if (created) {
		await post("/_editor/api/edit", writer, { path: "items/draftone/report.rmd", revision: created.head, pull: created.number, source: draft("Draft one, revised") }, "editor revise");
		// Approved by the owner after a protected preview build, then published and built.
		const revised = git.pull(created.number).head.sha;
		await stage(revised, catalogue(), "preview build");
		await post("/_editor/api/approve", owner, { number: created.number, head: revised }, "approve");
		const merged = await post<{ commit: string }>("/_editor/api/publish", owner, { number: created.number, head: revised }, "publish");
		if (merged) {
			const release = await stage(merged.commit, catalogue(item("draftone", { state: "draft" })), "release build");
			if (await activate(release, generation, "activate merged")) generation++;
			releases.push(release);
		}
	}
	// A Git-side pull request is imported, approved, fails, is retried, then closes unmerged.
	git.addPull(50, { user: { login: "author" } });
	await post("/_publish/sync", publisher, {}, "sync import");
	const imported = git.pull(50).head.sha;
	await stage(imported, catalogue(), "preview import");
	await post("/_editor/api/approve", owner, { number: 50, head: imported }, "approve import");
	await post("/_publish/building", publisher, { commit: commit(6) }, "building");
	await post("/_publish/failed", publisher, { commit: commit(7), reason: "Fixture build failed" }, "failed");
	await post("/_publish/failed", publisher, { commit: imported, reason: "Fixture preview failed" }, "failed preview");
	await post("/_editor/api/retry", owner, { number: 50 }, "retry");
	Object.assign(git.pull(50), { state: "closed" });
	await post("/_publish/sync", publisher, {}, "sync closed");
	// A second pull request is merged in Git and observed by the next sync.
	const mergedPull = git.addPull(51, { state: "closed", merged_at: NOW, merge_commit_sha: commit(8) });
	await post("/_publish/building", publisher, { commit: commit(8) }, "building merged");
	await stage(commit(8), catalogue(), "stage merged");
	await post("/_publish/sync", publisher, {}, "sync merged");
	assert.ok(mergedPull);
	// A different super editor restores the hidden story; the owner rolls back, then resumes publication.
	await post("/_editor/api/restore", second, { id: "story", reason: "Fixture legal hold ended" }, "restore");
	await post("/_publish/rollback", owner, { release: releases[1].id, reason: "Fixture rollback", generation }, "rollback");
	await post("/_editor/api/resume", owner, { reason: "Fixture resume" }, "resume");
	await post("/_editor/api/roles", owner, { identity: writer.key, role: "reader", sections: [], audiences: ["members"] }, "role change");
	// The newest producer's final ledger is always kept whole, whatever the older versions already showed.
	if (last) snapshot("final", true);
	return { version, stored, skipped, gitErrors: git.errors };
}

async function generate(dir: string, out: string) {
	frozenClock();
	const versions = fs.readdirSync(dir).filter((name) => /^[0-9a-f]{7,40}$/.test(name));
	// Oldest first, in the order the commits were given to `prepare`.
	const order = fs.existsSync(path.join(dir, "order.txt")) ? fs.readFileSync(path.join(dir, "order.txt"), "utf8").trim().split("\n") : versions;
	const snapshots: LegacySnapshot[] = [];
	const known = new Set<string>();
	const runs = [];
	for (const version of order) runs.push(await run(dir, version, snapshots, known, version === order.at(-1)));
	const producers = runs.map(({ version, stored, skipped }) => ({ version, stored, skipped }));
	// One snapshot per line keeps the file reviewable.
	fs.writeFileSync(out, `{"producers":${JSON.stringify(producers)},\n"snapshots":[\n${snapshots.map((s) => JSON.stringify(s)).join(",\n")}\n]}\n`);
	console.log(JSON.stringify({ snapshots: snapshots.length, runs }, null, 1));
}

const [mode, dir, ...rest] = process.argv.slice(2);
if (mode === "prepare") {
	fs.mkdirSync(dir, { recursive: true });
	fs.writeFileSync(path.join(dir, "order.txt"), `${rest.join("\n")}\n`);
	prepare(dir, rest);
} else if (mode === "generate") await generate(dir, rest[0]);
else {
	console.error("usage: generate-ledgers.ts prepare <dir> <commit>... | generate <dir> <out.json>");
	process.exit(2);
}
