// Built-in gate checks. Each takes the run context and returns
// `{status: "pass"|"fail"|"unrun", reason?, details?}`.
//
// Context: {cwd, base, head, mode, pr, runDir, log(line)} where `base` and `head` are commits,
// `mode` is fast|pre-pr|dev|prod, and `pr` holds the parsed PR fields in dev mode.

import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import { listFiles, matchesAny } from "./glob.mjs";
import { parsePrBody, validatePrFields } from "./pr.mjs";
import { sh } from "./proc.mjs";
import { OUTSIDE_TIERS, STEPS } from "./steps.mjs";
import { evidenceDir, listReceipts, readReceipt, receiptNotes } from "./store.mjs";
import { workspaceBoundaries } from "./layers.mjs";
import { fullPromotionProblems } from "./pre-prod.mjs";
import {historicalAffectedProblems} from "./affected-promotion.mjs";

const pass = (details) => ({ status: "pass", details });
const fail = (reason, details) => ({ status: "fail", reason, details });

// ---------------------------------------------------------------------------------------------
// DEL-49, DEL-22: the gate refuses to run unless the toolchain pins match.

export const PINS = { rust: "1.99.0", wasmBindgen: "0.2.129", cargoDeny: "0.20.2", playwright: "1.65.0" };

export function pins({ cwd }) {
	const problems = [];
	const found = {};
	const toolchain = readFileSync(join(cwd, "rust-toolchain.toml"), "utf8").match(/channel\s*=\s*"([^"]+)"/)?.[1];
	found.rustToolchain = toolchain;
	if (toolchain !== PINS.rust) problems.push(`rust-toolchain.toml pins ${toolchain}, expected ${PINS.rust}`);
	found.rustc = sh("rustc", ["--version"], { cwd });
	if (!found.rustc.includes(PINS.rust)) problems.push(`rustc is '${found.rustc}'`);
	const lock = readFileSync(join(cwd, "Cargo.lock"), "utf8").match(/name = "wasm-bindgen"\nversion = "([^"]+)"/)?.[1];
	found.wasmBindgenLock = lock;
	if (lock !== PINS.wasmBindgen) problems.push(`Cargo.lock has wasm-bindgen ${lock}`);
	try {
		found.wasmBindgenCli = sh("wasm-bindgen", ["--version"], { cwd });
		if (!found.wasmBindgenCli.endsWith(PINS.wasmBindgen)) problems.push(`wasm-bindgen CLI is '${found.wasmBindgenCli}'`);
	} catch {
		problems.push("wasm-bindgen CLI is not installed");
	}
	try {
		found.cargoDeny = sh("cargo-deny", ["--version"], { cwd });
		if (!found.cargoDeny.endsWith(PINS.cargoDeny)) problems.push(`cargo-deny is '${found.cargoDeny}', expected ${PINS.cargoDeny}`);
	} catch {
		problems.push("cargo-deny is not installed");
	}
	const pkg = JSON.parse(readFileSync(join(cwd, "package.json"), "utf8"));
	found.playwrightPin = pkg.devDependencies?.playwright;
	if (found.playwrightPin !== PINS.playwright) problems.push(`package.json pins playwright ${found.playwrightPin}`);
	const installed = join(cwd, "node_modules/playwright/package.json");
	found.playwrightInstalled = existsSync(installed) ? JSON.parse(readFileSync(installed, "utf8")).version : undefined;
	if (found.playwrightInstalled !== PINS.playwright) problems.push(`installed playwright is ${found.playwrightInstalled ?? "missing"} (run npm ci)`);
	const nodePin = readFileSync(join(cwd, ".node-version"), "utf8").trim().replace(/^v/, "");
	found.nodePin = nodePin;
	found.node = process.version.slice(1);
	if (nodePin !== found.node) problems.push(`.node-version pins ${nodePin}, the gate host runs ${found.node}`);
	try {
		found.macos = sh("sw_vers", ["-buildVersion"]);
	} catch {
		found.macos = "unknown";
	}
	return problems.length ? fail(problems.join("; "), found) : pass(found);
}

// ---------------------------------------------------------------------------------------------
// DEL-23: every test entry point belongs to exactly one tier.

export const ENTRY_GLOBS = [
	"scripts/verify-*",
	"scripts/check-*",
	"scripts/design-review/capture.mjs",
	"tools/lockdown-app/test/*.test.mjs",
	"tools/rv-gate/test/*.test.mjs",
	"tools/agent-use/test/*.test.mjs",
	"cloudflare/publish/test/*.test.mjs",
	"packages/vscode/test/*.test.cjs",
	"crates/*/tests/*.rs",
	"richview-chart/tests/*.rs",
];

export function stepEntries(step) {
	const entries = [...(step.entries ?? [])];
	for (const command of step.run ?? []) if (command.entry) entries.push(command.entry);
	return entries;
}

