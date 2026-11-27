// The golden step of the release run (DEL-60): every golden input renders in every builtin style
// with the release CLI, with no error diagnostic and no warning its fixture does not declare, and
// the getting-started commands run from a clean temporary directory.
//
// Inputs (INT-55, ARC-54, FEA-SF-26, FEA-SF-42, FEA-BR-13): the INT-55 list, every other folder
// under examples/, the typography specimen, the downloadable first report (examples/first-report,
// which the INT-55 list names too) and one report per editor snippet.
//
// A fixture declares the warnings it expects in `golden-warnings.json` next to its report:
// `{"<diagnostic code>": "*" | ["<style>", ...] | {"styles": "*" | [...], "lines": [<line>, ...]}}`.
// With `lines`, the warning is expected at exactly those source lines, so another block starting to
// raise the same code still fails (review 1j note 3). A declared warning that never shows is a failure.
//
// Connector examples (lead, 28 Oct): a DuckDB example with data/seed.sql renders from a seeded copy; a
// MetricFlow example renders only when the environment names the runtime, else it is validated and
// listed as unrun with its reason (lib/connectors.mjs).
//
// Usage: node tools/rv-gate/golden.mts --bin target/release/richview --out <dir> [--only <text>] [--seed-bin <path>]

import { spawn, spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, symlinkSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { cpus, tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { builtinStyles } from "./lib/captures.mts";
import { MF_ENV, SKIP_COPY, classifyConnectors, missingEnv, prepareSeedExample, seedBinPath, unrunLine } from "./lib/connectors.mts";

import type { AddressInfo } from "node:net";

type GoldenInput = {id: string; dir: string; why: string[]; rmd?: string};
type StyleList = "*" | string[];
type Declaration = Record<string, StyleList | {styles: StyleList; lines: number[]}>;
type GoldenDiagnostic = {level: string; code: string; message?: string; line?: number; expected?: string};
type CliReport = {diagnostics?: GoldenDiagnostic[]; error?: string};
type Failure = {kind: string; message?: string; input?: string; style?: string; code?: string; line?: number; command?: string};
type Unrun = {reason: string; validated: boolean};
type GoldenStage = {args: string[]; env: NodeJS.ProcessEnv; skip?: boolean; failures?: Failure[]; unrun?: Unrun};
type GoldenArgs = {cwd: string; bin: string; out: string; only?: string; seedBin?: string};

/** INT-55's golden inputs; every other examples/ folder joins them (INT-55, part 04d). */
export const GOLDEN_LIST = [
	"examples/openai-vs-anthropic",
	"examples/first-report",
	"examples/hello-world",
	"examples/sales-report",
	"examples/authoring",
	"examples/agent-output",
	"examples/histogram",
	"examples/boxplot",
	"examples/interval",
	// The home page's example: the hero's report, which the live example edits.
	"assets/landing/anchor",
	"benchmarks/chart-regression",
];
/** FEA-BR-13: the typography specimen, a golden input only: not on the site, so it lives with the gate. */
export const SPECIMEN = "tools/rv-gate/fixtures/typography";
/** FEA-SF-26: where the editor's snippets live. */
export const SNIPPETS_FILE = "packages/vscode/snippets/rmd.json";
/** FEA-SF-42: the page whose commands the step runs. */
export const GETTING_STARTED = "docs/public/first-report.md";
export const DECLARATION_FILE = "golden-warnings.json";

const hasReport = (cwd: string, dir: string) => existsSync(join(cwd, dir, "report.rmd"));

/** The golden folders, deduplicated and sorted, each with why it is in. Throws on a missing INT-55 input. */
export function discoverInputs(cwd: string) {
	const found = new Map<string, string[]>();
	const add = (dir: string, why: string) => found.set(dir, [...(found.get(dir) ?? []), why]);
	for (const dir of GOLDEN_LIST) {
		if (!hasReport(cwd, dir)) throw new Error(`Golden input ${dir} has no report.rmd (INT-55)`);
		add(dir, "INT-55");
	}
	const examples = join(cwd, "examples");
	for (const name of readdirSync(examples).sort()) {
		const dir = `examples/${name}`;
		if (statSync(join(examples, name)).isDirectory() && hasReport(cwd, dir)) add(dir, "examples");
	}
	if (!hasReport(cwd, SPECIMEN)) throw new Error(`The typography specimen ${SPECIMEN} is missing (FEA-BR-13)`);
	add(SPECIMEN, "FEA-BR-13");
	add("examples/first-report", "FEA-SF-42");
	return [...found.entries()].sort(([a], [b]) => a.localeCompare(b)).map(([dir, why]) => ({ id: dir, dir, why: [...new Set(why)] }));
}

/**
 * Expand a VS Code snippet body: `${1:text}` and `${1|a,b|}` take their default, `$1` and `$0` vanish,
 * and the escapes `\$`, `\}` and `\\` become the character they stand for. Escapes are set aside
 * first, so an escaped brace never closes a placeholder.
 */
export function expandSnippet(body: string | string[]) {
	const text = Array.isArray(body) ? body.join("\n") : body;
	const literals: string[] = [];
	let out = text.replace(/\\([$}\\])/g, (_, ch: string) => `\u0000${literals.push(ch) - 1}\u0000`);
	for (let pass = 0; pass < 5; pass++) {
		out = out.replace(/\$\{\d+\|([^}|]*)\|\}/g, (_, choices: string) => choices.split(",")[0]).replace(/\$\{\d+:([^${}]*)\}/g, "$1");
	}
	return out.replace(/\$\{\d+\}|\$\d+/g, "").replace(/\u0000(\d+)\u0000/g, (_, i) => literals[Number(i)]);
}

