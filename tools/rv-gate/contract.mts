// The contract step of the release run (ARC-08, TYP-53, FEA-RP-01, FEA-CH-59, ARC-62): every golden
// input and a few deliberately broken reports run through each surface the way a real client drives
// it, and the diagnostics, the `ok` envelope, the HTML and the chart geometry are compared.
//
// Surfaces (ARC-08: a surface decodes its input, calls the operation and encodes the result):
//   cli.validate, cli.check   `richview validate|check <report> --format json`
//   mcp.validate, mcp.check   `richview mcp` over stdio JSON-RPC: initialize, tools/list, tools/call
//   editor.lint               `richview-editor` (LSP over stdio): didOpen, then textDocument/publishDiagnostics
//   wasm.validate, wasm.check dist/web/richview_site.js and its _bg.wasm, loaded in Node; the host duty
//                             (folder inventory, data and media bytes) is done here the way worker.js does it
//   http.validate, http.check `richview server` on loopback with a token: POST /api/validate and /api/check with
//                             the source, the folder inventory (root as its basename) and, for check, the data the
//                             Wasm host loop gathered; nothing is stored (ARC-08)
//
// The diagnostic record is TYP-53's (OPN-57's proposal, built unanswered): level, code, message, fix,
// expected, retry, path, file, line, column, tag, details. ARC-08's shorter record is its subset.
// The editor's LSP diagnostic has slots for level, code, message and a range only, so it is compared on
// the six fields the brief names (level, code, message, path, line, tag); path and tag it cannot carry
// show up as differences, because ARC-08 says a surface drops nothing.
//
// Usage: node tools/rv-gate/contract.mts --bin target/release/richview --out <dir>
//        [--editor <bin>] [--dist <dir>] [--only <text>] [--seed-bin <seed_duckdb>]
//
// Connector examples (lead, 28 Oct; lib/connectors.mjs): a DuckDB example with data/seed.sql is copied,
// seeded and compared on the native surfaces (CLI, MCP, editor) with the seeded data; a MetricFlow example
// is compared on the validate surfaces only and lists its other surfaces in `notCompared`.

