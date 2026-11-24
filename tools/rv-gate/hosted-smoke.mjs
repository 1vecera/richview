// Host reason (ARC-03): local HTTP/Workerd smoke over built native and Publish Wasm artifacts.
// No deploy, cloud account or remote transport. DNS/TLS/Access/live-store/restore checks remain
// deployment-owner gaps (DEL-77); this command proves only the isolated local services.
import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { createHash, randomBytes } from "node:crypto";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { basename, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { parseArgs } from "node:util";
import { makeRelease } from "./journeys-publish.mjs";

const TITLE = "Local hosted smoke", BODY = "This report stays private until its owner shares it.";
const PRIVATE_TITLE = "Restricted Worker smoke", PRIVATE_BODY = "Only the publication editor may read this fixture.";
const COMMIT = "a".repeat(40), ISSUER = "https://test.cloudflareaccess.com", AUDIENCE = "local-hosted-smoke";
const sha = bytes => createHash("sha256").update(bytes).digest("hex");
const SOURCE = `---\ntitle: ${TITLE}\ndescription: Offline hosted smoke.\nslug: hosted-smoke\nversion: 1.0.0\n---\n\n# ${TITLE}\n\n${BODY}\n`;
const GAP = "Only local services are checked. Deployed DNS, TLS, Cloudflare Access, live-store/probe-account limits, managed data/media, restore drills, post-deploy and scheduled smoke execution, rollback/incident wiring, and positive shared-story link-preview thumbnail delivery remain unrun.";
const DEPLOYED_CHECKS = [
	"DNS/TLS/Cloudflare Access",
	"Live-service probe accounts and cross-account limits",
	"Managed data/media lifecycle",
	"Hosted-store restore drills",
	"Post-deploy and scheduled hosted smoke execution",
	"Failed-deploy rollback and scheduled-failure incident wiring",
	"Positive shared-story link-preview thumbnail delivery",
];

/** Fetch only an explicitly owned HTTP loopback origin; redirects are inspected before following. */
export function localFetch(url, { origins, ...options }) {
	const target = new URL(url);
	assert.ok(target.protocol === "http:" && target.hostname === "127.0.0.1" && !target.username && !target.password, `Non-loopback URL refused: ${target.origin}`);
	assert.ok(origins.has(target.origin), `Unowned loopback origin refused: ${target.origin}`);
	return fetch(target, { ...options, redirect: "manual" });
}

async function request(url, options) {
	const response = await localFetch(url, options);
	const text = await response.text();
	let json;
	try { json = JSON.parse(text); } catch { /* HTML and refused requests are not JSON. */ }
	return { status: response.status, text, json, headers: response.headers, url: response.url };
}

/** Read an owned origin, refusing any redirect that could escape it or disclose the fixture token. */
export async function readLocal(url, options) {
	for (let hop = 0; hop < 6; hop++) {
		const response = await request(url, options);
		const location = response.headers.get("location");
		if (response.status < 300 || response.status >= 400 || !location) return response;
		url = new URL(location, url).href;
	}
	throw new Error("The local reader exceeded six redirects");
}

/** A refused page must disclose neither report bytes nor link-preview metadata. */
export function checkHidden(page, markers) {
	assert.ok([401, 403, 404, 410].includes(page.status) || (page.status === 200 && /[?&](rv=signin|m=Sign)/i.test(page.url)), `Expected refusal or sign-in, received ${page.status} at ${page.url}`);
	for (const marker of markers) assert.ok(!page.text.includes(marker), "A private/deleted report disclosed its content");
	assert.doesNotMatch(page.text, /property=["']og:(title|image)["']/i, "A private/deleted report disclosed preview metadata");
}

/** Successful report pages carry the product's hardening policy and the expected content. */
export function checkPage(page, marker) {
	assert.equal(page.status, 200, page.text.slice(0, 300));
	assert.match(page.headers.get("content-type") ?? "", /^text\/html\b/);
	assert.equal(page.headers.get("x-content-type-options"), "nosniff");
	assert.ok(["no-referrer", "strict-origin-when-cross-origin"].includes(page.headers.get("referrer-policy")), "Missing reader referrer policy");
	assert.ok(page.text.includes(marker), "The reader lost the report's content");
	const meta = /http-equiv="Content-Security-Policy" content="([^"]+)"/.exec(page.text)?.[1];
	const policy = (page.headers.get("content-security-policy") || meta || "").replaceAll("&#39;", "'");
	assert.match(policy, /(?:^|;)\s*default-src 'none'(?:;|$)/);
	assert.match(policy, /(?:^|;)\s*base-uri 'none'(?:;|$)/);
	assert.doesNotMatch(policy, /script-src[^;]*\s'unsafe-eval'/);
}

function aborted(signal) {
	signal?.throwIfAborted();
}

async function untilAbort(promise, signal) {
	if (signal.aborted) { Promise.resolve(promise).catch(() => {}); throw signal.reason; }
	let abort;
	try {
		return await Promise.race([promise, new Promise((_, reject) => {
			abort = () => reject(signal.reason);
			signal.addEventListener("abort", abort, { once: true });
		})]);
	} finally { signal.removeEventListener("abort", abort); }
}

/** Start the real native server with fixture-owned state; even a failed startup reaps its process. */
export async function nativeServer({ bin, cwd, work, token, signal, evidence }) {
	aborted(signal);
	const home = join(work, "home");
	mkdirSync(home);
	const child = spawn(bin, ["server", "--bind", "127.0.0.1", "--port", "0", "--token", token, "--default-visibility", "private", "--data-dir", join(work, "store")], {
		cwd, env: { PATH: process.env.PATH, HOME: home, XDG_CONFIG_HOME: home, XDG_DATA_HOME: home, XDG_STATE_HOME: home }, stdio: ["ignore", "pipe", "pipe"],
	});
	evidence.pid = child.pid;
	let log = "", ended = false, stopping;
	const closed = new Promise(accept => child.once("close", () => { ended = true; accept(); }));
	const stop = () => stopping ??= (async () => {
		if (!ended) {
			child.kill("SIGTERM");
			let timer;
			try { await Promise.race([closed, new Promise(accept => { timer = setTimeout(accept, 2000); })]); }
			finally { clearTimeout(timer); }
			if (!ended) child.kill("SIGKILL");
			await closed;
		}
		evidence.closed = true;
	})();
	try {
		const ready = new Promise((accept, reject) => {
			child.once("error", reject);
			child.once("exit", (code, sig) => reject(new Error(`Native server exited ${code ?? sig}: ${log.slice(-1000)}`)));
			for (const stream of [child.stdout, child.stderr]) stream.on("data", chunk => {
				log = (log + chunk).slice(-65536);
				const app = /RichView at (http:\/\/127\.0\.0\.1:\d+)/.exec(log)?.[1];
				const report = /Reports at (http:\/\/127\.0\.0\.1:\d+)/.exec(log)?.[1];
				if (app && report) accept({ app, report });
			});
		});
		return { ...await untilAbort(ready, signal), stop };
	} catch (error) { await stop(); throw error; }
}

/** Logical Access/Git fixture requests never reach a socket; every other outbound request fails closed. */
export function offlineServices({ jwk, requests }) {
	return async request => {
		const url = new URL(request.url);
		const entry = { method: request.method, url: url.origin + url.pathname, allowed: false };
		requests.push(entry);
		if (request.method === "GET" && !url.search && !url.username && !url.password) {
			if (url.origin === ISSUER && url.pathname === "/cdn-cgi/access/certs") {
				entry.allowed = true;
				return Response.json({ keys: [jwk] });
			}
			if (url.origin === "https://api.github.com" && ["/repos/example/publication", "/repos/example/publication/git/ref/heads/publish/news"].includes(url.pathname)) {
				entry.allowed = true;
				return Response.json(url.pathname.endsWith("/publish/news") ? { object: { sha: COMMIT } } : { private: true });
			}
		}
		return new Response("Outbound request refused by local smoke", { status: 502 });
	};
}

async function bundleWorker(cwd, work, wasm, signal) {
	const pkg = join(work, "pkg"), root = join(work, "bundle");
	mkdirSync(pkg); mkdirSync(root);
	aborted(signal);
	const generated = spawnSync("wasm-bindgen", ["--target", "web", "--out-dir", pkg, wasm], { encoding: "utf8", timeout: 10000, env: { PATH: process.env.PATH } });
	assert.equal(generated.status, 0, `wasm-bindgen: ${generated.error?.message ?? generated.stderr}`);
	const esbuild = await import("esbuild"), modules = [];
	try {
		const result = await untilAbort(esbuild.build({
			entryPoints: [join(cwd, "cloudflare/publish/entry.mjs")], outfile: join(root, "entry.mjs"), bundle: true, write: false, format: "esm", platform: "browser", target: "es2022",
			plugins: [{ name: "local-publish-modules", setup(build) {
				build.onResolve({ filter: /\/pkg\/richview_publish_server\.js$/ }, () => ({ path: join(pkg, "richview_publish_server.js") }));
				build.onResolve({ filter: /\.(wasm|woff2)$/ }, args => {
					const name = basename(args.path), file = args.path.endsWith(".wasm") ? join(pkg, name) : resolve(args.resolveDir, args.path);
					modules.push({ type: args.path.endsWith(".wasm") ? "CompiledWasm" : "Data", path: join(root, name), contents: readFileSync(file) });
					return { path: `./${name}`, external: true };
				});
			} }],
		}), signal);
		return { modulesRoot: root, modules: [{ type: "ESModule", path: join(root, "entry.mjs"), contents: result.outputFiles[0].text }, ...modules] };
	} finally { esbuild.stop(); }
}

async function nativeSmoke(context, work, evidence) {
	const token = randomBytes(24).toString("hex");
	const server = await nativeServer({ ...context, work, token, evidence });
	const origins = new Set([server.app, server.report]);
	const api = (actor, method, path, data) => request(server.app + path, {
		origins, signal: context.signal, method, headers: { ...(actor ? { authorization: `Bearer ${actor}` } : {}), ...(data ? { "content-type": "application/json" } : {}) }, body: data ? JSON.stringify(data) : undefined,
	});
	const read = actor => readLocal(server.app + "/r/hosted-smoke", { origins, signal: context.signal, headers: actor ? { authorization: `Bearer ${actor}` } : {} });
	try {
		let response = await api(null, "GET", "/health");
		assert.equal(response.status, 200); assert.equal(response.json?.ok, true);
		const person = async (name, role) => {
			const made = await api(token, "POST", "/api/governance/users", { name, email: `${name.toLowerCase()}@example.test`, role });
			assert.equal(made.status, 201, made.text);
			const issued = await api(token, "POST", `/api/governance/users/${made.json.id}/tokens`, { name: "smoke" });
			assert.equal(issued.status, 201, issued.text); assert.ok(issued.json.token);
			return issued.json.token;
		};
		const owner = await person("Owner", "author"), outsider = await person("Outsider", "reader");
		const publish = { slug: "hosted-smoke", source: SOURCE, reportFolder: { root: "hosted-smoke", source: "report.rmd", files: ["report.rmd"] } };
		assert.equal((await api(null, "POST", "/api/reports", publish)).status, 401);
		response = await api(owner, "POST", "/api/reports", publish);
		assert.equal(response.status, 200, response.text); assert.equal(response.json.visibility, "private");
		for (const actor of [null, outsider]) checkHidden(await read(actor), [TITLE, BODY]);
		const page = await read(owner); checkPage(page, BODY);
		assert.match(page.headers.get("cache-control") ?? "", /private/);
		response = await api(owner, "GET", "/raw/hosted-smoke/v/1.0.0");
		assert.equal(response.text, SOURCE, "Published source bytes changed");
		const access = "/api/reports/hosted-smoke/access", deletion = "/api/reports/hosted-smoke/v/1.0.0";
		assert.equal((await api(outsider, "PUT", access, { visibility: "public" })).status, 404);
		assert.equal((await api(owner, "PUT", access, { visibility: "public" })).status, 200);
		checkPage(await read(null), BODY);
		assert.equal((await api(outsider, "DELETE", deletion)).status, 403);
		assert.equal((await api(owner, "DELETE", deletion)).status, 200);
		for (const actor of [null, owner, outsider]) checkHidden(await read(actor), [TITLE, BODY]);
		evidence.checks = ["health", "anonymous-write-refused", "private-and-cross-account-refusal", "owner-content-source-CSP", "viewer-cannot-share-or-delete", "private-to-public", "delete-refusal"];
		evidence.renderedSha256 = sha(page.text);
		return page.text;
	} finally { await server.stop(); }
}

async function workerSmoke(context, work, rendered, evidence) {
	const { generateKeyPair, exportJWK, SignJWT } = await import(pathToFileURL(join(context.cwd, "cloudflare/publish/node_modules/jose/dist/webapi/index.js")));
	const { Miniflare, convertV4MiniflareOptions } = await import("miniflare");
	const bundle = await bundleWorker(context.cwd, work, context.wasm, context.signal);
	const { privateKey, publicKey } = await generateKeyPair("RS256");
	const jwk = { ...await exportJWK(publicKey), kid: "local-smoke", use: "sig", alg: "RS256" };
	const credentials = {};
	for (const actor of ["publisher", "owner", "outsider"]) credentials[actor] = await new SignJWT({ email: `${actor}@example.test` })
		.setProtectedHeader({ alg: "RS256", kid: jwk.kid }).setIssuer(ISSUER).setAudience(AUDIENCE).setSubject(actor).setIssuedAt().setExpirationTime("5m").sign(privateKey);
	const fixture = makeRelease(COMMIT, [
		{ id: "story", state: "published", audience: "public", title: TITLE, body: BODY },
		{ id: "restricted", state: "published", audience: "editors", title: PRIVATE_TITLE, body: PRIVATE_BODY },
	], "hosted-smoke");
	const hash = sha(rendered);
	fixture.blobs.set(hash, Buffer.from(rendered));
	fixture.release.objects[hash] = { sha256: hash, bytes: Buffer.byteLength(rendered), content_type: "text/html; charset=utf-8" };
	fixture.release.items[0].article = hash; fixture.release.items[0].content = hash;
	fixture.release.objects = Object.fromEntries(Object.entries(fixture.release.objects).sort(([a], [b]) => a.localeCompare(b)));
	fixture.release.id = ""; fixture.release.id = sha(JSON.stringify(fixture.release));
	evidence.outbound = [];
	const mf = new Miniflare(convertV4MiniflareOptions({
		...bundle, compatibilityDate: "2026-10-18", host: "127.0.0.1", port: 0, telemetry: { enabled: false },
		r2Buckets: ["RELEASES"], r2Persist: join(work, "r2"), durableObjects: { PUBLICATION: { className: "Publication", useSQLite: true } }, durableObjectsPersist: join(work, "do"), cachePersist: join(work, "cache"),
		bindings: { PUBLICATION_HOST: "127.0.0.1", ACCESS_ISSUER: ISSUER, ACCESS_AUD: AUDIENCE, BOOTSTRAP_SUPER_EDITOR: "email:owner@example.test", PUBLISHER_IDENTITY: "email:publisher@example.test", GITHUB_REPOSITORY: "example/publication", PUBLICATION_BRANCH: "publish/news", CONTENT_ROOT: "publications/news", BOOTSTRAP_COMMIT: COMMIT },
		outboundService: offlineServices({ jwk, requests: evidence.outbound }),
	}));
	try {
		const origin = await untilAbort(mf.ready, context.signal), origins = new Set([origin.origin]);
		const call = (path, actor, method = "GET", bytes) => request(new URL(path, origin), {
			origins, signal: context.signal, method, headers: { ...(actor ? { Cookie: `CF_Authorization=${credentials[actor]}` } : {}), ...(bytes ? { "content-length": String(Buffer.byteLength(bytes)) } : {}) }, body: bytes,
		});
		let response = await call("/_status"); assert.equal(response.status, 200); assert.equal(response.json.state, "unpublished");
		assert.equal((await call("/_publish/stage", "outsider", "POST", JSON.stringify(fixture.release))).status, 403);
		for (const [id, bytes] of fixture.blobs) {
			response = await call(`/_publish/objects/${id}`, "publisher", "PUT", bytes); assert.equal(response.status, 200, response.text);
		}
		response = await call("/_publish/stage", "publisher", "POST", JSON.stringify(fixture.release)); assert.equal(response.status, 200, response.text);
		// Before the first activation the product serves its explicit unpublished state, including previews.
		response = await call("/stories/story"); assert.equal(response.status, 503); assert.match(response.text, /first publication is being prepared/);
		assert.ok(!response.text.includes(TITLE) && !response.text.includes(BODY));
		response = await call("/_publish/activate", "publisher", "POST", JSON.stringify({ release: fixture.release.id, generation: 0 })); assert.equal(response.status, 200, response.text);
		response = await call("/_status"); assert.equal(response.json.state, "live"); assert.equal(response.json.release, fixture.release.id);
		response = await call("/stories/story"); checkPage(response, BODY);
		assert.match(response.headers.get("cache-control") ?? "", /private.*no-store/);
		assert.equal(sha(response.text), hash, "Worker reader changed the native renderer's actual HTML bytes");
		const frame = await call("/_content/story/report.html"); checkPage(frame, BODY);
		assert.match(frame.headers.get("content-security-policy"), /^sandbox\b/);
		assert.doesNotMatch(frame.headers.get("content-security-policy"), /\ballow-same-origin\b/);
		const bucket = await mf.getR2Bucket("RELEASES"), stored = await bucket.get(`objects/${hash}`);
		assert.equal(sha(new Uint8Array(await stored.arrayBuffer())), hash, "Local R2 changed published object bytes");
		const preview = `/_preview/${fixture.release.id}/stories/story`;
		for (const actor of [null, "outsider"]) {
			checkHidden(await call(preview, actor), [TITLE, BODY]);
			checkHidden(await call("/stories/restricted", actor), [PRIVATE_TITLE, PRIVATE_BODY]);
			checkHidden(await call("/_content/restricted/report.html", actor), [PRIVATE_TITLE, PRIVATE_BODY]);
		}
		checkPage(await call("/stories/restricted", "owner"), PRIVATE_BODY);
		checkPage(await call(preview, "owner"), BODY);
		assert.equal((await call("/_editor/api/hide", "outsider", "POST", JSON.stringify({ id: "story", reason: "Local smoke" }))).status, 403);
		response = await call("/_editor/api/hide", "owner", "POST", JSON.stringify({ id: "story", reason: "Local smoke" })); assert.equal(response.status, 202, response.text);
		for (const path of ["/stories/story", "/_content/story/report.html", `/_releases/${fixture.release.id}/stories/story`]) checkHidden(await call(path), [TITLE, BODY]);
		assert.ok(evidence.outbound.length > 0 && evidence.outbound.every(entry => entry.allowed), "An unexpected outbound request was blocked");
		evidence.checks = ["unpublished-health", "publisher-only-writes", "stage-unpublished", "activate-public", "actual-native-and-R2-bytes", "headers-and-sandboxed-frame-CSP", "protected-preview", "restricted-page-and-frame", "cross-account-refusal", "urgent-hide-current-and-historical"];
		evidence.contentSha256 = hash;
	} finally { await mf.dispose(); evidence.closed = true; }
}

/** Run both isolated services with private artifact inputs and remove all fixture state on exit. */
export async function hostedSmoke({ cwd, bin, wasm, out }) {
	const context = { cwd, bin: realpathSync(bin), wasm: realpathSync(wasm), signal: AbortSignal.timeout(50_000) };
	const hashes = { native: sha(readFileSync(context.bin)), publishWasm: sha(readFileSync(context.wasm)) };
	mkdirSync(out, { recursive: true });
	const started = Date.now(), work = mkdtempSync(join(out, "state-"));
	const evidence = {
		scope: "local-loopback", deploymentGap: GAP,
		unrun: DEPLOYED_CHECKS.map(name => ({ name, status: "unrun", reason: "Needs an authorized deployed target and its deployment owner; local smoke grants no deployment authority." })),
		artifacts: hashes, native: {}, worker: {},
	};
	try {
		const rendered = await nativeSmoke(context, work, evidence.native);
		await workerSmoke(context, work, rendered, evidence.worker);
		assert.equal(sha(readFileSync(context.bin)), hashes.native, "Native artifact changed during smoke");
		assert.equal(sha(readFileSync(context.wasm)), hashes.publishWasm, "Publish Wasm artifact changed during smoke");
		evidence.status = "pass";
	} catch (error) { evidence.status = "fail"; evidence.error = error.stack; }
	finally {
		rmSync(work, { recursive: true, force: true });
		evidence.durationMs = Date.now() - started;
		if (evidence.durationMs >= 60_000) { evidence.status = "fail"; evidence.error = "Local hosted smoke exceeded one minute"; }
		writeFileSync(join(out, "hosted-smoke.json"), JSON.stringify(evidence, null, 2) + "\n");
	}
	return evidence;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
	const { values } = parseArgs({ options: { cwd: { type: "string", default: process.cwd() }, bin: { type: "string", default: "target/release/richview" }, "publish-wasm": { type: "string", default: "target/wasm32-unknown-unknown/release/richview_publish_server.wasm" }, out: { type: "string" } } });
	assert.ok(values.out, "Usage: hosted-smoke.mjs --out <evidence-directory> [--bin <private-cli>] [--publish-wasm <private-wasm>] [--cwd <repo>]");
	const cwd = resolve(values.cwd);
	for (const path of [values.bin, values["publish-wasm"]]) assert.ok(existsSync(resolve(cwd, path)), `Missing built artifact: ${path}`);
	const evidence = await hostedSmoke({ cwd, bin: resolve(cwd, values.bin), wasm: resolve(cwd, values["publish-wasm"]), out: resolve(values.out) });
	console.log(JSON.stringify(evidence, null, 2));
	if (evidence.status !== "pass") process.exitCode = 1;
}