/** One report per snippet: a snippet that is a whole frontmatter stands alone, any other sits under one. */
export function snippetReports(json: Record<string, {body: string | string[]; prefix: string}>) {
	return Object.entries(json).map(([name, snippet]) => {
		const body = expandSnippet(snippet.body);
		const rmd = body.startsWith("---") ? `${body}\n\nThe snippet above is the whole frontmatter.\n` : `---\ntitle: Snippet ${snippet.prefix}\n---\n\n${body}\n`;
		return { id: `snippet/${snippet.prefix}`, name, rmd };
	});
}

/** Whether `styles` is `"*"` or a non-empty list of style names. */
const isStyles = (styles: unknown): styles is StyleList => styles === "*" || (Array.isArray(styles) && styles.length > 0 && styles.every((s) => typeof s === "string"));

/**
 * Parse `golden-warnings.json`: each code keeps `"*"` or its list of style names as written, or becomes
 * `{styles, lines}` with its lines sorted. Throws on anything else, and on an empty list of lines.
 */
export function parseDeclaration(text: string, where = DECLARATION_FILE) {
	const json: unknown = JSON.parse(text);
	if (json === null || typeof json !== "object" || Array.isArray(json)) throw new Error(`${where}: expected an object of diagnostic code to "*", a list of styles or {styles, lines}`);
	const declaration: Declaration = {};
	for (const [code, candidate] of Object.entries(json as Record<string, unknown>)) {
		const value = candidate as {styles?: unknown; lines?: number[]};
		if (isStyles(candidate)) declaration[code] = candidate;
		else if (value && typeof value === "object" && isStyles(value.styles) && Array.isArray(value.lines) && value.lines.length > 0 && value.lines.every((l) => Number.isInteger(l) && l > 0) && Object.keys(value).every((k) => k === "styles" || k === "lines"))
			declaration[code] = { styles: value.styles, lines: [...value.lines].sort((a, b) => a - b) };
		else throw new Error(`${where}: ${code} must be "*", a non-empty list of style names, or {"styles": ..., "lines": [...]}`);
	}
	return declaration;
}

/**
 * Judge one input's results. `results` is `{style: [diagnostic]}`; returns the failures:
 * errors, warnings that are not declared for that style, and declarations that never showed.
 */
