// The step registry: the single list of gate steps, their tier and the lockdown cards they
// implement (DEL-08, DEL-23, DEL-24). Tiers are cumulative: fast ⊂ pre-pr ⊂ integration ⊂ full.
// A step that is not built yet carries `unrun` with its reason and blocks from the day it exists.
//
// Removing a step, or an entry point from a step, is a gate change that waits for Daniel
// (DEL-36); `weakening.mjs` compares this registry between base and head.

import { MF_ENV, MF_REASON, MF_TEST } from "./connectors.mts";
import { FIGURE_THEMES } from "./figure-cases.mts";
import { WINDOWS_STANDING } from "./windows.mts";
import { typeScriptPaths } from "./source-paths.mts";

export const TIERS = ["fast", "pre-pr", "integration", "full"] as const;
export type Tier = typeof TIERS[number];

export interface GateCommand {
	cmd: string;
	args: string[];
	entry?: string;
	env?: NodeJS.ProcessEnv;
	id?: string;
	installReleaseMode?: boolean;
	shard?: { kind: "interaction" | "docs" | "no-js"; count: number; out: string };
	workers?: { arg?: string; env?: string; count: number };
	whenEnvMissing?: { vars: string[]; args: string[]; unrun: { name: string; reason: string }[] };
}

export interface GateStep {
	id: string;
	tier: Tier;
	title: string;
	cards: string[];
	run?: GateCommand[];
	entries?: string[];
	fn?: string;
	mode?: "dev" | "prod";
	needs?: "build";
	after?: string[];
	parallel?: number;
	alone?: boolean;
	cohort?: "rust-debug";
	reportsIgnored?: boolean;
	retryArtifacts?: string[];
	unrun?: string;
	unrunIn?: Partial<Record<Tier, string>>;
}

type HeadStep = Pick<GateStep, "id" | "run" | "unrun">;
type RegisteredScript = GateCommand & { entry: string };

// DEL-78 (lead ruling, 1 Nov 2026): the fresh-agent harness runs at each candidate and weekly on the pre-prod head, never in
// every full, so a full reports it unrun with FRESH_AGENT_SCHEDULE. Daniel ruled on 1 Nov that Codex is no longer used; the
// candidate and weekly runs take Claude only and report Codex unrun with CODEX_UNRUN, until he rules on it at the candidate.
export const FRESH_AGENT_SCHEDULE = "DEL-78: candidate and weekly pre-prod only";
export const CODEX_UNRUN = "Daniel, 1 Nov 2026: Codex is no longer used and its quota is 0";

/** Every crate that ships as Wasm: the site, the report runtime and the publish server. */
export const WASM_CRATES = ["richview-site", "richview-runtime", "richview-publish-server"];

export function tierIncludes(tier: Tier, stepTier: Tier) {
	return TIERS.indexOf(stepTier) <= TIERS.indexOf(tier);
}

const cargo = (...args: string[]): GateCommand => ({ cmd: "cargo", args });
const node = (...args: string[]): GateCommand => ({ cmd: "node", args });
const suites = (names: string[], extra: Partial<GateCommand> = {}): GateCommand[] =>
	names.map((name) => ({ ...node(`scripts/verify-${name}.ts`), entry: `scripts/verify-${name}.ts`, ...extra }));

/** Chromium behaviour suites (DEL-15). Each starts its own loopback server and browser. */
export const CHROMIUM_SUITES = [
	"browser",
	"cloudflare",
	"flagship",
	"figure-expand",
	"chart-keyboard",
	"facet-text",
	"chart-readouts",
	"readout-placement",
	"wedge-readouts",
	"chord-readouts",
	"funnel-readouts",
	"point-modes",
	"chart-interaction",
	"chart-feedback",
	"report-runtime",
	"controls",
	"tabs",
	"slides",
	"docs",
];

/** Data-integrity suites (DEL-16). */
export const DATA_SUITES = [
	"numeric-safety",
	"source-tables",
	"temporal-fidelity",
	"temporal-decimals",
	"csv-decimals",
];

/** Suites the full tier re-runs in Firefox and WebKit (DEL-33, DEL-74). */
export const ENGINE_LOOP_SUITES = [
	"figure-expand",
	"chart-keyboard",
	"chart-readouts",
	"wedge-readouts",
	"chord-readouts",
	"funnel-readouts",
	"point-modes",
	"chart-feedback",
	"controls",
	"numeric-safety",
	"source-tables",
	"temporal-fidelity",
	"temporal-decimals",
	"csv-decimals",
	"browser",
];

