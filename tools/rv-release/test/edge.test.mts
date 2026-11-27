// Host reason (ARC-03): tests of the check before the deploy: the staged site answered by a local edge, with release 31's .html redirect as the case (DEL-83).
import test from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import type { AddressInfo } from "node:net";
import type { CheckContext, SiteChecker } from "../lib/predeploy.mts";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { edgeEnvironment, edgeFetch, startEdge } from "../lib/edge.mts";
import { siteEdge } from "../lib/predeploy.mts";
import { INSTALL_COMMAND, stagedHeaders } from "../lib/probes.mts";
import { checkLiveRoutes, verifyLiveRoutes } from "../../rv-gate/deploy-site.mjs";
import { harness } from "./harness.mts";

const sha = (bytes: Buffer) => createHash("sha256").update(bytes).digest("hex");
function files(root: string, prefix = ""): string[] {
	return readdirSync(join(root, prefix)).sort().flatMap((name) => { const rel = prefix ? `${prefix}/${name}` : name; return statSync(join(root, rel)).isDirectory() ? files(root, rel) : [rel]; });
}

/** A staged deploy folder: the routes map, the install switch, and a built site of a few pages. */
function stagedSite({ serve = ["/", "/page"] } = {}) {
	const worktree = mkdtempSync(join(tmpdir(), "rvedge-")), web = join(worktree, "dist/web");
	mkdirSync(join(worktree, "assets/install"), { recursive: true });
	mkdirSync(web, { recursive: true });
	writeFileSync(join(worktree, "assets/routes.json"), JSON.stringify({ serve, install: ["/install"], redirect: { "/old": "/page" }, gone: ["/gone"] }));
	writeFileSync(join(worktree, "assets/install/release.json"), JSON.stringify({ enabled: true }));
	writeFileSync(join(web, "index.html"), "<h1>home</h1>");
	writeFileSync(join(web, "page.html"), "<h1>page</h1>");
	writeFileSync(join(web, "doc.html"), "<h1>doc</h1>");
	writeFileSync(join(web, "install.html"), `<pre>${INSTALL_COMMAND}</pre>`);
	writeFileSync(join(web, "richview_runtime.js"), "// runtime");
	writeFileSync(join(web, "richview_runtime_bg.wasm"), "wasm");
	writeFileSync(join(web, "_headers"), "/*\n  X-Content-Type-Options: nosniff\n  Referrer-Policy: no-referrer\n/\n  Content-Security-Policy: default-src 'self'\n");
	return { worktree, web };
}

/** A stand-in for the Workers runtime serving that folder: `.html` names redirect to the page without it, as Cloudflare answers them. */
async function fakeEdge(web: string) {
	const server = createServer((req, res) => {
		const path = new URL(req.url!, "http://x").pathname, base = `http://${req.headers.host}`;
		const send = (status: number, body: string | Buffer = "", headers = {}) => { res.writeHead(status, headers); res.end(body); };
		if (path === "/api/interest") return send(405, "", { allow: "POST" });
		if (path === "/old") return send(301, "", { location: `${base}/page` });
		if (path.endsWith(".html")) return send(307, "", { location: `${base}${path.slice(0, -5).replace(/index$/, "") || "/"}` });
		const file = path === "/" ? "index.html" : existsSync(join(web, path.slice(1))) && statSync(join(web, path.slice(1))).isFile() ? path.slice(1) : existsSync(join(web, `${path.slice(1)}.html`)) ? `${path.slice(1)}.html` : null;
		if (!file || ["/gone", "/install.sh", "/install-release.json", "/downloads/"].includes(path)) return send(404, "not found");
		send(200, readFileSync(join(web, file)), stagedHeaders(web, path));
	});
	await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
	return { base: `http://127.0.0.1:${(server.address() as AddressInfo).port}`, stop: () => new Promise<void>((resolve) => server.close(() => resolve())) };
}