import { spawn, spawnSync } from "node:child_process";
import { createHash, randomBytes } from "node:crypto";
import { cpSync, existsSync, mkdirSync, readFileSync, readdirSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { basename, dirname, extname, join, relative, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import vm from "node:vm";
import { MF_REASON, SKIP_COPY, classifyConnectors, prepareSeedExample, seedBinPath } from "./lib/connectors.mts";
import { discoverInputs } from "./golden.mts";

import type { ChildProcessWithoutNullStreams } from "node:child_process";

type Diagnostic = Record<string, unknown> & {level?: string; code?: string; message?: string; line?: number | null; path?: string; details?: Record<string, unknown> & {measured?: string}};
type Position = {line: number; character: number};
type LspDiagnostic = {severity?: number; code: string; message: string; range?: {start: Position; end: Position}; data?: Diagnostic};
type Difference = {path: string; native: unknown; wasm: unknown};
type DiagnosticDiff =
	| {kind: "changed"; code?: string; line: number | null; changes: {field: string; base: unknown; other: unknown}[]}
	| {kind: "missing" | "added"; diagnostic: Diagnostic}
	| {kind: "order"; base: (string | undefined)[]; other: (string | undefined)[]};
type EnvelopeDiff = {kind: "ok"; surface: string; base: boolean | null; other: boolean | null; note: string};
type NamedDiff = {group: string; surface: string; against?: string} & (DiagnosticDiff | EnvelopeDiff | {kind: "surface-failed" | "host-error" | "connector-unsupported" | "geometry" | "html"; error?: string; [key: string]: unknown});
type ProductEnvelope = {ok?: boolean; status?: string; error?: string; diagnostics?: Diagnostic[]; charts?: unknown[]};
type RawResult = {json: ProductEnvelope; exit?: number | null; isError?: boolean; error?: never} | {error: string; json?: never};
type Surface = {error?: string; hostError?: string; diagnostics?: Diagnostic[]; envelope?: boolean; ok?: boolean | null; status?: string | null; charts?: unknown[] | null};
type HtmlComparison = ReturnType<typeof compareHtml> | {equal: false; error: string} | {equal: null; note: string};
type GeometryComparison = {equal: true; charts: number; surfaces: string[]} | {equal: false; surface: string} & Difference | {equal: null; note: string};
type NativeOnly = {surfaces: string[]; reason: string};
type FixtureResult = {id: string; kind: string; connectorKind: string | null; surfaces: Record<string, Surface>; html: HtmlComparison | null; geometry: GeometryComparison | null; browserStage: {nativeRan: boolean; wasmRan: boolean; compared: boolean} | null; diffs: NamedDiff[]; notCompared?: NativeOnly; ok?: boolean};
type ContractReport = {step: string; cards: string[]; bin: string; editor: string; dist: string; surfaces: Record<string, string>; mcpTools: string[]; fixtures: FixtureResult[]; seconds: number; totals: {fixtures: number; withDiffs: number; diffs: number}};
type RpcMessage = {id?: number; method?: string; error?: unknown; params?: {uri?: string; diagnostics?: LspDiagnostic[]}; result?: {tools?: {name: string}[]; isError?: boolean; structuredContent?: ProductEnvelope}};
type Fixture = {id: string; from: string; kind: string};
type ReportInput = {id: string; file: string; source: string};
type Data = Record<string, string>;
type ReportFolder = {root: string; source: string; files: string[]};
type WasmReport = {html: string; diagnostics?: Diagnostic[]; ok?: boolean};
type WasmBindings = {
	initSync(input: {module: Buffer}): unknown;
	provide_font_catalog(css: string): unknown;
	validate(source: string, options: {report_folder: ReportFolder}): unknown;
	check(source: string, options: {report_folder: ReportFolder; source: string; data: Data}): unknown;
	worker_request(source: string, options: string): string;
};
type WasmFailure = {error: string; diagnostics?: never; report?: never; data?: never; ok?: never; status?: never; charts?: never} | {diagnostics: Diagnostic[]; error?: never; report?: never; data?: never; ok?: never; status?: never; charts?: never};
type WasmReply = {ok: boolean | null; status: string | null; diagnostics: Diagnostic[]; charts?: unknown[] | null; error?: never} | WasmFailure;
type WasmRenderReply = {report: WasmReport; data: Data; error?: never; diagnostics?: never} | WasmFailure;
type HttpServer = {base: string; token: string; stop: () => boolean; error?: never} | {error: string; base?: never; token?: never; stop?: never};

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(HERE, "../..");
const BROKEN_DIR = "tools/rv-gate/fixtures/contract";
const REPORT = "report.rmd";

/** TYP-53's record, in the order a diagnostic is printed. */
export const RECORD_FIELDS = ["level", "code", "message", "fix", "expected", "retry", "path", "file", "line", "column", "tag", "details"];
/** What the brief and ARC-08 name; the only fields an LSP diagnostic is judged on. */
export const CORE_FIELDS = ["level", "code", "message", "path", "line", "tag"];

// ---------------------------------------------------------------------------------------------
// Pure parts: normalization, the diff and the LSP mapping. Tested in test/contract.test.mjs.
// ---------------------------------------------------------------------------------------------

/** Key-sorted JSON, so two equal values give equal text. */
export function canonical(value: unknown): string {
	if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
	if (value && typeof value === "object") {
		const fields = value as Record<string, unknown>;
		return `{${Object.keys(fields).sort().filter((k) => fields[k] !== undefined).map((k) => `${JSON.stringify(k)}:${canonical(fields[k])}`).join(",")}}`;
	}
	return JSON.stringify(value) ?? "null";
}

/**
 * Normalization 1, absolute paths: a native message names the absolute report folder (TYP-53 wants
 * report-relative paths only, but today the CLI embeds them). Each surface sees the same folder under
 * a different spelling (symlink-resolved, file: URL), so every spelling becomes `<report>`.
 */
export function normalizeText(text: string, roots: readonly string[]) {
	if (typeof text !== "string") return text;
	let out = text;
	const spellings = new Set<string>();
	for (const root of roots) {
		spellings.add(root);
		spellings.add(pathToFileURL(root).href);
	}
	for (const spelling of [...spellings].sort((a, b) => b.length - a.length)) out = out.split(spelling).join("<report>");
	return out;
}

/** A diagnostic reduced to the compared fields, with absolute paths made report-relative. */
export function normalizeDiagnostic(d: Diagnostic, roots: readonly string[], fields = RECORD_FIELDS): Diagnostic {
	const out: Diagnostic = {};
	for (const field of fields) {
		let value = d[field];
		if (value === undefined || value === null) continue;
		if (typeof value === "string") value = normalizeText(value, roots);
		else if (field === "details") value = JSON.parse(normalizeText(JSON.stringify(value), roots));
		out[field] = value;
	}
	return out;
}

/** LSP severity to a level (the editor sends 1 error, 2 warning, 3 anything else). */
export function levelOfSeverity(severity?: number) {
	return severity === 1 ? "error" : severity === 2 ? "warning" : "info";
}

/**
 * Normalization 2, LSP lines: a range is 0-based, a diagnostic's `line` is 1-based, so the range's
 * start line gains one. A range is mandatory in LSP, so a diagnostic with no position is sent as the
 * zero-width range 0:0; that range maps back to "no line". The record fields LSP has no slot for
 * (`path`, `tag` and the rest of TYP-53's) travel in `data`, and are read back from there.
 */
export function lspToDiagnostic(lsp: LspDiagnostic) {
	const out: Diagnostic = { level: levelOfSeverity(lsp.severity), code: lsp.code, message: lsp.message };
	const { start, end } = lsp.range ?? {};
	const unlocated = start && end && start.line === 0 && start.character === 0 && end.line === 0 && end.character === 0;
	if (start && !unlocated) out.line = start.line + 1;
	for (const field of RECORD_FIELDS) {
		if (out[field] === undefined && lsp.data?.[field] !== undefined && lsp.data[field] !== null) out[field] = lsp.data[field];
	}
	return out;
}

function pick(d: Diagnostic, fields: string[]) {
	const out: Diagnostic = {};
	for (const f of fields) if (d[f] !== undefined && d[f] !== null) out[f] = d[f];
	return out;
}

/**
 * Compare two diagnostic lists as multisets. Returns the differences:
 * `missing` (in `base`, not in `other`), `added` (in `other` only), `changed` (same code, different
 * fields: lists each field with both values) and `order` when only the sequence differs.
 */
export function diffDiagnostics(base: Diagnostic[], other: Diagnostic[], fields = RECORD_FIELDS) {
	const a = base.map((d) => pick(d, fields));
	const b = other.map((d) => pick(d, fields));
	const diffs: DiagnosticDiff[] = [];
	const left = a.map((d, i) => ({ d, i, key: canonical(d) }));
	let right = b.map((d, i) => ({ d, i, key: canonical(d) }));
	const unmatched = [];
	for (const item of left) {
		const at = right.findIndex((r) => r.key === item.key);
		if (at >= 0) right.splice(at, 1);
		else unmatched.push(item);
	}
	const stillLeft = [];
	for (const item of unmatched) {
		// Same code, preferring the same line, then the same path: it is that diagnostic, changed.
		const candidates = right.filter((r) => r.d.code === item.d.code);
		const best = candidates.find((r) => r.d.line === item.d.line) ?? candidates.find((r) => r.d.path === item.d.path) ?? candidates[0];
		if (!best) {
			stillLeft.push(item);
			continue;
		}
		right = right.filter((r) => r !== best);
		const changes = fields.filter((f) => canonical(item.d[f]) !== canonical(best.d[f])).map((f) => ({ field: f, base: item.d[f] ?? null, other: best.d[f] ?? null }));
		diffs.push({ kind: "changed", code: item.d.code, line: item.d.line ?? null, changes });
	}
	for (const item of stillLeft) diffs.push({ kind: "missing", diagnostic: item.d });
	for (const item of right) diffs.push({ kind: "added", diagnostic: item.d });
	if (!diffs.length && canonical(a) !== canonical(b)) {
		// Equal as sets but a different sequence: agents read diagnostics in order.
		diffs.push({ kind: "order", base: a.map((d) => d.code), other: b.map((d) => d.code) });
	}
	return diffs;
}

/** The `ok` of a machine result, or null when its shape has none (ARC-08: one envelope, including `ok`). */
export function okOf(result: unknown) {
	return result && typeof result === "object" && !Array.isArray(result) && "ok" in result && typeof result.ok === "boolean" ? result.ok : null;
}

/** Differences among the `ok` values of one operation's machine outputs; `{surface: ok|null}`. */
export function envelopeDiffs(oks: Record<string, boolean | null>, baseline: string) {
	const diffs: EnvelopeDiff[] = [];
	for (const [surface, ok] of Object.entries(oks)) {
		if (surface !== baseline && ok !== oks[baseline]) diffs.push({ kind: "ok", surface, base: oks[baseline], other: ok, note: ok === null ? "the result has no `ok`" : "`ok` differs" });
	}
	return diffs;
}

/**
 * FEA-CH-59: a diagnostic measured by the browser glyph pass says so. This is the stage's marker:
 * `details.measured === "browser"` or the words "measured in a browser" in the message.
 */
export function isBrowserMeasured(d: Diagnostic | undefined) {
	return d?.details?.measured === "browser" || /measured in a browser/i.test(d?.message ?? "");
}

/**
 * Split browser-measured diagnostics from the rest. They are compared only when both sides ran the
 * stage; otherwise they are set aside and the pair is reported as not compared.
 */
export function splitBrowserStage(native: Diagnostic[], wasm: Diagnostic[]) {
	const ran = { native: native.some(isBrowserMeasured), wasm: wasm.some(isBrowserMeasured) };
	const both = ran.native && ran.wasm;
	const keep = (list: Diagnostic[]) => (both ? list : list.filter((d) => !isBrowserMeasured(d)));
	return { ran, compared: both, native: keep(native), wasm: keep(wasm) };
}

/** First place two JSON values differ, as a path with both values, or null when they are equal. */
export function firstDifference(a: unknown, b: unknown, path = "$"): Difference | null {
	if (canonical(a) === canonical(b)) return null;
	if (Array.isArray(a) && Array.isArray(b)) {
		if (a.length !== b.length) return { path: `${path}.length`, native: a.length, wasm: b.length };
		for (let i = 0; i < a.length; i++) {
			const inner = firstDifference(a[i], b[i], `${path}[${i}]`);
			if (inner) return inner;
		}
	}
	if (a && b && typeof a === "object" && typeof b === "object" && !Array.isArray(a) && !Array.isArray(b)) {
		for (const key of [...new Set([...Object.keys(a), ...Object.keys(b)])].sort()) {
			const inner = firstDifference((a as Record<string, unknown>)[key], (b as Record<string, unknown>)[key], `${path}.${key}`);
			if (inner) return inner;
		}
	}
	return { path, native: a ?? null, wasm: b ?? null };
}

/** Two HTML strings: equal, or where they first differ with a little context and both hashes. */
export function compareHtml(native: string, wasm: string) {
	const sha = (s: string|NodeJS.ArrayBufferView<ArrayBufferLike>) => createHash("sha256").update(s).digest("hex").slice(0, 12);
	if (native === wasm) return { equal: true, sha256: sha(native), bytes: Buffer.byteLength(native) };
	let at = 0;
	while (at < native.length && at < wasm.length && native[at] === wasm[at]) at++;
	return {
		equal: false,
		native: { sha256: sha(native), bytes: Buffer.byteLength(native) },
		wasm: { sha256: sha(wasm), bytes: Buffer.byteLength(wasm) },
		firstDifferenceAt: at,
		nativeContext: native.slice(Math.max(0, at - 40), at + 80),
		wasmContext: wasm.slice(Math.max(0, at - 40), at + 80),
	};
}

/** Split an LSP byte stream into JSON messages; returns `{messages, rest}`. */
export function parseFrames<Message = unknown>(buffer: Buffer) {
	const messages: Message[] = [];
	let rest = buffer;
	for (;;) {
		const end = rest.indexOf("\r\n\r\n");
		if (end < 0) break;
		const length = Number(/Content-Length:\s*(\d+)/i.exec(rest.subarray(0, end).toString("latin1"))?.[1]);
		if (!Number.isFinite(length) || rest.length < end + 4 + length) break;
		messages.push(JSON.parse(rest.subarray(end + 4, end + 4 + length).toString("utf8")));
		rest = rest.subarray(end + 4 + length);
	}
	return { messages, rest };
}

const frame = (message: unknown) => {
	const body = Buffer.from(JSON.stringify(message));
	return Buffer.concat([Buffer.from(`Content-Length: ${body.length}\r\n\r\n`), body]);
};

// ---------------------------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------------------------

/** Golden inputs and focused contract reports, each copied into `<out>/fixtures/<id>`. */
export function discoverFixtures(cwd: string): Fixture[] {
	const fixtures = discoverInputs(cwd).map(({ id, dir }) => ({ id, from: join(cwd, dir), kind: "golden" }));
	const projection = "scatter3d-projection";
	fixtures.push({ id: `contract/${projection}`, from: join(cwd, "tools/rv-gate/fixtures", projection), kind: "regression" });
	const broken = join(cwd, BROKEN_DIR);
	for (const name of readdirSync(broken).sort()) {
		if (existsSync(join(broken, name, REPORT))) fixtures.push({ id: `contract/${name}`, from: join(broken, name), kind: "broken" });
	}
	return fixtures;
}

function listFiles(root: string) {
	const files: string[] = [];
	const walk = (dir: string) => {
		for (const entry of readdirSync(dir, { withFileTypes: true })) {
			if (entry.isDirectory()) walk(join(dir, entry.name));
			else files.push(relative(root, join(dir, entry.name)).split("\\").join("/"));
		}
	};
	walk(root);
	return files;
}

// ---------------------------------------------------------------------------------------------
// Surfaces
// ---------------------------------------------------------------------------------------------

function runCli(bin: string, args: string[], cwd: string): RawResult {
	const run = spawnSync(bin, args, { cwd, encoding: "utf8", maxBuffer: 256 * 1024 * 1024 });
	try {
		return { json: JSON.parse(run.stdout), exit: run.status };
	} catch {
		return { error: `exit ${run.status}: ${(run.stderr || run.stdout).slice(0, 300)}` };
	}
}

/** A JSON-RPC client over a child's stdio, one JSON message per line (MCP) or Content-Length frames (LSP). */
class Rpc {
	declare child: ChildProcessWithoutNullStreams;
	declare framing: string;
	declare pending: Map<number, (message: RpcMessage) => void>;
	declare notifications: RpcMessage[];
	declare waiters: ((message: RpcMessage) => boolean)[];
	declare nextId: number;
	declare buffer: Buffer;
	declare stderr: string;
	declare exited: Promise<number | null>;

	constructor(command: string, args: readonly string[], framing: string) {
		this.child = spawn(command, args, { stdio: ["pipe", "pipe", "pipe"] });
		this.framing = framing;
		this.pending = new Map();
		this.notifications = [];
		this.waiters = [];
		this.nextId = 1;
		this.buffer = Buffer.alloc(0);
		this.stderr = "";
		this.child.stderr.on("data", (c) => (this.stderr += c));
		this.child.stdout.on("data", (chunk: Uint8Array<ArrayBufferLike>) => this.#read(chunk));
		this.exited = new Promise<number | null>((done) => this.child.on("close", done));
	}
	#read(chunk: Uint8Array<ArrayBufferLike>) {
		this.buffer = Buffer.concat([this.buffer, chunk]);
		let messages: RpcMessage[];
		if (this.framing === "lines") {
			messages = [];
			let at;
			while ((at = this.buffer.indexOf(10)) >= 0) {
				const line = this.buffer.subarray(0, at).toString("utf8").trim();
				this.buffer = this.buffer.subarray(at + 1);
				if (line) messages.push(JSON.parse(line));
			}
		} else {
			({ messages, rest: this.buffer } = parseFrames<RpcMessage>(this.buffer));
		}
		for (const message of messages) {
			if (message.id !== undefined && this.pending.has(message.id) && !message.method) {
				this.pending.get(message.id)!(message);
				this.pending.delete(message.id);
			} else {
				this.notifications.push(message);
				this.waiters = this.waiters.filter((w) => !w(message));
			}
		}
	}
	#send(message: {jsonrpc: string; id?: number; method: string; params: unknown}) {
		this.child.stdin.write(this.framing === "lines" ? `${JSON.stringify(message)}\n` : frame(message));
	}
	request(method: string, params: unknown, timeoutMs = 120_000) {
		const id = this.nextId++;
		return new Promise<RpcMessage>((done, fail) => {
			const timer = setTimeout(() => fail(new Error(`${method} timed out; stderr: ${this.stderr.slice(0, 300)}`)), timeoutMs);
			this.pending.set(id, (m) => {
				clearTimeout(timer);
				done(m);
			});
			this.#send({ jsonrpc: "2.0", id, method, params });
		});
	}
	notify(method: string, params: unknown) {
		this.#send({ jsonrpc: "2.0", method, params });
	}
	/** Resolves with the first notification (already received or future) the predicate accepts. */
	waitFor(predicate: (message: RpcMessage) => boolean, timeoutMs = 60_000) {
		const seen = this.notifications.find(predicate);
		if (seen) return Promise.resolve(seen);
		return new Promise<RpcMessage>((done, fail) => {
			const timer = setTimeout(() => fail(new Error(`no matching notification; stderr: ${this.stderr.slice(0, 300)}`)), timeoutMs);
			this.waiters.push((m) => {
				if (!predicate(m)) return false;
				clearTimeout(timer);
				done(m);
				return true;
			});
		});
	}
	async close(graceful?: () => Promise<void>) {
		try {
			await graceful?.();
		} catch {
			/* the child is killed below */
		}
		this.child.stdin.end();
		const timer = setTimeout(() => this.child.kill("SIGKILL"), 5000);
		await this.exited;
		clearTimeout(timer);
	}
}

/** MCP: initialize, tools/list, then richview_validate and richview_check for every report. */
async function driveMcp(bin: string, reports: ReportInput[]) {
	const rpc = new Rpc(bin, ["mcp"], "lines");
	const results: Record<string, Record<string, RawResult>> = {};
	const notes: {tools?: string[]} = {};
	try {
		const init = await rpc.request("initialize", { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "rv-gate-contract", version: "0" } });
		if (init.error) throw new Error(`initialize: ${JSON.stringify(init.error)}`);
		rpc.notify("notifications/initialized", {});
		const listed = await rpc.request("tools/list", {});
		const names = (listed.result?.tools ?? []).map((t) => t.name);
		notes.tools = names;
		for (const tool of ["richview_validate", "richview_check"]) {
			if (!names.includes(tool)) throw new Error(`tools/list has no ${tool}: ${names.join(", ")}`);
		}
		for (const { id, file } of reports) {
			results[id] = {};
			for (const [key, tool] of [["validate", "richview_validate"], ["check", "richview_check"]]) {
				const reply = await rpc.request("tools/call", { name: tool, arguments: { inputPath: file } });
				const structured = reply.result?.structuredContent;
				results[id][key] = structured ? { json: structured, isError: reply.result!.isError === true } : { error: `no structuredContent: ${JSON.stringify(reply).slice(0, 300)}` };
			}
		}
	} finally {
		await rpc.close();
	}
	return { results, notes };
}

