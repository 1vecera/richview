// Host reason (ARC-03): Cloudflare Worker adapter. It owns bindings (R2, the Durable Object), the Access
// identity hand-off and network I/O; every access, editorial and delivery rule runs in Rust compiled to Wasm.
import { identity } from "./auth.ts";
import type { AccessBindings, Principal } from "./auth.ts";
import { deskHTML, deskScript } from "./desk.ts";

/** Every function the adapter calls on the Wasm server. */
export const REQUIRED_WASM = ["publish_handle", "publish_migrate"] as const;
export type PublishWasm = Record<(typeof REQUIRED_WASM)[number], (input: string) => string>;

type StorageTransaction = {
	get<T>(key: string): Promise<T | undefined>;
	put(key: string, value: unknown): Promise<void>;
};
type StorageList = { prefix?: string; reverse?: boolean; limit?: number };
export type PublishStorage = StorageTransaction & {
	list<T>(options: StorageList): Promise<Map<string, T>>;
	transaction<T>(closure: (tx: StorageTransaction) => Promise<T>): Promise<T>;
};
export type PublicationContext = { storage: PublishStorage };

type Bucket = {
	get(key: string): Promise<{ arrayBuffer(): Promise<ArrayBufferLike> } | null>;
	head(key: string): Promise<{ size: number; customMetadata?: Record<string, string> } | null>;
	put(key: string, bytes: Uint8Array, options: {
		onlyIf?: { etagDoesNotMatch: string };
		customMetadata?: Record<string, string>;
	}): Promise<unknown>;
};
export type PublicationBindings = {
	RELEASES: Bucket;
	PUBLICATION_HOST?: string;
	GITHUB_REPOSITORY?: string;
	PUBLICATION_BRANCH?: string;
	CONTENT_ROOT?: string;
	PUBLISHER_IDENTITY?: string;
	BOOTSTRAP_SUPER_EDITOR?: string;
	BOOTSTRAP_GITHUB_LOGIN?: string;
	BOOTSTRAP_COMMIT?: string;
	GITHUB_TOKEN?: string;
};
export type GatewayBindings<Id> = AccessBindings & {
	PUBLICATION: {
		idFromName(name: string): Id;
		get(id: Id): { fetch(request: Request): Promise<Response> };
	};
};

// The wire structures match the fields the adapter consumes from wire.rs and host.rs.
type Ledger = Record<string, unknown> & { revision?: number; access_revision?: number };
type AuditRecord = { sequence: number; at: string; actor: string; action: string; details: unknown };
type StoredAudit = AuditRecord | (Record<string, unknown> & {
	at: string; actor: string; action: string; details?: undefined;
});
type ForgeCall = { need: "forge"; key: string; method: string; path: string; body: string | null };
type Need =
	| { need: "blob"; key: string }
	| { need: "head"; key: string }
	| ForgeCall
	| { need: "audit"; limit: number };
type BucketWrite = { key: string; bytes: string; if_absent: boolean; sha256: string | null };
type Outcome =
	| { kind: "invalid" }
	| { kind: "need"; needs: Need[] }
	| {
		kind: "done";
		response: { status: number; headers: Record<string, string>; body: string };
		state: Ledger | null;
		audit: AuditRecord[];
		writes: BucketWrite[];
	};
type Migration = { kind: "migrated"; state: Ledger } | { kind: "invalid"; error?: unknown };
type WireRequest = { method: string; host: string; path: string; query: Record<string, string>; headers: Record<string, string>; body: string };
type Io = {
	blobs: Record<string, string | null>;
	heads: Record<string, { size: number; sha256: string } | null>;
	forge: Record<string, { status: number; body: unknown }>;
	audit: AuditRecord[] | null;
};
type Carry = { nonce: string; now: string; io: Io };

const MAX_BODY = 20 * 1024 * 1024;
const b64 = (bytes: Uint8Array) => {
	let s = "";
	for (let i = 0; i < bytes.length; i += 0x8000)
		s += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
	return btoa(s);
};
const unb64 = (text: string) => Uint8Array.from(atob(text), (c) => c.charCodeAt(0));
const settings = (env: PublicationBindings) => ({
	publication_host: env.PUBLICATION_HOST,
	repository: env.GITHUB_REPOSITORY,
	branch: env.PUBLICATION_BRANCH,
	content_root: env.CONTENT_ROOT,
	publisher_identity: env.PUBLISHER_IDENTITY || "",
	bootstrap_super_editor: env.BOOTSTRAP_SUPER_EDITOR || "",
	bootstrap_github_login: env.BOOTSTRAP_GITHUB_LOGIN || null,
	bootstrap_commit: env.BOOTSTRAP_COMMIT || null,
});
const unavailable = () =>
	new Response("The publication is temporarily unavailable. No access was granted.", {
		status: 503,
		headers: { "Cache-Control": "private, no-store", "X-Robots-Tag": "noindex" },
	});