function siteDeps(worktree: string, web: string, edges: Awaited<ReturnType<typeof fakeEdge>>[]): NonNullable<CheckContext["deps"]> {
	const filesOf = () => Object.fromEntries(files(web).map((file) => [file, { sha256: sha(readFileSync(join(web, file))), bytes: statSync(join(web, file)).size }]));
	return {
		worktree, kept: join(worktree, "kept"), wrangler: "unused", evidence: false,
		git: (_, ...args) => (args[0] === "rev-parse" ? "2c220f5929aca0cc0b59cdae0a11d5b77b9a2db2" : ""),
		site: { verifySiteBuild: () => ({ files: filesOf() }), checkLiveRoutes: checkLiveRoutes as SiteChecker["checkLiveRoutes"], verifyLiveRoutes },
		startEdge: async () => { const edge = await fakeEdge(web); edges.push(edge); return edge; },
	};
}

test("the staged site passes when the local edge answers every route, redirect, file and probe as staged", async () => {
	const h = harness({ journalKeys: [["M", "a".repeat(64)]] });
	const { worktree, web } = stagedSite(), edges: Awaited<ReturnType<typeof fakeEdge>>[] = [];
	const result = await siteEdge({ desc: h.desc, journal: h.journal, repo: h.dir, scratch: join(h.dir, "scratch"), deps: siteDeps(worktree, web, edges) });
	assert.equal(result.status, "pass");
	assert.match(result.detail, /\d+ routes and files and 8 probes answered by the local edge/);
});

test("a route the edge redirects fails the check before the deploy, as release 31's publisher.html did on the live site", async () => {
	const h = harness({ journalKeys: [["M", "a".repeat(64)]] });
	const { worktree, web } = stagedSite({ serve: ["/", "/page", "/doc.html"] });
	await assert.rejects(siteEdge({ desc: h.desc, journal: h.journal, repo: h.dir, scratch: join(h.dir, "scratch"), deps: siteDeps(worktree, web, []) }), /1 of \d+ routes and files differ at the local edge: serve \/doc\.html answered 307 to https:\/\/richview\.ai\/doc/);
});

test("a changed staged header fails the probes", async () => {
	const h = harness({ journalKeys: [["M", "a".repeat(64)]] });
	const { worktree, web } = stagedSite();
	const deps = siteDeps(worktree, web, []);
	const startOriginal = deps.startEdge!;
	deps.startEdge = async (options) => { const edge = await startOriginal(options); writeFileSync(join(web, "_headers"), "/*\n  X-Content-Type-Options: nosniff\n  Referrer-Policy: origin\n/\n  Content-Security-Policy: default-src 'self'\n"); return edge; };
	await assert.rejects(siteEdge({ desc: h.desc, journal: h.journal, repo: h.dir, scratch: join(h.dir, "scratch"), deps }), /Staged asset changed|Header differs/);
});

test("the check refuses a worktree that is not at the candidate or is not clean, and a journal without the manifest digest", async () => {
	const h = harness({ journalKeys: [["M", "a".repeat(64)]] });
	const { worktree, web } = stagedSite();
	const base = { desc: h.desc, journal: h.journal, repo: h.dir, scratch: join(h.dir, "scratch") };
	await assert.rejects(siteEdge({ ...base, deps: { ...siteDeps(worktree, web, []), git: () => "0".repeat(40) } }), /is not at/);
	await assert.rejects(siteEdge({ ...base, deps: { ...siteDeps(worktree, web, []), git: (_, ...a) => (a[0] === "rev-parse" ? h.desc.final : " M file") } }), /is not clean/);
	const empty = harness();
	await assert.rejects(siteEdge({ desc: empty.desc, journal: empty.journal, repo: empty.dir, scratch: join(empty.dir, "s"), deps: siteDeps(worktree, web, []) }), /no build manifest digest/);
});