/** Editor service: LSP didOpen for each report; the first publishDiagnostics for it is the answer. */
async function driveEditor(editor: string, reports: ReportInput[]) {
	const rpc = new Rpc(editor, [], "frames");
	const results: Record<string, {lsp: LspDiagnostic[]; error?: never} | {error: string; lsp?: never}> = {};
	try {
		const init = await rpc.request("initialize", { processId: null, rootUri: null, capabilities: {} });
		if (init.error) throw new Error(`initialize: ${JSON.stringify(init.error)}`);
		rpc.notify("initialized", {});
		for (const { id, file, source } of reports) {
			const uri = pathToFileURL(file).href;
			const published = rpc.waitFor((m) => m.method === "textDocument/publishDiagnostics" && m.params?.uri === uri && m.params.diagnostics !== undefined);
			rpc.notify("textDocument/didOpen", { textDocument: { uri, languageId: "richview", version: 1, text: source } });
			try {
				results[id] = { lsp: (await published).params!.diagnostics! };
			} catch (error) {
				results[id] = { error: String((error as Error).message) };
			}
			rpc.notify("textDocument/didClose", { textDocument: { uri } });
			rpc.notifications.length = 0;
		}
	} finally {
		await rpc.close(async () => {
			await rpc.request("shutdown", null, 5000);
			rpc.notify("exit", null);
		});
	}
	return results;
}

