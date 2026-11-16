// The coverage step names an entry whose file, suite or test does not exist (review 1l should-fix 7, DEL-79).
import assert from "node:assert/strict";
import test from "node:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { coverage, coverageEntryProblem, coverageProblems } from "../lib/checks.mjs";

/** A small checkout: one crate with a test file and a module tree, one verify script, one journeys script. */
function checkout(items) {
	const cwd = mkdtempSync(join(tmpdir(), "coverage-"));
	const put = (path, text) => {
		mkdirSync(dirname(join(cwd, path)), { recursive: true });
		writeFileSync(join(cwd, path), text);
	};
	put("crates/richview-core/Cargo.toml", "[package]\n");
	put("crates/richview-core/tests/figures.rs", "#[test]\nfn a_figure_keeps_its_caption() {}\nmod radius { fn dialog_is_square() {} }\n");
	put("crates/richview-core/src/lib.rs", "");
	put("crates/richview-core/src/layout/mod.rs", "mod stats;\n");
	put("crates/richview-core/src/layout/stats.rs", "fn median() {}\n");
	put("scripts/verify-controls.mjs", "");
	put("tools/rv-gate/journeys.mjs", 'export const SETS = { release: ["author", "agent"] };\n');
	put("tools/rv-gate/coverage.json", JSON.stringify({ items: items ?? {} }));
	return cwd;
}

const STEPS = ["rust-tests", "chromium", "journeys", "tool-tests"].map((id) => ({ id, entries: id === "journeys" ? ["tools/rv-gate/journeys.mjs"] : [] }));
const problem = (cwd, check) => coverageEntryProblem({ cwd, check, steps: STEPS });

test("an entry that names a missing file, test, module or suite is a problem; one that names real things is not", () => {
	const cwd = checkout();
	try {
		assert.equal(problem(cwd, "rust-tests:richview-core tests/figures.rs"), undefined);
		assert.match(problem(cwd, "rust-tests:richview-core tests/nonexistent.rs"), /crates\/richview-core\/tests\/nonexistent\.rs does not exist/);
		assert.equal(problem(cwd, "rust-tests:richview-core tests/figures.rs::a_figure_keeps_its_caption"), undefined);
		assert.equal(problem(cwd, "rust-tests:richview-core tests/figures.rs::radius::dialog_is_square"), undefined);
		assert.match(problem(cwd, "rust-tests:richview-core tests/figures.rs::a_test_nobody_wrote"), /a_test_nobody_wrote does not occur in crates\/richview-core\/tests\/figures\.rs/);
		assert.equal(problem(cwd, "rust-tests:richview-core layout::stats"), undefined);
		assert.equal(problem(cwd, "rust-tests:richview-core layout::median"), undefined, "a segment with no file of its own must occur in the module before it");
		assert.match(problem(cwd, "rust-tests:richview-core layout::absent"), /module 'absent'/);
		assert.match(problem(cwd, "rust-tests:richview-core nowhere"), /module 'nowhere'/);
		assert.match(problem(cwd, "rust-tests:richview-nothing tests/figures.rs"), /no crate, path, script/);
		assert.equal(problem(cwd, "chromium:controls"), undefined);
		assert.match(problem(cwd, "chromium:showcase"), /verify-showcase\.mjs/);
		assert.equal(problem(cwd, "journeys:agent"), undefined);
		assert.match(problem(cwd, "journeys:cowork"), /quoted name/);
		assert.equal(problem(cwd, "tool-tests:tools/rv-gate"), undefined);
		assert.match(problem(cwd, "unknown-step:x"), /unknown check/);
		assert.equal(problem(cwd, "chromium"), undefined, "a bare step needs no suite");
	} finally {
		rmSync(cwd, { recursive: true, force: true });
	}
});

test("the coverage step fails an item whose check names nothing real, naming the entry", () => {
	const cwd = checkout({ "STY-01": { checks: ["rust-tests:richview-core tests/figures.rs"] }, "STY-02": { checks: ["rust-tests:richview-core tests/nonexistent.rs"] }, "STY-03": { judged: true } });
	try {
		assert.equal(coverage({ cwd, pr: { items: ["STY-01", "STY-03"] } }).status, "pass");
		const failed = coverage({ cwd, pr: { items: ["STY-02"] } });
		assert.equal(failed.status, "fail");
		assert.match(failed.reason, /STY-02 → 'rust-tests:richview-core tests\/nonexistent\.rs': .*does not exist/);
		assert.deepEqual(coverageProblems({ cwd }).map((entry) => entry.id), ["STY-02"], "every entry can be checked without a PR");
	} finally {
		rmSync(cwd, { recursive: true, force: true });
	}
});