export function entrypoints({ cwd, steps = STEPS }) {
	const files = listFiles(cwd).filter((path) => matchesAny(path, ENTRY_GLOBS));
	const owners = new Map();
	for (const step of steps) for (const glob of stepEntries(step)) owners.set(glob, step.id);
	const missing = [];
	const claimed = {};
	for (const file of files) {
		if (OUTSIDE_TIERS[file]) continue;
		const steps = [...owners].filter(([glob]) => matchesAny(file, [glob])).map(([, id]) => id);
		const unique = [...new Set(steps)];
		if (unique.length === 0) missing.push(file);
		claimed[file] = unique;
	}
	const multi = Object.entries(claimed).filter(([, ids]) => ids.length > 1);
	if (missing.length || multi.length) {
		return fail(
			[
				missing.length ? `In no tier: ${missing.join(", ")}` : "",
				multi.length ? `In several steps: ${multi.map(([f, ids]) => `${f} (${ids.join(", ")})`).join(", ")}` : "",
			]
				.filter(Boolean)
				.join("; "),
		);
	}
	return pass({ entryPoints: files.length, outside: Object.keys(OUTSIDE_TIERS) });
}

// ---------------------------------------------------------------------------------------------
// DEL-54, ARC-02, ARC-03: core and engine stay pure; handwritten JavaScript only where allowed.

/** Handwritten JavaScript in product paths and its host reason (ARC-03). */
export const JS_ALLOWLIST = {
	"assets/landing/bootstrap.js": { reason: "Landing bootstrap: starts the Wasm module on reader intent" },
	"packages/vscode/extension.cjs": { reason: "VS Code adapter" },
	"cloudflare/interest-worker.mjs": { reason: "Cloudflare Worker adapter" },
	"cloudflare/publish/adapter.mjs": { reason: "Cloudflare Worker adapter: bindings, identity hand-off and I/O around the Rust Publish server" },
	"cloudflare/publish/auth.mjs": { reason: "Cloudflare Worker adapter: verifies the Access token and hands the principal over" },
	"cloudflare/publish/entry.mjs": { reason: "Worker bootstrap: loads the Wasm module and serves the bundled fonts" },
	"cloudflare/publish/deploy.mjs": { reason: "Operator runbook printer for the Cloudflare deployment and rollback; prints text, runs nothing" },
	"cloudflare/publish/desk.mjs": { reason: "Embedded source editor glue on the signed-in desk page (ARC-03)" },
	"cloudflare/publish/pkg/*.js": { reason: "Generated wasm-bindgen glue" },
};

const PRODUCT_JS = /^(?:assets|cloudflare|packages|crates|richview-chart)\/.*\.(?:m?js|cjs)$/;
const NOT_PRODUCT = /(?:^|\/)(?:test|tests|node_modules|dist)\//;

/** Check tracked product scripts against ARC-03 and require each host reason. */
export function productJsProblems(cwd, tracked) {
	const problems = [];
	for (const file of tracked.filter((p) => PRODUCT_JS.test(p) && !NOT_PRODUCT.test(p))) {
		const allowed = Object.entries(JS_ALLOWLIST).find(([glob]) => matchesAny(file, [glob]));
		if (!allowed) problems.push(`${file} is handwritten JavaScript outside ARC-03's allowlist`);
		else if (!allowed[1].transitional && !/^\/\/.+/.test(readFileSync(join(cwd, file), "utf8")))
			problems.push(`${file} lacks its host-reason header`);
	}
	return problems;
}