const MIME: Record<string, string> = {
	".png": "image/png", ".jpg": "image/jpeg", ".jpeg": "image/jpeg", ".gif": "image/gif", ".webp": "image/webp", ".avif": "image/avif",
	".svg": "image/svg+xml", ".mp4": "video/mp4", ".webm": "video/webm", ".mp3": "audio/mpeg", ".ogg": "audio/ogg", ".wav": "audio/x-wav",
	".woff": "font/woff", ".woff2": "font/woff2", ".ttf": "font/ttf", ".otf": "font/otf", ".csv": "text/csv", ".json": "application/json",
};

/** The Wasm bindings loaded in Node, plus the runtime the native binary embeds, from `dist/web`. */
export function loadWasm(dist: string) {
	const js = join(dist, "richview_site.js");
	if (!existsSync(js)) throw new Error(`${js} is missing; build with richview-build or pass --dist`);
	const context: {fontsProvided?: unknown} = {};
	vm.runInThisContext(`${readFileSync(js, "utf8")}\n;globalThis.__rvContractWasm = wasm_bindgen;`, { filename: js });
	// wasm-bindgen initializes the staged script in Node rather than importing an ES module.
	const bindings = globalThis as typeof globalThis & {__rvContractWasm?: WasmBindings};
	const wasm = bindings.__rvContractWasm!;
	delete bindings.__rvContractWasm;
	wasm.initSync({ module: readFileSync(join(dist, "richview_site_bg.wasm")) });
	const fonts = join(dist, "richview-fonts.css");
	// ARC-62: supply the complete staged catalog; math is embedded only by renders that use it.
	if (existsSync(fonts)) {
		const math = join(dist, "fonts-math.css");
		const catalog = readFileSync(fonts, "utf8") + (existsSync(math) ? "\n" + readFileSync(math, "utf8") : "");
		context.fontsProvided = wasm.provide_font_catalog(catalog);
	}
	const runtime = {
		runtime_js: readFileSync(join(dist, "richview_runtime.js"), "utf8"),
		runtime_wasm: readFileSync(join(dist, "richview_runtime_bg.wasm.gz")).toString("base64"),
		runtime_wasm_codec: "gzip",
	};
	return { wasm, runtime, context };
}