/**
 * Every step. `run` is a list of commands (run in order, or concurrently when `parallel` is set),
 * or `fn` names a built-in check in `checks/`. `needs: "build"` means the step tests the release
 * artifacts the build step produced (DEL-13).
 */
export const STEPS: GateStep[] = [
	// Fast tier: after each series of agent steps and before every push (DEL-24).
	{ id: "pins", tier: "fast", cards: ["DEL-49", "DEL-22"], title: "Host and toolchain pins", fn: "pins" },
	{ id: "entrypoints", tier: "fast", cards: ["DEL-23"], title: "Every test entry point is in a tier", fn: "entrypoints" },
	{ id: "fmt", tier: "fast", cards: ["DEL-09"], title: "Formatting", run: [cargo("fmt", "--all", "--check")] },
	{
		id: "clippy",
		tier: "fast",
		cards: ["DEL-10"],
		title: "Native lint",
		run: [cargo("clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings")],
	},
	{
		id: "clippy-wasm",
		tier: "fast",
		cards: ["DEL-11"],
		title: "Wasm lint",
		run: [cargo("clippy", ...WASM_CRATES.flatMap((name) => ["-p", name]), "--target", "wasm32-unknown-unknown", "--locked", "--", "-D", "warnings")],
	},
	{ id: "arch", tier: "fast", cards: ["DEL-54", "ARC-02", "ARC-03"], title: "Architecture boundaries", fn: "arch" },
	{ id: "secrets", tier: "fast", cards: ["DEL-55"], title: "Secrets and private data", fn: "secrets" },
	{ id: "public-tree", tier: "fast", cards: ["DEL-55"], title: "No internal file and no issue id in a tracked file", fn: "publicTree" },
	{
		id: "rust-tests",
		tier: "fast",
		cards: ["DEL-12", "DEL-24"],
		title: "Rust tests (fast tier)",
		run: [cargo("test", "--workspace", "--locked", "--no-fail-fast")],
		reportsIgnored: true,
		entries: ["crates/*/tests/*.rs", "richview-chart/tests/*.rs", "crates/*/src/**/*.rs", "richview-chart/src/**/*.rs"],
	},
	{
		id: "tool-tests",
		tier: "fast",
		cards: ["DEL-23", "ARC-65", "FEA-SF-65"],
		title: "Lockdown app, gate, public API, dependency rules, agent-use harness and release command tests",
		run: [node("--test", "tools/lockdown-app/test/*.test.m?s", "tools/rv-gate/test/*.test.m?s", "tools/api-contracts/test/*.test.m?s", "tools/dependency-rules/test/*.test.m?s", "tools/agent-use/test/*.test.m?s", "tools/rv-release/test/*.test.m?s", "packages/vscode/test/*.test.cts")],
		entries: ["tools/lockdown-app/test/*.test.m?s", "tools/rv-gate/test/*.test.m?s", "tools/api-contracts/test/*.test.m?s", "tools/dependency-rules/test/*.test.m?s", "tools/agent-use/test/*.test.m?s", "tools/rv-release/test/*.test.m?s", "packages/vscode/test/*.test.cts"],
	},
	{
		id: "publish-tests",
		tier: "fast",
		cards: ["ARC-57", "INT-52"],
		title: "Publish Worker adapter tests against the Wasm server",
		run: [
			node("-e", "const typed = './tools/rv-gate/lib/npm.mts'; import('node:fs').then(({existsSync}) => import(existsSync(typed) ? typed : './tools/rv-gate/lib/npm.mjs')).then((m) => m.npmCi(process.cwd(), { prefix: 'cloudflare/publish', env: process.env }))"),
			{ cmd: "npm", args: ["run", "build:wasm", "--prefix", "cloudflare/publish"] },
			// Which sources the glue was built from, for the Publish journey's freshness check.
			node("-e", "import('./tools/rv-gate/journeys-publish.mts').then((m) => m.stampPkg(process.cwd()))"),
			node("--test", "cloudflare/publish/test/*.test.ts"),
			// Parity with PR #70's Worker on a real News release, which the suite above skips without one.
			node("-e", "const fs=require('fs'),p=process.argv[1];fs.mkdirSync(p,{recursive:true});fs.writeFileSync(p+'/richview_runtime.js','/* rt */');fs.writeFileSync(p+'/richview_runtime_bg.wasm',Buffer.from([0,97,115,109,1,0,0,0]))", "{run}/parity/runtime"),
			cargo("run", "--locked", "-p", "richview-publish", "--", "build", "--site", "publications/news/site.yaml", "--output", "{run}/parity/out", "--runtime", "{run}/parity/runtime", "--source-commit", "abcdefabcdefabcdefabcdefabcdefabcdefabcd"),
			{ ...node("--test", "cloudflare/publish/test/parity.test.ts"), env: { PARITY_RELEASE_DIR: "{run}/parity/out", PARITY_REQUIRED: "1" } },
		],
		entries: ["cloudflare/publish/test/*.test.ts"],
	},

	// Integration tier: `rv-gate dev`, every merge into dev (DEL-08).
	{ id: "pr-fields", tier: "integration", cards: ["DEL-32", "DEL-44", "DEL-70"], title: "Issue, decision class and cause", fn: "prFields", mode: "dev" },
	{ id: "weakening", tier: "integration", cards: ["DEL-36"], title: "No step removed, no threshold raised", fn: "weakening", mode: "dev" },
	{ id: "retired-tests", tier: "integration", cards: ["DEL-36", "DEL-64"], title: "Every deleted test has a reason", fn: "retiredTests", mode: "dev" },
	{
		id: "build",
		tier: "integration",
		cards: ["DEL-13"],
		title: "Release build and fixtures",
		run: [
			{ ...cargo("run", "--release", "--locked", "-p", "richview-build"), installReleaseMode: true },
			cargo("run", "--release", "--locked", "-p", "richview-bench", "--", "--write-fixtures", "--fixtures-only"),
			// The dev-only seeding example: golden and contract seed the SQL example's database with it (lead, 28 Oct).
			cargo("build", "--release", "--locked", "-p", "richview-app", "--example", "seed_duckdb"),
		],
	},
	// Runtime-dependent and slow tests run after the real staged build (ARC-07, DEL-25).
	{
		id: "rust-slow",
		tier: "pre-pr",
		needs: "build",
		cards: ["DEL-12", "DEL-25"],
		title: "Rust slow tests",
		// The MetricFlow end-to-end test needs the server's runtime (FEA-BR-15): without it the step skips
		// the test and records it as unrun with its reason instead of letting it panic.
		run: [
			{
				...cargo("test", "--workspace", "--locked", "--features", "richview-app/embedded-runtime,richview-editor/embedded-runtime", "--no-fail-fast", "--", "--ignored"),
				// `--exact` makes the skip match that one test's full name, not every name containing it.
				whenEnvMissing: { vars: MF_ENV, args: ["--skip", MF_TEST, "--exact"], unrun: [{ name: MF_TEST, reason: MF_REASON }] },
			},
		],
		reportsIgnored: true,
	},

	{
		id: "host",
		tier: "integration",
		needs: "build",
		cards: ["DEL-14"],
		title: "Host checks: editor, CLI, MCP, server",
		run: [
			{ ...node("scripts/verify-editor.cts"), entry: "scripts/verify-editor.cts" },
			{ ...node("scripts/verify-native.ts"), entry: "scripts/verify-native.ts" },
		],
	},
	{
		id: "chromium",
		tier: "integration",
		needs: "build",
		cards: ["DEL-15"],
		title: "Chromium behaviour suites and docs",
		parallel: 4,
		// The interaction suite also writes per-type evidence into the run folder (FEA-CH-75, FEA-CH-39).
		run: suites(CHROMIUM_SUITES).flatMap<GateCommand>((entry) => {
			// The readout placement run is the longest one by far (43 minutes whole), and its styles are independent: one command
			// each, side by side, and together exactly the whole run (a gate test keeps `--styles` equal to the script's list).
			if (entry.entry === "scripts/verify-readout-placement.ts") return FIGURE_THEMES.map((style) => ({ ...entry, args: [...entry.args, `--styles=${style}`] }));
			if (entry.entry === "scripts/verify-chart-interaction.ts") return [{ ...entry, args: [...entry.args, "--out", "{run}/interaction"], shard: { kind: "interaction", count: 2, out: "{run}/interaction" } }];
			if (entry.entry === "scripts/verify-docs.ts") return [{ ...entry, args: [...entry.args, "--out", "{run}/docs"], shard: { kind: "docs", count: 5, out: "{run}/docs" } }];
			return [entry];
		}),
		// verify-cloudflare.mjs runs the signup check against a local Wrangler preview.
		entries: ["scripts/verify-interest.ts"],
	},
	{
		id: "chart-type-keyboard",
		tier: "integration",
		needs: "build",
		cards: ["FEA-CH-75", "FEA-CH-39", "FEA-CH-55"],
		title: "Every chart type by keyboard and accessible name",
		run: [{ ...node("scripts/verify-chart-type-keyboard.ts", "--out", "{run}/chart-type-keyboard"), entry: "scripts/verify-chart-type-keyboard.ts", env: { RICHVIEW_VERIFY_BROWSER: "chromium" } }],
	},
	{
		id: "chart-type-pdf",
		tier: "integration",
		needs: "build",
		cards: ["FEA-CH-75", "FEA-RP-43"],
		title: "Every chart type keeps print-width vector marks and labels in every builtin",
		run: [{ ...node("scripts/verify-chart-type-pdf.ts", "--out", "{run}/chart-type-pdf"), entry: "scripts/verify-chart-type-pdf.ts", workers: { env: "RICHVIEW_VERIFY_WORKERS", count: 2 } }],
	},
	{ id: "data-integrity", tier: "integration", needs: "build", cards: ["DEL-16"], title: "Data integrity", parallel: 3, run: suites(DATA_SUITES) },
	{
		id: "theme-switch",
		tier: "integration",
		needs: "build",
		cards: ["DEL-17"],
		title: "Theme-switch proof",
		run: suites(["authoring"]),
	},
	{
		id: "visual-matrix",
		tier: "integration",
		needs: "build",
		cards: ["DEL-19"],
		title: "Visual review matrix",
		run: [{ ...node("scripts/verify-chart-review.ts", "--out", "{run}/chart-review", "--phase", "after"), entry: "scripts/verify-chart-review.ts", workers: { env: "RICHVIEW_VERIFY_WORKERS", count: 2 } }],
	},
	{ id: "captures", tier: "integration", needs: "build", after: ["visual-matrix"], cards: ["DEL-27", "DEL-28", "DEL-29"], title: "Captures and exact pixel diff against the dev head", fn: "captures", retryArtifacts: ["captures", "fixtures", "specimen", "captures.json", "captures-index.json", "capture-attempts.json", "diffs"] },
	{ id: "surface", tier: "integration", needs: "build", after: ["build"], cards: ["DEL-53"], title: "Public-surface snapshots", fn: "surface", retryArtifacts: ["surface"] },
	{
		id: "fonts",
		tier: "integration",
		cards: ["ARC-62"],
		title: "Font pipeline checks",
		run: [
			{ cmd: "uv", args: ["run", "scripts/fonts/build.py", "--check"], entry: "scripts/fonts/build.py" },
			{ cmd: "uv", args: ["run", "scripts/fonts/optical.py", "--check"], entry: "scripts/fonts/optical.py" },
		],
	},
	{
		id: "docs-match",
		tier: "integration",
		cards: ["DEL-18", "ARC-09", "TYP-04"],
		title: "Docs match the code",
		// The spec's reference tables are the registry's; docs count what the build has.
		run: [cargo("test", "-p", "richview-core", "--locked", "--test", "integration", "--", "spec_docs::"), cargo("test", "-p", "richview-app", "--locked", "--test", "integration", "--", "docs_match::")],
	},
	{
		id: "golden",
		tier: "integration",
		needs: "build",
		cards: ["DEL-60", "INT-55", "ARC-54", "FEA-SF-26", "FEA-SF-42", "FEA-BR-13"],
		title: "Golden inputs, snippets and docs commands",
		run: [{ ...node("tools/rv-gate/golden.mts", "--bin", "target/release/richview", "--out", "{run}/golden"), entry: "tools/rv-gate/golden.mts" }],
	},
	{ id: "no-js", tier: "integration", needs: "build", cards: ["DEL-57"], title: "No-JS reading and runtime drill", run: [{ ...node("scripts/verify-no-js.ts", "--out", "{run}/no-js"), entry: "scripts/verify-no-js.ts", shard: { kind: "no-js", count: 2, out: "{run}/no-js" } }] },
	{
		id: "hostile",
		tier: "integration",
		needs: "build",
		cards: ["DEL-56", "ARC-13", "INT-79"],
		title: "Hostile-input corpus",
		// Preserve the native style/mode matrix; the browser command audits the same corpus live.
		run: [
			cargo("test", "-p", "richview-core", "--locked", "--test", "integration", "--", "hostile::"),
			{ ...node("scripts/verify-hostile-browser.ts", "--bin", "target/release/richview", "--out", "{run}/hostile-browser"), entry: "scripts/verify-hostile-browser.ts" },
		],
	},
	{
		id: "a11y",
		tier: "integration",
		needs: "build",
		cards: ["DEL-58", "FEA-RP-49", "DEL-27", "FEA-CH-55"],
		title: "Accessibility audit and keyboard pass",
		run: [{ ...node("tools/rv-gate/a11y.mts", "--bin", "target/release/richview", "--out", "{run}/a11y"), entry: "tools/rv-gate/a11y.mts" }],
	},
	{ id: "site", tier: "integration", needs: "build", cards: ["DEL-59", "FEA-SF-62"], title: "Site routes, links, chart pages, policy and layout", run: suites(["site"]) },
	{
		id: "cross-style",
		tier: "integration",
		needs: "build",
		cards: ["DEL-52", "DEL-80", "FEA-CH-37"],
		title: "Cross-style values at the author's precision",
		run: [{ ...node("tools/rv-gate/cross-style.mts", "--bin", "target/release/richview", "--out", "{run}/cross-style"), entry: "tools/rv-gate/cross-style.mts", workers: { arg: "--jobs", count: 2 } }],
	},
	{
		id: "disclosure-probe",
		tier: "integration",
		needs: "build",
		cards: ["DEL-80", "FEA-CH-37"],
		title: "Every example over source values finer than shown",
		run: [{ ...node("tools/rv-gate/disclosure-probe.mts", "--bin", "target/release/richview", "--out", "{run}/disclosure-probe"), entry: "tools/rv-gate/disclosure-probe.mts" }],
	},
	{
		id: "label-truth",
		tier: "integration",
		needs: "build",
		cards: ["TYP-36", "FEA-CH-37"],
		title: "On-chart value texts match the Table",
		run: [
			{ ...node("tools/rv-gate/label-truth.mts", "--bin", "target/release/richview", "--out", "{run}/label-truth"), entry: "tools/rv-gate/label-truth.mts", workers: { arg: "--jobs", count: 2 } },
			{ ...node("tools/rv-gate/label-truth.mts", "--bin", "target/release/richview", "--out", "{run}/label-truth-fixtures", "--fixtures"), entry: "tools/rv-gate/label-truth.mts", workers: { arg: "--jobs", count: 2 } },
		],
	},
	{ id: "geometry", tier: "integration", needs: "build", cards: ["DEL-72"], title: "Geometry-model oracle and draw fidelity", run: [{ ...node("scripts/verify-geometry.ts", "--out", "{run}/geometry"), entry: "scripts/verify-geometry.ts" }] },
	{
		id: "engines-smoke",
		tier: "integration",
		needs: "build",
		cards: ["DEL-74"],
		title: "WebGL smoke in Chromium, Firefox and WebKit",
		parallel: 2,
		// Chromium runs the same suite in the chromium step.
		run: ["firefox", "webkit"].map((engine) => ({
			...node("scripts/verify-report-runtime.ts"),
			env: { RICHVIEW_VERIFY_BROWSER: engine },
		})),
	},
	{
		id: "parallel-shaders",
		tier: "integration",
		needs: "build",
		cards: ["DEL-74", "FEA-RP-37", "FEA-CH-47"],
		title: "Shader readiness and context recovery in three engines",
		parallel: 3,
		run: ["chromium", "firefox", "webkit"].map((engine) => ({
			...node("scripts/verify-parallel-shaders.ts"),
			entry: "scripts/verify-parallel-shaders.ts",
			env: { RICHVIEW_VERIFY_BROWSER: engine },
		})),
	},
	{
		id: "motion",
		retryArtifacts: ["motion", "work"],
		tier: "integration",
		needs: "build",
		cards: ["DEL-75", "DEL-30", "FEA-CH-75", "FEA-CH-51", "FEA-RP-36"],
		title: "Motion tests on the steppable clock",
		// Steps Playwright's paused clock through every every-block chart's entrance in Sleek at 1440 px.
		run: [{ ...node("scripts/verify-motion.ts", "--out", "{run}"), entry: "scripts/verify-motion.ts" }],
	},
	{
		id: "report-motion",
		tier: "integration",
		needs: "build",
		cards: ["FEA-RP-35", "FEA-RP-36", "FEA-RP-48"],
		title: "Report entrances, reduced motion and scrollytelling in every style",
		run: [{ ...node("scripts/verify-report-motion.ts"), entry: "scripts/verify-report-motion.ts" }],
	},
	{
		id: "journeys",
		tier: "integration",
		needs: "build",
		cards: ["DEL-76", "FEA-SF-09", "TYP-69"],
		title: "Author and agent journeys on the release artifacts",
		// The cheap journeys, in every release run (DEL-76 option 1); the full run adds the rest.
		run: [{ ...node("tools/rv-gate/journeys.mts", "--bin", "target/release/richview", "--out", "{run}/journeys", "--set", "release"), entry: "tools/rv-gate/journeys.mts" }],
	},
	{
		id: "contract",
		tier: "integration",
		needs: "build",
		cards: ["ARC-08", "TYP-53", "FEA-RP-01", "FEA-CH-59", "OPN-57", "ARC-62"],
		title: "Cross-surface contract test",
		run: [{ ...node("tools/rv-gate/contract.mts", "--bin", "target/release/richview", "--out", "{run}/contract"), entry: "tools/rv-gate/contract.mts" }],
	},
	{ id: "deps", tier: "integration", cards: ["ARC-68"], title: "Dependency and licence gate", run: [cargo("deny", "check"), node("tools/rv-gate/npm-licences.mts"), node("tools/rv-gate/npm-advisories.mts"), node("scripts/third-party-notices.ts", "--check")] },
	{ id: "coverage", tier: "integration", cards: ["DEL-79"], title: "Every accepted item names its check", fn: "coverage" },
	{
		id: "perf",
		tier: "integration",
		needs: "build",
		// Runs last and alone, after every other step passed: it waits for a quiet host and holds the benchmark lock (PRF-02).
		alone: true,
		cards: ["DEL-51", "PRF-02", "PRF-04", "PRF-05", "PRF-30", "PRF-40", "OPN-50"],
		title: "Performance budgets, alone on a quiet host",
		run: [{ ...node("tools/rv-gate/perf.mts", "--bin", "target/release/richview", "--dist", "dist/web", "--out", "{run}/perf"), entry: "tools/rv-gate/perf.mts" }],
	},

	// Full tier: `rv-gate prod`, daily on a changed dev head and on each frozen candidate (DEL-33, DEL-71).
	{
		id: "webkit",
		tier: "full",
		needs: "build",
		cards: ["DEL-33", "DEL-74"],
		title: "WebKit behaviour loop",
		parallel: 3,
		run: suites(ENGINE_LOOP_SUITES, { env: { RICHVIEW_VERIFY_BROWSER: "webkit" } }).map((step) => ({ ...step, entry: undefined })),
	},
	{
		id: "firefox",
		tier: "full",
		needs: "build",
		cards: ["DEL-33", "DEL-74"],
		title: "Firefox behaviour loop",
		parallel: 3,
		run: suites(ENGINE_LOOP_SUITES, { env: { RICHVIEW_VERIFY_BROWSER: "firefox" } }).map((step) => ({
			...step, entry: undefined,
			// Temporary diagnostics on the original canonical child; no assertion or timeout change. The
			// screenshots of its preview reloads stay with the run.
			...(step.args[0] === "scripts/verify-browser.ts" ? { env: { ...step.env, DEBUG: "pw:protocol", DEBUG_COLORS: "0", RV_GATE_FIREFOX_PROTOCOL_LOG: "{run}/firefox-protocol.jsonl", RV_GATE_PREVIEW_RELOAD_DIR: "{run}/firefox-preview-reloads" } } : {}),
		})),
	},
	{
		id: "journeys-full",
		tier: "full",
		needs: "build",
		cards: ["DEL-76", "INT-07", "FEA-SF-55", "FEA-BR-26", "FEA-BR-29"],
		title: "Author, agent, Cowork and Publish journeys",
		// The VS Code journey stays with the vscode step (OPN-55).
		run: [{ ...node("tools/rv-gate/journeys.mts", "--bin", "target/release/richview", "--out", "{run}/journeys-full", "--set", "full"), entry: "tools/rv-gate/journeys.mts" }],
	},
	{
		id: "exports",
		tier: "full",
		needs: "build",
		cards: ["DEL-33", "FEA-RP-43"],
		title: "PDF and export checks",
		// types.json, the vector check of every chart type (FEA-CH-75), lands in the run folder.
		run: [{ ...node("scripts/verify-exports.ts", "--out", "{run}/exports"), entry: "scripts/verify-exports.ts" }],
	},
	{ id: "assets", tier: "full", needs: "build", cards: ["DEL-33", "ARC-34"], title: "Asset upload", run: suites(["assets"]) },
	{
		id: "packaging",
		tier: "full",
		needs: "build",
		cards: ["DEL-33", "FEA-SF-21"],
		title: "VSIX packaging",
		run: [{ cmd: "python3", args: ["scripts/verify-packaging.py", "--package"], entry: "scripts/verify-packaging.py" }],
	},
	{
		id: "vscode",
		tier: "full",
		cards: ["DEL-22", "DEL-33"],
		title: "Installed VS Code extension on the current host in an isolated test installation",
		needs: "build",
		after: ["packaging"],
		parallel: 1, // Finish packaging before reading its VSIX; one native extension test host.
		run: [node("scripts/verify-vscode.ts", "--out", "{run}/vscode")],
		entries: ["scripts/verify-vscode.ts", "scripts/verify-vscode.cjs", "scripts/verify-vscode.sh"],
	},
	{ id: "voiceover", tier: "full", cards: ["DEL-58"], title: "VoiceOver pass", unrun: "Standing gap: no agent drives VoiceOver reliably (DELIVERY.md)." },
	{ id: "windows", tier: "full", cards: ["ARC-67", "DEL-22"], title: "Windows end-to-end walk on a fresh AWS Windows machine", unrun: WINDOWS_STANDING },
	{
		id: "fresh-agent",
		tier: "full",
		needs: "build",
		cards: ["DEL-78", "INT-69", "FEA-SF-65"],
		title: "Fresh-agent run",
		unrunIn: { full: FRESH_AGENT_SCHEDULE },
		// Three serial runs per product on subscription capacity; admitted providers may overlap.
		// It fails only when a run
		// could not execute; pass rates and drops are findings in {run}/fresh-agent/appendix.md.
		run: [{ ...node("tools/agent-use/gate.mjs", "--richview", "target/release/richview", "--products", "claude", "--unrun", `codex:${CODEX_UNRUN}`, "--out", "{run}/fresh-agent"), workers: { arg: "--jobs", count: 2 } }],
	},
	{ id: "hosted-smoke", tier: "full", needs: "build", after: ["build", "publish-tests"], cards: ["DEL-77"], title: "Local hosted services smoke", fn: "hostedSmoke", entries: ["tools/rv-gate/hosted-smoke.mts"], retryArtifacts: ["hosted-smoke"] },
	{ id: "reviews", tier: "full", cards: ["DEL-35", "DEL-71"], title: "Design verdicts and the batch's adversarial review were recorded", fn: "reviews", mode: "prod" },
	{ id: "audit", tier: "full", cards: ["DEL-38"], title: "Merge and deploy audit", fn: "audit" },
];