export function arch({ cwd, base }) {
	const problems = [];
	// The editor's source operations are pure too: the site runs them in Wasm (S27-931).
	for (const crate of ["crates/richview-core/src", "richview-chart/src", "crates/richview-authoring/src"]) {
		for (const file of listFiles(join(cwd, crate))) {
			if (!file.endsWith(".rs") || file.startsWith("bin/")) continue;
			const text = readFileSync(join(cwd, crate, file), "utf8");
			const hit = text.match(/\bstd::(fs|net|process|env)\b/);
			if (hit) problems.push(`${crate}/${file} uses std::${hit[1]} (ARC-02)`);
		}
	}
	const tracked = sh("git", ["ls-files"], { cwd }).split("\n");
	// ARC-04: Wasm modules expose only their wasm-bindgen API, never handwritten C exports.
	for (const file of tracked.filter((p) => p.endsWith(".rs"))) {
		const text = readFileSync(join(cwd, file), "utf8");
		const hit = text.match(/#\[(?:unsafe\()?no_mangle\)?\]|\bextern\s+"C"\s+fn\b/);
		if (hit) problems.push(`${file} has a handwritten C export (${hit[0]}) (ARC-04)`);
	}
	if (/cdylib/.test(readFileSync(join(cwd, "richview-chart/Cargo.toml"), "utf8")))
		problems.push("richview-chart is built as a cdylib; only the runtimes are Wasm modules (ARC-04)");
	problems.push(...productJsProblems(cwd, tracked));
	const interfaces = workspaceBoundaries({ cwd, base });
	for (const finding of interfaces.api.findings) problems.push(`API ${finding.contract ?? finding.code}: ${finding.message}`);
	for (const finding of interfaces.dependencies.findings) problems.push(`${finding.code}: ${finding.message}`);
	return problems.length ? fail(problems.join("; "), { interfaces }) : pass({ interfaces });
}

// ---------------------------------------------------------------------------------------------
// DEL-55: credentials and private data never merge.

export const SECRET_PATTERNS = [
	["GitHub token", /\b(?:gh[pousr]_[A-Za-z0-9]{36,}|github_pat_[A-Za-z0-9_]{50,})\b/],
	["AWS access key", /\b(?:AKIA|ASIA)[0-9A-Z]{16}\b/],
	["private key", /-----BEGIN (?:RSA |EC |OPENSSH |DSA |PGP )?PRIVATE KEY-----/],
	["Anthropic or OpenAI key", /\bsk-(?:ant-)?[A-Za-z0-9_-]{32,}\b/],
	["Slack token", /\bxox[abprs]-[A-Za-z0-9-]{10,}\b/],
	["Linear key", /\blin_api_[A-Za-z0-9]{32,}\b/],
	["Cloudflare token assignment", /\b(?:CLOUDFLARE|CF)_API_TOKEN\s*[=:]\s*["']?[A-Za-z0-9_-]{30,}/],
	["connection string with password", /\b(?:postgres(?:ql)?|mysql|mongodb(?:\+srv)?|redis|snowflake):\/\/[^\s:/@]+:[^\s@/]{3,}@/i],
	["credential assignment", /\b(?:api[_-]?key|secret|password|passwd|token|access[_-]?key)\b\s*[=:]\s*["'][A-Za-z0-9+/_\-]{24,}["']/i],
];

export const PRIVATE_PATTERNS = [
	["Notion link", /\bhttps?:\/\/(?:www\.|app\.)?notion\.(?:so|site|com)\/[^\s)]+/i],
	["host home path", /\/Users\/(?!example\b|you\b|me\b|name\b|user\b)[a-z][\w.-]*\/[^\s"'`)<>]*/],
	["Linux home path", /\/home\/(?!user\b|runner\b|example\b)[a-z][\w.-]*\/[^\s"'`)<>]*/],
	["vault path", /(?:^|[\s(`'"])~\/vault\//],
	["personal email", /\b[A-Za-z0-9._%+-]+@(?:gmail|outlook|hotmail|icloud|yahoo|proton(?:mail)?|seznam)\.[a-z.]{2,}\b/i],
];

/** Allowed findings: `{file glob, pattern label, reason}`; changing it is a gate change (DEL-36). */
export function loadSecretAllowlist(cwd) {
	const path = join(cwd, "tools/rv-gate/secrets-allow.json");
	return existsSync(path) ? JSON.parse(readFileSync(path, "utf8")) : [];
}

export function scanText(file, text, allow = []) {
	const findings = [];
	const lines = text.split("\n");
	const privateLabels = new Set(PRIVATE_PATTERNS.map(([label]) => label));
	for (const [label, pattern] of [...SECRET_PATTERNS, ...PRIVATE_PATTERNS]) {
		if (allow.some((entry) => entry.pattern === label && matchesAny(file, [entry.file]))) continue;
		const every = new RegExp(pattern.source, `${pattern.flags.replace("g", "")}g`);
		lines.forEach((line, index) => {
			for (const match of line.matchAll(every)) {
				findings.push({ file, line: index + 1, label, match: match[0], private: privateLabels.has(label) });
			}
		});
	}
	return findings;
}

/**
 * Drop private findings (addresses, home paths, links) whose exact text the file already held at
 * `base`: a rewritten line that keeps them exposes nothing new. Secrets are always findings.
 * `baseText(file)` returns the file at `base`, or undefined when it did not exist.
 */
export function newPrivateOnly(findings, baseText) {
	const cache = new Map();
	return findings.filter((finding) => {
		if (!finding.private) return true;
		if (!cache.has(finding.file)) cache.set(finding.file, baseText(finding.file) ?? "");
		return !cache.get(finding.file).includes(finding.match);
	});
}

/** Added lines per file between `base` and the working tree (or `head`), untracked files included. */
export function addedLines({ cwd, base, head }) {
	const args = ["diff", "--unified=0", "--no-color", "--no-ext-diff", base];
	if (head) args.push(head);
	const diff = sh("git", args, { cwd });
	const added = new Map();
	let file;
	for (const line of diff.split("\n")) {
		if (line.startsWith("+++ ")) file = line.slice(4).replace(/^b\//, "");
		else if (line.startsWith("+") && file && file !== "/dev/null") {
			added.set(file, `${added.get(file) ?? ""}${line.slice(1)}\n`);
		}
	}
	if (!head) {
		for (const path of sh("git", ["ls-files", "--others", "--exclude-standard"], { cwd }).split("\n").filter(Boolean)) {
			try {
				added.set(path, readFileSync(join(cwd, path), "utf8"));
			} catch {
				// Unreadable or binary files are skipped.
			}
		}
	}
	return added;
}

export function secrets(ctx) {
	const allow = loadSecretAllowlist(ctx.cwd);
	const findings = [];
	for (const [file, text] of addedLines(ctx)) {
		findings.push(...scanText(file, text, allow));
	}
	const baseText = (file) => {
		try {
			return sh("git", ["show", `${ctx.base}:${file}`], { cwd: ctx.cwd });
		} catch {
			return undefined;
		}
	};
	findings.splice(0, findings.length, ...newPrivateOnly(findings, baseText));
	if (!findings.length) return pass();
	return fail(
		`${findings.length} finding(s): ${findings
			.slice(0, 20)
			.map((f) => `${f.file} (${f.label})`)
			.join(", ")}`,
		findings,
	);
}

// ---------------------------------------------------------------------------------------------
// DEL-32, DEL-44, DEL-70: the PR declares its issue, class, cause and items.

export function knownItems(cwd) {
	const ids = new Set();
	const dir = join(cwd, "docs/lockdown/parts");
	if (!existsSync(dir)) return ids;
	for (const file of listFiles(dir).filter((f) => /^\d\d[a-z]?-.+\.json$/.test(f))) {
		for (const item of JSON.parse(readFileSync(join(dir, file), "utf8")).items ?? []) ids.add(item.id);
	}
	return ids;
}

export function prFields({ cwd, pr }) {
	if (!pr) return fail("No PR context.");
	const problems = validatePrFields(pr, { knownItems: knownItems(cwd) });
	return problems.length ? fail(problems.join(" ")) : pass(pr);
}

// ---------------------------------------------------------------------------------------------
// DEL-36: no step is removed, no threshold or budget raised and no answer edited by hand.

function stepsAt(cwd, commit) {
	try {
		const text = sh("git", ["show", `${commit}:tools/rv-gate/lib/steps.mjs`], { cwd });
		const ids = [...text.matchAll(/\bid:\s*"([\w-]+)"/g)].map((m) => m[1]);
		const entries = [...text.matchAll(/\bentry:\s*`?["'`]?([\w./-]+)/g)].map((m) => m[1]);
		return { ids: new Set(ids), entries: new Set(entries) };
	} catch {
		return undefined;
	}
}

/**
 * Files that hold a check's thresholds, budgets, allowances or expected values. A change to one may
 * raise a budget or widen an allowance without removing a step, so the receipt lists each one for the
 * batch review to judge (DEL-36); removing a step or editing an answer still fails outright.
 */
export const THRESHOLD_FILES = [
	// Derived, not listed: every gate script and data file under tools/rv-gate can loosen a check, so
	// all of them count except the tests; fixtures and the step table have their own lines below.
	/^tools\/rv-gate\/(?!test\/|fixtures\/)(?:[\w.-]+\/)*[\w.-]+\.(?:mjs|json)$/,
	/^tools\/rv-gate\/fixtures\//,
	/(?:^|\/)golden-warnings\.json$/,
	/^deny\.toml$/,
	/^scripts\/verify-[\w-]+\.(?:mjs|cjs|py)$/,
	// The loopback server every browser suite runs under, with the site's security headers (review 1g note 10).
	/^scripts\/browser-harness\.mjs$/,
	// A shared Rust test helper can loosen every test that calls it (review 1g note 10, should-fix 3).
	/(?:^|\/)tests\/common\/(?:[\w.-]+\/)*[\w.-]+\.rs$/,
];

export function thresholdChanges(paths) {
	return paths.filter((path) => THRESHOLD_FILES.some((pattern) => pattern.test(path)));
}

export function weakening({ cwd, base, head }) {
	const problems = [];
	const before = stepsAt(cwd, base);
	const after = stepsAt(cwd, head);
	if (before && after) {
		for (const id of before.ids) if (!after.ids.has(id)) problems.push(`step '${id}' removed`);
	}
	const changed = sh("git", ["diff", "--name-only", base, head], { cwd }).split("\n").filter(Boolean);
	const answers = changed.filter((path) => path.startsWith("docs/lockdown/answers/") && path.endsWith(".json"));
	if (answers.length) {
		const log = sh("git", ["show", `${head}:docs/lockdown/answers/log.jsonl`], { cwd })
			.split("\n")
			.filter(Boolean)
			.map((line) => JSON.parse(line));
		const latest = new Map();
		for (const entry of log) latest.set(entry.itemId, entry);
		for (const path of answers) {
			let file;
			try {
				file = JSON.parse(sh("git", ["show", `${head}:${path}`], { cwd }));
			} catch {
				continue;
			}
			for (const [itemId, answer] of Object.entries(file.answers ?? {})) {
				const logged = latest.get(itemId);
				if (!logged || logged.decision !== answer.decision || (logged.text ?? "") !== (answer.text ?? "")) {
					problems.push(`${itemId} answer differs from its log.jsonl entry (DEL-66)`);
				}
			}
		}
	}
	const review = thresholdChanges(changed);
	const details = review.length ? { thresholdChanges: review, note: "Each listed file can loosen a check; the batch review judges it (DEL-36)." } : undefined;
	return problems.length ? fail(`Held for Daniel (DEL-36): ${problems.join("; ")}`, details) : pass(details);
}

// ---------------------------------------------------------------------------------------------
// DEL-36, DEL-64: each deleted or skipped test is listed with its reason.

const RUST_TEST = /#\[(?:tokio::)?test[^\]]*\]\s*(?:#\[[^\]]*\]\s*)*(?:async\s+)?fn\s+(\w+)/g;
const NODE_TEST = /\btest\(\s*(['"`])((?:(?!\1).)+)\1/g;
// Skips count per language, so a Rust attribute quoted in a JavaScript fixture is not a skip.
const RUST_SKIP = /^\s*#\[ignore/gm;
const NODE_SKIP = /\b(?:test|it|describe)\.(?:skip|todo)\(|\{\s*(?:skip|todo)\s*:\s*true/g;

/** Files whose tests the check reads: Rust, and JavaScript or TypeScript in any module flavour. */
const TEST_SOURCE = /\.(?:rs|[cm]?[jt]sx?)$/;

function testsIn(text, path) {
	if (path.endsWith(".rs")) return [...text.matchAll(RUST_TEST)].map((m) => m[1]);
	if (/\.[cm]?[jt]sx?$/.test(path)) return [...text.matchAll(NODE_TEST)].map((m) => m[2]);
	return [];
}

export function deletedTests({ cwd, base, head }) {
	const removed = [];
	const changes = sh("git", ["diff", "--name-status", "--no-renames", base, head], { cwd }).split("\n").filter(Boolean);
	// A test that moved to another file (a renamed crate, a split module) is kept: its name must
	// be new in that file, not merely present there before.
	const moved = new Set();
	for (const line of changes) {
		const [status, path] = line.split("\t");
		if (status === "D" || !TEST_SOURCE.test(path)) continue;
		const after = sh("git", ["show", `${head}:${path}`], { cwd, allowFail: true }).stdout ?? "";
		const before = status === "A" ? "" : (sh("git", ["show", `${base}:${path}`], { cwd, allowFail: true }).stdout ?? "");
		const had = new Set(testsIn(before, path));
		for (const name of testsIn(after, path)) if (!had.has(name)) moved.add(`${path.endsWith(".rs") ? "rs" : "js"}:${name}`);
	}
	for (const line of changes) {
		const [status, path] = line.split("\t");
		if (!TEST_SOURCE.test(path)) continue;
		if (/^scripts\/verify-/.test(path) && status === "D") {
			removed.push({ test: path, kind: "suite" });
			continue;
		}
		const before = status === "A" ? "" : sh("git", ["show", `${base}:${path}`], { cwd, allowFail: true }).stdout ?? "";
		const after = status === "D" ? "" : sh("git", ["show", `${head}:${path}`], { cwd, allowFail: true }).stdout ?? "";
		const kept = new Set(testsIn(after, path));
		const language = path.endsWith(".rs") ? "rs" : "js";
		for (const name of testsIn(before, path)) if (!kept.has(name) && !moved.has(`${language}:${name}`)) removed.push({ test: `${path}::${name}`, kind: "test" });
		const skips = path.endsWith(".rs") ? RUST_SKIP : NODE_SKIP;
		const skippedBefore = (before.match(skips) ?? []).length;
		const skippedAfter = (after.match(skips) ?? []).length;
		if (skippedAfter > skippedBefore) removed.push({ test: `${path} (skip added)`, kind: "skip" });
	}
	return removed;
}

/**
 * The retired-test ledger, `{ tests: [{ test, reason, commit }] }`: the checkout's copy and the
 * gate's own (origin/dev's), so a release batch can be explained by a later commit on dev.
 */
export function retiredLedger(cwd) {
	const paths = [join(cwd, "tools/rv-gate/retired-tests.json"), fileURLToPath(new URL("../retired-tests.json", import.meta.url))];
	return [...new Set(paths)]
		.filter((path) => existsSync(path))
		.flatMap((path) => JSON.parse(readFileSync(path, "utf8")).tests ?? [])
		.filter((entry) => entry.test && entry.reason);
}

/** `path::name`, ignoring module segments between them (`build.rs::tests::x` is `build.rs::x`). */
export function sameTest(a, b) {
	const key = (test) => {
		const match = test.match(/^([^:]+)::(?:[^:]+::)*([^:]+)$/);
		return match ? `${match[1]}::${match[2]}` : test.trim();
	};
	return key(a) === key(b);
}

export function retiredTests(ctx) {
	const removed = deletedTests(ctx);
	// Every script a gated run drops from dev's registry is a retired test too, whatever its
	// name or language, and also when it was already missing (DEL-36).
	for (const script of ctx.registry?.retired ?? []) {
		if (!removed.some((entry) => entry.test === script)) removed.push({ test: script, kind: "dropped" });
	}
	// Since DEL-81 changes reach dev without a PR: a commit lists its retired tests in its message,
	// or a later commit adds them to tools/rv-gate/retired-tests.json with the reason.
	// A ledger entry explains only a removal its own commit made in this batch, named exactly
	// (path::name, the suite path, or "path (skip added)"): it never pre-explains a later one.
	// An entry's `commit` is a commit in the batch, or an issue a batch commit refers to.
	const messages = sh("git", ["log", "--format=%B", `${ctx.base}..${ctx.head ?? "HEAD"}`], { cwd: ctx.cwd, allowFail: true }).stdout ?? "";
	const inBatch = (commit) => {
		if (!commit) return false;
		if (/^[A-Z][A-Z0-9]*-\d+$/.test(commit)) return new RegExp(`\\b${commit}\\b`).test(messages);
		const git = (...args) => sh("git", args, { cwd: ctx.cwd, allowFail: true }).status === 0;
		return git("merge-base", "--is-ancestor", commit, ctx.head ?? "HEAD") && !git("merge-base", "--is-ancestor", commit, ctx.base);
	};
	const listed = [...(ctx.pr?.retired ?? []), ...retiredLedger(ctx.cwd).filter((entry) => inBatch(entry.commit))];
	const unexplained = removed.filter((entry) => !listed.some((item) => sameTest(item.test, entry.test)));
	if (unexplained.length) {
		return fail(
			`List each under "## Retired tests" with its reason: ${unexplained.map((e) => e.test).join(", ")}`,
			{ removed, listed },
		);
	}
	return pass({ removed, listed });
}

// ---------------------------------------------------------------------------------------------
// DEL-79: an item a PR names must have a check or be marked judged.

export function loadCoverage(cwd) {
	const path = join(cwd, "tools/rv-gate/coverage.json");
	return existsSync(path) ? JSON.parse(readFileSync(path, "utf8")) : { items: {} };
}

/** The text of a file, or of every `.rs`/`.mjs` file under a folder, or undefined when the path is missing. */
function textAt(path) {
	if (!existsSync(path)) return undefined;
	if (!statSync(path).isDirectory()) return readFileSync(path, "utf8");
	return readdirSync(path, { withFileTypes: true })
		.flatMap((entry) => {
			const full = join(path, entry.name);
			if (entry.isDirectory()) return entry.name === "node_modules" || entry.name === "target" ? [] : [textAt(full)];
			return /\.(rs|mjs)$/.test(entry.name) ? [readFileSync(full, "utf8")] : [];
		})
		.join("\n");
}

const occurs = (text, word) => new RegExp(`(?<![A-Za-z0-9_])${word.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}(?![A-Za-z0-9_])`).test(text);

/**
 * Why one `coverage.json` check entry names nothing real, or undefined when it does. The entry is
 * `<step>[:<suite>[ <locator>]]` (DEL-79):
 * - the step is one of the gate's steps;
 * - the suite is a crate folder (`crates/<suite>` or `<suite>`), a path that exists, a script `scripts/verify-<suite>.mjs`,
 *   or a name the step's own entry files list as a quoted string;
 * - a locator that is a path (`tests/figures.rs`) must exist under the crate, or under the checkout when the suite is no crate;
 *   after it, `::name` must occur in that file (`tests/figures.rs::a_test`);
 * - a locator that is no path is a module path (`layout::stats`) that must resolve in the crate's `src/`, as a file or folder
 *   per segment, and where a segment has no file it must occur in the text of the module found so far; for a suite that is no
 *   crate it must occur in the suite's script.
 */
export function coverageEntryProblem({ cwd, check, steps = STEPS }) {
	const [head, ...restParts] = check.split(" ");
	const [stepId, suite] = head.split(":");
	const step = steps.find((candidate) => candidate.id === stepId);
	if (!step) return `unknown check '${check}'`;
	if (suite === undefined) return restParts.length ? `'${check}' names a locator but no suite` : undefined;
	const locator = restParts.join(" ").trim();
	const crate = [join("crates", suite), suite].find((dir) => existsSync(join(cwd, dir, "Cargo.toml")));
	const script = join("scripts", `verify-${suite}.mjs`);
	let base;
	if (crate) base = crate;
	else if (existsSync(join(cwd, suite))) base = suite;
	else if (existsSync(join(cwd, script))) base = script;
	else {
		// A journey or set the step's own script lists by name, as a quoted string ("agent").
		const quoted = new RegExp(`["'\`]${suite.replace(/[^\w-]/g, "\\$&")}["'\`]`);
		base = stepEntries(step).find((entry) => !/[*?]/.test(entry) && existsSync(join(cwd, entry)) && quoted.test(textAt(join(cwd, entry)) ?? ""));
		if (!base) return `'${check}': no crate, path, script verify-${suite}.mjs or quoted name in the entry files of step '${stepId}'`;
	}
	if (!locator) return undefined;
	const [place, ...names] = locator.split("::");
	if (/\.[a-z]+$/.test(place)) {
		const file = crate ? join(base, place) : existsSync(join(cwd, base)) && statSync(join(cwd, base)).isDirectory() ? join(base, place) : place;
		const text = textAt(join(cwd, file));
		if (text === undefined) return `'${check}': ${file} does not exist`;
		const absent = names.filter((name) => !occurs(text, name));
		return absent.length ? `'${check}': ${absent.join("::")} does not occur in ${file}` : undefined;
	}
	const segments = [place, ...names];
	if (!crate) {
		const text = textAt(join(cwd, base)) ?? "";
		const absent = segments.filter((segment) => !occurs(text, segment));
		return absent.length ? `'${check}': '${absent.join("::")}' does not occur in ${base}` : undefined;
	}
	let dir = join(cwd, base, "src");
	let text = "";
	for (const segment of segments) {
		if (existsSync(join(dir, `${segment}.rs`))) {
			text = readFileSync(join(dir, `${segment}.rs`), "utf8");
			dir = join(dir, segment);
		} else if (existsSync(join(dir, segment))) {
			dir = join(dir, segment);
			text = textAt(dir) ?? "";
		} else if (!text || !occurs(text, segment)) return `'${check}': module '${segment}' is no file or folder under ${base}/src and does not occur in the module before it`;
	}
	return undefined;
}

/** Every check entry of `coverage.json` that names nothing real, as `[{ id, check, problem }]`. */
export function coverageProblems({ cwd, items = loadCoverage(cwd).items, only, steps = STEPS }) {
	return Object.entries(items)
		.filter(([id]) => !only || only.includes(id))
		.flatMap(([id, entry]) => (entry.checks ?? []).flatMap((check) => {
			const problem = coverageEntryProblem({ cwd, check, steps });
			return problem ? [{ id, check, problem }] : [];
		}));
}

export function coverage({ cwd, pr }) {
	const map = loadCoverage(cwd).items;
	const named = pr ? [...pr.items] : [];
	const missing = [];
	for (const id of named) {
		const entry = map[id];
		if (!entry) {
			missing.push(id);
			continue;
		}
		if (!(entry.checks?.length || entry.judged)) missing.push(id);
	}
	for (const { id, problem } of coverageProblems({ cwd, only: named })) missing.push(`${id} → ${problem}`);
	if (missing.length) return fail(`Name a check or mark judged in tools/rv-gate/coverage.json (DEL-79): ${missing.join(", ")}`);
	return pass({ items: named.length, mapped: Object.keys(map).length });
}

// ---------------------------------------------------------------------------------------------
// DEL-38: every dev commit since the last prod merge has a receipt with a matching tree.

export function audit({ cwd, head = "origin/pre-prod", mode, candidate }) {
	const prod = sh("git", ["rev-parse", "origin/prod"], { cwd });
	// A diagnostic full is not promoted yet: audit actual pre-prod, never demand a prior
	// release receipt for the source whose first complete full is still running.
	const top = sh("git", ["rev-parse", mode === "full" ? "origin/pre-prod" : head], { cwd });
	const commits = sh("git", ["rev-list", "--first-parent", `${prod}..${top}`], { cwd }).split("\n").filter(Boolean);
	const receipts = listReceipts();
	const merged = new Map();
	const problems = [];
	// Since DEL-81 a release receipt covers its whole batch; the tree must match at its head.
	const batched = new Map();
	for (const receipt of receipts) {
		for (const note of receiptNotes(receipt.id)) {
			if (note.kind === "merge" && note.commit) merged.set(note.commit, { receipt, note });
			if (note.kind === "bootstrap" && note.commit) merged.set(note.commit, { receipt, note });
			if (["release", "promotion"].includes(note.kind) && note.commit) {
				// Promotion already checked the then-current registry. Preserve that cohort when
				// later checks are added; older notes predate the snapshot and use their receipt.
				const required = Object.hasOwn(note, "required") ? note.required : receipt.steps?.map(step => step.id);
				const cohort = Array.isArray(required) ? required.map(id => ({ id })) : [];
				if (receipt.result !== "pass" || (note.kind === "promotion" && (receipt.kind === "affected" ? historicalAffectedProblems({cwd, receipt, proof: note.affectedProof, read: readReceipt}) : fullPromotionProblems(receipt, cohort, {policy: receipt.phasePolicy ?? {phase: "launch", rotation: []}, now: receipt.at, history: receipts,
					...(Object.hasOwn(note, "stepDependencies") && {stepDependencies: note.stepDependencies})})).length)) {
					problems.push(`${note.commit.slice(0, 10)} has no passing ${note.kind} receipt`);
					continue;
				}
				merged.set(note.commit, { receipt, note });
				for (const commit of note.commits ?? []) if (commit !== note.commit) batched.set(commit, receipt);
			}
		}
	}
	// Judgment is pre-prod provenance, not a passing receipt. It may be recorded while its
	// originating run is still pending, and it covers only the exact first-parent fast-forward.
	const judgments = [], judged = new Set();
	const preprodAudit = !candidate && (mode === "full" || (mode === undefined && head === "origin/pre-prod"));
	const directory = join(evidenceDir(), "receipts");
	if (preprodAudit && existsSync(directory)) for (const file of readdirSync(directory)) {
		const match = /^((?:full|affected)-\d{8}T\d{6}Z-[0-9a-f]{10})\.lead-judgment-promotion\.json$/.exec(file);
		if (!match) continue;
		try {
			const note = JSON.parse(readFileSync(join(directory, file), "utf8"));
			if (!commits.includes(note.commit)) continue;
			const legacy = typeof note.authority === "string" && /\blead\b/i.test(note.authority)
				&& /\bDaniel\b/i.test(note.authority) && /10 Nov 2026/i.test(note.authority) && /17:25\s*CET/i.test(note.authority);
			const structured = note.actor === "release lead" && typeof note.ruling === "string"
				&& /\bDaniel\b/i.test(note.ruling) && /10 Nov 2026/i.test(note.ruling) && /17:25\s*CET/i.test(note.ruling);
			const validSteps = Array.isArray(note.steps) && note.steps.length > 0
				&& note.steps.every(step => typeof step?.id === "string" && step.id && ["pass", "fail", "unrun", "pending"].includes(step.status))
				&& new Set(note.steps.map(step => step.id)).size === note.steps.length;
			if (note.receipt !== match[1] || note.kind !== "lead-judgment-promotion" || !/^[0-9a-f]{40}$/.test(note.base ?? "")
				|| !/^[0-9a-f]{40}$/.test(note.commit ?? "") || !/^[0-9a-f]{40}$/.test(note.tree ?? "")
				|| !note.receipt.endsWith(`-${note.commit.slice(0, 10)}`) || note.prodEligible !== false
				|| !["pass", "fail", "in-progress"].includes(note.resultAtPromotion) || !(legacy || structured) || !validSteps
				|| sh("git", ["rev-parse", `${note.commit}^{tree}`], {cwd}) !== note.tree) continue;
			const recorded = receipts.find(receipt => receipt.id === note.receipt);
			if (recorded && (recorded.commit !== note.commit || recorded.tree !== note.tree || recorded.kind !== note.receipt.split("-")[0])) continue;
			const lineage = sh("git", ["rev-list", "--first-parent", note.commit], {cwd}).split("\n");
			const index = lineage.indexOf(note.base);
			if (index < 0) continue;
			for (const commit of index === 0 ? [note.commit] : lineage.slice(0, index)) judged.add(commit);
			judgments.push({receipt:note.receipt, base:note.base, commit:note.commit, tree:note.tree, authority:note.authority,
				...(structured && {actor:note.actor, ruling:note.ruling}), resultAtPromotion:note.resultAtPromotion, steps:note.steps});
		} catch { /* An unreadable or unbound note never supplies provenance. */ }
	}
	const bootstrap = loadBootstrap(cwd);
	for (const commit of commits) {
		if (bootstrap[commit] || batched.has(commit)) continue;
		const record = merged.get(commit);
		if (!record) {
			if (judged.has(commit)) continue;
			problems.push(`${commit.slice(0, 10)} has no gate or release receipt`);
			continue;
		}
		const tree = sh("git", ["rev-parse", `${commit}^{tree}`], { cwd });
		if (record.receipt.tree !== tree) problems.push(`${commit.slice(0, 10)} tree differs from receipt ${record.receipt.id}`);
	}
	return problems.length ? fail(problems.join("; ")) : pass({ commits: commits.length, ...(judgments.length && {judgments}) });
}

/** Commits merged into dev by hand under DEL-07's bootstrap exception, with their reason. */
export function loadBootstrap(cwd) {
	const path = join(cwd, "tools/rv-gate/bootstrap-merges.json");
	return existsSync(path) ? JSON.parse(readFileSync(path, "utf8")) : {};
}

// ---------------------------------------------------------------------------------------------
// DEL-35, DEL-71: the prod step fails when a design entry has no verdict or the batch's
// adversarial review did not run.

export function reviews({ changes = [], reviewRecords = [] }) {
	const verdicts = new Set(reviewRecords.filter((r) => r.kind === "design-verdict").map((r) => r.receipt));
	const missing = changes.filter((entry) => entry.class === "design" && !verdicts.has(entry.receipt));
	const adversarial = reviewRecords.some((r) => r.kind === "adversarial-review");
	const problems = [];
	if (missing.length) problems.push(`design entries without a verdict: ${missing.map((e) => e.pr ?? e.commit).join(", ")}`);
	if (!adversarial) problems.push("the batch's adversarial review was not recorded");
	return problems.length ? fail(problems.join("; ")) : pass();
}

export { parsePrBody };

/** Tracked files with changes in `cwd`'s work tree (`git status --porcelain --untracked-files=no`). */
export function trackedChanges(cwd) {
	return sh("git", ["status", "--porcelain", "--untracked-files=no"], { cwd })
		.split("\n")
		.filter(Boolean)
		// `sh` trims the output, so the first entry may have lost its leading space: strip the status by pattern.
		.map((entry) => entry.replace(/^\S+\s+/, "").replace(/^.* -> /, ""));
}

/**
 * A passing build step that left tracked files changed fails, naming each file: the build must not
 * rewrite what the commit holds, and the `checkout -f` before a run would otherwise hide the drift
 * (review 1e should-fix 30). Other results pass through.
 */
export function buildTreeResult(result, cwd, changes = trackedChanges) {
	if (result.status !== "pass") return result;
	const dirty = changes(cwd);
	if (!dirty.length) return result;
	return { ...result, status: "fail", reason: `The build rewrote ${dirty.length} tracked file(s): ${dirty.join(", ")}. Commit the regenerated output or fix the generator.` };
}