/** The Worker: verifies identity, then hands the request to the one Durable Object. */
export function gateway<Id = unknown>() {
	return async (request: Request, env: GatewayBindings<Id>) => {
		try {
			const principal = await identity(request, env);
			const stub = env.PUBLICATION.get(env.PUBLICATION.idFromName("publication"));
			const forwarded = new Headers(request.headers);
			forwarded.set("x-publish-identity", JSON.stringify(principal));
			const response = await stub.fetch(new Request(request, { headers: forwarded }));
			return request.method === "HEAD" ? new Response(null, response) : response;
		} catch {
			return unavailable();
		}
	};
}

/** One strongly consistent authority: the ledger lives in this object's storage. */
export class PublicationBase {
	declare ctx: PublicationContext;
	declare env: PublicationBindings;
	declare wasm: PublishWasm;

	constructor(ctx: PublicationContext, env: PublicationBindings, wasm: Partial<PublishWasm> | null) {
		// A missing export would turn every request into a 503 (or lose the first-load migration), so fail at construction.
		for (const name of REQUIRED_WASM) {
			if (typeof wasm?.[name] !== "function") throw new Error(`The Wasm module lacks ${name}`);
		}
		this.ctx = ctx;
		this.env = env;
		this.wasm = wasm as PublishWasm;
	}
	async load() {
		const state = await this.ctx.storage.get<Ledger>("state");
		if (!state) return {};
		return state.revision === undefined ? this.migrate(state) : state;
	}
	/** First load after PR #70's Worker: convert its ledger in place, keeping a copy of the old record. */
	async migrate(old: Ledger) {
		const strip = (prefix: string, list: Map<string, unknown>) =>
			Object.fromEntries([...list].map(([key, value]) => [key.slice(prefix.length), value]));
		const out: Migration = JSON.parse(
			this.wasm.publish_migrate(
				JSON.stringify({
					state: old,
					activated: strip("activated:", await this.ctx.storage.list({ prefix: "activated:" })),
					runtimes: strip("runtime:", await this.ctx.storage.list({ prefix: "runtime:" })),
				}),
			),
		);
		if (out.kind !== "migrated") {
			// Serde errors may quote stored values: log field names, never the ledger or configuration.
			const reason = typeof out.error === "string" ? out.error.match(/(?:missing|unknown) field `[a-z_]+`/)?.[0] : null;
			console.error("Publish ledger migration failed:", reason || "invalid legacy ledger");
			throw new Error("The stored ledger could not be migrated");
		}
		await this.ctx.storage.transaction(async (tx) => {
			if ((await tx.get<Ledger>("state"))?.revision !== undefined) return;
			if ((await tx.get("state-pr70")) === undefined) await tx.put("state-pr70", old);
			await tx.put("state", out.state);
		});
		return (await this.ctx.storage.get<Ledger>("state")) || out.state;
	}
	async fetch(request: Request): Promise<Response> {
		try {
			const url = new URL(request.url);
			const page = deskPage(request, url);
			if (page) return page;
			const principal: Principal | null = JSON.parse(request.headers.get("x-publish-identity") || "null");
			const headers = Object.fromEntries(
				[...request.headers].filter(([k]) => k !== "x-publish-identity"),
			);
			let body = "";
			if (request.method !== "GET" && request.method !== "HEAD") {
				// Only the publishing and desk APIs take bodies, and none takes more than an object's 20 MiB.
				// No principal, no declared length, or a length above the cap: refused before the body is read.
				const length = request.headers.get("content-length");
				const size = length === null || !/^\d+$/.test(length) ? Number.NaN : Number(length);
				const refuse = (status: number) => new Response("Request body refused.", { status, headers: { "Cache-Control": "private, no-store" } });
				if (!/^\/_(publish|editor)\//.test(url.pathname)) return refuse(405);
				if (!principal) return refuse(401);
				if (Number.isNaN(size)) return refuse(411);
				if (size > MAX_BODY) return refuse(413);
				const bytes = new Uint8Array(await request.arrayBuffer());
				if (bytes.length > MAX_BODY) return new Response("Request body refused.", { status: 413, headers: { "Cache-Control": "private, no-store" } });
				body = b64(bytes);
			}
			const wire: WireRequest = {
				method: request.method,
				host: url.hostname,
				path: url.pathname,
				query: Object.fromEntries(url.searchParams),
				headers,
				body,
			};
			// A retry after a lost ledger race repeats the call with the same time, nonce and replies, reads included,
			// so the replay decides exactly as the first pass did and Git changes are never made twice.
			const carry: Carry = {
				nonce: crypto.randomUUID(),
				now: new Date().toISOString(),
				io: { blobs: {}, heads: {}, forge: {}, audit: null },
			};
			for (let attempt = 0; attempt < 4; attempt++) {
				const answer = await this.run(principal, wire, carry);
				if (answer) return answer;
			}
			return unavailable();
		} catch {
			return unavailable();
		}
	}
	/** One pass: run until done, commit, and return null if the ledger moved underneath the call. */
	async run(principal: Principal | null, request: WireRequest, carry: Carry): Promise<Response | null> {
		const state = await this.load();
		const { io, now, nonce } = carry;
		io.audit = null;
		for (let step = 0; step < 40; step++) {
			const out: Outcome = JSON.parse(
				this.wasm.publish_handle(
					JSON.stringify({
						config: settings(this.env),
						principal,
						state,
						io,
						now,
						nonce,
						request,
					}),
				),
			);
			if (out.kind === "need") {
				await Promise.all(out.needs.map((need) => this.answer(need, io, carry)));
				continue;
			}
			if (out.kind !== "done") return unavailable();
			for (const w of out.writes) await this.write(w);
			if (out.state) {
				const committed = await this.ctx.storage.transaction(async (tx) => {
					const current = (await tx.get<Ledger>("state")) || {};
					if ((current.revision || 0) !== (state.revision || 0)) return false;
					for (const entry of out.audit)
						await tx.put(`audit:${String(entry.sequence).padStart(10, "0")}`, entry);
					await tx.put("state", out.state);
					return true;
				});
				if (!committed) return null;
			} else if (request.method === "GET" || request.method === "HEAD") {
				// A read that raced a withdrawal or role change is repeated under the new ledger.
				const now = await this.load();
				if ((now.access_revision || 0) !== (state.access_revision || 0)) return null;
			}
			return new Response(unb64(out.response.body), {
				status: out.response.status,
				headers: out.response.headers,
			});
		}
		return unavailable();
	}
	async answer(need: Need, io: Io, _carry: Carry) {
		if (need.need === "blob") {
			const object = await this.env.RELEASES.get(need.key);
			io.blobs[need.key] = object ? b64(new Uint8Array(await object.arrayBuffer())) : null;
		} else if (need.need === "head") {
			const head = await this.env.RELEASES.head(need.key);
			io.heads[need.key] = head
				? { size: head.size, sha256: head.customMetadata?.sha256 || "" }
				: null;
		} else if (need.need === "forge") {
			io.forge[need.key] = await this.forge(need);
		} else if (need.need === "audit") {
			const list = await this.ctx.storage.list<StoredAudit>({ prefix: "audit:", reverse: true, limit: need.limit });
			// PR #70 stored entries flat, without a sequence; the key carries it.
			io.audit = [...list].map(([key, e]) =>
				e.details === undefined
					? { sequence: Number(key.slice(6)), at: e.at, actor: e.actor, action: e.action, details: Object.fromEntries(Object.entries(e).filter(([k]) => !["at", "actor", "action"].includes(k))) }
					: e as AuditRecord,
			);
		}
	}
	/** The Git host call; the token never leaves this adapter. Redirects are refused, not followed. */
	async forge(need: ForgeCall) {
		const response = await fetch(
			`https://api.github.com/repos/${this.env.GITHUB_REPOSITORY}${need.path}`,
			{
				method: need.method,
				headers: {
					Authorization: `Bearer ${this.env.GITHUB_TOKEN}`,
					"User-Agent": "RichView-Publish",
					Accept: "application/vnd.github+json",
					"X-GitHub-Api-Version": "2022-11-28",
					"Content-Type": "application/json",
				},
				body: need.body ?? undefined,
				redirect: "manual",
			},
		);
		let body: unknown = null;
		try {
			body = response.status === 204 ? null : await response.json();
		} catch {
			/* An empty or non-JSON body is an empty answer. */
		}
		return { status: response.status, body };
	}
	async write(w: BucketWrite) {
		await this.env.RELEASES.put(w.key, unb64(w.bytes), {
			...(w.if_absent ? { onlyIf: { etagDoesNotMatch: "*" } } : {}),
			...(w.sha256 ? { customMetadata: { sha256: w.sha256 } } : {}),
		});
	}
}

/** The desk page and its script hold no data; the API behind them checks the role on every call. */
function deskPage(request: Request, url: URL) {
	if (request.method !== "GET" || !request.headers.get("x-publish-identity")?.startsWith("{")) return null;
	const script = url.pathname === "/_editor/app.js";
	if (!script && url.pathname !== "/_editor/") return null;
	return new Response(script ? deskScript : deskHTML, {
		headers: {
			"Content-Type": script ? "text/javascript; charset=utf-8" : "text/html; charset=utf-8",
			"Cache-Control": "private, no-store",
			"X-Robots-Tag": "noindex",
			...(script ? {} : { "Content-Security-Policy": "default-src 'none'; script-src 'self'; connect-src 'self'; style-src 'unsafe-inline'; base-uri 'none'; frame-ancestors 'none'" }),
		},
	});
}

/** Publication fonts live at fixed public paths. Sandboxed report frames have an opaque origin, so font requests are CORS. */
export function fontResponse(request: Request, name: string, fonts: Record<string, BodyInit>) {
	if (!["GET", "HEAD"].includes(request.method) || !fonts[name])
		return new Response("Font not found.", { status: 404, headers: { "Cache-Control": "private, no-store" } });
	return new Response(request.method === "HEAD" ? null : fonts[name], {
		headers: {
			"Content-Type": "font/woff2",
			"Cache-Control": "public, max-age=31536000, immutable",
			"X-Content-Type-Options": "nosniff",
			"Access-Control-Allow-Origin": "*",
			"Cross-Origin-Resource-Policy": "cross-origin",
		},
	});
}