/** The error a Wasm export throws: diagnostics as JSON text, or a plain message. */
function wasmFailure(error: unknown): WasmFailure {
	const text = String((error as Error | undefined)?.message ?? error);
	try {
		const parsed = JSON.parse(text);
		if (Array.isArray(parsed)) return { diagnostics: parsed };
	} catch {
		/* a plain message */
	}
	return { error: text };
}

/**
 * Wasm validate: `validate(source, {report_folder})` answers the shared envelope of
 * `richview validate --format json` (ARC-08), the folder's files checked as findings (ade8786).
 */
export function wasmValidate({ wasm }: {wasm: Pick<WasmBindings, "validate">}, root: string, source: string, files: string[]): WasmReply {
	try {
		return envelopeResult(wasm.validate(source, { report_folder: { root, source: REPORT, files } }));
	} catch (error) {
		return wasmFailure(error);
	}
}

/** Wasm check: `check(source, options)` with the data the host loop loaded, the same envelope. */
export function wasmCheck({ wasm }: {wasm: Pick<WasmBindings, "check">}, root: string, source: string, files: string[], data: Data): WasmReply {
	try {
		return envelopeResult(wasm.check(source, { report_folder: { root, source: REPORT, files }, source: join(root, REPORT), data }));
	} catch (error) {
		return wasmFailure(error);
	}
}

/** A Wasm result object as `{ok, status, diagnostics, charts}`; a bare array has no envelope. */
function envelopeResult(result: unknown): WasmReply {
	const value = (result instanceof Map ? Object.fromEntries(result) : result) as ProductEnvelope | Diagnostic[] | undefined;
	if (Array.isArray(value)) return { ok: null, status: null, diagnostics: value };
	return { ok: okOf(value), status: value?.status ?? null, diagnostics: value?.diagnostics ?? [], charts: value?.charts ?? null };
}

/**
 * Wasm render: the host loop of worker.js. `worker_request` names the references still missing, the
 * host loads them (text for data and styles, a data: URI for media), and the next call renders.
 */
export function wasmRender({ wasm, runtime }: Pick<ReturnType<typeof loadWasm>, "wasm" | "runtime">, root: string, source: string, files: string[], { withRuntime }: {withRuntime: boolean}): WasmRenderReply {
	const data: Data = {};
	const options = () => JSON.stringify({ report_folder: { root, source: REPORT, files }, source: join(root, REPORT), data, ...(withRuntime ? runtime : {}) });
	for (let round = 0; round < 8; round++) {
		let reply: {report?: WasmReport; references: {path: string; kind: string}[]};
		try {
			reply = JSON.parse(wasm.worker_request(round === 0 ? source : "", options()));
		} catch (error) {
			return wasmFailure(error);
		}
		if (reply.report) return { report: reply.report, data };
		let progress = false;
		for (const { path, kind } of reply.references) {
			if (data[path] !== undefined || !files.includes(path)) continue;
			const bytes = readFileSync(join(root, path));
			data[path] = kind === "media" ? `data:${MIME[extname(path).toLowerCase()] ?? "application/octet-stream"};base64,${bytes.toString("base64")}` : bytes.toString("utf8");
			progress = true;
		}
		if (!progress) return { error: `references the host cannot supply: ${reply.references.map((r) => r.path).join(", ")}` };
	}
	return { error: "worker_request kept asking for references" };
}

// ---------------------------------------------------------------------------------------------
// HTTP: the stateless validate and check endpoints of `richview server` (ARC-08)
// ---------------------------------------------------------------------------------------------

/** `richview server` on loopback with an empty store and a fresh token; `{ base, token, stop }` or `{ error }`. */
async function startHttp(bin: string, dataDir: string): Promise<HttpServer> {
	rmSync(dataDir, { recursive: true, force: true });
	mkdirSync(dataDir, { recursive: true });
	const token = randomBytes(16).toString("hex");
	const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.startsWith("RICHVIEW_")));
	const child = spawn(bin, ["server", "--bind", "127.0.0.1", "--port", "0", "--data-dir", dataDir, "--token", token], { env, stdio: ["ignore", "pipe", "pipe"] });
	let log = "";
	try {
		const base = await new Promise<string>((resolveBase, reject) => {
			const timer = setTimeout(() => reject(new Error(`no address after 20 s: ${log.slice(-300)}`)), 20_000);
			const read = (chunk: string) => {
				log += chunk;
				const match = /RichView at (http:\/\/127\.0\.0\.1:\d+)/.exec(log);
				if (match) {
					clearTimeout(timer);
					resolveBase(match[1]);
				}
			};
			child.stdout.on("data", read);
			child.stderr.on("data", read);
			child.on("exit", (code) => {
				clearTimeout(timer);
				reject(new Error(`exited with ${code}: ${log.slice(-300)}`));
			});
		});
		return { base, token, stop: () => child.kill() };
	} catch (error) {
		child.kill();
		return { error: `richview server: ${(error as Error).message}` };
	}
}

/** One POST; a body without a diagnostics list on a non-200 answer is a failed surface, as a CLI crash would be. */
async function httpCall(server: HttpServer, path: string, body: unknown): Promise<RawResult> {
	if (server.error) return { error: server.error };
	try {
		const response = await fetch(`${server.base}${path}`, { method: "POST", headers: { authorization: `Bearer ${server.token}`, "content-type": "application/json" }, body: JSON.stringify(body) });
		const text = await response.text();
		let json: ProductEnvelope;
		try {
			json = JSON.parse(text);
		} catch {
			return { error: `HTTP ${response.status}: ${text.slice(0, 200)}` };
		}
		if (response.status !== 200 && !Array.isArray(json?.diagnostics)) return { error: `HTTP ${response.status}: ${json?.error ?? text.slice(0, 200)}` };
		return { json };
	} catch (error) {
		return { error: `HTTP request failed: ${(error as Error).message}` };
	}
}

/** The folder's files as the data map a host sends when no Wasm host loop named the references. */
function folderData(dir: string, files: string[]) {
	const data: Data = {};
	for (const path of files) {
		if (path === REPORT || path.startsWith("Rendered/")) continue;
		const bytes = readFileSync(join(dir, path));
		const mime = MIME[extname(path).toLowerCase()];
		data[path] = mime && !mime.startsWith("text/") && mime !== "application/json" ? `data:${mime};base64,${bytes.toString("base64")}` : bytes.toString("utf8");
	}
	return data;
}

// ---------------------------------------------------------------------------------------------
// The run
// ---------------------------------------------------------------------------------------------

