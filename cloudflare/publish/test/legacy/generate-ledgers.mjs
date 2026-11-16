// Generates the PR #70 ledger snapshots that crates/richview-publish-server/tests/legacy_ledgers.rs migrates.
//
// The snapshots come from the real PR #70 Worker, editor, repository and policy modules, run against a fake
// GitHub and in-memory Durable Object storage, so no shape is written by hand (S27-655, news 503 of 30 Oct 2026).
// One run per Worker version that changed what it stores; a snapshot is kept when it adds a record shape.
//
//   node generate-ledgers.mjs prepare <dir> <commit>...    export the old modules of each commit and install
//                                                          their pinned dependencies into <dir>/<commit>
//   node generate-ledgers.mjs generate <dir> <out.json>    run every prepared version, write the snapshots
//
// The old modules were deleted from the tree by 5a84d5c3 and live on in Git history only.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

const MODULES = ["worker", "editor", "repository", "policy", "auth", "editor-ui"];
const NOW = "2026-10-18T09:00:00.000Z";
/** The ledger fields that map an id to a record. */
const RECORD_MAPS = ["hidden", "policies", "roles", "edits", "approvals", "authorized", "builds"];

function prepare(dir, commits) {
	for (const commit of commits) {
		const target = path.join(dir, commit);
		fs.rmSync(target, { recursive: true, force: true });
		fs.mkdirSync(target, { recursive: true });
		const show = (file) => execFileSync("git", ["show", `${commit}:cloudflare/publish/${file}`], { encoding: "buffer", maxBuffer: 1 << 26 });
		for (const name of MODULES) fs.writeFileSync(path.join(target, `${name}.mjs`), show(`${name}.mjs`));
		fs.writeFileSync(path.join(target, "package.json"), show("package.json"));
		fs.writeFileSync(path.join(target, "package-lock.json"), show("package-lock.json"));
		execFileSync("npm", ["ci", "--ignore-scripts", "--no-audit", "--no-fund"], { cwd: target, stdio: "inherit" });
	}
}

const sha = (value) => crypto.createHash("sha256").update(value).digest("hex");
const commit = (n) => n.toString(16).repeat(40);
const head = (label) => sha(`head-${label}`).slice(0, 40);

const frozenClock = () => {
	const RealDate = Date;
	globalThis.Date = class extends RealDate {
		constructor(...args) {
			super(...(args.length ? args : [NOW]));
		}
		static now() {
			return new RealDate(NOW).valueOf();
		}
	};
	let counter = 0;
	crypto.webcrypto.randomUUID = () => `00000000-0000-4000-8000-${String(++counter).padStart(12, "0")}`;
};

