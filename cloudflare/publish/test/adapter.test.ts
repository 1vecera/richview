import test from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import type { Principal } from "../auth.ts";
import { PublicationBase, gateway, fontResponse, REQUIRED_WASM, type PublicationBindings, type PublishStorage } from "../adapter.ts";

// The rules are tested in Rust; these tests cover only the adapter: identity hand-off, replay of needs,
// the ledger commit and the desk page. They need the Wasm build (`npm run build:wasm`).
const built = existsSync(new URL("../pkg/richview_publish_server_bg.wasm", import.meta.url));
const skip = built ? false : "build the Wasm first: npm run build:wasm";

type StoredLedger = {
	roles: Record<string, { name: string }>;
	revision: number;
	audit_sequence: number;
	access_revision: number;
	activated: Record<string, string>;
	authorized: { bootstrap: { pull: number } };
};
type SnapshotFixture = {
	snapshots: {
		version: string;
		step: string;
		state: { active: string | null; generation: number; auditSequence: number };
		activated: Record<string, { commit: string }>;
		runtimes: Record<string, unknown>;
	}[];
};

class Storage implements PublishStorage {
	data = new Map<string, unknown>();
	async get<T>(k: string): Promise<T | undefined> { return structuredClone(this.data.get(k)) as T | undefined; }
	async put(k: string, v: unknown) { this.data.set(k, structuredClone(v)); }
	async list<T>({ prefix = "", reverse = false, limit = 1000 } = {}): Promise<Map<string, T>> {
		const entries = [...this.data].filter(([k]) => k.startsWith(prefix)).sort(([a], [b]) => a.localeCompare(b));
		if (reverse) entries.reverse();
		return new Map(entries.slice(0, limit)) as Map<string, T>;
	}
	async transaction<T>(fn: (tx: Storage) => Promise<T>) { return fn(this); }
}
async function setup() {
	const wasm = await import("../pkg/richview_publish_server.js");
	wasm.initSync({ module: readFileSync(new URL("../pkg/richview_publish_server_bg.wasm", import.meta.url)) });
	const storage = new Storage();
	const blobs = new Map<string, string>();
	const env: PublicationBindings = {
		PUBLICATION_HOST: "news.example.test",
		BOOTSTRAP_SUPER_EDITOR: "email:super@example.test",
		PUBLISHER_IDENTITY: "service:publisher",
		GITHUB_REPOSITORY: "o/r",
		PUBLICATION_BRANCH: "publish/news",
		CONTENT_ROOT: "publication",
		RELEASES: {
			get: async (k) => (blobs.has(k) ? { arrayBuffer: async () => new TextEncoder().encode(blobs.get(k)!).buffer } : null),
			head: async () => null,
			put: async () => {},
		},
	};
	const object = new PublicationBase({ storage }, env, wasm);
	const call = (path: string, principal: Principal | null = null, init: RequestInit & { headers?: Record<string, string> } = {}) =>
		object.fetch(new Request(`https://news.example.test${path}`, {
			...init,
			headers: { "x-publish-identity": JSON.stringify(principal), ...(init.headers || {}) },
		}));
	return { storage, object, call, env, blobs };
}
const owner = { key: "email:super@example.test", name: "Owner", email: "super@example.test" };
const post = (body: unknown) => ({
	method: "POST",
	headers: { "content-type": "application/json", origin: "https://news.example.test", "content-length": String(Buffer.byteLength(JSON.stringify(body))) },
	body: JSON.stringify(body),
});