function parseArgs(argv: string[]) {
	const args: Record<string, string | null> = { bin: null, out: null, editor: null, dist: null, only: null, "seed-bin": null };
	for (let i = 0; i < argv.length; i++) {
		const flag = argv[i];
		if (["--bin", "--out", "--editor", "--dist", "--only", "--seed-bin"].includes(flag)) args[flag.slice(2)] = argv[++i];
		else throw new Error(`Unknown argument ${flag}`);
	}
	if (!args.bin || !args.out) throw new Error("Usage: contract.mjs --bin <richview> --out <dir> [--editor <bin>] [--dist <dir>] [--only <text>] [--seed-bin <path>]");
	args.bin = resolve(args.bin);
	args.out = resolve(args.out);
	args.editor = resolve(args.editor ?? join(dirname(args.bin), "richview-editor"));
	args.dist = resolve(args.dist ?? join(REPO, "dist/web"));
	const seedBin = seedBinPath(REPO, args["seed-bin"]);
	return {bin: args.bin, out: args.out, editor: args.editor, dist: args.dist, only: args.only, seedBin};
}

function envelopeOf(result: unknown) {
	return okOf(result);
}

function normalizedList(list: Diagnostic[] | undefined, roots: readonly string[], fields?: string[]) {
	return (list ?? []).map((d) => normalizeDiagnostic(d, roots, fields));
}

/**
 * The surfaces a fixture is not compared on, with the reason, or undefined. Connectors run SQL and
 * semantic-layer queries on the native host, so a browser or a server sent the report as text cannot
 * run them: every Wasm and HTTP surface is skipped. The Wasm host cannot supply files, so a fixture
 * whose point is an absent data file is skipped on Wasm check and generate. The reason goes into the
 * report (`notCompared`), so the skip is never silent.
 */
export const NATIVE_ONLY_FIXTURES: Record<string, NativeOnly> = {
	"contract/missing-data": { surfaces: ["wasm.check", "wasm.generate"], reason: "the data file is absent on purpose and the Wasm host cannot supply files" },
};

/** A MetricFlow example is compared on the validate surfaces only: everything that runs the connector is skipped. */
export const RUNTIME_SKIPPED = ["cli.check", "mcp.check", "cli.generate", "wasm.generate"];

export function nativeOnlySurfaces(fixture: Pick<Fixture, "id"> & {files: string[]; connectorKind?: string}) {
	const parts = [];
	if (fixture.files.includes("report.connectors.yaml")) parts.push({ surfaces: ["wasm.validate", "wasm.check", "wasm.generate", "http.validate", "http.check"], reason: "report.connectors.yaml: connectors are a native host duty" });
	if (fixture.connectorKind === "runtime") parts.push({ surfaces: [...RUNTIME_SKIPPED], reason: MF_REASON });
	if (NATIVE_ONLY_FIXTURES[fixture.id]) parts.push(NATIVE_ONLY_FIXTURES[fixture.id]);
	if (!parts.length) return undefined;
	return { surfaces: [...new Set(parts.flatMap((part) => part.surfaces))], reason: parts.map((part) => part.reason).join("; ") };
}

export function compareGroup(name: string, baselineKey: string, surfaces: Record<string, Surface>, fixtureResult: unknown, coreOnly: ReadonlySet<string>) {
	const base = surfaces[baselineKey];
	const diffs: NamedDiff[] = [];
	if (!base || base.error) {
		diffs.push({ group: name, surface: baselineKey, kind: "surface-failed", error: base?.error ?? "no result" });
		return diffs;
	}
	for (const [key, other] of Object.entries(surfaces)) {
		if (key === baselineKey) continue;
		if (!other || other.error) {
			diffs.push({ group: name, surface: key, kind: "surface-failed", error: other?.error ?? "no result" });
			continue;
		}
		const fields = coreOnly.has(key) ? CORE_FIELDS : RECORD_FIELDS;
		for (const diff of diffDiagnostics(base.diagnostics!, other.diagnostics!, fields)) diffs.push({ group: name, surface: key, against: baselineKey, ...diff });
	}
	const oks = Object.fromEntries(Object.entries(surfaces).filter(([k, s]) => s && !s.error && s.envelope !== false).map(([k, s]) => [k, s.ok!]));
	for (const diff of envelopeDiffs(oks, baselineKey)) diffs.push({ group: name, against: baselineKey, ...diff });
	void fixtureResult;
	return diffs;
}