test("the edge fetch answers only the live host from the local edge and rewrites a redirect back to the live host", async () => {
	const { web } = stagedSite(), edge = await fakeEdge(web);
	try {
		const fetch = edgeFetch(edge.base);
		const old = await fetch("https://richview.ai/old", { redirect: "manual" });
		assert.equal(old.status, 301);
		assert.equal(old.headers.get("location"), "https://richview.ai/page");
		assert.equal((await fetch("https://richview.ai/page", { redirect: "manual" })).status, 200);
		await assert.rejects(fetch("https://richview-teaser.vecera.workers.dev/"), /makes no request outside the local edge/);
	} finally { await edge.stop(); }
});

test("the local edge is started with no credential, waits until it answers and is stopped by its own PID", async () => {
	const dir = mkdtempSync(join(tmpdir(), "rvwrangler-"));
	const script = join(dir, "wrangler");
	writeFileSync(script, `#!/usr/bin/env node
const http = require("node:http"), fs = require("node:fs");
const port = Number(process.argv[process.argv.indexOf("--port") + 1]);
fs.writeFileSync(process.env.HOME + "/env.json", JSON.stringify({ env: process.env, argv: process.argv.slice(2) }));
setTimeout(() => http.createServer((q, s) => { s.writeHead(200); s.end("ok"); }).listen(port, "127.0.0.1"), 600);
`);
	chmodSync(script, 0o755);
	process.env.CLOUDFLARE_API_TOKEN = "must-not-reach-the-edge";
	const scratch = join(dir, "scratch");
	let edge;
	try {
		edge = await startEdge({ wrangler: script, config: join(dir, "wrangler.toml"), cwd: dir, scratch, pollMs: 100, timeoutMs: 20_000 });
		assert.match(edge.base, /^http:\/\/127\.0\.0\.1:\d+$/);
		const seen = JSON.parse(readFileSync(join(scratch, "home", "env.json"), "utf8"));
		assert.equal(seen.env.CLOUDFLARE_API_TOKEN, undefined, "no Cloudflare credential reaches the edge");
		assert.ok(seen.argv.includes("--local") && seen.argv.includes("127.0.0.1"), "local mode, loopback only");
		assert.equal(seen.env.HOME, join(scratch, "home"), "a home folder with no login");
		const pid = edge.pid;
		await edge.stop();
		assert.throws(() => process.kill(pid!, 0), "the edge process is gone");
	} finally { delete process.env.CLOUDFLARE_API_TOKEN; await edge?.stop(); }
});

test("a local edge that stops before it answers fails with its log", async () => {
	const dir = mkdtempSync(join(tmpdir(), "rvwrangler-"));
	const script = join(dir, "wrangler");
	writeFileSync(script, "#!/usr/bin/env node\nconsole.error('Missing binding INTEREST_DB'); process.exit(7);\n");
	chmodSync(script, 0o755);
	await assert.rejects(startEdge({ wrangler: script, config: "x", cwd: dir, scratch: join(dir, "s"), pollMs: 50, timeoutMs: 10_000 }), /exit 7[\s\S]*Missing binding INTEREST_DB/);
});

test("the edge environment holds a path, a scratch home and nothing from the caller's secrets", () => {
	const env = edgeEnvironment(mkdtempSync(join(tmpdir(), "rvenv-")), { PATH: "/bin", HOME: "/home/x", AWS_SECRET_ACCESS_KEY: "s", CLOUDFLARE_API_TOKEN: "t", TMPDIR: "/tmp" });
	assert.deepEqual(Object.keys(env).sort(), ["CI", "CLOUDFLARE_SEND_METRICS", "HOME", "NO_COLOR", "PATH", "TMPDIR", "WRANGLER_SEND_METRICS", "XDG_CONFIG_HOME"]);
	assert.notEqual(env.HOME, "/home/x");
});

test("the install command the edge probes expect is the one the gate's live check expects", () => {
	assert.ok(readFileSync(new URL(existsSync(new URL("../../rv-gate/deploy-site.mts", import.meta.url)) ? "../../rv-gate/deploy-site.mts" : "../../rv-gate/deploy-site.mjs", import.meta.url), "utf8").includes(`const INSTALL_COMMAND = "${INSTALL_COMMAND}";`));
});
