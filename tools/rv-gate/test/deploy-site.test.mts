// Host reason (ARC-03): deployment tests use disposable files, fake commands and in-memory HTTP responses.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import test from "node:test";
import { checkLiveHttp, checkLiveRoutes, deploySite as deployImplementation, describeDeploymentFailure, prepareDeploymentWorktree, verifyBrowserSmoke, verifyLiveRoutes, verifySiteBuild } from "../deploy-site.mjs";

const hash = (value: string | Uint8Array) => createHash("sha256").update(value).digest("hex");
const commit = "a".repeat(40), tree = "b".repeat(40), old = "11111111-1111-4111-8111-111111111111", next = "22222222-2222-4222-8222-222222222222";
const now = () => new Date("2026-11-09T14:00:00Z");
const put = (file: string, value: string | Uint8Array) => { mkdirSync(dirname(file), { recursive: true }); writeFileSync(file, value); };
function fetchUrl(input: string | URL | Request) {
	return input instanceof Request ? input.url : String(input);
}

function smokeResult() {
	return {
		pages: [1440, 390].flatMap(width => ["/", "/playground", "/docs/", "/docs/charts", "/showcase/openai-vs-anthropic/", "/impressum", "/privacy"].map(path => ({ width, path, status: 200, runtimeStarted: true, problems: [] as string[] }))),
		frame: { runtimeStarted: true, charts: 18, problems: [] as string[] },
	};
}
// The browser fixture supplies the verdict inputs. It does not capture screenshots.
const deploySite = deployImplementation as unknown as (options: Omit<NonNullable<Parameters<typeof deployImplementation>[0]>, "smoke"> & {
	smoke: () => Promise<ReturnType<typeof smokeResult>>;
}) => Promise<{ id: string; result: string; version?: string; rollback?: { version: string }; live_checks: unknown[] }>;