/** Entry points that belong to no gate step, each with the reason it stays outside. */
export const OUTSIDE_TIERS: Record<string, string> = {
	"scripts/check-performance.py": "Superseded by tools/rv-gate/perf.mts: its Linux VmHWM memory gate is retired (PRF-03, PRF-40).",
	"scripts/design-review/capture.ts": "Motion evidence for motion PRs (DEL-30); joins the motion step.",
};

export function stepsForTier(tier: Tier, mode: string, steps = STEPS) {
	return steps.filter((step) => tierIncludes(tier, step.tier) && (!step.mode || step.mode === mode));
}

/**
 * The one script a command runs: its `entry`, or the script file a `node` command names first.
 * An entry that is a glob names a set of files, not one script, so it has none.
 */
function scriptOf(command: GateCommand) {
	const glob = (path: string) => /[*?[]/.test(path);
	if (command.entry) return glob(command.entry) ? undefined : command.entry;
	return nodeScript(command);
}

/** The script file a `node` command runs: its first argument that is not a flag. */
function nodeScript(command: GateCommand) {
	const first = command.cmd === "node" ? command.args?.find((arg) => !arg.startsWith("-")) : undefined;
	return first && !/[*?[]/.test(first) && /\.[cm]?[jt]s$/.test(first) ? first : undefined;
}

/** A head command as the gate runs it: only `node <script>`, with its entry taken from the script. */
function headCommand(command: GateCommand): RegisteredScript | undefined {
	const script = nodeScript(command);
	return script ? { cmd: "node", args: command.args, entry: script, ...(command.env ? { env: command.env } : {}) } : undefined;
}

/**
 * The registry a PR's checkout runs under `rv-gate dev` (DEL-36). Steps, their checks, tiers,
 * order and entry globs stay `base`'s, the gate's own copy from origin/dev. Each step's commands
 * keep base's scripts that still exist in the checkout, including a same-step TypeScript rename.
 * A missing script is a retired test, and
 * `retired-tests` holds the PR until its body lists it with a reason. A step also gains the
 * scripts the checkout's registry (`head`) adds to it, and a step base has not built yet takes
 * head's commands when every one of them runs a script that exists. Nothing head removes or
 * relaxes is taken.
 */
export function mergeRegistry(base: readonly GateStep[], head: readonly HeadStep[] | undefined, exists: (path: string) => boolean) {
	const heads = new Map((head ?? []).map((step) => [step.id, step]));
	const retired: string[] = [];
	const added: string[] = [];
	const steps = base.map((step) => {
		const theirs = heads.get(step.id);
		// Head's commands are taken only as `node <script>` with an existing script; their
		// `entry` labels, `needs` and `parallel` are not trusted.
		if (step.unrun) {
			const theirsRun = theirs && !theirs.unrun && Array.isArray(theirs.run) ? theirs.run : [];
			const run = theirsRun.map(headCommand);
			if (!run.length || !run.every((command): command is RegisteredScript => command !== undefined && exists(command.entry))) return step;
			added.push(...run.map((command) => command.entry));
			const { unrun: _built, ...rest } = step;
			return { ...rest, run };
		}
		if (!step.run) return step;
		const moved = new Map<string, string>();
		const kept = step.run.flatMap((command) => {
			const script = scriptOf(command);
			if (!script || exists(script)) return [command];
			const counterpart = nodeScript(command) === script && typeScriptPaths(script).find(path =>
				exists(path) && (theirs?.run ?? []).some(candidate => nodeScript(candidate) === path));
			if (counterpart) {
				moved.set(script, counterpart);
				const index = command.args.indexOf(script);
				const args = command.args.with(index, counterpart);
				return [{ ...command, args, ...(command.entry && { entry: counterpart }) }];
			}
			retired.push(script);
			return [];
		});
		const known = new Set([...step.run.map(scriptOf).filter(Boolean), ...moved.values()]);
		const extra = (theirs?.run ?? [])
			.map(headCommand)
			.filter((command): command is RegisteredScript => command !== undefined && !known.has(command.entry) && exists(command.entry));
		added.push(...extra.map((command) => command.entry));
		return { ...step, run: [...kept, ...extra], ...(step.entries && moved.size && { entries: step.entries.map(entry => moved.get(entry) ?? entry) }) };
	});
	return { steps, retired: [...new Set(retired)], added: [...new Set(added)] };
}

/** Whether `value` is a registry as `mergeRegistry` reads it; anything else is ignored. */
export function isRegistry(value: unknown): value is HeadStep[] {
	const object = (entry: unknown): entry is Record<string, unknown> => entry !== null && typeof entry === "object";
	const command = (c: unknown) => object(c) && typeof c.cmd === "string" && Array.isArray(c.args) && c.args.every((a: unknown) => typeof a === "string") && (c.entry === undefined || typeof c.entry === "string");
	return Array.isArray(value) && value.every((step: unknown) => object(step) && typeof step.id === "string" && (step.run === undefined || (Array.isArray(step.run) && step.run.every(command))));
}
