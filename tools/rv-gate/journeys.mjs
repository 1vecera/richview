// The journeys step of the gate (DEL-76): complete journeys on the release artifacts, on loopback,
// with real browsers and fresh data directories. A failure is a product finding: every assertion
// names the surface that owns it, and a feature that is not built fails with a "not built" message.
//
// Release set (every release run): the author journey and the agent journey.
//  - Author (INT-04): `init` in an empty folder, a scripted edit adding a chart and a table, `check`,
//    `generate`, the offline file opened in Chromium, Firefox and WebKit with its charts live (the
//    runtime loaded, geometry models returned, no console error, no CSP violation), then `--to pdf`.
//  - Agent (FEA-SF-09): a scripted MCP client over stdio runs initialize, tools/list, learn, render,
//    check, a deliberate error, its fix taken from the diagnostic's own text, check again and publish to
//    a local server with a token, asserting the JSON every step returns.
// Full set (the candidate): those two, and
//  - Cowork (INT-07, FEA-SF-55): a report published from the CLI and one from MCP; a reader opens the
//    permalink and gets the same geometry models as the offline file; the owner shares, pins and
//    deletes; a reader without access is refused; a shared story's link preview carries its title and
//    image, and a private story's reveals neither.
//  - Publish (FEA-BR-26, FEA-BR-29): journeys-publish.mjs.
// The VS Code journey stays with the `vscode` step (OPN-55).
//
// Usage: node tools/rv-gate/journeys.mjs --bin target/release/richview --out <dir> --set release|full
//        [--public <dist/web>] [--publish-wasm <wasm>] [--only <journey>] [--cwd <repo>]

import { launchBrowser } from "./lib/browser-launch.mjs";
import { spawn, spawnSync } from "node:child_process";
import { randomBytes } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { Journey, reportLines, runJourney } from "./lib/journey.mjs";
import { previewTags, publishJourney } from "./journeys-publish.mjs";

export const SETS = { release: ["author", "agent"], full: ["author", "agent", "cowork", "publish"] };
export const ENGINES = ["chromium", "firefox", "webkit"];

/** Only a prelaunch full's dispatcher narrows the offline author journey. */
export function journeyEngines(env = process.env) {
	return env.RV_GATE_PRELAUNCH_CHROMIUM_ONLY === "1" ? ["chromium"] : ENGINES;
}
export const TOOLS = ["richview_learn", "richview_validate", "richview_check", "richview_render", "richview_publish", "richview_asset_put", "richview_asset_verify", "richview_image_search", "richview_image_use"];
/** FEA-SF-09: the tools that only read. */
export const READ_ONLY = ["richview_learn", "richview_validate", "richview_check", "richview_asset_verify", "richview_image_search"];
// FEA-SF-09: openWorldHint marks the tools that reach beyond the machine.
export const OPEN_WORLD = ["richview_publish", "richview_asset_put", "richview_asset_verify", "richview_image_search", "richview_image_use"];
export const TITLE = "Direct visits lead our traffic";
const LEVELS = ["error", "warning", "info"];

// ---------------------------------------------------------------------------------------------
// Pure parts.
// ---------------------------------------------------------------------------------------------

/** Problems with one diagnostic's shape `{level, code, message, path, line?, tag?}`; empty when it is right. */
export function diagnosticProblems(d) {
	const problems = [];
	if (!d || typeof d !== "object") return ["is not an object"];
	if (!LEVELS.includes(d.level)) problems.push(`level ${JSON.stringify(d.level)} is not one of ${LEVELS.join(", ")}`);
	for (const key of ["code", "message", "path"]) if (typeof d[key] !== "string" || !d[key]) problems.push(`${key} is not a non-empty string`);
	if (d.line !== undefined && !(Number.isInteger(d.line) && d.line >= 1)) problems.push("line is not a positive integer");
	if (d.tag !== undefined && typeof d.tag !== "string") problems.push("tag is not a string");
	return problems;
}

/** The problems of every diagnostic in a result, each named by its index and code. */
export function resultProblems(result) {
	if (!result || typeof result.ok !== "boolean") return ["ok is not a boolean"];
	if (!Array.isArray(result.diagnostics)) return ["diagnostics is not an array"];
	return result.diagnostics.flatMap((d, i) => diagnosticProblems(d).map((p) => `diagnostics[${i}] (${d?.code}) ${p}`));
}

/**
 * Apply a diagnostic's own advice to a source: its `expected` values (TYP-53), else its `fix` text, else its message. Schema
 * errors say `<key>: expected a, b or c`; the fix sets that key, on the diagnostic's line, to the first allowed value. Returns `{source, line, from, to}` or null.
 */