export function judge(results: Record<string, GoldenDiagnostic[]>, declaration: Declaration = {}, styles = Object.keys(results)) {
	const failures: Failure[] = [];
	// A bare `"*"` or style list (the older shape, and the tests') means `{styles}`.
	const entry = (code: string): {styles: StyleList; lines?: number[]} | undefined => {
		const value = declaration[code];
		return value === undefined ? undefined : isStyles(value) ? { styles: value } : value;
	};
	const declares = (code: string, style: string, line?: number) => {
		const e = entry(code);
		return !!e && (e.styles === "*" || e.styles.includes(style)) && (!e.lines || e.lines.includes(line!));
	};
	for (const [style, diagnostics] of Object.entries(results)) {
		for (const d of diagnostics) {
			if (d.level === "error") failures.push({ style, code: d.code, line: d.line, message: d.message, kind: "error" });
			else if (d.level === "warning" && !declares(d.code, style, d.line)) failures.push({ style, code: d.code, line: d.line, message: d.message, kind: "undeclared warning" });
		}
	}
	for (const code of Object.keys(declaration)) {
		const e = entry(code)!;
		for (const style of e.styles === "*" ? styles : e.styles) {
			if (!results[style]) failures.push({ style, code, message: `declared for style ${style}, which is not a builtin`, kind: "stale declaration" });
			else
				for (const line of e.lines ?? [undefined]) {
					if (!results[style].some((d) => d.level === "warning" && d.code === code && (line === undefined || d.line === line)))
						failures.push({ style, code, line, message: line === undefined ? "declared warning did not occur" : `declared warning did not occur at line ${line}`, kind: "stale declaration" });
				}
		}
	}
	return failures;
}

/** The builtin style names the CLI itself lists in its `style.unknown` error's `expected` field. */
export function parseKnownStyles(expected: unknown) {
	const names = String(expected ?? "").split(",").map((s) => s.trim()).filter(Boolean);
	if (!names.length) throw new Error(`Cannot read the builtin styles from: ${expected}`);
	return names;
}

export function productStyles(bin: string, cwd: string) {
	const probe = mkdtempSync(join(tmpdir(), "rv-golden-styles-"));
	try {
		writeFileSync(join(probe, "report.rmd"), "---\ntitle: Styles\n---\n\nText.\n");
		const run = spawnSync(bin, ["check", join(probe, "report.rmd"), "--style", "no-such-style", "--format", "json"], { cwd, encoding: "utf8" });
		const diagnostic = (JSON.parse(run.stdout) as {diagnostics: GoldenDiagnostic[]}).diagnostics.find(d => d.code === "style.unknown");
		if (!diagnostic) throw new Error("The CLI did not report style.unknown for an unknown style");
		return parseKnownStyles(diagnostic.expected).sort();
	} finally {
		rmSync(probe, { recursive: true, force: true });
	}
}

/** Read the tutorial's commands, generated file and downloadable example from its guide. */
export function extractGettingStarted(markdown: string) {
	const blocks = [...markdown.matchAll(/```sh\n([\s\S]*?)```/g)];
	if (!blocks.length) throw new Error("No ```sh block in the getting-started page");
	const commands = blocks.flatMap((block) => block[1].split("\n").map((l: string) => l.trim()).filter((l: string) => l && !l.startsWith("#")));
	const init = commands.find((c) => /^richview init\s/.test(c));
	if (!init) throw new Error("The getting-started commands have no `richview init`");
	const dir = init.split(/\s+/)[2];
	const rendered = /`([^`]+\/Rendered\/report\.html)`/.exec(markdown)?.[1];
	const exampleDir = /```text\n([^\n/]+)\/\n\s+report\.rmd/.exec(markdown)?.[1];
	if (!rendered || !exampleDir) throw new Error("The page no longer names the generated file or the example folder");
	return { commands, dir, rendered, sample: `${exampleDir}/report.rmd`, rest: commands.filter((c) => c !== init) };
}

/** The commands for the first report: the page's own, with the init folder's report swapped for the sample. */
export function sampleCommands({ commands, dir, sample, init = `richview init ${dir}` }: {commands: string[]; dir: string; sample: string; init?: string}) {
	return commands.filter((c: string) => c !== init).map((c: string) => c.replaceAll(`${dir}/report.rmd`, sample));
}

const freePort = () =>
	new Promise<number>((ok, fail) => {
		const server = createServer().unref();
		server.on("error", fail);
		server.listen(0, "127.0.0.1", () => {
			const { port } = server.address() as AddressInfo;
			server.close(() => ok(port));
		});
	});