class Storage {
	data = new Map();
	async get(key) {
		return structuredClone(this.data.get(key));
	}
	async put(key, value) {
		this.data.set(key, structuredClone(value));
	}
	async delete(key) {
		return this.data.delete(key);
	}
	async list({ prefix = "" }) {
		return new Map([...this.data].filter(([key]) => key.startsWith(prefix)));
	}
	async transaction(fn) {
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
function fakeGit(env) {
	const prefix = `/repos/${env.GITHUB_REPOSITORY}`;
	const root = env.CONTENT_ROOT;
	const snapshots = new Map(); // commit sha -> Map(path -> text)
	const blobs = new Map();
	const trees = new Map(); // tree sha -> base commit + changes
	const commits = new Map(); // commit sha -> { tree, parents }
	const pulls = new Map();
	const git = { head: commit(1), nextPull: 100, trace: [], reviewer: null };
	const source = (id, state) =>
		`---\ntitle: Fixture ${id}\npublish:\n  id: ${id}\n  slug: ${id}\n  section: research\n  audience: public\n  state: ${state}\n---\nBody of ${id}.\n`;
	snapshots.set(
		git.head,
		new Map([
			[`${root}/items/story/report.rmd`, source("story", "published")],
			[`${root}/items/other/report.rmd`, source("other", "published")],
		]),
	);
	/** Moves the branch to `sha`, which keeps the files of the commit it replaces and has `parent` as its parent. */
	git.chain = {};
	git.advance = (sha, parent) => {
		snapshots.set(sha, new Map(snapshots.get(git.head)));
		git.chain[sha] = parent;
		git.head = sha;
	};
	git.checkout = (sha) => {
		git.head = sha;
	};
	git.addPull = (number, fields = {}) => {
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
		return pulls.get(number);
	};
	git.pull = (number) => pulls.get(number);
	git.fetch = async (raw, init = {}) => {
		try {
			return await git.dispatch(raw, init);
		} catch (e) {
			git.errors.push(String(e.stack ?? e).split("\n").slice(0, 3).join(" | "));
			throw e;
		}
	};
	git.errors = [];
	git.dispatch = async (raw, init = {}) => {
		const url = new URL(raw);
		assert.equal(url.hostname, "api.github.com");
		assert.ok(url.pathname.startsWith(prefix), url.pathname);
		const route = url.pathname.slice(prefix.length);
		const method = init.method ?? "GET";
		const body = init.body ? JSON.parse(init.body) : undefined;
		git.trace.push(`${method} ${route}`);
		let value;
		let status = 200;
		const publicPull = (pull) => {
			const { files, reviews, ...rest } = pull;
			return { ...rest, changed_files: files.length };
		};
		let match;
		if (route === "") value = { private: true };
		else if (route === `/git/ref/heads/${env.PUBLICATION_BRANCH}`) value = { object: { sha: git.head } };
		else if (method === "GET" && (match = route.match(/^\/contents\/(.+)$/))) {
			const ref = url.searchParams.get("ref");
			const text = snapshots.get(ref)?.get(decodeURIComponent(match[1]));
			if (text === undefined) {
				status = 404;
				value = { message: "Not Found" };
			} else {
				value = { type: "file", size: text.length, encoding: "base64", content: Buffer.from(text).toString("base64"), sha: sha(text).slice(0, 40) };
			}
		} else if (method === "GET" && (match = route.match(/^\/git\/commits\/([a-f0-9]{40})$/))) {
			// The numbered fixture commits form a chain; any other commit is one this fake created.
			const numbered = Object.entries(git.chain).find(([sha]) => sha === match[1]);
			value = commits.get(match[1]) ?? { tree: { sha: match[1] }, parents: numbered ? [{ sha: numbered[1] }] : [] };
		} else if (method === "POST" && route === "/git/blobs") {
			const blobSha = sha(`blob-${body.content}`).slice(0, 40);
			blobs.set(blobSha, body.content);
			value = { sha: blobSha };
		} else if (method === "POST" && route === "/git/trees") {
			const treeSha = sha(`tree-${JSON.stringify(body)}`).slice(0, 40);
			trees.set(treeSha, { base: body.base_tree, changes: body.tree });
			value = { sha: treeSha };
		} else if (method === "POST" && route === "/git/commits") {
			const commitSha = sha(`commit-${JSON.stringify(body)}`).slice(0, 40);
			const tree = trees.get(body.tree);
			const files = new Map(snapshots.get(body.parents[0]) ?? snapshots.get(tree.base) ?? []);
			for (const change of tree.changes) files.set(change.path, blobs.get(change.sha));
			snapshots.set(commitSha, files);
			commits.set(commitSha, { tree: { sha: body.tree }, parents: body.parents.map((sha) => ({ sha })) });
			value = { sha: commitSha };
		} else if (route === "/git/refs" || route.startsWith("/git/refs/heads/")) {
			const branch = (body.ref ?? route).replace(/^refs\/heads\//, "").replace(/^\/git\/refs\/heads\//, "");
			const number = [...pulls.values()].find((p) => p.head.ref === branch)?.number;
			if (number) pulls.get(number).head.sha = body.sha;
			else git.branches = { ...git.branches, [branch]: body.sha };
			value = { ref: branch, object: { sha: body.sha } };
		} else if (method === "POST" && route === "/pulls") {
			const number = git.nextPull++;
			const created = git.addPull(number, {
				head: { sha: git.branches[body.head], ref: body.head, repo: { full_name: env.GITHUB_REPOSITORY } },
				files: [...snapshots.get(git.branches[body.head])].filter(([file, text]) => snapshots.get(git.head).get(file) !== text).map(([file]) => file),
			});
			value = publicPull(created);
		} else if (method === "GET" && route === "/pulls") {
			value = [...pulls.values()].filter((p) => p.state === "open").map(publicPull);
		} else if (method === "GET" && (match = route.match(/^\/pulls\/(\d+)\/files$/))) {
			value = pulls.get(Number(match[1])).files.map((filename) => ({ filename }));
		} else if (method === "GET" && (match = route.match(/^\/pulls\/(\d+)\/reviews$/))) {
			value = pulls.get(Number(match[1])).reviews;
		} else if (method === "PUT" && (match = route.match(/^\/pulls\/(\d+)\/merge$/))) {
			const pull = pulls.get(Number(match[1]));
			const mergeSha = sha(`merge-${pull.number}-${pull.head.sha}`).slice(0, 40);
			snapshots.set(mergeSha, snapshots.get(pull.head.sha));
			commits.set(mergeSha, { tree: { sha: mergeSha }, parents: [{ sha: git.head }, { sha: pull.head.sha }] });
			Object.assign(pull, { state: "closed", merged_at: NOW, merge_commit_sha: mergeSha });
			git.head = mergeSha;
			value = { merged: true, sha: mergeSha };
		} else if (method === "PATCH" && (match = route.match(/^\/pulls\/(\d+)$/))) {
			Object.assign(pulls.get(Number(match[1])), body);
			value = publicPull(pulls.get(Number(match[1])));
		} else if (method === "GET" && (match = route.match(/^\/pulls\/(\d+)$/))) {
			value = publicPull(pulls.get(Number(match[1])));
		} else if (method === "GET" && (match = route.match(/^\/commits\/([a-f0-9]{40})\/pulls$/))) {
			value = [...pulls.values()].filter((p) => p.merge_commit_sha === match[1]).map(publicPull);
		} else throw Error(`Unmodelled fixture Git request ${method} ${route}`);
		return Response.json(value, { status });
	};
	git.branches = {};
	return git;
}

/** Records the ledger's shape: which keys each record kind has, by type, so equal shapes are not repeated. */
function shapes(state, storage) {
	const kind = (value) => (value === null ? "null" : Array.isArray(value) ? "array" : typeof value);
	const out = new Set();
	for (const [field, value] of Object.entries(state)) {
		if (RECORD_MAPS.includes(field)) {
			for (const record of Object.values(value)) {
				const keys = Object.entries(record).map(([key, inner]) => `${key}:${kind(inner)}${key === "state" ? `=${inner}` : ""}${key === "name" ? `=${inner}` : ""}`);
				out.add(`${field}{${keys.sort().join(",")}}`);
			}
		} else out.add(`${field}:${kind(value)}${field === "paused" ? `=${value}` : ""}`);
	}
	for (const key of storage.data.keys()) out.add(`key:${key.replace(/[0-9a-f]{64}|[0-9]{10}/g, "#")}`);
	return out;
}

async function run(dir, version, snapshots, known, last) {
	const here = path.join(dir, version);
	const { Publication } = await import(pathToFileURL(path.join(here, "worker.mjs")));
	const owner = { key: "email:owner@example.test", name: "Owner", email: "owner@example.test" };
	const second = { key: "email:second@example.test", name: "Second", email: "second@example.test" };
	const writer = { key: "email:writer@example.test", name: "Writer", email: "writer@example.test" };
	const publisher = { key: "service:publisher", name: "Publisher", service: true };
	const objects = new Map();
	const env = {
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
			async get(key) {
				const bytes = objects.get(key)?.bytes;
				return bytes ? { json: async () => JSON.parse(bytes.toString()), arrayBuffer: async () => Uint8Array.from(bytes).buffer, text: async () => bytes.toString() } : null;
			},
			async put(key, value, options) {
				objects.set(key, { bytes: Buffer.from(value), metadata: options?.customMetadata });
			},
			async head(key) {
				const found = objects.get(key);
				return found ? { size: found.bytes.length, customMetadata: found.metadata } : null;
			},
		},
	};
	const git = fakeGit(env);
	globalThis.fetch = git.fetch;
	const storage = new Storage();
	const app = new Publication({ storage }, env);
	const stored = [];
	const skipped = [];

	const snapshot = (label, force = false) => {
		const state = storage.data.get("state");
		if (!state) return;
		const seen = shapes(state, storage);
		if (!force && ![...seen].some((shape) => !known.has(shape))) return;
		for (const shape of seen) known.add(shape);
		const withPrefix = (prefix) => Object.fromEntries([...storage.data].filter(([key]) => key.startsWith(prefix)).map(([key, value]) => [key.slice(prefix.length), structuredClone(value)]));
		snapshots.push({ version, step: label, state: structuredClone(state), activated: withPrefix("activated:"), runtimes: withPrefix("runtime:") });
		stored.push(label);
	};
	const post = async (route, who, body, label) => {
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
	const item = (id, extra = {}) => ({
		id, slug: id, summary: "Fixture", byline: "Writer", section: "research", tags: [], published: NOW, updated: NOW, state: "published",
		audience: "public", kind: "report", aliases: [], title: `Fixture ${id}`, source: `items/${id}/report.rmd`, route: `/stories/${id}`,
		article: objectHash, content: objectHash, card: "<article>Fixture</article>", lead: "<article>Fixture</article>", assets: {}, ...extra,
	});
	const stage = async (at, items, label) => {
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
	const activate = async (release, generation, label) => post("/_publish/activate", publisher, { release: release.id, generation }, label);
	const catalogue = (...extra) => [item("story"), item("other"), ...extra];

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
	const draft = (title) => `---\ntitle: ${title}\npublish:\n  id: draftone\n  slug: draftone\n  section: research\n  audience: public\n  state: draft\n---\nBody.\n`;
	const created = await post("/_editor/api/create", writer, { path: "items/draftone/report.rmd", revision: git.head, source: draft("Draft one") }, "editor create");
	if (created) {
		await post("/_editor/api/edit", writer, { path: "items/draftone/report.rmd", revision: created.head, pull: created.number, source: draft("Draft one, revised") }, "editor revise");
		// Approved by the owner after a protected preview build, then published and built.
		const revised = git.pull(created.number).head.sha;
		await stage(revised, catalogue(), "preview build");
		await post("/_editor/api/approve", owner, { number: created.number, head: revised }, "approve");
		const merged = await post("/_editor/api/publish", owner, { number: created.number, head: revised }, "publish");
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

async function generate(dir, out) {
	frozenClock();
	const versions = fs.readdirSync(dir).filter((name) => /^[0-9a-f]{7,40}$/.test(name));
	// Oldest first, in the order the commits were given to `prepare`.
	const order = fs.existsSync(path.join(dir, "order.txt")) ? fs.readFileSync(path.join(dir, "order.txt"), "utf8").trim().split("\n") : versions;
	const snapshots = [];
	const known = new Set();
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
	console.error("usage: generate-ledgers.mjs prepare <dir> <commit>... | generate <dir> <out.json>");
	process.exit(2);
}