function fixture(t: test.TestContext) {
	mkdirSync(join(process.cwd(), "tmp"), { recursive: true });
	const root = mkdtempSync(join(process.cwd(), "tmp/deploy-site-"));
	t.after(() => rmSync(root, { recursive: true, force: true }));
	const cwd = join(root, "checkout"), store = join(root, "gate/evidence"), snapshot = join(store, "runs/hotfix/build"), gitDir = join(root, "repo/.git/worktrees/candidate");
	put(join(cwd, ".git"), `gitdir: ${gitDir}\n`); put(join(gitDir, "locked"), "active stream");
	put(join(cwd, "wrangler.toml"), readFileSync(new URL("../../../wrangler.toml", import.meta.url)));
	const typedWorker = new URL("../../../cloudflare/interest-worker.ts", import.meta.url);
	if (existsSync(typedWorker)) put(join(cwd, "cloudflare/interest-worker.ts"), readFileSync(typedWorker));
	put(join(cwd, "package.json"), JSON.stringify({ devDependencies: { wrangler: "4.154.0" } }));
	put(join(cwd, "node_modules/wrangler/package.json"), JSON.stringify({ version: "4.154.0" }));
	put(join(cwd, "package-lock.json"), JSON.stringify({ packages: { "node_modules/wrangler": { version: "4.154.0" } } }));
	put(join(cwd, "Cargo.lock"), "lock"); put(join(cwd, "rust-toolchain.toml"), "toolchain");
	const routes = { serve: ["/"], redirect: { "/app": "/playground" } as Record<string, string>, gone: ["/private"], install: ["/install"] };
	put(join(cwd, "assets/routes.json"), JSON.stringify(routes));
	put(join(cwd, "assets/install/release.json"), JSON.stringify({ enabled: false }));
	const headers = "/*\n  X-Content-Type-Options: nosniff\n  Referrer-Policy: strict-origin-when-cross-origin\n/\n  Content-Security-Policy: default-src 'self'; object-src 'none'\n/richview_runtime.js\n  Access-Control-Allow-Origin: *\n/richview_runtime_bg.wasm\n  Access-Control-Allow-Origin: *\n";
	const assets: Record<string, string> = { "index.html": "<h1>new candidate</h1>", "_headers": headers, "_redirects": "", "richview_runtime.js": "runtime", "richview_runtime_bg.wasm": "runtime wasm", "richview_site.js": "site", "richview_site_bg.wasm": "site wasm" };
	for (const [file, value] of Object.entries(assets)) put(join(snapshot, "dist/web", file), value);
	const manifest = { schema: 1, mode: "full", inputs: { commit, dirty: false, source_diff_sha256: hash(""), config: Object.fromEntries(["Cargo.lock", "package-lock.json", "rust-toolchain.toml"].map(file => [file, hash(readFileSync(join(cwd, file)))])) }, outputs: Object.fromEntries(Object.entries(assets).map(([file, value]) => [`checkout:dist/web/${file}`, { bytes: Buffer.byteLength(value), sha256: hash(value) }])) };
	const receipt = { id: `hotfix-20261109T130000Z-${commit.slice(0, 10)}`, kind: "hotfix", result: "pass", candidate: "candidate/9", commit, tree, prod: "c".repeat(40), sourceCommit: "d".repeat(40), steps: [{ id: "fast", status: "pass" }], build: { snapshot, manifestSha256: "" } };
	const receiptFile = join(store, "receipts", `${receipt.id}.json`);
	const save = () => { put(join(snapshot, "build-manifest.json"), JSON.stringify(manifest)); receipt.build.manifestSha256 = hash(readFileSync(join(snapshot, "build-manifest.json"))); put(receiptFile, JSON.stringify(receipt)); };
	save();
	let deployed = false, listCount = 0;
	const calls: { program: string; args: string[] }[] = [], http: { url: string; options?: RequestInit }[] = [];
	const state = { head: commit, prod: commit, candidate: commit, tree, dirty: "", cloudflarePaths: [] as string[], changedWorker: false, routeFailure: false, deployFailure: false, advanceProd: false, promotionLocked: false };
	const deployment = (version: string) => JSON.stringify([{ id: "deployment", created_on: "2026-11-09T13:00:00Z", versions: [{ version_id: version, percentage: 100 }] }]);
	const run = (program: string, args: string[]) => {
		calls.push({ program, args }); let stdout = "";
		if (program === "git") {
			if (args[0] === "status") stdout = state.dirty;
			else if (args[0] === "diff") {
				assert.deepEqual(args, ["diff", "--name-only", "--no-renames", "-z", receipt.prod, commit, "--", "cloudflare/"]);
				stdout = state.cloudflarePaths.map(path => `${path}\0`).join("");
			}
			else if (args[0] === "rev-parse") stdout = ({ HEAD: state.head, "HEAD^{tree}": state.tree, "refs/remotes/origin/prod": state.prod, "refs/remotes/origin/candidate/9": state.candidate, "--absolute-git-dir": gitDir, "--git-common-dir": join(root, "repo/.git") } as Record<string, string>)[args[1]];
			else assert.equal(args[0], "fetch");
		} else {
			assert.equal(program, join(cwd, "node_modules/.bin/wrangler"));
			if (args[0] === "--version") stdout = "4.154.0\n";
			else if (args[0] === "deployments") { listCount++; stdout = deployment(deployed || (state.changedWorker && listCount > 1) ? next : old); }
			else if (args[0] === "versions") stdout = JSON.stringify({ id: old });
			else if (args[0] === "deploy") {
				assert.equal(state.promotionLocked, true, "the promotion lock must cover deployment");
				const backup = join(store, "runs", `deploy-20261109T140000Z-${commit.slice(0, 10)}`, "rollback.json");
				assert.equal(JSON.parse(readFileSync(backup, "utf8")).version, old, "rollback record must be durable before deployment");
				assert.equal(readFileSync(join(cwd, "dist/web/index.html"), "utf8"), assets["index.html"]);
				assert.equal(existsSync(join(cwd, "dist/web/stale.html")), false);
				if (state.deployFailure) return { status: 1, stdout: "upload interrupted", stderr: "failed", error: undefined };
				deployed = true; stdout = `Current Version ID: ${next}\n`;
				if (state.advanceProd) state.prod = "f".repeat(40);
			} else assert.fail(`Unexpected Wrangler command: ${args}`);
		}
		assert.equal(typeof stdout, "string"); return { status: 0, stdout, stderr: "", error: undefined };
	};
	const fetch: typeof globalThis.fetch = async (input, options) => {
		const url = fetchUrl(input);
		assert.ok(options);
		http.push({ url, options }); assert.equal(options.redirect, "manual"); assert.equal(options.method, undefined);
		const path = new URL(url).pathname;
		if (url.includes("workers.dev")) return new Response("", { status: 302, headers: { location: "https://richview.cloudflareaccess.com/cdn-cgi/access/login" } });
		if (state.routeFailure && path === "/") return new Response("foreign deployment", { status: 200 });
		if (routes.redirect[path]) return new Response("", { status: 301, headers: { location: routes.redirect[path] } });
		if (routes.gone.includes(path)) return new Response("", { status: 404 });
		if (url.endsWith("/api/interest")) return new Response("", { status: 405, headers: { allow: "POST" } });
		if ([...routes.install, "/install.sh", "/install-release.json", "/downloads/"].includes(path)) return new Response("", { status: 404 });
		const headers: Record<string, string> = { "x-content-type-options": "nosniff", "referrer-policy": "strict-origin-when-cross-origin" };
		if (url.endsWith(".js") || url.endsWith(".wasm")) headers["access-control-allow-origin"] = "*";
		else headers["content-security-policy"] = "default-src 'self'; object-src 'none'";
		return new Response(assets[path === "/" ? "index.html" : path.slice(1)] ?? "", { headers });
	};
	put(join(cwd, "dist/web/stale.html"), "stale bytes");
	const options = { cwd, store, receipt: receiptFile, go: "Daniel: deploy this candidate, 2026-11-09", run, fetch, smoke: async () => smokeResult(), now, lockPromotion: () => { state.promotionLocked = true; return () => { state.promotionLocked = false; }; } };
	return { root, cwd, store, snapshot, gitDir, manifest, receipt, receiptFile, assets, routes, calls, http, state, save, options, deployCalls: () => calls.filter(call => call.args[0] === "deploy") };
}