export async function run(argv: string[]) {
	const args = parseArgs(argv);
	const started = Date.now();
	mkdirSync(args.out, { recursive: true });
	const out = realpathSync(args.out);
	const state = join(out, "state");
	rmSync(state, { recursive: true, force: true });
	process.env.XDG_STATE_HOME = state;
	const fixturesDir = join(out, "fixtures");
	rmSync(fixturesDir, { recursive: true, force: true });
	let discovered = discoverFixtures(REPO);
	if (args.only) discovered = discovered.filter((f) => f.id.includes(args.only!));
	if (!discovered.length) throw new Error("No fixtures selected");

	const fixtures = discovered.map((f) => {
		const destination = join(fixturesDir, f.id);
		mkdirSync(dirname(destination), { recursive: true });
		let connectorArgs: string[] = [];
		const connectors = f.kind === "golden" ? classifyConnectors(f.from) : null;
		const connectorKind = connectors?.kind;
		if (connectorKind === "seed") {
			// Seeded copy; the trusted connectors file sits beside it, outside the report folder (FEA-BR-14).
			connectorArgs = prepareSeedExample({ source: f.from, dest: destination, seedBin: args.seedBin }).args;
		} else {
			cpSync(f.from, destination, { recursive: true, filter: (src) => !SKIP_COPY.has(basename(src)) });
		}
		const dir = realpathSync(destination);
		const file = join(dir, REPORT);
		const source = readFileSync(file, "utf8");
		const files = listFiles(dir);
		const roots = [...new Set([dir, destination])];
		return { ...f, dir, connectorArgs, connectorKind, file, source, files, roots };
	});

	const wasm = ((): (ReturnType<typeof loadWasm> & {error?: never}) | {error: string} => {
		try {
			return loadWasm(args.dist);
		} catch (error) {
			return { error: String((error as Error).message) };
		}
	})();

	const reports = fixtures.map((f) => ({ id: f.id, file: f.file, source: f.source }));
	const mcp = await driveMcp(args.bin, reports);
	const editor = await driveEditor(args.editor, reports);
	const http = await startHttp(args.bin, join(args.out, "http-store"));

	const result: Omit<ContractReport, "seconds" | "totals"> & Partial<Pick<ContractReport, "seconds" | "totals">> = {
		step: "contract",
		cards: ["ARC-08", "TYP-53", "FEA-RP-01", "FEA-CH-59", "OPN-57", "ARC-62"],
		bin: args.bin,
		editor: args.editor,
		dist: args.dist,
		surfaces: {
			"cli.validate": "driven", "cli.check": "driven", "cli.generate": "driven (HTML and geometry)", "mcp.validate": "driven", "mcp.check": "driven",
			"editor.lint": "driven", "wasm.validate": wasm.error ? `failed: ${wasm.error}` : "driven", "wasm.check": wasm.error ? `failed: ${wasm.error}` : "driven",
			"http.validate": http.error ? `failed: ${http.error}` : "driven", "http.check": http.error ? `failed: ${http.error}` : "driven",
		},
		mcpTools: mcp.notes.tools!,
		fixtures: [],
	};

	for (const f of fixtures) {
		const entry: FixtureResult = { id: f.id, kind: f.kind, connectorKind: f.connectorKind ?? null, surfaces: {}, html: null, geometry: null, browserStage: null, diffs: [] };
		const surfaceFromJson = (raw: RawResult, extraFields?: (json: ProductEnvelope) => Partial<Surface>): Surface => {
			if (raw.error) return { error: raw.error };
			const json = raw.json!;
			return { ok: envelopeOf(json), status: json.status ?? null, diagnostics: normalizedList(json.diagnostics, f.roots), ...(json.error ? { hostError: normalizeText(json.error, f.roots) } : {}), ...extraFields?.(json) };
		};
		const skipped = nativeOnlySurfaces(f);
		const nativeOnly = new Set(skipped?.surfaces);
		if (skipped) entry.notCompared = skipped;
		if (f.connectorKind === "unsupported") entry.diffs.push({ group: "connectors", surface: "gate", kind: "connector-unsupported", error: "the gate cannot prepare this example's connectors" });
		const checkRuns = !nativeOnly.has("cli.check");
		const cliValidate = runCli(args.bin, ["validate", f.file, "--format", "json"], f.dir);
		entry.surfaces["cli.validate"] = surfaceFromJson(cliValidate);
		if (checkRuns) {
			const cliCheck = runCli(args.bin, ["check", f.file, "--format", "json", ...f.connectorArgs], f.dir);
			entry.surfaces["cli.check"] = surfaceFromJson(cliCheck, (j) => ({ charts: j.charts ?? null }));
		}
		// A failure told only in `error`, outside the diagnostics, is a finding even when every surface fails
		// alike: ARC-08 and TYP-53 carry failures as diagnostics.
		for (const name of ["cli.validate", "cli.check"]) {
			const surface = entry.surfaces[name];
			if (surface && (surface.error || surface.hostError)) entry.diffs.push({ group: name.split(".")[1], surface: name, kind: "host-error", error: surface.error ?? surface.hostError });
		}
		const mcpOf = mcp.results[f.id];
		entry.surfaces["mcp.validate"] = surfaceFromJson(mcpOf.validate);
		entry.surfaces["mcp.check"] = surfaceFromJson(mcpOf.check, (j) => ({ charts: j.charts ?? null }));

		const lsp = editor[f.id];
		entry.surfaces["editor.lint"] = lsp.error
			? { error: lsp.error }
			: { envelope: false, ok: null, diagnostics: normalizedList(lsp.lsp!.map((d) => ({ ...lspToDiagnostic(d), message: normalizeText(d.message, f.roots) })), f.roots) };

		let hostData: Data | null | undefined = null;
		if (wasm.error !== undefined) {
			entry.surfaces["wasm.validate"] = { error: wasm.error };
			entry.surfaces["wasm.check"] = { error: wasm.error };
		} else {
			const v = wasmValidate(wasm, f.dir, f.source, f.files);
			entry.surfaces["wasm.validate"] = v.error ? { error: v.error } : { ok: v.ok ?? null, status: v.status ?? null, diagnostics: normalizedList(v.diagnostics, f.roots) };
			// The host loop loads the files a render references; `check` then runs on that data.
			const c = wasmRender(wasm, f.dir, f.source, f.files, { withRuntime: false });
			if (!c.error) hostData = c.data;
			const checked = c.error ? c : wasmCheck(wasm, f.dir, f.source, f.files, c.data ?? {});
			if (checked.error) entry.surfaces["wasm.check"] = { error: checked.error };
			else entry.surfaces["wasm.check"] = { ok: checked.ok ?? null, status: checked.status ?? null, diagnostics: normalizedList(checked.diagnostics, f.roots), charts: checked.charts ?? null };
		}

		// HTTP sends the folder's name, never its absolute path (TYP-65), and check gets the same data map.
		const httpBody = { source: f.source, reportFolder: { root: basename(f.dir), source: REPORT, files: f.files } };
		// The server names the folder by the basename it was sent; spell it as the folder so normalization makes it `<report>`.
		const named = (raw: RawResult) => {
			for (const d of raw.json?.diagnostics ?? []) if (typeof d.message === "string") d.message = d.message.split(`'${basename(f.dir)}'`).join(`'${f.dir}'`);
			return raw;
		};
		entry.surfaces["http.validate"] = surfaceFromJson(named(await httpCall(http, "/api/validate", httpBody)));
		entry.surfaces["http.check"] = surfaceFromJson(named(await httpCall(http, "/api/check", { ...httpBody, data: hostData ?? folderData(f.dir, f.files) })), (j) => ({ charts: j.charts ?? null }));
		// Connectors run SQL and semantic-layer queries on the native host; a browser or a server sent the
		// report as text cannot, so those fixtures compare the native surfaces only.
		const compared = (group: Record<string, Surface>) => Object.fromEntries(Object.entries(group).filter(([name]) => !nativeOnly.has(name)));

		// Diagnostics, per operation, against the CLI.
		entry.diffs.push(...compareGroup("validate", "cli.validate", compared({ "cli.validate": entry.surfaces["cli.validate"], "mcp.validate": entry.surfaces["mcp.validate"], "editor.lint": entry.surfaces["editor.lint"], "wasm.validate": entry.surfaces["wasm.validate"], "http.validate": entry.surfaces["http.validate"] }), entry, new Set()));
		if (!checkRuns) {
			// MetricFlow needs the server's runtime (FEA-BR-15): only the validate surfaces were compared.
			entry.geometry = { equal: null, note: `not compared: ${skipped!.reason}` };
			entry.html = { equal: null, note: `not compared: ${skipped!.reason}` };
			entry.ok = entry.diffs.length === 0;
			result.fixtures.push(entry);
			continue;
		}
		const checkSurfaces = compared({ "cli.check": entry.surfaces["cli.check"], "mcp.check": entry.surfaces["mcp.check"], "wasm.check": entry.surfaces["wasm.check"], "http.check": entry.surfaces["http.check"] });
		const nativeCheck = entry.surfaces["cli.check"];
		const wasmChecked = entry.surfaces["wasm.check"];
		if (!nativeOnly.has("wasm.check") && !nativeCheck.error && !wasmChecked.error) {
			// FEA-CH-59: the browser glyph stage is compared only where both sides ran it.
			const split = splitBrowserStage(nativeCheck.diagnostics!, wasmChecked.diagnostics!);
			entry.browserStage = { nativeRan: split.ran.native, wasmRan: split.ran.wasm, compared: split.compared };
			nativeCheck.diagnostics = split.native;
			wasmChecked.diagnostics = split.wasm;
		}
		entry.diffs.push(...compareGroup("check", "cli.check", checkSurfaces, entry, new Set()));

		// FEA-RP-01 across targets: the native HTML and the chart geometry against Wasm.
		if (nativeCheck.charts === null || nativeCheck.charts === undefined) {
			// `check` returned no geometry (an error stopped it before rendering), so there is nothing to compare;
			// the failure itself is in the diagnostic and `ok` differences.
			entry.geometry = { equal: null, note: "native check returned no charts" };
		} else if (!nativeCheck.error) {
			// FEA-RP-01: every host that ran check lays the charts out as the CLI does.
			const results: ({surface: string; equal: true} | ({surface: string; equal: false} & Difference))[] = [];
			for (const name of ["wasm.check", "http.check"]) {
				const other = entry.surfaces[name];
				if (nativeOnly.has(name) || !other || other.error) continue;
				const g = firstDifference(nativeCheck.charts, other.charts);
				results.push(g ? { surface: name, equal: false, ...g } : { surface: name, equal: true });
				if (g) entry.diffs.push({ group: "geometry", surface: name, against: "cli.check", kind: "geometry", ...g });
			}
			const first = results.find((r) => !r.equal);
			entry.geometry = results.length ? (first ? { ...first } : { equal: true, charts: (nativeCheck.charts ?? []).length, surfaces: results.map((r) => r.surface) }) : { equal: null, note: "no other host ran check" };
		}
		const generated = runCli(args.bin, ["generate", f.file, "-o", "contract.html", "--format", "json", ...f.connectorArgs], f.dir);
		const htmlPath = join(f.dir, "Rendered", "contract.html");
		entry.surfaces["cli.generate"] = generated.error ? { error: generated.error } : { ok: envelopeOf(generated.json), status: generated.json!.status ?? null, diagnostics: normalizedList(generated.json!.diagnostics, f.roots) };
		if (nativeOnly.has("wasm.generate")) {
			entry.html = { equal: null, note: `not compared on wasm.generate: ${skipped!.reason}` };
		} else if (!generated.error && existsSync(htmlPath) && wasm.error === undefined) {
			const withRuntime = wasmRender(wasm, f.dir, f.source, f.files.filter((p: string) => !p.startsWith("Rendered/")), { withRuntime: true });
			if (withRuntime.error) entry.html = { equal: false, error: `wasm render failed: ${withRuntime.error}` };
			else if (withRuntime.diagnostics) entry.html = { equal: false, error: `wasm render failed with ${withRuntime.diagnostics.map((d) => d.code).join(", ")}` };
			else {
				entry.html = compareHtml(readFileSync(htmlPath, "utf8"), withRuntime.report!.html);
				entry.surfaces["wasm.generate"] = { ok: okOf(withRuntime.report), diagnostics: normalizedList(withRuntime.report!.diagnostics, f.roots) };
				for (const diff of diffDiagnostics(entry.surfaces["cli.generate"].diagnostics!, entry.surfaces["wasm.generate"].diagnostics!)) entry.diffs.push({ group: "generate", surface: "wasm.generate", against: "cli.generate", ...diff });
			}
			if (!entry.html.equal) entry.diffs.push({ group: "html", surface: "wasm.generate", against: "cli.generate", kind: "html", ...entry.html });
		} else if (generated.error || !existsSync(htmlPath)) {
			// A report with errors writes no HTML; there is nothing to compare, which is not a mismatch.
			entry.html = { equal: null, note: "native wrote no HTML" };
		}
		entry.ok = entry.diffs.length === 0;
		result.fixtures.push(entry);
	}
	if (!http.error) http.stop!();

	result.seconds = Math.round((Date.now() - started) / 100) / 10;
	result.totals = {
		fixtures: result.fixtures.length,
		withDiffs: result.fixtures.filter((f) => !f.ok).length,
		diffs: result.fixtures.reduce((n, f) => n + f.diffs.length, 0),
	};
	writeFileSync(join(out, "contract.json"), `${JSON.stringify(result, null, 2)}\n`);
	writeFileSync(join(out, "summary.txt"), summarize(result as ContractReport));
	return result as ContractReport;
}