export function fixFromDiagnostic(diagnostic, source) {
	const expected = typeof diagnostic.expected === "string" && diagnostic.expected ? [diagnostic.expected, diagnostic.expected] : null;
	const advice = expected ?? /expected ([^.;]+)$/.exec(diagnostic.fix ?? "") ?? /expected ([^.;]+)$/.exec(diagnostic.message ?? "");
	const key = diagnostic.path?.split(".").at(-1);
	if (!advice || !key || !diagnostic.line) return null;
	// Core writes the expected values as "one of 'up', 'down', 'neutral' or 'hold'"; older text has no quotes.
	const options = advice[1]
		.replace(/^one of\s+/i, "")
		.split(/,\s*|\s+or\s+/)
		.map((o) => o.trim().replace(/^(['"`])(.*)\1$/, "$2"))
		.filter(Boolean);
	const lines = source.split("\n");
	for (let i = diagnostic.line - 1; i < Math.min(lines.length, diagnostic.line + 3); i++) {
		const m = new RegExp(`^(\\s*${key}:\\s*)(.*)$`).exec(lines[i]);
		if (m) {
			lines[i] = `${m[1]}${options[0]}`;
			return { source: lines.join("\n"), line: i + 1, from: m[2], to: options[0] };
		}
	}
	return null;
}

/** Hosts of a server on this machine; its report origin is one of these (ARC-15). */
const LOOPBACK = new Set(["127.0.0.1", "localhost", "[::1]"]);

/**
 * Read a report as a publishing client does: follow each redirect by hand, since the app origin
 * hands `/r/<slug>` to the report origin (ARC-15) and fetch drops `Authorization` across origins.
 * The bearer token goes only to the server's own two origins: the address asked for, and the
 * loopback origin its first redirect names (the report origin). A redirect to any other origin is
 * followed without the token, and a third origin never gets it, even one reached through the
 * server's own. A private address without a token ends at sign-in.
 */
export async function readReport(url, token, fetchImpl = fetch) {
	const own = new Set([new URL(url).origin]);
	let response;
	for (let hop = 0; hop < 5; hop++) {
		const trusted = own.has(new URL(url).origin);
		response = await fetchImpl(url, { headers: token && trusted ? { authorization: `Bearer ${token}` } : {}, redirect: "manual" });
		const location = response.headers.get("location");
		if (response.status < 300 || response.status >= 400 || !location) break;
		url = new URL(location, url).href;
		if (hop === 0 && own.size === 1 && LOOPBACK.has(new URL(url).hostname)) own.add(new URL(url).origin);
	}
	return { status: response.status, url, text: await response.text(), headers: response.headers };
}

/** A private page ends at a refusal or sign-in, and never discloses its title. */
export function refusedReport(page, title) {
	const refused = [401, 403, 404].includes(page.status) || (page.status === 200 && /sign.?in/i.test(decodeURIComponent(page.url)));
	return refused && !page.text.includes(title);
}

/** The first `http://127.0.0.1:<port>` a server logs. */
export function serverUrl(log) {
	return /RichView at (http:\/\/127\.0\.0\.1:\d+)/.exec(log)?.[1];
}

/** The text a report page has outside tags, scripts and styles. */
export function pageText(html) {
	return html.replace(/<(script|style)[\s\S]*?<\/\1>/gi, " ").replace(/<[^>]+>/g, " ").replace(/&amp;/g, "&").replace(/\s+/g, " ").trim();
}

/** What compares two pages' charts: each model's width, kind and geometry, in order. */
export function modelDigest(models) {
	return models.map((m) => ({ width: m.width, geometry: m.geometry }));
}

/** The blocks the author journey's scripted edit adds. */
export const ADDED_BLOCKS = `
## Orders by plan

\`\`\`chart
type: bar
title: Team plans placed the most orders
data: {inline: [{plan: Solo, orders: 120}, {plan: Team, orders: 180}, {plan: Enterprise, orders: 60}]}
encoding: {x: plan, y: orders}
axes:
  y: {label: Orders}
\`\`\`

\`\`\`table
data:
  inline:
    - {plan: Solo, orders: 120}
    - {plan: Team, orders: 180}
    - {plan: Enterprise, orders: 60}
columns:
  - {key: plan, label: Plan}
  - {key: orders, label: Orders, format: number, align: right}
\`\`\`
`;

// ---------------------------------------------------------------------------------------------
// Processes.
// ---------------------------------------------------------------------------------------------

function cleanEnv(publicDir, extra = {}) {
	const env = { ...process.env, ...(publicDir ? { RICHVIEW_PUBLIC_DIR: publicDir } : {}), ...extra };
	for (const name of ["RICHVIEW_SERVER", "RICHVIEW_TOKEN", "RICHVIEW_PUBLIC_URL", "RICHVIEW_CONNECTORS", "RICHVIEW_STORES", "RICHVIEW_ADMIN_EMAIL", "RICHVIEW_ADMIN_PASSWORD", "RICHVIEW_DEFAULT_VISIBILITY"]) {
		if (!(name in extra)) delete env[name];
	}
	return env;
}

/** Run the CLI; `json` is the parsed stdout when it is JSON. */
function cli(c, args, { env = {}, timeout = 120_000 } = {}) {
	const run = spawnSync(c.bin, args, { cwd: c.cwd, encoding: "utf8", env: cleanEnv(c.publicDir, env), timeout, maxBuffer: 64 << 20 });
	let json;
	try {
		json = JSON.parse(run.stdout);
	} catch {
		json = undefined;
	}
	return { status: run.status, stdout: run.stdout, stderr: run.stderr, json };
}

/** A local `richview server` on a loopback port with a fresh data directory. */
async function startServer(c, { dataDir, token, visibility = "private" }) {
	rmSync(dataDir, { recursive: true, force: true });
	const child = spawn(c.bin, ["server", "--bind", "127.0.0.1", "--port", "0", "--data-dir", dataDir, "--token", token, "--default-visibility", visibility], { cwd: c.cwd, env: cleanEnv(c.publicDir), stdio: ["ignore", "pipe", "pipe"] });
	let log = "";
	const base = await new Promise((accept, reject) => {
		const timer = setTimeout(() => reject(new Error(`the server printed no address in 15 s: ${log.slice(0, 300)}`)), 15_000);
		child.once("error", reject);
		child.once("exit", (code) => reject(new Error(`the server stopped with ${code}: ${log.slice(0, 300)}`)));
		for (const stream of [child.stdout, child.stderr]) {
			stream.on("data", (chunk) => {
				log += chunk;
				const url = serverUrl(log);
				if (url) {
					clearTimeout(timer);
					accept(url);
				}
			});
		}
	});
	return {
		base,
		token,
		async stop() {
			const exited = new Promise((accept) => child.once("exit", accept));
			child.kill("SIGINT");
			await Promise.race([exited, new Promise((accept) => setTimeout(accept, 3000))]);
			if (child.exitCode === null) child.kill("SIGKILL");
		},
	};
}

/** A scripted MCP client over stdio: one JSON message per line. */
class Mcp {
	constructor(c, env = {}) {
		this.child = spawn(c.bin, ["mcp"], { cwd: c.cwd, env: cleanEnv(c.publicDir, env), stdio: ["pipe", "pipe", "pipe"] });
		this.pending = new Map();
		this.next = 1;
		this.stderr = "";
		this.stray = [];
		let buffer = "";
		this.child.stdout.on("data", (chunk) => {
			buffer += chunk;
			let at;
			while ((at = buffer.indexOf("\n")) >= 0) {
				const line = buffer.slice(0, at);
				buffer = buffer.slice(at + 1);
				if (!line.trim()) continue;
				let message;
				try {
					message = JSON.parse(line);
				} catch {
					this.stray.push(line);
					continue;
				}
				const waiting = this.pending.get(message.id);
				if (waiting) {
					this.pending.delete(message.id);
					waiting(message);
				}
			}
		});
		this.child.stderr.on("data", (chunk) => (this.stderr += chunk));
	}

	request(method, params, timeout = 120_000) {
		const id = this.next++;
		return new Promise((accept, reject) => {
			const timer = setTimeout(() => reject(new Error(`MCP ${method} had no reply in ${timeout / 1000} s`)), timeout);
			this.pending.set(id, (message) => {
				clearTimeout(timer);
				accept(message);
			});
			this.child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`);
		});
	}

	/** The handshake every session starts with. */
	async start() {
		await this.request("initialize", { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "journeys", version: "1" } });
		this.notify("notifications/initialized");
	}

	notify(method, params) {
		this.child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", method, params })}\n`);
	}

	/** Call a tool and return `{isError, data}` with the structured result (or the text parsed as JSON). */
	async call(name, args) {
		const reply = await this.request("tools/call", { name, arguments: args });
		const result = reply.result ?? {};
		let data = result.structuredContent;
		if (data === undefined) {
			try {
				data = JSON.parse(result.content?.[0]?.text ?? "");
			} catch {
				data = undefined;
			}
		}
		return { reply, isError: result.isError, data, text: result.content?.[0]?.text ?? "" };
	}

	close() {
		this.child.stdin.end();
		setTimeout(() => this.child.kill("SIGKILL"), 2000).unref();
	}
}

// ---------------------------------------------------------------------------------------------
// Browsers.
// ---------------------------------------------------------------------------------------------

async function playwright(cwd) {
	return import(pathToFileURL(join(cwd, "node_modules/playwright/index.mjs")).href);
}

/** Open a page, wait for the runtime to settle, and read what a reader's browser shows of its charts. */
async function probePage(context, url, { settleMs = 30_000 } = {}) {
	const page = await context.newPage();
	const errors = [];
	const failed = [];
	page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
	page.on("pageerror", (e) => errors.push(e.message));
	page.on("requestfailed", (r) => failed.push(`${r.url().slice(0, 100)}: ${r.failure()?.errorText}`));
	await page.addInitScript(() => {
		window.__violations = [];
		document.addEventListener("securitypolicyviolation", (e) => window.__violations.push(`${e.violatedDirective} ${e.blockedURI}`));
	});
	const response = await page.goto(url, { waitUntil: "load" });
	const status = response?.status() ?? 0;
	const loaded = await page.waitForFunction(() => window.__richview, null, { timeout: 15_000 }).then(() => true, () => false);
	let settled = false;
	let models = [];
	if (loaded) {
		settled = await page.evaluate((ms) => Promise.race([window.__richview.settled().then(() => true), new Promise((r) => setTimeout(() => r(false), ms))]), settleMs);
		models = await page.evaluate(() => window.__richview.charts().map((m) => ({ width: m.width, settled: m.settled, geometry: m.geometry })));
	}
	const violations = await page.evaluate(() => window.__violations ?? []);
	const text = await page.evaluate(() => document.body.innerText);
	await page.close();
	return { status, loaded, settled, models, errors, failed, violations, text };
}

// ---------------------------------------------------------------------------------------------
// Journeys.
// ---------------------------------------------------------------------------------------------

/** init, a scripted edit, check, generate, the offline file in three engines, then a PDF. */
async function authorJourney(j, c) {
	const root = join(c.out, "author");
	rmSync(root, { recursive: true, force: true });
	mkdirSync(root, { recursive: true });
	const folder = join(root, "report");
	const file = join(folder, "report.rmd");

	const init = cli(c, ["init", folder]);
	j.need("init creates a report folder", "app surfaces", init.status === 0 && existsSync(file), "exit 0 and report.rmd", `exit ${init.status} ${init.stderr.slice(0, 160)}`);
	const created = readFileSync(file, "utf8");
	const frontmatter = /^---\r?\n([\s\S]*?)\r?\n---(?:\r?\n|$)/.exec(created)?.[1] ?? "";
	const title = frontmatter.split(/\r?\n/).find((line) => line.startsWith("title:"));
	j.need("the template declares its title in frontmatter", "app surfaces", title === `title: ${TITLE}`, `title: ${TITLE}`, title);
	writeFileSync(file, `${created.trimEnd()}\n${ADDED_BLOCKS}`);

	const check = cli(c, ["check", file, "--format", "json"]);
	const errors = (check.json?.diagnostics ?? []).filter((d) => d.level === "error");
	j.need("check returns JSON that is ok", "app surfaces", check.status === 0 && check.json?.ok === true && errors.length === 0, "ok: true and no error diagnostic", `exit ${check.status} ok=${check.json?.ok} errors=${errors.map((d) => d.code)}`);
	j.equal("check measured both charts", "chart engine", (check.json.charts ?? []).map((ch) => ch.measured), [true, true]);

	const generate = cli(c, ["generate", file, "--format", "json"]);
	const html = generate.json?.output;
	j.need("generate writes the offline report", "app surfaces", generate.status === 0 && generate.json?.ok === true && html && existsSync(html), "ok: true and an HTML file", `exit ${generate.status} ${generate.stderr.slice(0, 160)}`);
	j.assert("the offline report is interactive", "renderer", generate.json.interactive === true, "interactive: true", generate.json.interactive);

	const pw = await playwright(c.cwd);
	const engines = journeyEngines();
	for (const name of ENGINES.filter(engine => !engines.includes(engine))) console.log(`UNRUN offline author journey in ${name}: Prelaunch policy: Chromium only until launch.`);
	for (const name of engines) {
		let browser;
		try {
			browser = await launchBrowser(pw[name], { headless: true, ...(name === "chromium" && process.env.RICHVIEW_CHROMIUM ? { executablePath: process.env.RICHVIEW_CHROMIUM } : {}) }, { TimeoutError: pw.errors.TimeoutError });
		} catch (error) {
			j.abort(`${name} starts`, "gate", "a launched browser", error.message.split("\n")[0]);
		}
		try {
			const context = await browser.newContext({ viewport: { width: 1280, height: 900 }, reducedMotion: "reduce" });
			const seen = await probePage(context, pathToFileURL(html).href);
			const at = `in ${name}`;
			j.assert(`the report runtime loads ${at}`, "site", seen.loaded, "window.__richview defined", "undefined");
			j.assert(`the charts settle ${at}`, "chart engine", seen.settled, "settled() resolves in 30 s", "timed out");
			j.assert(`charts() returns both models ${at}`, "chart engine", seen.models.length === 2 && seen.models.every((m) => m.geometry && m.geometry.series?.length), "2 models with series", `${seen.models.length} models: ${seen.models.map((m) => Object.keys(m.geometry ?? {}).length)} geometry keys`);
			j.equal(`no console error ${at}`, "renderer", seen.errors, []);
			j.equal(`no CSP violation ${at}`, "renderer", seen.violations, []);
			j.equal(`no failed request ${at}`, "renderer", seen.failed, []);
			j.assert(`the title and table text read ${at}`, "renderer", seen.text.includes(TITLE) && seen.text.includes("Enterprise"), "title and table text", seen.text.slice(0, 80));
			await context.close();
		} finally {
			await browser.close();
		}
	}

	const pdf = cli(c, ["generate", file, "--to", "pdf", "--format", "json"], { timeout: 180_000 });
	const pdfFile = pdf.json?.output;
	if (!j.assert("generate --to pdf writes a PDF", "app surfaces", pdf.status === 0 && pdf.json?.ok === true && pdfFile?.endsWith(".pdf") && existsSync(pdfFile), "ok: true and a .pdf file", `exit ${pdf.status} ${(pdf.json?.diagnostics ?? []).map((d) => `${d.code}: ${d.message}`).join("; ") || pdf.stderr.slice(0, 200)}`)) return;
	const info = spawnSync("pdfinfo", [pdfFile], { encoding: "utf8" });
	if (info.error) return void j.assert("Poppler's pdfinfo and pdftotext read the PDF", "gate", false, "pdfinfo on PATH", info.error.message);
	const pages = Number(/^Pages:\s+(\d+)/m.exec(info.stdout)?.[1] ?? 0);
	j.assert("the PDF has pages", "renderer", pages >= 1 && pages === pdf.json.pages, `${pdf.json.pages} pages`, pages);
	const text = spawnSync("pdftotext", ["-enc", "UTF-8", pdfFile, "-"], { encoding: "utf8" }).stdout.replace(/\s+/g, " ");
	j.assert("the PDF contains the title text", "renderer", text.includes(TITLE), TITLE, text.slice(0, 120));
	j.assert("the PDF contains the added table", "renderer", text.includes("Enterprise") && text.includes("180"), "the table's rows", text.slice(0, 160));
}

/** A scripted MCP client: every step's JSON is asserted. */
async function agentJourney(j, c) {
	const root = join(c.out, "agent");
	rmSync(root, { recursive: true, force: true });
	mkdirSync(root, { recursive: true });
	const folder = join(root, "report");
	const file = join(folder, "report.rmd");
	const init = cli(c, ["init", folder]);
	j.need("init creates the agent's report folder", "app surfaces", init.status === 0 && existsSync(file), "exit 0", `exit ${init.status}`);

	const server = await startServer(c, { dataDir: join(root, "server-data"), token: randomBytes(16).toString("hex"), visibility: "private" });
	const mcp = new Mcp(c, { RICHVIEW_SERVER: server.base, RICHVIEW_TOKEN: server.token });
	try {
		const start = await mcp.request("initialize", { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "journeys", version: "1" } });
		j.need("initialize answers", "app surfaces", start.result?.serverInfo?.name === "richview" && typeof start.result.protocolVersion === "string", "serverInfo.name richview and a protocolVersion", JSON.stringify(start).slice(0, 200));
		j.assert("initialize announces tools and instructions", "app surfaces", start.result.capabilities?.tools && typeof start.result.instructions === "string", "capabilities.tools and instructions", Object.keys(start.result.capabilities ?? {}));
		mcp.notify("notifications/initialized");

		const list = await mcp.request("tools/list", {});
		const tools = list.result?.tools ?? [];
		j.equal("tools/list names the nine tools of FEA-SF-09", "app surfaces", tools.map((t) => t.name).sort(), [...TOOLS].sort());
		j.assert("every tool has a description, an object schema and annotations", "app surfaces", tools.every((t) => t.description && t.inputSchema?.type === "object" && typeof t.annotations?.readOnlyHint === "boolean"), "description, inputSchema.type object, readOnlyHint", tools.filter((t) => !t.description || t.inputSchema?.type !== "object" || typeof t.annotations?.readOnlyHint !== "boolean").map((t) => t.name));
		j.equal("the read-only tools are learn, validate, check, asset_verify and image_search", "app surfaces", tools.filter((t) => t.annotations?.readOnlyHint).map((t) => t.name).sort(), [...READ_ONLY].sort());
		j.equal("the open-world tools are publish, both asset tools and both image tools", "app surfaces", tools.filter((t) => t.annotations?.openWorldHint).map((t) => t.name).sort(), [...OPEN_WORLD].sort());

		const learn = await mcp.call("richview_learn", { section: "workflow" });
		j.assert("learn returns the workflow guide", "app surfaces", learn.isError === false && learn.text.length > 200, "isError false and guide text", `isError ${learn.isError}, ${learn.text.length} chars`);

		const validate = await mcp.call("richview_validate", { inputPath: file });
		j.assert("validate returns ok with well-formed diagnostics", "app surfaces", validate.isError === false && validate.data?.ok === true && resultProblems(validate.data).length === 0, "ok: true and the diagnostic shape", `isError ${validate.isError} ${JSON.stringify(validate.data)?.slice(0, 160)} ${resultProblems(validate.data)}`);

		const render = await mcp.call("richview_render", { inputPath: file, outputName: "agent.html" });
		j.assert("render writes the report and returns ok", "app surfaces", render.isError === false && render.data?.ok === true && render.data.interactive === true && resultProblems(render.data).length === 0 && existsSync(join(folder, "Rendered/agent.html")), "ok, interactive, Rendered/agent.html", `isError ${render.isError} ${JSON.stringify(render.data)?.slice(0, 200)}`);

		const check = await mcp.call("richview_check", { inputPath: file });
		j.assert("check returns ok, the chart geometry and no error", "app surfaces", check.isError === false && check.data?.ok === true && Array.isArray(check.data.charts) && check.data.charts.length === 1 && resultProblems(check.data).length === 0, "ok: true, one chart, the diagnostic shape", `isError ${check.isError} ${JSON.stringify(check.data)?.slice(0, 200)} ${resultProblems(check.data)}`);
		const cliCheck = cli(c, ["check", file, "--format", "json"]);
		j.equal("check over MCP equals check on the CLI", "app surfaces", check.data, cliCheck.json);

		// A deliberate error: an unknown callout kind.
		const good = readFileSync(file, "utf8");
		const broken = good.replace("kind: neutral", "kind: loud");
		j.need("the scripted edit breaks the callout", "gate", broken !== good, "kind: loud written", "the template has no kind: neutral");
		writeFileSync(file, broken);
		const bad = await mcp.call("richview_check", { inputPath: file });
		const error = (bad.data?.diagnostics ?? []).find((d) => d.level === "error");
		j.need("check reports the deliberate error", "app surfaces", bad.data?.ok === false && error?.code === "block.schema" && resultProblems(bad.data).length === 0, "ok: false and a block.schema error of the diagnostic shape", `ok ${bad.data?.ok}, ${JSON.stringify(bad.data?.diagnostics)?.slice(0, 220)} ${resultProblems(bad.data)}`);
		// TYP-69: isError is for calls that could not run; a report with errors is a result.
		j.equal("a report with errors is a result, not a failed call (isError false)", "app surfaces", bad.isError, false);
		// TYP-53, INT-21: the error names its line and how to recover: a fix, or for a value outside its enum the expected values.
		const recovery = [error.fix, error.expected].find((text) => typeof text === "string" && text.length > 0);
		j.assert("the error diagnostic names its line and a fix or the expected values", "core format", Number.isInteger(error.line) && recovery !== undefined, "line and a fix or expected text on the diagnostic", `line ${error.line}, fix ${JSON.stringify(error.fix)}, expected ${JSON.stringify(error.expected)} (keys ${Object.keys(error)})`);

		// Its fix, from the diagnostic's own text.
		const fix = fixFromDiagnostic(error, broken);
		j.need("the diagnostic's own text names a fix (fix, else message)", "app surfaces", fix, "'expected a, b or c' on the offending line", `${error.message} (line ${error.line})`);
		writeFileSync(file, fix.source);
		const fixed = await mcp.call("richview_check", { inputPath: file });
		j.assert(`the fix (${fix.from} -> ${fix.to}) clears the error`, "app surfaces", fixed.data?.ok === true && !(fixed.data.diagnostics ?? []).some((d) => d.level === "error"), "ok: true and no error", JSON.stringify(fixed.data?.diagnostics)?.slice(0, 200));

		// Publish to the local server.
		const publish = await mcp.call("richview_publish", { inputPath: file, new: true });
		const slug = "hello-world";
		const published = publish.data ?? {};
		j.assert("publish returns ok with the diagnostic shape", "publish", publish.isError === false && published.ok === true && resultProblems(published).length === 0, "ok: true", `isError ${publish.isError} ${JSON.stringify(published)?.slice(0, 220)} ${publish.text.slice(0, 120)}`);
		j.equal("the publish result carries the URL and the version", "publish", [published.url, published.permalink, published.version, published.slug], [`${server.base}/r/${slug}`, `${server.base}/r/${slug}/v/0.0.1`, "0.0.1", slug]);
		j.equal("the publish result says the report was created, with the server's default visibility", "publish", [published.created, published.visibility], [true, "private"]);
		const address = published.url ?? `${server.base}/r/${slug}`;
		const withToken = await readReport(address, server.token);
		j.assert("the published URL serves the report to the token", "publish", withToken.status === 200 && withToken.text.includes(TITLE), "200 with the title", `${withToken.status} at ${withToken.url}`);
		const without = await readReport(address);
		j.assert("the private report is refused without the token", "publish", refusedReport(without, TITLE), "refused or sent to sign-in, without the title", `${without.status} at ${without.url}`);
		j.equal("the MCP stream carried protocol messages only", "app surfaces", mcp.stray, []);
	} finally {
		mcp.close();
		await server.stop();
	}
}

/** Publish from the CLI and MCP; read, share, pin, delete, refuse and preview. */
async function coworkJourney(j, c) {
	const root = join(c.out, "cowork");
	rmSync(root, { recursive: true, force: true });
	mkdirSync(root, { recursive: true });
	const service = randomBytes(16).toString("hex");
	const server = await startServer(c, { dataDir: join(root, "server-data"), token: service, visibility: "private" });
	// Page reads cross the app/report origins; API mutations retain manual redirects below.
	const read = (token, path) => readReport(`${server.base}${path}`, token);
	const api = async (token, method, path, body) => {
		const response = await fetch(`${server.base}${path}`, { method, headers: { ...(token ? { authorization: `Bearer ${token}` } : {}), ...(body ? { "content-type": "application/json" } : {}) }, body: body ? JSON.stringify(body) : undefined, redirect: "manual" });
		const text = await response.text();
		let json;
		try {
			json = JSON.parse(text);
		} catch {
			json = undefined;
		}
		return { status: response.status, text, json, headers: response.headers };
	};
	const person = async (name, role) => {
		const made = await api(service, "POST", "/api/governance/users", { name, email: `${name.toLowerCase()}@example.test`, role });
		const token = await api(service, "POST", `/api/governance/users/${made.json?.id}/tokens`, { name: "journey" });
		return { id: made.json?.id, token: token.json?.token };
	};
	let mcp;
	try {
		const [owner, reader, outsider] = [await person("Olga", "author"), await person("Rita", "reader"), await person("Sam", "reader")];
		j.need("the server makes an owner, a reader and an outsider", "publish", [owner, reader, outsider].every((p) => p.id && p.token), "three people with tokens", JSON.stringify([owner, reader, outsider]).slice(0, 160));
		mcp = new Mcp(c, { RICHVIEW_SERVER: server.base, RICHVIEW_TOKEN: owner.token });
		await mcp.start();

		// The same source, offline: the reference for the permalink.
		const make = (name, title, extra = "") => {
			const folder = join(root, name);
			cli(c, ["init", folder]);
			const file = join(folder, "report.rmd");
			let source = readFileSync(file, "utf8");
			source = source.replace("slug: hello-world", `slug: ${name}`).replace(`# ${TITLE}`, `# ${title}`).replace(`title: ${TITLE}`, `title: ${title}`);
			writeFileSync(file, `${source.trimEnd()}\n${ADDED_BLOCKS}${extra}`);
			return file;
		};
		const cliFile = make("cli-story", "The CLI story");
		const mcpFile = make("mcp-story", "The MCP story");
		const offline = cli(c, ["generate", cliFile, "--format", "json"]);
		j.need("the offline report builds", "app surfaces", offline.json?.ok === true, "ok", offline.stderr.slice(0, 160));

		const first = cli(c, ["publish", cliFile, "--server", server.base, "--token", owner.token, "--new", "--format", "json"]);
		j.need("publish from the CLI", "app surfaces", first.status === 0 && first.json?.ok === true && first.json.url === `${server.base}/r/cli-story`, "ok and the report's URL", `exit ${first.status} ${first.stdout.slice(0, 200)} ${first.stderr.slice(0, 120)}`);
		const second = await mcp.call("richview_publish", { inputPath: mcpFile, new: true });
		j.need("publish from MCP", "app surfaces", second.data?.ok === true && second.data.url === `${server.base}/r/mcp-story`, "ok and the report's URL", `${JSON.stringify(second.data)?.slice(0, 200)} ${second.text.slice(0, 120)}`);
		j.equal("both reports are owned by the publisher and private", "publish", [first.json.visibility, second.data.visibility], ["private", "private"]);

		// A reader with a grant opens the permalink in Chromium.
		const shared = await api(owner.token, "PUT", "/api/reports/cli-story/access", { visibility: "private", grants: [{ user: reader.id, role: "viewer" }] });
		j.need("the owner shares with one reader", "publish", shared.status === 200, "200", `${shared.status} ${shared.text.slice(0, 160)}`);
		const pw = await playwright(c.cwd);
		const browser = await launchBrowser(pw.chromium, { headless: true, ...(process.env.RICHVIEW_CHROMIUM ? { executablePath: process.env.RICHVIEW_CHROMIUM } : {}) }, { TimeoutError: pw.errors.TimeoutError });
		try {
			const open = async (headers, url, viewport = { width: 1280, height: 900 }) => {
				const context = await browser.newContext({ viewport, reducedMotion: "reduce", extraHTTPHeaders: headers });
				try {
					return await probePage(context, url);
				} finally {
					await context.close();
				}
			};
			const local = await open({}, pathToFileURL(offline.json.output).href);
			const permalink = await open({ authorization: `Bearer ${reader.token}` }, `${server.base}/r/cli-story`);
			j.assert("the reader opens the permalink and the runtime draws its charts", "site", permalink.status === 200 && permalink.loaded && permalink.settled && permalink.models.length === 2, "200, runtime loaded, 2 models", `${permalink.status} loaded ${permalink.loaded} settled ${permalink.settled} models ${permalink.models.length}`);
			j.equal("the permalink has no console error or CSP violation", "renderer", [permalink.errors, permalink.violations], [[], []]);
			j.equal("the permalink's geometry models equal the offline file's", "chart engine", modelDigest(permalink.models), modelDigest(local.models));
			const refusedOpen = await open({ authorization: `Bearer ${outsider.token}` }, `${server.base}/r/cli-story`);
			j.assert("a reader without access is refused in the browser", "publish", [401, 403, 404].includes(refusedOpen.status) && !refusedOpen.text.includes("The CLI story"), "refused without the title", `${refusedOpen.status} ${refusedOpen.text.slice(0, 60)}`);
		} finally {
			await browser.close();
		}
		for (const [who, token] of [["an outsider", outsider.token], ["an anonymous request", undefined]]) {
			const refused = await read(token, "/r/cli-story");
			j.assert(`${who} is refused the page`, "publish", refusedReport(refused, "The CLI story"), "refused or sent to sign-in, without the title", `${refused.status} at ${refused.url}`);
			const raw = await api(token, "GET", "/raw/cli-story");
			j.assert(`${who} is refused the source`, "publish", [401, 403, 404].includes(raw.status) && !raw.text.includes("Orders by plan"), "refused without the source", raw.status);
		}
		const edit = await api(reader.token, "PUT", "/api/reports/cli-story/access", { visibility: "public" });
		j.assert("a viewer cannot share the report", "publish", [401, 403, 404].includes(edit.status), "refused", edit.status);

		// Pin and delete: a second version, pinned back to the first, then removed.
		writeFileSync(cliFile, readFileSync(cliFile, "utf8").replace("version: 0.0.1", "version: 0.0.2").replace("## Orders by plan", "## Orders by plan, second version"));
		const v2 = cli(c, ["publish", cliFile, "--server", server.base, "--token", owner.token, "--format", "json"]);
		j.need("the owner publishes version 0.0.2", "app surfaces", v2.json?.ok === true && v2.json.version === "0.0.2" && v2.json.permalink === `${server.base}/r/cli-story/v/0.0.2`, "ok, 0.0.2 and its permalink", `${v2.stdout.slice(0, 200)} ${v2.stderr.slice(0, 120)}`);
		let latest = await read(owner.token, "/r/cli-story");
		j.assert("the latest version is 0.0.2", "publish", latest.status === 200 && latest.text.includes("second version"), "200 and the second version's text", latest.status);
		const pin = await api(owner.token, "PUT", "/api/reports/cli-story/latest", { version: "0.0.1" });
		j.assert("the owner pins 0.0.1", "publish", pin.status === 200 && pin.json?.pinned === "0.0.1", "200 pinned 0.0.1", `${pin.status} ${pin.text.slice(0, 120)}`);
		latest = await read(owner.token, "/r/cli-story");
		j.assert("the pinned version is served as the latest", "publish", latest.status === 200 && latest.text.includes("The CLI story") && !latest.text.includes("second version"), "0.0.1's text", latest.status);
		const unpin = await api(owner.token, "DELETE", "/api/reports/cli-story/latest");
		latest = await read(owner.token, "/r/cli-story");
		j.assert("unpinning serves 0.0.2 again", "publish", unpin.status === 200 && latest.status === 200 && latest.text.includes("second version"), "200 and the second version", `${unpin.status}, page ${latest.status}`);
		const deleted = await api(owner.token, "DELETE", "/api/reports/cli-story/v/0.0.2");
		const gone = await read(owner.token, "/r/cli-story/v/0.0.2");
		j.assert("the owner deletes 0.0.2", "publish", deleted.status === 200 && [404, 410].includes(gone.status), "200 then 404 or 410", `${deleted.status} then ${gone.status}`);
		const refusedDelete = await api(reader.token, "DELETE", "/api/reports/cli-story/v/0.0.1");
		j.assert("a viewer cannot delete a version", "publish", [401, 403, 404].includes(refusedDelete.status), "refused", refusedDelete.status);

		// Link previews (INT-07, FEA-SF-55).
		const shareOut = await api(owner.token, "PUT", "/api/reports/cli-story/access", { visibility: "public" });
		j.need("the owner shares the story with everyone", "publish", shareOut.status === 200 && shareOut.json?.visibility === "public", "200 public", `${shareOut.status} ${shareOut.text.slice(0, 120)}`);
		const page = await read(undefined, "/r/cli-story");
		j.assert("the shared story's page is served", "publish", page.status === 200, "200", page.status);
		const tags = previewTags(page.text);
		j.equal("a shared story's preview carries its title and description", "publish", [tags["og:title"], typeof tags["og:description"]], ["The CLI story", "string"]);
		if (!tags["og:image"]) {
			j.notBuilt("a shared story's preview carries a thumbnail (og:image, twitter:card, FEA-SF-55, ARC-69)", "publish", "the story thumbnail rendered when the version is stored");
		} else {
			const image = await fetch(tags["og:image"].replace(/^https?:\/\/[^/]+/, server.base));
			const bytes = Buffer.from(await image.arrayBuffer());
			j.assert("the thumbnail is a 1200x630 PNG the crawler can fetch", "publish", image.status === 200 && image.headers.get("content-type") === "image/png" && bytes.readUInt32BE(16) === 1200 && bytes.readUInt32BE(20) === 630 && tags["twitter:card"] === "summary_large_image", "200 image/png 1200x630 and summary_large_image", `${image.status} ${image.headers.get("content-type")} ${bytes.length} bytes, card ${tags["twitter:card"]}`);
		}
		const hidden = await read(undefined, "/r/mcp-story");
		j.assert("a private story's preview reveals neither title nor image", "publish", !hidden.text.includes("The MCP story") && !hidden.text.includes("og:image") && !hidden.text.includes("og:title"), "no title and no image", `${hidden.status} ${hidden.text.slice(0, 100)}`);
		// Compare the same requested URL: sign-in legitimately echoes its next destination.
		const removedPrivate = await api(owner.token, "DELETE", "/api/reports/mcp-story/v/0.0.1");
		const absentToOwner = await read(owner.token, "/r/mcp-story");
		j.need("the owner removes the private story before its missing-page comparison", "publish", removedPrivate.status === 200 && [404, 410].includes(absentToOwner.status), "200 delete then 404 or 410", `${removedPrivate.status}, owner page ${absentToOwner.status}`);
		const missing = await read(undefined, "/r/mcp-story");
		j.assert("a private story answers exactly as a missing one", "publish", hidden.status === missing.status && hidden.text === missing.text && hidden.headers.get("content-type") === missing.headers.get("content-type"), `${missing.status} and the same bytes`, `${hidden.status}, same bytes: ${hidden.text === missing.text}`);
	} finally {
		mcp?.close();
		await server.stop();
	}
}

const JOURNEYS = {
	author: { title: "Author", run: authorJourney },
	agent: { title: "Agent", run: agentJourney },
	cowork: { title: "Cowork", run: coworkJourney },
	publish: { title: "Publish", run: (j, c) => publishJourney(j, { cwd: c.cwd, out: c.out, wasm: c.wasm }) },
};

export async function runJourneys({ cwd, bin, out, set, publicDir, wasm, only }) {
	const binary = resolve(cwd, bin);
	if (!existsSync(binary)) throw new Error(`The release CLI ${binary} does not exist; the journeys step needs the build step first`);
	if (!SETS[set]) throw new Error(`Unknown set ${set}; use release or full`);
	mkdirSync(out, { recursive: true });
	const real = realpathSync(out);
	const dist = [publicDir && resolve(cwd, publicDir), join(cwd, "dist/web"), join(resolve(binary, ".."), "dist/web")].find((p) => p && existsSync(p));
	const c = { cwd, bin: binary, out: real, publicDir: dist, wasm };
	const journeys = [];
	for (const name of SETS[set].filter((n) => !only || n === only)) {
		const journey = new Journey(name, JOURNEYS[name].title);
		await runJourney(journey, (j) => JOURNEYS[name].run(j, c));
		journeys.push(journey);
		for (const line of reportLines([journey])) console.log(line);
	}
	writeFileSync(join(real, "journeys.json"), `${JSON.stringify({ set, journeys: journeys.map((j) => ({ name: j.name, passed: j.passed, seconds: j.seconds, steps: j.steps })) }, null, 2)}\n`);
	return journeys;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
	const arg = (name) => {
		const at = process.argv.indexOf(name);
		return at < 0 ? undefined : process.argv[at + 1];
	};
	const [bin, out, set] = [arg("--bin"), arg("--out"), arg("--set") ?? "release"];
	if (!bin || !out) {
		console.error("usage: journeys.mjs --bin <richview> --out <dir> --set release|full [--public <dist/web>] [--publish-wasm <wasm>] [--only <journey>] [--cwd <repo>]");
		process.exit(2);
	}
	const journeys = await runJourneys({ cwd: resolve(arg("--cwd") ?? process.cwd()), bin, out: resolve(out), set, publicDir: arg("--public"), wasm: arg("--publish-wasm"), only: arg("--only") });
	const failed = journeys.filter((j) => !j.passed);
	console.log(`journeys (${set}): ${journeys.length - failed.length}/${journeys.length} passed in ${journeys.reduce((s, j) => s + j.seconds, 0).toFixed(1)} s`);
	process.exit(failed.length ? 1 : 0);
}