test("deployment stages only verified bytes, preserves rollback first and writes one passing receipt", async t => {
	const f = fixture(t), report = await deploySite(f.options);
	assert.ok(report && "version" in report && "rollback" in report);
	assert.equal(report.result, "pass"); assert.equal(report.version, next); assert.equal(report.rollback?.version, old);
	assert.equal(report.live_checks.length, 3); assert.equal(f.deployCalls().length, 1);
	assert.deepEqual(f.deployCalls()[0].args, ["deploy", "--keep-vars", "--config", join(f.cwd, "wrangler.toml")]);
	assert.equal(f.http.length, 17);
	assert.deepEqual(JSON.parse(readFileSync(join(f.store, "receipts", `${report.id}.json`), "utf8")), report);
	assert.equal(existsSync(join(dirname(f.store), "deploy-site.lock")), false);
	assert.equal(f.state.promotionLocked, false);
	await assert.rejects(deploySite(f.options), /receipt already exists/);
	assert.equal(f.deployCalls().length, 1, "an existing receipt cannot authorize a replay");
});

test("missing go and invalid receipt refuse before any command or publication", async t => {
	for (const kind of ["go", "short-go", "result", "step", "snapshot", "candidate"]) {
		const f = fixture(t);
		if (kind === "go") f.options.go = " ";
		if (kind === "short-go") f.options.go = "yes go";
		if (kind === "result") f.receipt.result = "fail";
		if (kind === "step") f.receipt.steps[0].status = "unrun";
		if (kind === "snapshot") f.receipt.build.snapshot = "relative";
		if (kind === "candidate") f.receipt.candidate = "dev";
		f.save(); await assert.rejects(deploySite(f.options)); assert.equal(f.calls.length, 0, kind);
	}
});