/** A readable summary: totals, then each kind of difference by surface and field with the fixtures it touches. */
export function summarize(result: ContractReport) {
	const lines = [`Contract step (${result.cards.join(", ")})`, `${result.totals.fixtures} fixtures, ${result.totals.withDiffs} with differences, ${result.totals.diffs} differences, ${result.seconds}s`, ""];
	lines.push("Surfaces:");
	for (const [name, state] of Object.entries(result.surfaces)) lines.push(`  ${name}: ${state}`);
	lines.push("");
	const groups = new Map<string, string[]>();
	for (const f of result.fixtures) {
		for (const d of f.diffs) {
			const fields = d.kind === "changed" ? d.changes.map((c) => c.field).join("+") : d.kind === "ok" ? "ok" : d.kind === "missing" || d.kind === "added" ? (d.diagnostic?.code ?? "") : "";
			const key = `${d.group} ${d.surface} ${d.kind}${fields ? ` ${fields}` : ""}`;
			groups.set(key, [...(groups.get(key) ?? []), f.id]);
		}
	}
	lines.push(groups.size ? "Differences (group surface kind detail: fixtures):" : "No differences.");
	for (const [key, ids] of [...groups.entries()].sort()) lines.push(`  ${key}: ${ids.length} fixture${ids.length === 1 ? "" : "s"} (${[...new Set(ids)].slice(0, 6).join(", ")}${new Set(ids).size > 6 ? ", ..." : ""})`);
	const partial = result.fixtures.filter((f) => f.notCompared && f.connectorKind);
	if (partial.length) {
		lines.push("", "Not compared (connector examples; unrun, neither pass nor difference; the gate lists each UNRUN line in the receipt):");
		for (const f of partial) lines.push(`UNRUN ${f.id}: ${f.notCompared!.surfaces.join(", ")} not compared: ${f.notCompared!.reason}`);
	}
	lines.push("", "Per fixture:");
	for (const f of result.fixtures) lines.push(`  ${f.ok ? (f.notCompared && f.connectorKind ? "part" : "ok  ") : "DIFF"} ${f.id}${f.ok ? "" : ` (${f.diffs.length})`}`);
	return `${lines.join("\n")}\n`;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
	run(process.argv.slice(2)).then(
		(result) => {
			process.stdout.write(summarize(result));
			process.stdout.write(`Wrote ${join(resolve(parseArgs(process.argv.slice(2)).out), "contract.json")}\n`);
			process.exit(result.totals.diffs ? 1 : 0);
		},
		(error) => {
			process.stderr.write(`contract: ${error.stack ?? error}\n`);
			process.exit(2);
		},
	);
}
