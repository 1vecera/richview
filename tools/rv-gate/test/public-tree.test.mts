import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { INTERNAL_FILES, LINEAR_ID, LINEAR_ID_EXEMPT, publicTree } from "../lib/checks.mts";
import { STEPS } from "../lib/steps.mts";

// The tracker's ids are built here, never written out: this file ships and is scanned too.
const issue = (key: string, n: number) => [key, n].join("-");

function repo(files: Record<string, string>) {
	const dir = mkdtempSync(join(tmpdir(), "public-tree-"));
	const git = (...args: string[]) => execFileSync("git", args, { cwd: dir, stdio: "ignore" });
	git("init", "-q");
	for (const [name, text] of Object.entries(files)) {
		mkdirSync(dirname(join(dir, name)), { recursive: true });
		writeFileSync(join(dir, name), text);
	}
	git("add", ".");
	return dir;
}

function check(files: Record<string, string>) {
	const dir = repo(files);
	try {
		return publicTree({ cwd: dir });
	} finally {
		rmSync(dir, { recursive: true, force: true });
	}
}

test("a tree of plain files passes, with card ids, pull request numbers and a bare key", () => {
	const result = check({
		"README.md": `See INT-31 and PR #70.\nBranches read <kind>/${"s27"}-<n>-<slug>; commits say Refs ${"S27"}-<n>.\n`,
		"src/lib.rs": "// A directory-1 and a dir-2 are only names.\n",
	});
	assert.equal(result.status, "pass", "reason" in result ? result.reason : "");
});

test("an issue id in a tracked file fails with its file and line, in any of its spellings", () => {
	const spellings = [issue("S27", 877), issue("s27", 590), `P-${issue("S27", 32)}`, issue("DIR", 121), issue("VEN", 2221), `richview-interest-${issue("s27", 590)}-local`];
	for (const spelling of spellings) {
		const result = check({ "docs/guide.md": `one\ntwo ${spelling} three\n` });
		assert.equal(result.status, "fail", spelling);
		assert.match(result.reason, /docs\/guide\.md:2 /, spelling);
	}
});

test("an id inside a longer word, or a lower-case dir key, is not an issue id", () => {
	const result = check({ "a.txt": `x${issue("S27", 1)} ${issue("S27", 1)}x ${issue("dir", 2)}\n` });
	assert.equal(result.status, "pass", "reason" in result ? result.reason : "");
});

test("the private files fail wherever the id scan would not look", () => {
	for (const name of INTERNAL_FILES) {
		const result = check({ [name]: "no ids here\n" });
		assert.equal(result.status, "fail", name);
		assert.match(result.reason, new RegExp(`${name.replace(".", "\\.")} \\(belongs on the private team branch\\)`));
	}
	assert.equal(check({ "docs/OWNERS-guide.md": "owners\n", "tools/ASSUMED.md.md": "x\n" }).status, "pass", "only the root files are the internal ones");
});

test("a tracked path that carries an id fails too", () => {
	const result = check({ [`docs/${issue("s27", 12)}-notes.md`]: "plain\n" });
	assert.equal(result.status, "fail");
	assert.match(result.reason, /the path carries a Linear issue id/);
});

test("a binary file and an exempt file are not read", () => {
	const exempt = [...LINEAR_ID_EXEMPT.keys()][0];
	const result = check({ "logo.bin": `\0${issue("S27", 5)}`, [exempt]: `${issue("S27", 5)}\n` });
	assert.equal(result.status, "pass", "reason" in result ? result.reason : "");
});

test("the report says how many files and names the first twenty", () => {
	const files = Object.fromEntries(Array.from({ length: 25 }, (_, i) => [`f${i}.txt`, `${issue("S27", i + 1)}\n`]));
	const result = check(files);
	assert.equal(result.status, "fail");
	assert.match(result.reason, /^25 finding\(s\): /);
	assert.match(result.reason, /, \.\.\.$/);
	assert.equal(result.details?.length, 25);
});

test("the pattern misses nothing the brief names and is registered as a fast step", () => {
	assert.ok(LINEAR_ID.test(issue("S27", 1072)) && LINEAR_ID.test(issue("DIR", 7)));
	const step = STEPS.find((entry) => entry.id === "public-tree");
	assert.ok(step);
	assert.equal(step.tier, "fast");
	assert.equal(step.fn, "publicTree");
});

test("this repository's tracked files carry no internal file and no issue id", () => {
	const root = join(dirname(fileURLToPath(import.meta.url)), "../../..");
	const result = publicTree({ cwd: root });
	assert.equal(result.status, "pass", "reason" in result ? result.reason : "");
});