test("candidate, clean worktree, lock and exact fetched prod are deployment preconditions", async t => {
	for (const kind of ["head", "tree", "prod", "candidate", "dirty", "lock", "pin"]) {
		const f = fixture(t);
		if (kind === "head" || kind === "tree" || kind === "prod" || kind === "candidate") f.state[kind] = "e".repeat(40);
		if (kind === "dirty") f.state.dirty = " M cloudflare/interest-worker.mjs";
		if (kind === "lock") rmSync(join(f.gitDir, "locked"));
		if (kind === "pin") put(join(f.cwd, "node_modules/wrangler/package.json"), '{"version":"4.153.0"}');
		await assert.rejects(deploySite(f.options)); assert.equal(f.deployCalls().length, 0, kind);
	}
});

test("Worker code and D1 migration changes refuse before any Wrangler or network command", async t => {
	for (const path of ["cloudflare/interest-worker.mjs", "cloudflare/migrations/0002.sql", "cloudflare/publish/adapter.mjs"]) {
		const f = fixture(t); f.state.cloudflarePaths = [path];
		await assert.rejects(deploySite(f.options), /changes cloudflare\/ Worker code or migrations/);
		assert.equal(f.calls.every(call => call.program === "git"), true);
		assert.equal(f.http.length, 0); assert.equal(f.deployCalls().length, 0);
		assert.equal(readFileSync(join(f.cwd, "dist/web/stale.html"), "utf8"), "stale bytes");
	}
});

test("the exact asset set accepts a directory alongside a same-prefix file", t => {
	const f = fixture(t), web = join(f.snapshot, "dist/web");
	for (const [path, value] of [["docs/charts.html", "gallery"], ["docs/charts/alluvial.html", "chart"]]) {
		put(join(web, path), value);
		f.manifest.outputs[`checkout:dist/web/${path}`] = { bytes: Buffer.byteLength(value), sha256: hash(value) };
	}
	f.save();
	const verified = verifySiteBuild({ receipt: f.receipt, cwd: f.cwd });
	const files = verified.files as Record<string, unknown>;
	assert.ok(files["docs/charts.html"] && files["docs/charts/alluvial.html"]);
	put(join(web, "docs/charts/alluvial.html"), "wrong");
	assert.throws(() => verifySiteBuild({ receipt: f.receipt, cwd: f.cwd }), /Web asset hash changed/);
});

test("snapshot mutations, extra files, symlinks and malformed manifest paths fail closed", async t => {
	for (const kind of ["changed", "missing", "extra", "symlink", "manifest", "provenance", "path", "install"]) {
		const f = fixture(t), web = join(f.snapshot, "dist/web");
		if (kind === "changed") put(join(web, "index.html"), "foreign bytes");
		if (kind === "missing") rmSync(join(web, "index.html"));
		if (kind === "extra") put(join(web, "foreign.txt"), "foreign");
		if (kind === "symlink") { rmSync(join(web, "index.html")); symlinkSync(join(f.cwd, "Cargo.lock"), join(web, "index.html")); }
		if (kind === "manifest") put(join(f.snapshot, "build-manifest.json"), "{}");
		if (kind === "provenance") { f.manifest.inputs.commit = "e".repeat(40); f.save(); }
		if (kind === "path") { f.manifest.outputs["checkout:dist/web/../outside"] = { bytes: 0, sha256: hash("") }; f.save(); }
		if (kind === "install") { f.manifest.outputs["checkout:dist/web/install.html"] = { bytes: 0, sha256: hash("") }; put(join(web, "install.html"), ""); f.save(); }
		assert.throws(() => verifySiteBuild({ receipt: f.receipt, cwd: f.cwd }), kind);
		await assert.rejects(deploySite(f.options)); assert.equal(f.deployCalls().length, 0, kind);
	}
});

test("a route named as an .html file is refused before any deploy, since the live site redirects it", async t => {
	for (const [list, route] of [["serve", "/docs/example/Rendered/publisher.html"], ["install", "/install.html"]]) {
		const f = fixture(t), file = join(f.cwd, "assets/routes.json");
		const routes = JSON.parse(readFileSync(file, "utf8"));
		routes[list].push(route);
		put(file, JSON.stringify(routes));
		assert.throws(() => verifySiteBuild({ receipt: f.receipt, cwd: f.cwd }), /names an \.html file/, list);
		await assert.rejects(deploySite(f.options)); assert.equal(f.deployCalls().length, 0, list);
	}
});