async function serveAnswers(command: string, cwd: string, env: { PATH: string; HOME: string; }) {
	const port = await freePort();
	const args = command.split(/\s+/).slice(1);
	const existingPort = args.indexOf("--port");
	if (existingPort !== -1) args.splice(existingPort, 2);
	args.push("--port", String(port));
	const child = spawn("richview", args, { cwd, env, stdio: "ignore" });
	try {
		for (let attempt = 0; attempt < 60; attempt++) {
			await new Promise((r) => setTimeout(r, 250));
			if (child.exitCode !== null) return `exited with ${child.exitCode}`;
			try {
				const res = await fetch(`http://127.0.0.1:${port}/`);
				return res.ok ? undefined : `answered ${res.status}`;
			} catch {
				// not listening yet
			}
		}
		return "never answered";
	} finally {
		child.kill();
	}
}

/** Run the getting-started commands from a clean temporary directory with the CLI on PATH. */
export async function runGettingStarted({ cwd, bin }: Pick<GoldenArgs, "cwd" | "bin">) {
	const spec = extractGettingStarted(readFileSync(join(cwd, GETTING_STARTED), "utf8"));
	const failures: Failure[] = [];
	const root = mkdtempSync(join(tmpdir(), "rv-golden-docs-"));
	try {
		const binDir = join(root, "bin");
		mkdirSync(binDir);
		symlinkSync(resolve(cwd, bin), join(binDir, "richview"));
		const env = { PATH: `${binDir}:/usr/bin:/bin`, HOME: root };
		const work = join(root, "work");
		mkdirSync(work);
		cpSync(join(cwd, "examples/first-report"), join(work, dirname(spec.sample)), { recursive: true });
		const steps = [...spec.commands.map((c) => ({ c, label: "tutorial", file: spec.rendered })), ...sampleCommands(spec).map((c) => ({ c, label: "first report", file: join(dirname(spec.sample), "Rendered", "report.html") }))];
		for (const { c, label, file } of steps) {
			if (/^richview serve\s/.test(c)) {
				const problem = await serveAnswers(c, work, env);
				if (problem) failures.push({ kind: "docs command", command: c, message: `${label}: ${problem}` });
				continue;
			}
			const run = spawnSync("sh", ["-c", c], { cwd: work, env, encoding: "utf8" });
			let message;
			if (run.status !== 0) message = `${label}: exit ${run.status}: ${(run.stderr || run.stdout).trim().slice(0, 300)}`;
			else if (c.includes("--format json")) {
				const errors = ((JSON.parse(run.stdout) as CliReport).diagnostics ?? []).filter((d: { level: string; }) => d.level === "error");
				if (errors.length) message = `${label}: ${errors.map((d) => `${d.code}: ${d.message}`).join("; ")}`;
				else if (/^richview generate\s/.test(c) && !existsSync(join(work, file))) message = `${label}: ${file} was not written`;
			}
			if (message) failures.push({ kind: "docs command", command: c, message });
		}
	} finally {
		rmSync(root, { recursive: true, force: true });
	}
	return failures;
}

/**
 * A render that failed without saying why in a diagnostic: a host error, a failing exit, or no HTML.
 * An error diagnostic already fails through `judge`, so these only count when there is none (DEL-60).
 */
export function renderProblems({ style, status, json, wroteHtml }: {style: string; status: number | null; json: CliReport; wroteHtml: boolean}) {
	const problems = [];
	const errors = (json.diagnostics ?? []).some((d: { level: string; }) => d.level === "error");
	if (typeof json.error === "string" && json.error) problems.push({ style, kind: "host error", code: "cli", message: json.error.slice(0, 300) });
	else if (status !== 0 && !errors) problems.push({ style, kind: "no result", code: "cli", message: `exit ${status} with no error diagnostic` });
	if (!wroteHtml && !errors && !problems.length) problems.push({ style, kind: "no HTML", code: "cli", message: "generate wrote no HTML and named no error" });
	return problems;
}

/** `richview validate` on a copy: the diagnostics that are errors or warnings (an example must validate clean). */
function validateOnly(bin: string, folder: string) {
	const run = spawnSync(bin, ["validate", join(folder, "report.rmd"), "--format", "json"], { cwd: folder, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 });
	let json: CliReport;
	try {
		json = JSON.parse(run.stdout);
	} catch {
		return [{ kind: "no result", code: "cli", message: `validate exit ${run.status}: ${(run.stderr || run.stdout).trim().slice(0, 300)}` }];
	}
	const found = (json.diagnostics ?? []).filter((d: { level: string; }) => d.level === "error" || d.level === "warning");
	const failures: Failure[] = found.map((d) => ({ kind: `validate ${d.level}`, code: d.code, line: d.line, message: d.message }));
	if (typeof json.error === "string" && json.error) failures.push({ kind: "host error", code: "cli", message: json.error.slice(0, 300) });
	return failures;
}