test("status of a publication with no release is unpublished", { skip }, async () => {
	const { call } = await setup();
	const response = await call("/_status");
	assert.equal(response.status, 200);
	assert.equal((await response.json()).state, "unpublished");
});
test("a reader of an empty publication learns nothing about it", { skip }, async () => {
	const { call } = await setup();
	assert.equal((await call("/")).status, 503);
	assert.equal((await call("/_content/x/report.html")).status, 503);
});
test("an unknown host is refused", { skip }, async () => {
	const { object } = await setup();
	const response = await object.fetch(new Request("https://evil.example/_status", { headers: { "x-publish-identity": "null" } }));
	assert.equal(response.status, 404);
});
test("a signed-in user gets the desk page and an anonymous one does not", { skip }, async () => {
	const { call } = await setup();
	assert.match(await (await call("/_editor/", owner)).text(), /RichView Publish desk/);
	assert.notEqual((await call("/_editor/", null)).status, 200);
});
test("a role change is committed to the ledger with its audit entry", { skip }, async () => {
	const { call, storage } = await setup();
	const response = await call("/_editor/api/roles", owner, post({ identity: "email:ed@example.test", role: "editor", sections: ["news"], audiences: ["members"] }));
	assert.equal(response.status, 200);
	const state = (await storage.get<StoredLedger>("state"))!;
	assert.equal(state.roles["email:ed@example.test"].name, "editor");
	assert.equal(state.revision, 1);
	assert.equal((await storage.get<{ action: string }>("audit:0000000001"))!.action, "role.updated");
});
test("a stale commit is replayed under the new ledger", { skip }, async () => {
	const { object, storage } = await setup();
	let first = true;
	const original = object.load.bind(object);
	object.load = async () => {
		const state = await original();
		if (first) { first = false; await storage.put("state", { revision: 7 }); }
		return state;
	};
	const response = await object.fetch(new Request("https://news.example.test/_editor/api/roles", {
		...post({ identity: "email:a@example.test", role: "reader", sections: [], audiences: [] }),
		headers: { "x-publish-identity": JSON.stringify(owner), "content-type": "application/json", "content-length": "90", origin: "https://news.example.test" },
	}));
	assert.equal(response.status, 200);
	assert.equal((await storage.get<StoredLedger>("state"))!.revision, 8);
});
test("the public gateway strips a forged identity header", async () => {
	let seen: string | null | undefined;
	const env = {
		PUBLICATION: { idFromName: () => "id", get: () => ({ fetch: async (request: Request) => { seen = request.headers.get("x-publish-identity"); return new Response("ok"); } }) },
	};
	await gateway()(new Request("https://news.example.test/", { headers: { "x-publish-identity": JSON.stringify(owner) } }), env);
	assert.equal(seen, "null");
});

