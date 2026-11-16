// Combined-state diagnostics (DEL-81, Daniel 29 Oct). pre-prod follows dev before checks;
// checks and reviews report defects and drive fixes rather than gate that move.
//
// Step selection is a pure function of the registry and the changed paths, so the tests drive it
// without Git or Cargo.

import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { MF_ENV, missingEnv } from "./connectors.mjs";
import { dirname, join, posix, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { affected } from "./impact.mjs";
import { ITEM_ID, NEAR_ID, parsePrBody } from "./pr.mjs";

/**
 * Paths whose change touches no shipped artifact: docs, tools, agent and repository settings,
 * and suite scripts, which run their own step. Shared harness scripts count as product: every
 * suite reads them.
 */
const NOT_PRODUCT = [/^docs\/(?!surface\/|public\/|RMD-SPEC\.md$)/, /^tools\//, /^\.github\//, /^\.claude\//, /^[^/]+\.md$/, /^LICENSE/, /^scripts\/verify-/];

/** Full-tier steps the release run adds when the batch touches what they check. */
export const RELEASE_EXTRAS = {
	exports: [/export/, /print\.css$/, /^scripts\/verify-exports\.mjs$/, /^crates\/richview-core\/src\/(figure|render)\.rs$/],
	assets: [/asset/, /^scripts\/verify-assets\.mjs$/],
	packaging: [/^packages\/vscode\//, /^crates\/richview-editor\//, /^scripts\/verify-packaging\.py$/, /^scripts\/vscode-notices\.cjs$/],
};

/** Steps that only mean something on one PR: its body's fields. */
const PR_ONLY = new Set(["pr-fields"]);

const CHEAP_FIRST = ["pins", "entrypoints", "secrets", "arch", "weakening", "retired-tests", "coverage"];

function scripts(step) {
	return (step.run ?? []).map((command) => command.entry ?? command.args?.find((arg) => /\.(m?js|cjs|py|ts)$/.test(arg))).filter(Boolean);
}

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const readRepo = (path) => {
	const file = join(REPO_ROOT, path);
	return existsSync(file) ? readFileSync(file, "utf8") : null;
};

/**
 * What a step reads besides the product: its scripts, the local modules they import (transitively), and
 * the gate fixture folders they name. A batch that changes any of them runs the step (DEL-21), so a
 * changed budget in `lib/perf.mjs` or a changed fixture is exercised before it reaches pre-prod.
 * Returns `{ files, folders }` of repo-relative paths; `read` returns a file's text or null.
 */
export function stepSources(step, read = readRepo) {
	const files = new Set();
	const folders = new Set();
	const queue = [...scripts(step), ...(step.entries ?? []).filter(entry => !/[*?[]/.test(entry))].map((script) => posix.normalize(script));
	if (step.fn) {
		const special = {captures: "captures.mjs", surface: "surface.mjs", hostedSmoke: "hosted-smoke.mjs", reviews: "checks.mjs"};
		if (Object.hasOwn(special, step.fn)) {
			// The dispatcher itself changes all special checks; their distinct helpers stay narrow.
			files.add("tools/rv-gate/lib/step-functions.mjs");
			queue.push(`tools/rv-gate/lib/${special[step.fn]}`);
		} else queue.push("tools/rv-gate/lib/checks.mjs");
	}
	while (queue.length) {
		const path = queue.shift();
		if (files.has(path)) continue;
		files.add(path);
		const text = read(path);
		if (text === null) continue;
		for (const match of text.matchAll(/(?:from\s+|import\s+|import\s*\(\s*|require\s*\(\s*)["'](\.{1,2}\/[^"']+)["']/g)) {
			const target = posix.normalize(posix.join(posix.dirname(path), match[1]));
			if (/\.(m?js|cjs)$/.test(target)) queue.push(target);
		}
		for (const match of text.matchAll(/["'](tools\/rv-gate\/fixtures\/[\w.-]+)/g)) folders.add(`${match[1]}/`);
	}
	return { files: [...files], folders: [...folders] };
}

/**
 * Choose and order the release run's steps.
 *
 * - `steps`: the registry.
 * - `changed`: paths that differ between `pre-prod` and the batch head.
 * - `graph`, `inputs`: the workspace graph and include inputs from `impact.mjs`.
 *
 * Returns `{ cheap, parallel, afterBuild, alone, skipped, crates }`:
 * - `cheap`: built-in checks that take seconds, run first and in order.
 * - `parallel`: commands that need no build, run beside the build.
 * - `afterBuild`: steps that read the release build.
 * - `alone`: steps that read the release build and need the host to themselves (`alone: true`, the perf
 *   step, DEL-51): they run last, one at a time, only when everything else passed, and are never reused.
 * - `skipped`: `{ id, reason }` for each step the batch cannot affect.
 * - `crates`: the crates whose tests run.
 */
export function releasePlan(steps, changed, graph, inputs, { read = readRepo, combined = false } = {}) {
	const impact = affected(changed, graph, inputs);
	const product = changed.some((file) => !NOT_PRODUCT.some((pattern) => pattern.test(file)));
	const touched = (patterns) => changed.some((file) => patterns.some((pattern) => pattern.test(file)));
	const scriptChanged = (step) => {
		const { files, folders } = stepSources(step, read);
		return changed.some((file) => files.includes(posix.normalize(file)) || folders.some((folder) => file.startsWith(folder)));
	};
	const cheap = [];
	const parallel = [];
	const afterBuild = [];
	const alone = [];
	const skipped = [];
	let build;
	for (const step of steps) {
		if (step.unrun || PR_ONLY.has(step.id)) continue;
		if (step.tier === "full" && !(RELEASE_EXTRAS[step.id] && (combined || touched(RELEASE_EXTRAS[step.id])))) continue;
		if (step.fn && !step.needs) {
			cheap.push(step);
			continue;
		}
		if (step.id === "build") {
			build = step;
			continue;
		}
		const reason = combined ? undefined : skipReason(step, { impact, product, changed, scriptChanged });
		if (reason) {
			skipped.push({ id: step.id, reason });
			continue;
		}
		(step.alone ? alone : step.needs === "build" ? afterBuild : parallel).push(step);
	}
	cheap.sort((a, b) => order(a.id) - order(b.id));
	// The pool starts steps in this order: the build first (the suites wait on it), then the quick
	// steps without Cargo, so a failing tool test shows in seconds, not after a 20-minute clippy.
	parallel.sort((a, b) => usesCargo(a) - usesCargo(b));
	if ((afterBuild.length || alone.length) && build) parallel.unshift(build);
	return { cheap, parallel, afterBuild, alone, skipped, crates: combined ? graph.map(crate => crate.name) : [...impact.crates] };
}

function usesCargo(step) {
	return (step.run ?? []).some((command) => command.cmd === "cargo") ? 1 : 0;
}

function order(id) {
	const index = CHEAP_FIRST.indexOf(id);
	return index === -1 ? CHEAP_FIRST.length : index;
}

function skipReason(step, { impact, product, changed, scriptChanged }) {
	// A product file no crate folder or include traces (styles, fonts, docs the build renders)
	// may still be read by Rust tests at run time, so it runs the Rust steps too.
	const untracedProduct = impact.untraced.some((file) => !NOT_PRODUCT.some((pattern) => pattern.test(file)));
	const rust = impact.crates.size > 0 || untracedProduct;
	if (["fmt", "clippy", "clippy-wasm", "rust-tests", "rust-slow"].includes(step.id)) return rust ? undefined : "No Rust crate changed.";
	// The tool tests read more than tools/ (the lockdown map, RMD-SPEC, examples/every-block, the
	// editor snippets) and take seconds, so every release runs them.
	if (step.id === "tool-tests") return undefined;
	// publish-tests builds the Rust Publish server to Wasm and runs the Worker against it.
	if (step.id === "publish-tests")
		return changed.some((file) => file.startsWith("cloudflare/publish/") || file.startsWith("publications/")) || ["richview-publish-server", "richview-publish"].some((name) => impact.crates.has(name))
			? undefined
			: "Neither the publish worker, its publications nor the Publish crates changed.";
	if (step.id === "fonts") return changed.some((file) => /font/i.test(file)) ? undefined : "No font file or font script changed.";
	if (step.needs === "build") return product || scriptChanged(step) ? undefined : "The batch changes no shipped artifact and none of this step's scripts.";
	return product || scriptChanged(step) ? undefined : "The batch changes no shipped artifact.";
}

/**
 * The commits of a batch, oldest first, from `git log --first-parent --format=%H%x1f%B%x1e`.
 * Each carries the fields its message names (`Refs`, `decision`, `items`, retired tests).
 */
export function batchCommits(log) {
	const commits = log
		.split("\x1e")
		.map((record) => record.trim())
		.filter(Boolean)
		.map((record) => {
			const [commit, message = ""] = record.split("\x1f");
			const fields = parsePrBody(message);
			return { commit: commit.trim(), subject: message.split("\n")[0], fields };
		})
		.reverse();
	return commits;
}

/**
 * Fields of a whole batch, as the coverage and retired-tests checks read a PR's. Only lockdown item
 * IDs count as items (an ID in parentheses too); any other word in an `items:` or `assumed:` line is
 * counted in `malformed`, one entry per commit, with the near-miss IDs such as "FEA-CH37" named (a PR is refused for it by the PR-fields check).
 */
export function batchFields(commits) {
	const ids = (list) => list.filter((value) => ITEM_ID.test(value));
	// One entry per commit: the near-miss IDs by name (a typo must be seen) and a count of the other words.
	const malformed = commits.flatMap((c) => {
		const bad = [...c.fields.items, ...c.fields.assumed].filter((value) => !ITEM_ID.test(value));
		if (!bad.length) return [];
		const near = [...new Set(bad.filter((value) => NEAR_ID.test(value)))];
		return [{ commit: c.commit.slice(0, 10), near, words: bad.length - bad.filter((value) => NEAR_ID.test(value)).length }];
	});
	return {
		items: [...new Set(commits.flatMap((c) => ids(c.fields.items)))],
		assumed: [...new Set(commits.flatMap((c) => ids(c.fields.assumed)))],
		retired: commits.flatMap((c) => c.fields.retired),
		malformed,
	};
}

/**
 * Run `tasks` (async functions returning `{status}`) at most `limit` at a time. After the first
 * failure no task starts and `abort` is signalled, so running commands stop; with `keepGoing`
 * every task runs.
 */
export async function runPool(tasks, limit, controller, { keepGoing = false, steps = [] } = {}) {
	const results = new Array(tasks.length);
	let next = 0;
	let failed = false;
	let firstFailure;
	const worker = async () => {
		while (next < tasks.length) {
			const index = next++;
			if (failed) {
				results[index] = undefined;
				continue;
			}
			const started = Date.now();
			let result;
			try { result = await tasks[index](controller.signal); }
			catch (error) {
				const stopped = failed && controller.signal.aborted;
				result = { status: stopped ? "unrun" : "fail", reason: stopped ? `Stopped: ${firstFailure ?? "another step"} failed first.` : String(error?.message ?? error), durationMs: Date.now() - started };
			}
			const step = steps[index];
			if (step) result = { id: step.id, title: step.title, cards: step.cards, ...result };
			results[index] = result;
			if (result.status === "fail" && !failed && !keepGoing) {
				failed = true;
				firstFailure = result.id;
				controller.abort();
			}
		}
	};
	await Promise.all(Array.from({ length: Math.min(limit, tasks.length) }, worker));
	return results;
}

/**
 * Steps never reused: captures feed the baseline, and deps reads the advisory database, which
 * changes outside the tree.
 */
const NOT_REUSED = new Set(["captures", "deps"]);
/** Tracked paths no step reads. */
const UNREAD = [/^docs\/lockdown\/(?:evidence|map|answers)\//, /^tools\/rv-gate\/test\//, /^tools\/lockdown-app\/test\//];

/**
 * The input key of a release step: its commands plus the tracked files it reads, from
 * `git ls-tree -r <head>` lines (`<mode> <type> <hash>\t<path>`). Every step reads the whole tree
 * except UNREAD; tool-tests read UNREAD too, apart from the evidence. `environment` adds what
 * the tree does not hold: the installed toolchain and browser versions and the environment flags
 * that change a build or a test. Returns undefined for steps that are never reused.
 */
export function reuseKey(step, treeLines, environment = "") {
	if (step.fn || NOT_REUSED.has(step.id)) return undefined;
	// Every tracked file counts except a short list no step reads: the lockdown's records and the
	// tools' own tests. Tests read root Markdown, `.claude/` and the gate's code too (docs_match
	// reads AGENTS.md), so a product/non-product split is not safe here.
	const reads = (path) => (step.id === "tool-tests" ? !path.startsWith("docs/lockdown/evidence/") : !UNREAD.some((pattern) => pattern.test(path)));
	const hash = createHash("sha256").update(JSON.stringify(step.run ?? [])).update("\n").update(environment).update("\n").update(`mf-runtime-missing:${missingEnv(MF_ENV).length}`).update("\n");
	for (const line of treeLines) {
		const path = line.slice(line.indexOf("\t") + 1);
		if (reads(path)) hash.update(line).update("\n");
	}
	return hash.digest("hex").slice(0, 32);
}

/**
 * Which planned steps a rerun may take from an earlier pass: those whose key has a recorded
 * pass. The build is reused only when every step that reads it is, since a reused build leaves
 * no binaries behind. `passes(id, key)` returns the earlier receipt id or undefined.
 */
export function reusable(plan, keys, passes) {
	const reused = new Map();
	for (const step of [...plan.parallel, ...plan.afterBuild]) {
		if (step.id === "build") continue;
		const receipt = keys.get(step.id) && passes(step.id, keys.get(step.id));
		if (receipt) reused.set(step.id, receipt);
	}
	const build = plan.parallel.find((step) => step.id === "build");
	if (build && [...plan.afterBuild, ...plan.alone].every((step) => reused.has(step.id))) {
		const receipt = keys.get("build") && passes("build", keys.get("build"));
		if (receipt) reused.set("build", receipt);
	}
	return reused;
}

/** Environment names that change what a build or a test does; their values join the reuse key. */
const KEY_ENV = /^(RUST|CARGO_|RICHVIEW|PARITY|PLAYWRIGHT|NODE_|CC|CFLAGS|LDFLAGS|MACOSX_DEPLOYMENT_TARGET|WASM_BINDGEN|CHROMIUM|HOSTILE_|DBT_|GARAGE_)/;
const VOLATILE_ENV = new Set(["CARGO_TARGET_DIR", "RV_GATE_HOLDER", "RV_CARGO_LOCK_HELD", "CARGO_TERM_COLOR"]);

/** The environment part of a reuse key: tool versions plus the flags in `env` that matter. */
export function reuseEnvironment(env, versions) {
	const flags = Object.entries(env)
		.filter(([name]) => KEY_ENV.test(name) && !VOLATILE_ENV.has(name))
		.sort(([a], [b]) => a.localeCompare(b))
		.map(([name, value]) => `${name}=${value}`);
	return [...versions, ...flags].join("\n");
}

/** Candidate checks retain their trusted pre-prod gate; development full/release runs use dev. */
export const RELEASE_TIER_COMMANDS = ["prod", "candidate"];

/** Printed when pre-prod has no release gate yet, so the batch's own gate runs. */
export const BOOTSTRAP_NOTE = "bootstrap: pre-prod has no release gate yet, so this batch's gate runs (DEL-36 bootstrap)";

/**
 * Which copy of the gate re-runs a command: the release tier's commands use the previous release's
 * gate, the copy on origin/pre-prod, once that copy has `tools/rv-gate/lib/release.mjs`; before
 * that, and for every other command, origin/dev's. `hasReleaseGate(ref)` says whether a ref holds it
 * (review 1f note 10: release 1za ran the batch's own gate).
 */
export function gateRefFor(command, hasReleaseGate) {
	if (!RELEASE_TIER_COMMANDS.includes(command)) return { ref: "origin/dev" };
	if (hasReleaseGate("origin/pre-prod")) return { ref: "origin/pre-prod" };
	return { ref: "origin/dev", note: BOOTSTRAP_NOTE };
}