/**
 * Put an input's files into `folder` and say how to render it. A connector example goes through
 * lib/connectors.mjs: `seed` renders from a seeded copy, `runtime` renders only when the gate's
 * environment names the MetricFlow runtime and is otherwise validated only and reported unrun,
 * `unsupported` is a failure. Returns `{ args, env, unrun?, failures? }`.
 */
export function stageInput({ cwd, bin, folder, input, seedBin, environment = process.env }: Pick<GoldenArgs, "cwd" | "bin" | "seedBin"> & {folder: string; input: GoldenInput; environment?: NodeJS.ProcessEnv}): GoldenStage {
	if (input.rmd !== undefined) {
		mkdirSync(folder, { recursive: true });
		writeFileSync(join(folder, "report.rmd"), input.rmd);
		// Snippets leave asset selection to their author; the golden report supplies the real
		// every-block media at those defaults so rendering exercises media instead of missing files.
		for (const [asset, fixture] of [["photo.png", "fixture-image.png"], ["interview.mp3", "tone.mp3"], ["demo.mp4", "clip.mp4"]]) {
			if (!input.rmd.includes(`media/${asset}`)) continue;
			mkdirSync(join(folder, "media"), { recursive: true });
			cpSync(join(cwd, "examples/every-block/media", fixture), join(folder, "media", asset));
		}
		return { args: [], env: {} };
	}
	const source = join(cwd, input.dir);
	const connectors = classifyConnectors(source);
	const copy = () => cpSync(source, folder, { recursive: true, filter: (src) => !SKIP_COPY.has(basename(src)) });
	if (!connectors) {
		copy();
		return { args: [], env: {} };
	}
	if (connectors.kind === "seed") {
		try {
			const prepared = prepareSeedExample({ source, dest: folder, seedBin });
			return { args: prepared.args, env: prepared.env };
		} catch (error) {
			return { args: [], env: {}, skip: true, failures: [{ kind: "connector seed", code: "gate", message: (error as Error).message }] };
		}
	}
	copy();
	if (connectors.kind === "unsupported") return { args: [], env: {}, skip: true, failures: [{ kind: "connector unsupported", code: "gate", message: (connectors as {reason: string}).reason }] };
	const missing = missingEnv(MF_ENV, environment);
	if (!missing.length) return { args: [], env: {} };
	return { args: [], env: {}, skip: true, failures: validateOnly(bin, folder), unrun: { reason: `${(connectors as {reason: string}).reason}; unset: ${missing.join(", ")}`, validated: true } };
}

/** Copy a golden folder (without its frozen research) and render it in each style. */
async function renderInput({ cwd, bin, out, input, styles, seedBin }: GoldenArgs & {input: GoldenInput; styles: string[]}) {
	const folder = join(out, "inputs", input.id.replaceAll("/", "__"));
	const staged = stageInput({ cwd, bin, folder, input, seedBin });
	if (staged.skip) {
		const unrun = staged.unrun && { input: input.id, ...staged.unrun };
		return { id: input.id, why: input.why, failures: staged.failures!, styles: 0, unrun };
	}
	const declarationPath = join(folder, DECLARATION_FILE);
	const declaration = existsSync(declarationPath) ? parseDeclaration(readFileSync(declarationPath, "utf8"), `${input.id}/${DECLARATION_FILE}`) : {};
	const results: Record<string, GoldenDiagnostic[]> = {};
	const crashes = [];
	for (const style of styles) {
		const run = await new Promise<{status: number | null; stdout: string; stderr: string}>((done) => {
			let stdout = "";
			let stderr = "";
			const child = spawn(bin, ["generate", join(folder, "report.rmd"), "--style", style, "-o", `${style}.html`, "--format", "json", ...staged.args], { cwd: folder, env: { ...process.env, ...staged.env } });
			child.stdout.on("data", (d) => (stdout += d));
			child.stderr.on("data", (d) => (stderr += d));
			child.on("close", (status) => done({ status, stdout, stderr }));
		});
		let json: CliReport;
		try {
			json = JSON.parse(run.stdout);
		} catch {
			results[style] = [];
			crashes.push({ style, kind: "no result", code: "cli", message: `exit ${run.status}: ${(run.stderr || run.stdout).trim().slice(0, 300)}` });
			continue;
		}
		results[style] = json.diagnostics ?? [];
		crashes.push(...renderProblems({ style, status: run.status, json, wroteHtml: existsSync(join(folder, "Rendered", `${style}.html`)) }));
	}
	return { id: input.id, why: input.why, failures: [...crashes, ...judge(results, declaration, styles)], styles: styles.length };
}