test("cross-site and cross-origin writes fail closed", { skip }, async () => {
	const { call, storage } = await setup();
	const body = { identity: "email:a@example.test", role: "editor", sections: ["*"], audiences: ["*"] };
	const origin = await call("/_editor/api/roles", owner, { ...post(body), headers: { "content-type": "application/json", origin: "https://evil.example", "content-length": "100" } });
	assert.equal(origin.status, 403);
	const site = await call("/_editor/api/roles", owner, { ...post(body), headers: { "content-type": "application/json", "sec-fetch-site": "cross-site", "content-length": "100" } });
	assert.equal(site.status, 403);
	assert.equal(await storage.get("state"), undefined);
});
test("a read that raced a withdrawal or role change is repeated under the new ledger", { skip }, async () => {
	const { object } = await setup();
	let loads = 0;
	const original = object.load.bind(object);
	object.load = async () => {
		loads++;
		const state = await original();
		// The ledger's access revision moves after the first pass has loaded it.
		return loads === 2 ? { ...state, access_revision: 1 } : state;
	};
	const response = await object.fetch(new Request("https://news.example.test/_status", { headers: { "x-publish-identity": "null" } }));
	assert.equal(response.status, 200);
	assert.ok(loads >= 3);
});
test("fonts are fixed public immutable paths and nothing else", async () => {
	const fonts = { "geist-latin-400-normal.woff2": new Uint8Array([1, 2, 3]) };
	const ok = fontResponse(new Request("https://x/_fonts/geist-latin-400-normal.woff2"), "geist-latin-400-normal.woff2", fonts);
	assert.equal(ok.status, 200);
	assert.match(ok.headers.get("cache-control")!, /immutable/);
	assert.equal(ok.headers.get("access-control-allow-origin"), "*");
	assert.equal(fontResponse(new Request("https://x/_fonts/../secret"), "../secret", fonts).status, 404);
	assert.equal(fontResponse(new Request("https://x/_fonts/geist-latin-400-normal.woff2", { method: "POST" }), "geist-latin-400-normal.woff2", fonts).status, 404);
});
test("bodies are refused outside the APIs, without a principal or a declared length, and above 20 MiB, before they are read", { skip }, async () => {
	const { call } = await setup();
	const post = (path: string, who: Principal | null, headers: Record<string, string>) => call(path, who, { method: "POST", headers, body: "0123456789" });
	assert.equal((await post("/", owner, { "content-length": "10" })).status, 405);
	assert.equal((await post("/_publish/stage", null, { "content-length": "10" })).status, 401);
	assert.equal((await post("/_publish/stage", owner, {})).status, 411);
	assert.equal((await post("/_publish/stage", owner, { "content-length": String(21 * 1024 * 1024) })).status, 413);
});
test("a retry after a lost ledger race replays the same time, nonce and Git replies", { skip }, async () => {
	const { object, storage } = await setup();
	const seen: [string, string][] = [];
	const handle = object.wasm.publish_handle;
	object.wasm = { ...object.wasm, publish_handle: (input) => { const parsed = JSON.parse(input); seen.push([parsed.now, parsed.nonce]); return handle(input); } };
	let first = true;
	const original = object.load.bind(object);
	object.load = async () => {
		const state = await original();
		if (first) { first = false; await storage.put("state", { revision: 3 }); }
		return state;
	};
	await object.fetch(new Request("https://news.example.test/_editor/api/roles", {
		method: "POST",
		headers: { "x-publish-identity": JSON.stringify(owner), "content-type": "application/json", "content-length": "80", origin: "https://news.example.test" },
		body: JSON.stringify({ identity: "email:a@example.test", role: "reader", sections: [], audiences: [] }),
	}));
	assert.ok(seen.length >= 2);
	assert.equal(new Set(seen.map(([now]) => now)).size, 1);
	assert.equal(new Set(seen.map(([, nonce]) => nonce)).size, 1);
});
test("PR #70's ledger is converted in place on first load, keeping a backup, its hides and its history", { skip }, async () => {
	const { call, storage, blobs } = await setup();
	const release = "a".repeat(64);
	blobs.set(`releases/${release}.json`, JSON.stringify({ schema: 1, id: release, commit: "c".repeat(40), toolchain: "t", runtime: {}, items: [], templates: {}, objects: {}, site: { title: "News", description: "d", base_url: "https://news.example.test", layout: "broadsheet", mode: "external", theme: "paper", sections: [{ id: "research", title: "Research", description: "r" }], navigation: [], lead: [], accent: "#1a6dac" } }));
	storage.data.set("state", {
		generation: 5, active: release, paused: false, hidden: {}, policies: {}, roles: {}, edits: {},
		approvals: {}, authorized: { bootstrap: { actor: "bootstrap", head: "bootstrap" } }, builds: {}, auditSequence: 1,
	});
	const original = structuredClone(await storage.get("state"));
	storage.data.set("activated:" + release, { commit: "c".repeat(40) });
	storage.data.set("audit:0000000001", { at: "2026-10-20T00:00:00Z", actor: "email:e@x", action: "release.activated", release });
	const status = await call("/_status");
	assert.equal(status.status, 200);
	const body = await status.json();
	assert.equal(body.generation, 5);
	assert.equal(body.release, release);
	const state = (await storage.get<StoredLedger>("state"))!;
	assert.equal(state.revision, 0);
	assert.equal(state.audit_sequence, 1);
	assert.equal(state.access_revision, 0);
	assert.equal(state.activated[release], "c".repeat(40));
	assert.equal(state.authorized.bootstrap.pull, 0);
	assert.deepEqual(await storage.get("state-pr70"), original);
	const audit = await call("/_editor/api/audit", owner);
	assert.equal(audit.status, 200);
	assert.equal((await audit.json())[0].action, "release.activated");
	// A second load does not convert again.
	await call("/_status");
	assert.deepEqual(await storage.get("state-pr70"), original);
});