test("another account, Worker, binding, route or extra config cannot use this deployment lane", async t => {
	for (const [before, after] of [["f321662ae21d0592d8b75bd8ddecc241", "0".repeat(32)], ["richview-teaser", "other-worker"], ["INTEREST_DB", "OTHER_DB"], ["richview.ai", "other.example"], ["workers_dev = true", "workers_dev = false"], ['not_found_handling = "404-page"', 'not_found_handling = "none"']]) {
		const f = fixture(t), path = join(f.cwd, "wrangler.toml");
		put(path, readFileSync(path, "utf8").replace(before, after));
		await assert.rejects(deploySite(f.options), /approved site Worker configuration/); assert.equal(f.deployCalls().length, 0);
	}
	const f = fixture(t), path = join(f.cwd, "wrangler.toml"); put(path, `${readFileSync(path, "utf8")}\n[[routes]]\npattern = "extra.example"\ncustom_domain = true\n`);
	await assert.rejects(deploySite(f.options), /approved site Worker configuration/); assert.equal(f.deployCalls().length, 0);
});

test("an unknown path is answered with the site's 404 page", () => {
	// Cloudflare serves the nearest 404.html, with status 404, only when the assets say so; the
	// build must then keep the page it stages. The live and the local review configuration agree.
	const read = (path: string) => readFileSync(new URL(`../../../${path}`, import.meta.url), "utf8");
	for (const config of ["wrangler.toml", "wrangler.interest-preview.toml"]) {
		const assets = read(config).split(/^\[/m).find(section => section.startsWith("assets]"));
		assert.ok(assets);
		assert.match(assets, /^not_found_handling = "404-page"$/m, config);
	}
	assert.ok(JSON.parse(read("assets/routes.json")).keep.includes("404.html"));
	assert.match(read("assets/404.html"), /<h1>Page not found<\/h1>/);
});

test("a changed rollback version refuses deployment and keeps the backed-up target", async t => {
	const f = fixture(t); f.state.changedWorker = true;
	await assert.rejects(deploySite(f.options), /Worker changed since rollback backup/);
	assert.equal(f.deployCalls().length, 0);
	const result = JSON.parse(readFileSync(join(f.store, "receipts", `deploy-20261109T140000Z-${commit.slice(0, 10)}.json`), "utf8"));
	assert.equal(result.rollback.version, old); assert.equal(result.deploymentAttempted, false); assert.equal(result.result, "fail");
});

test("wrong live bytes and interrupted deploys produce failure receipts with rollback", async t => {
	for (const kind of ["routeFailure", "deployFailure"] as const) {
		const f = fixture(t); f.state[kind] = true;
		await assert.rejects(deploySite(f.options), kind === "routeFailure" ? /Live route check failed/ : /deploy failed/);
		const result = JSON.parse(readFileSync(join(f.store, "receipts", `deploy-20261109T140000Z-${commit.slice(0, 10)}.json`), "utf8"));
		assert.equal(result.result, "fail"); assert.equal(result.rollback.version, old); assert.equal(result.deploymentAttempted, true);
		assert.equal(result.live_checks[0].status, kind === "routeFailure" ? "fail" : "unrun");
		assert.equal(result.live_checks[1].status, "unrun");
		assert.equal(f.http.some(row => row.url.endsWith("/install")), false); assert.equal(f.deployCalls().length, 1);
		assert.equal(f.state.promotionLocked, false);
	}
});

test("post-deploy failure prints the recorded rollback command without executing it", async t => {
	const f = fixture(t); f.state.routeFailure = true;
	let failure;
	try { await deploySite(f.options); } catch (error) { failure = error; }
	assert.ok(failure instanceof Error && "receipt" in failure);
	const evidence = failure.receipt as { rollback: { command: string[] } };
	const text = describeDeploymentFailure(failure);
	assert.match(text, /Recorded rollback \(follow DEL-48\):/);
	assert.ok(text.includes(evidence.rollback.command.join(" ")));
	assert.equal(f.calls.some(call => call.args[0] === "rollback"), false);
	assert.doesNotMatch(describeDeploymentFailure(new Error("preflight failed")), /Recorded rollback/);
});

test("a remote prod advance after deployment is detected before the receipt can pass", async t => {
	const f = fixture(t); f.state.advanceProd = true;
	await assert.rejects(deploySite(f.options), /current fetched origin\/prod/);
	const result = JSON.parse(readFileSync(join(f.store, "receipts", `deploy-20261109T140000Z-${commit.slice(0, 10)}.json`), "utf8"));
	assert.equal(result.result, "fail"); assert.equal(result.version, next); assert.equal(result.rollback.version, old);
	assert.equal(f.deployCalls().length, 1); assert.equal(f.state.promotionLocked, false);
});

test("a failed browser frame is recorded as failed while its preceding live checks remain passed", async t => {
	const f = fixture(t);
	f.options.smoke = async () => { const result = smokeResult(); result.frame.charts = 0; return result; };
	await assert.rejects(deploySite(f.options), /Flagship frame\/runtime check failed/);
	const result = JSON.parse(readFileSync(join(f.store, "receipts", `deploy-20261109T140000Z-${commit.slice(0, 10)}.json`), "utf8"));
	assert.deepEqual(result.live_checks.map((row: { status: string }) => row.status), ["pass", "pass", "fail"]);
	assert.equal(result.rollback.version, old); assert.equal(result.result, "fail"); assert.equal(f.state.promotionLocked, false);
});

test("live HTTP probes reject exposed install, API, Access or security header regressions", async t => {
	for (const kind of ["install", "interest", "preview", "headers"]) {
		const f = fixture(t), fake: typeof globalThis.fetch = async (input, options) => {
			const url = fetchUrl(input);
			if ((kind === "install" && url.endsWith("/install")) || (kind === "interest" && url.endsWith("/api/interest")) || (kind === "preview" && url.includes("workers.dev")) || (kind === "headers" && url === "https://richview.ai/")) return new Response("exposed", { status: 200 });
			return f.options.fetch(url, options);
		};
		await assert.rejects(checkLiveHttp({ cwd: f.cwd, web: join(f.snapshot, "dist/web"), fetch: fake }), kind);
	}
});

test("enabled install page and retired hosted routes are checked", async t => {
	const f = fixture(t), web = join(f.snapshot, "dist/web");
	const command = "curl -fsSL https://github.com/1vecera/richview/releases/latest/download/install.sh | sh";
	const page = `<h1>Install RichView</h1><pre>${command}</pre>`;
	put(join(f.cwd, "assets/install/release.json"), JSON.stringify({ enabled: true }));
	put(join(web, "install.html"), page);
	f.manifest.outputs["checkout:dist/web/install.html"] = { bytes: Buffer.byteLength(page), sha256: hash(page) }; f.save();
	const webFiles = verifySiteBuild({ receipt: f.receipt, cwd: f.cwd }).files;
	const files = webFiles as Record<string, unknown>;
	assert.ok(files["install.html"]);
	const fetch: typeof globalThis.fetch = async (url, options) => fetchUrl(url).endsWith("/install") ? new Response(page) : f.options.fetch(url, options);
	assert.equal((await checkLiveHttp({ cwd: f.cwd, web, fetch })).status, "pass");
	const routes = await checkLiveRoutes({ cwd: f.cwd, web, webFiles, fetch });
	assert.equal(verifyLiveRoutes(routes, { cwd: f.cwd, webFiles }).status, "pass");
	assert.ok(routes.results.some(row => row.kind === "serve" && row.route === "/install" && row.ok));
	const incomplete = { ...routes, results: routes.results.filter(row => !(row.kind === "serve" && row.route === "/install")) };
	assert.throws(() => verifyLiveRoutes(incomplete, { cwd: f.cwd, webFiles }), /differ from staged routes/);
	await assert.rejects(checkLiveHttp({ cwd: f.cwd, web, fetch: async (url, options) => fetchUrl(url).endsWith("/install") ? new Response("wrong") : fetch(url, options) }), /GitHub release page/);
	await assert.rejects(checkLiveHttp({ cwd: f.cwd, web, fetch: async (url, options) => fetchUrl(url).endsWith("/install.sh") ? new Response("old") : fetch(url, options) }), /Retired installer route/);
	put(join(web, "install.html"), "wrong");
	assert.throws(() => verifySiteBuild({ receipt: f.receipt, cwd: f.cwd }), /GitHub release command/);
});

test("browser verdict requires every page and width, runtime startup and the embedded chart frame", () => {
	assert.equal(verifyBrowserSmoke(smokeResult()).status, "pass");
	for (const mutate of [(result: ReturnType<typeof smokeResult>) => result.pages.pop(), (result: { pages: { problems: string[]; }[]; }) => result.pages[0].problems.push("CSP violation"), (result: { pages: { runtimeStarted: boolean; }[]; }) => result.pages[0].runtimeStarted = false, (result: { frame: { runtimeStarted: boolean; }; }) => result.frame.runtimeStarted = false, (result: { frame: { charts: number; }; }) => result.frame.charts = 0]) {
		const result = smokeResult(); mutate(result); assert.throws(() => verifyBrowserSmoke(result));
	}
});

test("another deployment owner's lock refuses without removing its lock", async t => {
	const f = fixture(t), lock = join(dirname(f.store), "deploy-site.lock"); mkdirSync(lock);
	put(join(lock, "owner.json"), JSON.stringify({ pid: 12345, host: "deployment-host", receipt: "prior-deploy" }));
	await assert.rejects(deploySite(f.options), error => {
		assert.ok(error instanceof Error);
		assert.match(error.message, /EEXIST/); assert.match(error.message, /ps -p 12345 -o pid=,lstart=,command=/);
		assert.match(error.message, /deployment-host/); assert.match(error.message, /Do not clear a live owner's lock/);
		assert.match(error.message, /Wrangler\/browser children have stopped/); assert.match(error.message, /rmdir/);
		return true;
	});
	assert.equal(f.calls.length, 0); assert.equal(existsSync(lock), true);
	assert.equal(JSON.parse(readFileSync(join(lock, "owner.json"), "utf8")).receipt, "prior-deploy");
	rmSync(join(lock, "owner.json"));
	await assert.rejects(deploySite(f.options), /owner file is missing, first verify no deployment is active/);
	assert.equal(existsSync(lock), true);
});

test("the CLI preparation creates and locks only its own detached candidate worktree", t => {
	const f = fixture(t), calls: string[][] = [];
	const run = (program: string, args: string[]) => {
		assert.equal(program, "git"); calls.push(args);
		if (args[0] === "rev-parse") return { status: 0, stdout: args[1] === "--git-common-dir" ? join(f.root, "repo/.git") : commit, stderr: "", error: undefined };
		if (args[0] === "fetch") return { status: 0, stdout: "", stderr: "", error: undefined };
		assert.equal(args[0], "worktree");
		if (args[1] === "add") mkdirSync(args[3], { recursive: true });
		return { status: 0, stdout: "", stderr: "", error: undefined };
	};
	const worktree = prepareDeploymentWorktree({ ...f.options, run });
	assert.equal(worktree.path, join(f.root, "repo/.claude/worktrees", `deploy-${f.receipt.id}`));
	assert.deepEqual(calls.filter(args => args[0] === "worktree"), [["worktree", "add", "--detach", worktree.path, commit], ["worktree", "lock", "--reason", `deployment ${f.receipt.id}`, worktree.path]]);
	assert.equal(readFileSync(join(worktree.path, "node_modules/wrangler/package.json"), "utf8"), '{"version":"4.154.0"}');
	assert.equal(readFileSync(join(f.cwd, "dist/web/stale.html"), "utf8"), "stale bytes");
	assert.throws(() => prepareDeploymentWorktree({ ...f.options, run }), /worktree already exists/);
});
