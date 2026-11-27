// Host reason (ARC-03): the deployment owner publishes a receipt-bound site only on Daniel's explicit go.
// Usage: node tools/rv-gate/deploy-site.mjs --receipt <hotfix-id> --go "<Daniel's words and date>"
// The CLI creates and locks its own detached candidate worktree. It removes it after a pass and keeps failures.
import { createHash, randomUUID } from "node:crypto";
import { spawnSync } from "node:child_process";
import type { SpawnSyncOptionsWithStringEncoding } from "node:child_process";
import { cpSync, existsSync, lstatSync, mkdirSync, readFileSync, readdirSync, realpathSync, renameSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import type { PathLike, PathOrFileDescriptor } from "node:fs";
import { createRequire } from "node:module";
import { hostname } from "node:os";
import { dirname, isAbsolute, join, relative, resolve, sep } from "node:path";
import { pathToFileURL } from "node:url";
import { acquireLock, evidenceDir, newReceiptId, writeOnce } from "./lib/store.mts";

type BuildSource = { snapshot: string; manifestSha256: string };
type Asset = { bytes: number; sha256: string };
type WebFiles = Record<string, Asset>;
type BuildManifest = {
	schema: number; mode: string;
	inputs: { commit: string; dirty: boolean; source_diff_sha256: string; config?: Record<string, string> };
	outputs: WebFiles;
};
type HotfixReceipt = {
	id: string; kind: string; result: string; commit: string; tree: string; prod: string; sourceCommit: string;
	candidate: string; steps: { status: string }[]; build: BuildSource;
};
type WorkerVersion = { percentage: number; version_id: string };
type WorkerDeployment = { created_on: string; versions: WorkerVersion[] };
type SiteRoutes = { serve: string[]; install: string[]; redirect: Record<string, string>; gone?: string[] | Record<string, unknown> };
type RouteRow = { kind: string; file?: string; route?: string; ok: boolean; status?: number; sha256?: string; location?: string | null; index?: number; error?: string };
type RouteProbe = { summary?: { base?: string; checked?: number; failed?: RouteRow[] }; results?: RouteRow[] };
type HttpProbe = { url: string; status: number; headers: Record<string, string>; sha256: string; body?: string };
type SmokePage = { path: string; width: number; status?: number; runtimeStarted: boolean; problems: string[]; shot?: string };
type SmokeResult = { pages: SmokePage[]; frame?: { runtimeStarted: boolean; charts: number; problems: string[] } };
type SmokeOptions = { cwd: string; out: string };
type CommandOptions = Omit<SpawnSyncOptionsWithStringEncoding, "encoding">;
type CommandRunner = (program: string, args: string[], options: CommandOptions) => { status: number | null; stdout: string; stderr?: string; error?: string };
type CandidateOptions = { cwd?: string; receipt?: string | string[]; go?: string; store?: string; run?: CommandRunner };
type Worktree = { path: string; source?: string; retainedOnFailure?: boolean };
type DeployOptions = CandidateOptions & { fetch?: typeof globalThis.fetch; smoke?: (options: SmokeOptions) => Promise<SmokeResult>; now?: () => Date; worktree?: Worktree; lockPromotion?: () => () => void };
type LiveCheck = { name: string; status: string; durationMs?: number; reason?: string };
type DeployReceipt = {
	id: string; kind: string; at: string; hotfix: string; candidate: string; commit: string; tree: string;
	go: string; target: string; worker: string; wrangler: string; sourceReceiptSha256: string;
	build: BuildSource; worktree: Worktree; commands: { name: string; program: string; args: string[]; status: number | null; durationMs: number }[];
	live_checks: LiveCheck[]; deploymentAttempted: boolean; result: string;
	configSha256?: string; assets?: WebFiles;
	rollback?: { version: string; deployment: WorkerDeployment; versionMetadata: unknown; command: string[] };
	promotionLock?: string; version?: string; error?: string; finished?: string;
};

const INSTALL_COMMAND = "curl -fsSL https://github.com/1vecera/richview/releases/latest/download/install.sh | sh";
const BASE = "https://richview.ai", PREVIEW = "https://richview-teaser.vecera.workers.dev";
const WRANGLER = "4.154.0", SHA = /^[0-9a-f]{64}$/, COMMIT = /^[0-9a-f]{40}$/;
const VERSION = /^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/;
// This lane has authority only for this existing site's configuration. Binding, route, account or
// compatibility changes need their own reviewed deployment path rather than a broader --go string.
const SITE_CONFIG = `name = "richview-teaser"
main = "cloudflare/interest-worker.mjs"
account_id = "f321662ae21d0592d8b75bd8ddecc241"
compatibility_date = "2026-10-04"
workers_dev = true
preview_urls = false
[assets]
directory = "./dist/web"
binding = "ASSETS"
html_handling = "auto-trailing-slash"
not_found_handling = "404-page"
run_worker_first = ["/api/*"]
[[d1_databases]]
binding = "INTEREST_DB"
database_name = "richview-interest"
database_id = "8610028f-1c4a-40de-8cff-599ae205eeed"
migrations_dir = "cloudflare/migrations"
[[ratelimits]]
name = "SIGNUP_RATE_LIMIT"
namespace_id = "61120261011"
[ratelimits.simple]
limit = 10
period = 60
[[routes]]
pattern = "richview.ai"
custom_domain = true`;
const configLines = (text: string) => text.split(/\r?\n/).map((line: string) => line.trim()).filter((line: string) => line && !line.startsWith("#")).join("\n");
const recordedGo = (go: unknown): go is string => typeof go === "string" && go.replace(/\s/g, "").length >= 8;
const shellWord = (value: string) => /^[A-Za-z0-9/_:.=-]+$/.test(value) ? value : `'${value.replaceAll("'", "'\\''")}'`;
const sha = (bytes: string | NodeJS.ArrayBufferView) => createHash("sha256").update(bytes).digest("hex");
function json<T>(file: PathOrFileDescriptor): T { return JSON.parse(readFileSync(file, "utf8")); }
function insist(ok: unknown, message: string): asserts ok { if (!ok) throw new Error(message); }
const regular = (file: PathLike) => insist(lstatSync(file).isFile(), `Expected a regular file: ${file}`);
const within = (base: PathLike, file: PathLike) => { const rel = relative(realpathSync(base), realpathSync(file)); return rel !== ".." && !rel.startsWith(`..${sep}`) && !isAbsolute(rel); };
function files(root: string, prefix = ""): string[] {
	insist(lstatSync(join(root, prefix)).isDirectory(), `Expected a regular directory: ${join(root, prefix)}`);
	return readdirSync(join(root, prefix)).sort().flatMap(name => {
		const rel = prefix ? `${prefix}/${name}` : name, stat = lstatSync(join(root, rel));
		insist(!stat.isSymbolicLink(), `Symlink in deployment assets: ${rel}`);
		if (stat.isDirectory()) return files(root, rel);
		insist(stat.isFile(), `Non-file deployment asset: ${rel}`);
		return [rel];
	});
}

/** Bind every deployed byte to the clean full build named by the immutable hotfix receipt. */
export function verifySiteBuild({ receipt, cwd, web = join(receipt.build.snapshot, "dist/web") }: { receipt: Pick<HotfixReceipt, "build" | "commit">; cwd: string; web?: string }) {
	const manifestFile = join(receipt.build.snapshot, "build-manifest.json");
	regular(manifestFile);
	const bytes = readFileSync(manifestFile), manifest = JSON.parse(bytes.toString("utf8")) as BuildManifest;
	insist(sha(bytes) === receipt.build.manifestSha256, "Hotfix build manifest hash changed");
	insist(manifest.schema === 1 && manifest.mode === "full", "Deployment requires a full schema-1 build");
	insist(manifest.inputs?.commit === receipt.commit && manifest.inputs?.dirty === false && manifest.inputs?.source_diff_sha256 === sha(""), "Hotfix build source differs from the clean candidate");
	for (const file of ["Cargo.lock", "package-lock.json", "rust-toolchain.toml"])
		insist(manifest.inputs.config?.[file] === sha(readFileSync(join(cwd, file))), `Build input changed: ${file}`);
	insist(manifest.outputs && typeof manifest.outputs === "object" && !Array.isArray(manifest.outputs), "Build outputs are missing");
	const outputs: WebFiles = {};
	for (const [key, record] of Object.entries(manifest.outputs)) {
		if (!key.startsWith("checkout:dist/web/")) continue;
		const rel = key.slice("checkout:dist/web/".length);
		insist(rel && !isAbsolute(rel) && !rel.includes("\\") && rel.split("/").every(part => part && part !== "." && part !== ".."), `Invalid web manifest path: ${key}`);
		insist(Number.isSafeInteger(record?.bytes) && record.bytes >= 0 && SHA.test(record?.sha256), `Invalid web manifest hash: ${key}`);
		outputs[rel] = record;
	}
	for (const required of ["index.html", "_headers", "richview_runtime.js", "richview_runtime_bg.wasm", "richview_site.js", "richview_site_bg.wasm"])
		insist(Object.hasOwn(outputs, required), `Build omits required web asset: ${required}`);
	const installEnabled = json<{ enabled?: boolean }>(join(cwd, "assets/install/release.json")).enabled;
	insist(typeof installEnabled === "boolean", "Install release switch is missing");
	insist(Object.hasOwn(outputs, "install.html") === installEnabled, "Install page must follow the release switch");
	insist(!Object.keys(outputs).some(file => ["install.sh", "install-release.json", "downloads"].includes(file.split("/")[0])), "Hosted installer artifacts are not allowed");
	// Cloudflare answers `/page.html` with a 307 to `/page`, so a route the live check requests must name the page as the live site serves it.
	const routes = json<SiteRoutes>(join(cwd, "assets/routes.json"));
	for (const route of [...routes.serve, ...(routes.install ?? [])]) insist(!route.endsWith(".html"), `Route ${route} names an .html file, which the live site redirects: name the page without .html`);
	if (installEnabled) insist(readFileSync(join(web, "install.html"), "utf8").includes(INSTALL_COMMAND), "Install page must show the GitHub release command");
	const actual = files(web).sort();
	insist(JSON.stringify(actual) === JSON.stringify(Object.keys(outputs).sort()), "Web assets differ from the exact manifest file set");
	for (const file of actual) {
		const bytes = readFileSync(join(web, file)), record = outputs[file];
		insist(bytes.length === record.bytes && sha(bytes) === record.sha256, `Web asset hash changed: ${file}`);
	}
	return { manifestSha256: sha(bytes), files: outputs };
}

function command(program: string, args: readonly string[], options: CommandOptions) {
	const result = spawnSync(program, args, { ...options, encoding: "utf8", timeout: 10 * 60_000, maxBuffer: 16 * 1024 * 1024 });
	return { status: result.status, stdout: result.stdout ?? "", stderr: result.stderr ?? "", error: result.error?.message };
}
function currentDeployment(text: string) {
	const deployments = JSON.parse(text) as WorkerDeployment[];
	insist(Array.isArray(deployments) && deployments.length > 0 && deployments.every(row => Number.isFinite(Date.parse(row.created_on))), "Cannot identify the current Worker deployment");
	const deployment = deployments.toSorted((a: { created_on: string; }, b: { created_on: string; }) => Date.parse(a.created_on) - Date.parse(b.created_on)).at(-1)!;
	insist(deployment.versions?.length === 1 && deployment.versions[0].percentage === 100 && VERSION.test(deployment.versions[0].version_id), "Rollback requires one current Worker version at 100%");
	return { deployment, version: deployment.versions[0].version_id };
}
function stagedHeaders(web: string, path: string) {
	const blocks: { path: string; headers: Record<string, string> }[] = [], lines = readFileSync(join(web, "_headers"), "utf8").split(/\r?\n/);
	let block: (typeof blocks)[number] | undefined;
	for (const line of lines) {
		if (!line.trim() || line.trimStart().startsWith("#")) continue;
		if (!/^\s/.test(line)) { block = { path: line.trim(), headers: {} }; blocks.push(block); }
		else {
			const match = line.trim().match(/^([^:]+):\s*(.*)$/);
			if (match && block) block.headers[match[1].toLowerCase()] = match[2];
		}
	}
	return Object.assign({}, ...blocks.filter(block => block.path === "/*" || block.path === path).map(block => block.headers)) as Record<string, string>;
}

/** Require the old route probe's actual rows, since its exit status alone does not report failures. */
export function verifyLiveRoutes(result: RouteProbe, { cwd, webFiles }: { cwd: string; webFiles: WebFiles }) {
	const routes = json<SiteRoutes>(join(cwd, "assets/routes.json"));
	const expected = [
		...[...routes.serve, ...(Object.hasOwn(webFiles, "install.html") ? routes.install : [])].map(route => `serve:${route}`),
		...Object.keys(routes.redirect).map(route => `redirect:${route}`),
		...(Array.isArray(routes.gone) ? routes.gone : Object.keys(routes.gone ?? {})).map(route => `gone:${route}`),
		...Object.keys(webFiles).filter(file => !["_headers", "_redirects", "worker.js"].includes(file.split("/")[0])).map(file => `file:${file}`),
	].sort();
	insist(result.summary?.base === BASE && Array.isArray(result.results) && result.summary.checked === expected.length && Array.isArray(result.summary.failed) && result.summary.failed.length === 0, "Live route check failed or omitted coverage");
	const actual = result.results.map(row => `${row.kind}:${row.kind === "file" ? row.file : row.route}`).sort();
	insist(JSON.stringify(actual) === JSON.stringify(expected) && result.results.every((row: { ok: boolean; }) => row.ok === true), "Live route rows failed or differ from staged routes/assets");
	return { status: "pass", checked: expected.length, summary: result.summary };
}

/** The tracked equivalent of tmp/deploy-2026-11-09/live_routes.py: every route and static body. */
export async function checkLiveRoutes({ cwd, web, webFiles, fetch = globalThis.fetch }: { cwd: string; web: string; webFiles: WebFiles; fetch?: typeof globalThis.fetch }) {
	const routes = json<SiteRoutes>(join(cwd, "assets/routes.json")), jobs: (() => Promise<RouteRow>)[] = [];
	const localFor = (route: string) => {
		const path = route.replace(/^\//, "");
		return [path, `${path}index.html`, `${path}.html`, `${path.replace(/\/$/, "")}.html`, `${path}/index.html`].find(file => Object.hasOwn(webFiles, file));
	};
	async function get(path: string) {
		const response = await fetch(BASE + path, { redirect: "manual", headers: { "Accept-Encoding": "identity", "User-Agent": "richview-deploy-check/1" }, signal: AbortSignal.timeout(30_000) });
		return { status: response.status, sha256: sha(Buffer.from(await response.arrayBuffer())), location: response.headers.get("location") };
	}
	for (const route of [...routes.serve, ...(Object.hasOwn(webFiles, "install.html") ? routes.install : [])]) jobs.push(async () => {
		const response = await get(route), file = localFor(route);
		return { kind: "serve", route, file, ...response, ok: response.status === 200 && !!file && response.sha256 === webFiles[file].sha256 };
	});
	for (const [route, target] of Object.entries(routes.redirect)) jobs.push(async () => {
		const response = await get(route);
		return { kind: "redirect", route, ...response, ok: [301, 308].includes(response.status) && [target, BASE + target].includes(response.location!) };
	});
	for (const route of Array.isArray(routes.gone) ? routes.gone : Object.keys(routes.gone ?? {})) jobs.push(async () => {
		const response = await get(route);
		return { kind: "gone", route, ...response, ok: response.status === 404 };
	});
	for (const file of Object.keys(webFiles).filter(file => !["_headers", "_redirects", "worker.js"].includes(file.split("/")[0]))) jobs.push(async () => {
		const route = "/" + (file.endsWith(".html") ? file.endsWith("index.html") ? file.slice(0, -10) : file.slice(0, -5) : file);
		const response = await get(route);
		return { kind: "file", file, route, ...response, ok: response.status === 200 && response.sha256 === webFiles[file].sha256 };
	});
	const results = new Array<RouteRow>(jobs.length);
	let cursor = 0;
	await Promise.all(Array.from({ length: Math.min(12, jobs.length) }, async () => {
		for (;;) {
			const index = cursor++;
			if (index >= jobs.length) return;
			try { results[index] = await jobs[index](); }
			catch (error) { results[index] = { kind: "error", index, ok: false, error: (error as Error).message }; }
		}
	}));
	// Recheck local bytes as well: a changed staged file cannot pass against a manifest from before it.
	for (const [file, record] of Object.entries(webFiles)) insist(sha(readFileSync(join(web, file))) === record.sha256, `Staged asset changed during live route checks: ${file}`);
	return { summary: { base: BASE, checked: results.length, failed: results.filter(row => !row.ok), by_kind: Object.fromEntries(["serve", "redirect", "gone", "file"].map(kind => [kind, results.filter(row => row.kind === kind).length])) }, results };
}

/** Read-only live probes: never submit the signup form or write D1. */
export async function checkLiveHttp({ cwd, web, fetch = globalThis.fetch }: { cwd: string; web: string; fetch?: typeof globalThis.fetch }) {
	const probes: HttpProbe[] = [];
	async function get(url: string): Promise<HttpProbe> {
		const response = await fetch(url, { redirect: "manual", headers: { "Accept-Encoding": "identity", "User-Agent": "richview-deploy-check/1" }, signal: AbortSignal.timeout(30_000) });
		const headers = Object.fromEntries(response.headers.entries());
		const body = Buffer.from(await response.arrayBuffer());
		return { url, status: response.status, headers, sha256: sha(body), body: body.toString("utf8") };
	}
	const install = json<SiteRoutes>(join(cwd, "assets/routes.json")).install;
	insist(Array.isArray(install) && JSON.stringify(install) === JSON.stringify(["/install"]), "Install route probe list must contain /install only");
	const enabled = json<{ enabled?: boolean }>(join(cwd, "assets/install/release.json")).enabled;
	insist(typeof enabled === "boolean", "Install release switch is missing");
	const row = await get(BASE + "/install");
	if (enabled) {
		const staged = readFileSync(join(web, "install.html"));
		insist(row.status === 200 && row.sha256 === sha(staged) && row.body!.includes(INSTALL_COMMAND), "Live install page must match the GitHub release page");
	} else insist(row.status === 404, `Install route must remain 404: /install (${row.status})`);
	delete row.body; probes.push(row);
	for (const path of ["/install.sh", "/install-release.json", "/downloads/"]) {
		const old = await get(BASE + path); delete old.body; probes.push(old);
		insist(old.status === 404, `Retired installer route must be 404: ${path} (${old.status})`);
	}
	const interest = await get(`${BASE}/api/interest`); delete interest.body; probes.push(interest);
	insist(interest.status === 405 && interest.headers.allow === "POST", "GET /api/interest must return 405 with Allow: POST");
	const preview = await get(PREVIEW); delete preview.body; probes.push(preview);
	let location;
	try { location = new URL(preview.headers.location); } catch { /* Refuse missing or malformed redirects. */ }
	insist(preview.status === 302 && location?.protocol === "https:" && location.hostname.endsWith(".cloudflareaccess.com"), "workers.dev must redirect to Cloudflare Access with 302");
	for (const path of ["/", "/richview_runtime.js", "/richview_runtime_bg.wasm"]) {
		const row = await get(BASE + path); delete row.body; probes.push(row);
		insist(row.status === 200, `Header probe did not serve ${path}`);
		const expected = stagedHeaders(web, path);
		insist(expected["x-content-type-options"] === "nosniff" && expected["referrer-policy"], "Staged security headers are incomplete");
		if (path === "/") insist(expected["content-security-policy"], "Staged home CSP is missing");
		for (const [name, value] of Object.entries(expected)) insist(row.headers[name] === value, `Live header differs on ${path}: ${name}`);
	}
	return { status: "pass", probes };
}

const SMOKE_PATHS = ["/", "/playground", "/docs/", "/docs/charts", "/showcase/openai-vs-anthropic/", "/impressum", "/privacy"];
export function verifyBrowserSmoke(result: SmokeResult) {
	insist(Array.isArray(result?.pages) && result.pages.length === SMOKE_PATHS.length * 2, "Browser smoke omitted pages or widths");
	const expected = [1440, 390].flatMap(width => SMOKE_PATHS.map(path => `${width}:${path}`)).sort();
	insist(JSON.stringify(result.pages.map(row => `${row.width}:${row.path}`).sort()) === JSON.stringify(expected), "Browser smoke coverage differs");
	insist(result.pages.every(row => row.status === 200 && row.runtimeStarted === true && Array.isArray(row.problems) && row.problems.length === 0), "Live browser smoke failed");
	insist(result.frame?.runtimeStarted === true && Number.isInteger(result.frame.charts) && result.frame.charts > 0 && Array.isArray(result.frame.problems) && result.frame.problems.length === 0, "Flagship frame/runtime check failed");
	return { status: "pass", ...result };
}
async function browserSmoke({ cwd, out }: SmokeOptions) {
	const { chromium } = createRequire(join(cwd, "package.json"))("playwright") as typeof import("playwright");
	const browser = await chromium.launch({ headless: true }), pages: SmokePage[] = [];
	let frame: SmokeResult["frame"];
	try {
		for (const width of [1440, 390]) {
			const context = await browser.newContext({ viewport: { width, height: width === 390 ? 844 : 900 }, deviceScaleFactor: 1 });
			try {
				for (const path of SMOKE_PATHS) {
					const page = await context.newPage(), problems: string[] = [];
					page.on("console", message => { if (message.type() === "error") problems.push(`console: ${message.text()}`); });
					page.on("pageerror", error => problems.push(`pageerror: ${error.message}`));
					page.on("requestfailed", request => problems.push(`requestfailed: ${request.url()} ${request.failure()?.errorText}`));
					page.on("response", response => { if (response.status() >= 400) problems.push(`HTTP ${response.status()}: ${response.url()}`); });
					await page.addInitScript(() => document.addEventListener("securitypolicyviolation", event => console.error(`CSP: ${event.violatedDirective} ${event.blockedURI}`)));
					const response = await page.goto(BASE + path, { waitUntil: "load", timeout: 30_000 });
					let runtimeStarted = true;
					if (await page.locator('script[src="/bootstrap.js"]').count()) runtimeStarted = await page.waitForFunction(() => document.documentElement.dataset.richviewStarted === "rust", null, { timeout: 15_000 }).then(() => true, () => false);
					if (path === "/docs/charts") runtimeStarted &&= await page.waitForFunction(() => document.documentElement.dataset.rvRuntime === "started", null, { timeout: 15_000 }).then(() => true, () => false);
					if (path.startsWith("/showcase") && width === 1440) {
						await page.waitForFunction(() => document.querySelector("iframe")?.contentDocument?.documentElement?.dataset.rvRuntime === "started" && document.querySelector("iframe")?.contentDocument?.querySelectorAll("figure[data-rv-chart] canvas").length! > 0, null, { timeout: 20_000 });
						const charts = await page.evaluate(() => document.querySelector("iframe")!.contentDocument!.querySelectorAll("figure[data-rv-chart]").length);
						frame = { runtimeStarted: true, charts, problems };
					}
					await page.waitForTimeout(1200);
					const shot = `${path.replace(/\W+/g, "_").replace(/^_|_$/g, "") || "home"}__${width}.png`;
					await page.screenshot({ path: join(out, shot) });
					pages.push({ path, width, status: response?.status(), runtimeStarted, problems, shot });
					await page.close();
				}
			} finally { await context.close(); }
		}
	} finally { await browser.close(); }
	return { pages, frame };
}

function loadHotfix(requested: string | string[] | undefined, store: string) {
	insist(typeof requested === "string" && requested.length > 0, "A hotfix receipt is required");
	const file = resolve(requested.includes(sep) ? requested : join(store, "receipts", `${requested}.json`));
	regular(file);
	const source = json<HotfixReceipt>(file);
	insist(/^hotfix-\d{8}T\d{6}Z-[0-9a-f]{10}$/.test(source.id) && realpathSync(file) === realpathSync(join(store, "receipts", `${source.id}.json`)), "Use the immutable hotfix receipt in the gate evidence store");
	insist(source.kind === "hotfix" && source.result === "pass" && COMMIT.test(source.commit) && COMMIT.test(source.tree) && COMMIT.test(source.prod) && COMMIT.test(source.sourceCommit) && /^candidate\/[1-9]\d*$/.test(source.candidate), "Invalid or failed hotfix receipt");
	insist(Array.isArray(source.steps) && source.steps.length > 0 && source.steps.every((step: { status: string; }) => step.status === "pass"), "Every selected hotfix step must pass");
	insist(isAbsolute(source.build?.snapshot ?? "") && SHA.test(source.build?.manifestSha256), "Hotfix receipt has no preserved build snapshot");
	return { file, source };
}

function deploymentLockError(lock: string) {
	const file = join(lock, "owner.json");
	let owner: { pid: number; host?: string } | undefined;
	try { owner = json(file); } catch { /* An interrupted lock creation may have no readable owner. */ }
	const pid = Number.isSafeInteger(owner?.pid) && owner!.pid > 0 ? String(owner!.pid) : "<owner-pid>";
	return new Error(`EEXIST: deployment lock ${lock} already exists. Read ${file}; verify the recorded owner on ${owner?.host ?? "the deployment host"} with: ps -p ${pid} -o pid=,lstart=,command=. Do not clear a live owner's lock. Only after confirming the owner and its Wrangler/browser children have stopped, inspect its deploy receipt and rollback record, then remove only ${shellWord(file)} and run rmdir ${shellWord(lock)}. If the owner file is missing, first verify no deployment is active; rmdir the empty lock only. No lock was removed.`);
}

/** Print the preserved recovery command without executing or authorizing a rollback. */
export function describeDeploymentFailure(error: unknown) {
	const { receipt, message } = error as Error & { receipt?: DeployReceipt };
	return [
		message,
		receipt && `Receipt: ${receipt.id}; deployment attempted: ${receipt.deploymentAttempted}`,
		receipt?.deploymentAttempted && receipt.rollback?.command && `Recorded rollback (follow DEL-48): ${receipt.rollback.command.map(shellWord).join(" ")}`,
	].filter(Boolean).join("\n");
}

/** Prepare only our own locked detached worktree; shared source checkouts and their assets stay still. */
export function prepareDeploymentWorktree({ cwd = process.cwd(), receipt, go, store = evidenceDir(), run = command }: CandidateOptions = {}) {
	insist(recordedGo(go), "Deployment needs Daniel's explicit --go with his words and date (at least 8 non-whitespace characters)");
	const { source } = loadHotfix(receipt, store);
	const git = (args: string[]) => {
		const result = run("git", args, { cwd, env: process.env });
		insist(result.status === 0 && !result.error, `Cannot prepare deployment worktree: git ${args[0]} failed`);
		return result.stdout.trim();
	};
	const common = resolve(cwd, git(["rev-parse", "--git-common-dir"]));
	const path = join(dirname(common), ".claude/worktrees", `deploy-${source.id}`);
	insist(!existsSync(path), `Deployment worktree already exists: ${path}`);
	const modules = [join(cwd, "node_modules"), join(dirname(common), "node_modules")].find(path => existsSync(join(path, "wrangler/package.json")));
	insist(modules, "Install the pinned dependencies before deployment");
	insist(json<{ version?: string }>(join(modules, "wrangler/package.json")).version === WRANGLER, `Deployment requires installed Wrangler ${WRANGLER}`);
	git(["fetch", "--no-tags", "origin", "refs/heads/prod:refs/remotes/origin/prod", `refs/heads/${source.candidate}:refs/remotes/origin/${source.candidate}`]);
	insist(git(["rev-parse", "refs/remotes/origin/prod"]) === source.commit && git(["rev-parse", `refs/remotes/origin/${source.candidate}`]) === source.commit, "Hotfix candidate is not the current fetched origin/prod");
	git(["worktree", "add", "--detach", path, source.commit]);
	git(["worktree", "lock", "--reason", `deployment ${source.id}`, path]);
	insist(!existsSync(join(path, "node_modules")), "Candidate unexpectedly contains node_modules");
	symlinkSync(realpathSync(modules), join(path, "node_modules"), "dir");
	return { path, source: cwd, retainedOnFailure: true };
}

/** The only outward write is the explicit pinned Wrangler deploy; all checks afterwards are reads. */
export async function deploySite({ cwd = process.cwd(), receipt: requested, go, store = evidenceDir(), run = command, fetch, smoke = browserSmoke, now = () => new Date(), worktree, lockPromotion = () => acquireLock("merge-prod") }: DeployOptions = {}) {
	insist(recordedGo(go), "Deployment needs Daniel's explicit --go with his words and date (at least 8 non-whitespace characters)");
	const { file, source } = loadHotfix(requested, store);
	const id = newReceiptId("deploy", source.commit, now()), folder = join(store, "runs", id), lock = join(dirname(store), "deploy-site.lock");
	mkdirSync(dirname(lock), { recursive: true });
	try { mkdirSync(lock); } // A second deployment owner must never race the rollback target.
	catch (error) { if ((error as NodeJS.ErrnoException).code === "EEXIST") throw deploymentLockError(lock); throw error; }
	let report: DeployReceipt | undefined, error: unknown, releasePromotion: (() => void) | undefined;
	try {
		writeOnce(join(lock, "owner.json"), { pid: process.pid, host: hostname(), at: now().toISOString(), receipt: id, hotfix: source.id, worktree: cwd });
		insist(!existsSync(folder) && !existsSync(join(store, "receipts", `${id}.json`)), `Deployment receipt already exists: ${id}`);
		mkdirSync(folder, { recursive: true });
		report = { id, kind: "deploy", at: now().toISOString(), hotfix: source.id, candidate: source.candidate, commit: source.commit, tree: source.tree, go: go.trim(), target: BASE, worker: "richview-teaser", wrangler: WRANGLER, sourceReceiptSha256: sha(readFileSync(file)), build: source.build, worktree: worktree ?? { path: cwd }, commands: [], live_checks: ["live_routes", "http", "browser_smoke_and_frame"].map(name => ({ name, status: "unrun" })), deploymentAttempted: false, result: "fail" };
		const liveCheck = async (name: string, action: () => Promise<{ status: string }>) => {
			const row = report!.live_checks.find(row => row.name === name)!, started = Date.now();
			row.status = "fail";
			try { Object.assign(row, await action()); }
			catch (error) { row.reason = (error as Error).message; throw error; }
			finally { row.durationMs = Date.now() - started; }
		};
		const exec = (name: string, program: string, args: string[]) => {
			const at = Date.now(), result = run(program, args, { cwd, env: { ...process.env, CI: "true", WRANGLER_SEND_METRICS: "false", CLOUDFLARE_SEND_METRICS: "false" } });
			writeFileSync(join(folder, `${name}.stdout`), result.stdout ?? "");
			writeFileSync(join(folder, `${name}.stderr`), result.stderr ?? "");
			report!.commands.push({ name, program, args, status: result.status, durationMs: Date.now() - at });
			insist(result.status === 0 && !result.error, `${name} failed (${result.status ?? result.error}); see ${folder}`);
			return (result.stdout ?? "").trim();
		};
		const git = (name: string, args: string[]) => exec(name, "git", args);
		const checkCandidate = (suffix: string) => {
			insist(git(`head${suffix}`, ["rev-parse", "HEAD"]) === source.commit && git(`tree${suffix}`, ["rev-parse", "HEAD^{tree}"]) === source.tree, "Worktree differs from the hotfix candidate");
			insist(git(`clean${suffix}`, ["status", "--porcelain", "--untracked-files=normal"]) === "", "Deployment needs a clean candidate worktree");
			git(`fetch-prod${suffix}`, ["fetch", "--no-tags", "origin", "refs/heads/prod:refs/remotes/origin/prod"]);
			git(`fetch-candidate${suffix}`, ["fetch", "--no-tags", "origin", `refs/heads/${source.candidate}:refs/remotes/origin/${source.candidate}`]);
			insist(git(`prod${suffix}`, ["rev-parse", "refs/remotes/origin/prod"]) === source.commit && git(`candidate${suffix}`, ["rev-parse", `refs/remotes/origin/${source.candidate}`]) === source.commit, "Hotfix candidate is not the current fetched origin/prod");
		};
		checkCandidate("");
		const cloudflareChanges = git("cloudflare-diff", ["diff", "--name-only", "--no-renames", "-z", source.prod, source.commit, "--", "cloudflare/"]);
		insist(cloudflareChanges === "", "Hotfix changes cloudflare/ Worker code or migrations; this site deployment lane does not cover those effects");
		const gitDir = git("git-dir", ["rev-parse", "--absolute-git-dir"]);
		insist(lstatSync(join(cwd, ".git")).isFile() && lstatSync(join(gitDir, "locked")).isFile(), "Deployment requires a locked linked worktree");
		const wrangler = join(cwd, "node_modules/.bin/wrangler");
		insist(json<{ devDependencies?: Record<string, string> }>(join(cwd, "package.json")).devDependencies?.wrangler === WRANGLER && json<{ packages?: Record<string, { version?: string }> }>(join(cwd, "package-lock.json")).packages?.["node_modules/wrangler"]?.version === WRANGLER && json<{ version?: string }>(join(cwd, "node_modules/wrangler/package.json")).version === WRANGLER, `Deployment requires installed Wrangler ${WRANGLER}`);
		insist(exec("wrangler-version", wrangler, ["--version"]).split(/\s/)[0] === WRANGLER, "Wrangler executable differs from the pinned package");
		const config = join(cwd, "wrangler.toml"), text = readFileSync(config, "utf8");
		// An older preserved hotfix candidate still points at the JavaScript Worker.
		const siteConfig = existsSync(join(cwd, "cloudflare/interest-worker.ts"))
			? SITE_CONFIG.replace('main = "cloudflare/interest-worker.mjs"', 'main = "cloudflare/interest-worker.ts"') : SITE_CONFIG;
		insist(configLines(text) === configLines(siteConfig), "Candidate changes the approved site Worker configuration; account, bindings and routes need separate explicit coverage");
		report.configSha256 = sha(text);
		const verified = verifySiteBuild({ receipt: source, cwd });
		const dist = join(cwd, "dist"); mkdirSync(dist, { recursive: true });
		insist(lstatSync(dist).isDirectory() && within(cwd, dist), "dist must remain within the locked worktree");
		const staged = join(dist, `.deploy-${randomUUID()}`), web = join(dist, "web");
		try {
			cpSync(join(source.build.snapshot, "dist/web"), staged, { recursive: true, dereference: false });
			verifySiteBuild({ receipt: source, cwd, web: staged });
			if (existsSync(web)) { insist(lstatSync(web).isDirectory(), "Refusing to replace a symlink or non-directory dist/web"); rmSync(web, { recursive: true }); }
			renameSync(staged, web);
		} finally { rmSync(staged, { recursive: true, force: true }); }
		report.assets = verified.files;
		const wr = (name: string, args: string[]) => exec(name, wrangler, [...args, "--config", config]);
		const before = currentDeployment(wr("deployments-before", ["deployments", "list", "--json"]));
		const version = JSON.parse(wr("version-before", ["versions", "view", before.version, "--json"])) as { id?: string };
		insist(version.id === before.version, "Rollback version backup differs from current deployment");
		report.rollback = { version: before.version, deployment: before.deployment, versionMetadata: version, command: [wrangler, "rollback", before.version, "--config", config] };
		writeOnce(join(folder, "rollback.json"), report.rollback);
		releasePromotion = lockPromotion();
		report.promotionLock = "merge-prod";
		checkCandidate("-before-deploy");
		verifySiteBuild({ receipt: source, cwd, web });
		insist(currentDeployment(wr("deployments-confirm", ["deployments", "list", "--json"])).version === before.version, "Worker changed since rollback backup; refusing deployment");
		report.deploymentAttempted = true;
		const deployed = wr("deploy", ["deploy", "--keep-vars"]), match = deployed.match(/Current Version ID:\s*([0-9a-f-]{36})/);
		insist(match && VERSION.test(match[1]) && match[1] !== before.version, "Deploy did not identify a new Worker version");
		report.version = match[1];
		insist(currentDeployment(wr("deployments-after", ["deployments", "list", "--json"])).version === report.version, "Live deployment does not match Wrangler's new version");
		await liveCheck("live_routes", async () => {
			const routes = await checkLiveRoutes({ cwd, web, webFiles: verified.files, fetch });
			writeOnce(join(folder, "live-routes.json"), routes);
			return verifyLiveRoutes(routes, { cwd, webFiles: verified.files });
		});
		await liveCheck("http", () => checkLiveHttp({ cwd, web, fetch }));
		await liveCheck("browser_smoke_and_frame", async () => {
			const shots = join(folder, "browser"); mkdirSync(shots);
			const browser = await smoke({ cwd, out: shots });
			writeOnce(join(folder, "browser.json"), browser);
			return verifyBrowserSmoke(browser);
		});
		insist(currentDeployment(wr("deployments-verified", ["deployments", "list", "--json"])).version === report.version, "Worker changed during live verification");
		checkCandidate("-after-deploy");
		report.result = "pass";
	} catch (cause) { error = cause; if (report) report.error = (cause as Error).message; }
	finally {
		try { if (report) { report.finished = now().toISOString(); writeOnce(join(store, "receipts", `${id}.json`), report); } }
		finally { try { releasePromotion?.(); } finally { rmSync(lock, { recursive: true }); } }
	}
	if (error) throw Object.assign(error as Error, { receipt: report });
	return report!;
}

export async function runDeploymentCli() {
	try {
		const options: Pick<CandidateOptions, "receipt" | "go"> = {};
		for (let i = 2; i < process.argv.length; i += 2) {
			const key = process.argv[i];
			const option = key.slice(2) as keyof typeof options;
			insist(["--receipt", "--go"].includes(key) && process.argv[i + 1] && !options[option], "Usage: deploy-site.mjs --receipt <hotfix-id> --go '<Daniel words and date>'");
			options[option] = process.argv[i + 1];
		}
		const worktree = prepareDeploymentWorktree(options);
		const receipt = await deploySite({ ...options, cwd: worktree.path, worktree });
		console.log(`${receipt.id}: ${receipt.result}; Worker ${receipt.version}; rollback ${receipt.rollback!.version}`);
		for (const args of [["worktree", "unlock", worktree.path], ["worktree", "remove", "--force", worktree.path]]) {
			const result = command("git", args, { cwd: worktree.source });
			insist(result.status === 0, `Deployment passed; worktree cleanup failed: ${worktree.path}`);
		}
	} catch (error) { console.error(describeDeploymentFailure(error)); process.exitCode = 1; }
}

if (import.meta.main) await runDeploymentCli();