// Snapshots from the real PR #70 modules at every version that changed what they store; regenerate them with
// test/legacy/generate-ledgers.ts. A hand-written record missed the bootstrap authorization on 30 Oct 2026.
test("every ledger a PR #70 producer wrote is converted on first load and keeps serving", { skip }, async () => {
	const fixture: SnapshotFixture = JSON.parse(readFileSync(new URL("../../../crates/richview-publish-server/tests/fixtures/pr70-legacy-ledgers.json", import.meta.url), "utf8"));
	assert.ok(fixture.snapshots.length >= 20);
	for (const snapshot of fixture.snapshots) {
		const label = `${snapshot.version} ${snapshot.step}`;
		const { call, storage, blobs } = await setup();
		const active = snapshot.state.active;
		if (active) blobs.set(`releases/${active}.json`, JSON.stringify({ schema: 1, id: active, commit: snapshot.activated[active]?.commit ?? "c".repeat(40), toolchain: "t", runtime: {}, items: [], templates: {}, objects: {}, site: { title: "News", description: "d", base_url: "https://news.example.test", layout: "broadsheet", mode: "external", theme: "paper", sections: [{ id: "research", title: "Research", description: "r" }], navigation: [], lead: [], accent: "#1a6dac" } }));
		storage.data.set("state", structuredClone(snapshot.state));
		for (const [id, record] of Object.entries(snapshot.activated)) storage.data.set(`activated:${id}`, record);
		for (const [id, record] of Object.entries(snapshot.runtimes)) storage.data.set(`runtime:${id}`, record);
		const original = structuredClone(snapshot.state);
		const status = await call("/_status");
		assert.equal(status.status, 200, label);
		const body = await status.json();
		assert.equal(body.generation, snapshot.state.generation, label);
		assert.equal(body.release ?? null, active, label);
		const state = (await storage.get<StoredLedger>("state"))!;
		assert.equal(state.revision, 0, label);
		assert.equal(state.audit_sequence, snapshot.state.auditSequence, label);
		assert.deepEqual(await storage.get("state-pr70"), original, label);
	}
});

test("a rejected legacy ledger leaves state, backup and side records unchanged", { skip }, async (t) => {
	const logged: unknown[][] = [];
	t.mock.method(console, "error", (...args: unknown[]) => logged.push(args));
	const { call, storage } = await setup();
	storage.data.set("state", { generation: 5, authorized: { commit: { actor: "email:approver@example.test", head: "commit" } } });
	storage.data.set("activated:release", { commit: "commit" });
	storage.data.set("runtime:runtime", { "richview_web.js": "hash" });
	const before = structuredClone([...storage.data]);
	const response = await call("/_status");
	assert.equal(response.status, 503);
	assert.equal(await response.text(), "The publication is temporarily unavailable. No access was granted.");
	assert.deepEqual([...storage.data], before);
	assert.equal(await storage.get("state-pr70"), undefined);
	assert.deepEqual(logged, [["Publish ledger migration failed:", "missing field `pull`"]]);
});

test("the Durable Object refuses a Wasm module that lacks an export the adapter calls", () => {
	const ctx = { storage: new Storage() };
	const env = { RELEASES: { get: async () => null, head: async () => null, put: async () => {} } };
	assert.throws(() => new PublicationBase(ctx, env, { publish_handle: () => "" }), /publish_migrate/);
	assert.doesNotThrow(() => new PublicationBase(ctx, env, Object.fromEntries(REQUIRED_WASM.map((n) => [n, () => ""]))));
});
test("the deployed entry passes the whole Wasm namespace to the Durable Object", () => {
	// entry.ts imports woff2 fonts that only wrangler can load, so its wiring is read, not run.
	const source = readFileSync(new URL("../entry.ts", import.meta.url), "utf8");
	assert.match(source, /import \* as wasm from "\.\/pkg\/richview_publish_server\.js"/);
	assert.match(source, /super\(ctx, env, wasm\)/);
	assert.match(source, /wasm\.initSync\(/);
});