async function pool<Item, Result>(items: Item[], size: number, work: (item: Item) => Promise<Result>) {
	const results: Result[] = [];
	let next = 0;
	await Promise.all(
		Array.from({ length: Math.min(size, items.length) }, async () => {
			while (next < items.length) {
				const index = next++;
				results[index] = await work(items[index]);
			}
		}),
	);
	return results;
}

export async function runGolden({ cwd, bin, out, only, seedBin }: GoldenArgs) {
	const started = Date.now();
	const binary = resolve(cwd, bin);
	const seed = seedBinPath(cwd, seedBin ?? null);
	if (!existsSync(binary)) throw new Error(`The release CLI ${binary} does not exist; the golden step needs the build step first`);
	const styles = productStyles(binary, cwd);
	const assetStyles = builtinStyles(cwd);
	const failures: Failure[] = [];
	if (JSON.stringify(styles) !== JSON.stringify(assetStyles)) failures.push({ input: "styles", kind: "style list", message: `CLI knows ${styles.join(", ")}; core's style files are ${assetStyles.join(", ")}` });
	mkdirSync(out, { recursive: true });
	const snippets = snippetReports(JSON.parse(readFileSync(join(cwd, SNIPPETS_FILE), "utf8"))).map((s) => ({ ...s, dir: SNIPPETS_FILE, why: ["FEA-SF-26"] }));
	const inputs = [...discoverInputs(cwd), ...snippets].filter((i) => !only || i.id.includes(only));
	const rendered = await pool(inputs, Math.max(2, cpus().length - 1), (input) => renderInput({ cwd, bin: binary, out, input, styles, seedBin: seed }));
	for (const r of rendered) for (const f of r.failures) failures.push({ input: r.id, ...f });
	if (!only) failures.push(...(await runGettingStarted({ cwd, bin })).map((f) => ({ input: "getting-started", ...f })));
	// An unrun input is neither a pass nor a failure: it is listed with its reason, here and in the receipt.
	const unrun = rendered.filter((r) => r.unrun).map((r) => r.unrun!);
	const summary = { inputs: inputs.length, styles, renders: rendered.reduce((n, r) => n + r.styles, 0), seconds: (Date.now() - started) / 1000, failures, unrun };
	writeFileSync(join(out, "golden.json"), `${JSON.stringify(summary, null, 2)}\n`);
	return summary;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
	const arg = (name: string) => {
		const at = process.argv.indexOf(name);
		return at < 0 ? undefined : process.argv[at + 1];
	};
	const bin = arg("--bin");
	const out = arg("--out");
	if (!bin || !out) {
		console.error("usage: golden.mjs --bin <richview> --out <dir> [--only <text>] [--cwd <repo>] [--seed-bin <seed_duckdb>]");
		process.exit(2);
	}
	const state = join(resolve(out), "state");
	rmSync(state, { recursive: true, force: true });
	process.env.XDG_STATE_HOME = state;
	const summary = await runGolden({ cwd: resolve(arg("--cwd") ?? process.cwd()), bin, out: resolve(out), only: arg("--only"), seedBin: arg("--seed-bin") });
	for (const f of summary.failures) console.log(`FAIL ${f.input}${f.style ? ` [${f.style}]` : ""} ${f.kind}: ${f.code ?? f.command ?? ""}${f.line ? ` (line ${f.line})` : ""} ${f.message}`);
	for (const u of summary.unrun) console.log(`UNRUN ${u.input}: ${u.reason}${u.validated ? " (validated only)" : ""}`);
	console.log(`golden: ${summary.inputs} inputs x ${summary.styles.length} styles = ${summary.renders} renders in ${summary.seconds.toFixed(1)} s; ${summary.failures.length} failures${summary.unrun.length ? `; ${unrunLine(summary.unrun)}` : ""}`);
	process.exit(summary.failures.length ? 1 : 0);
}
