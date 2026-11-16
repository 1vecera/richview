// Tests for the fixtures the captures step reads (DEL-27): a missing fixture is an error, never a substitute.
import assert from "node:assert/strict";
import test from "node:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { everyBlockFixture, typographySpecimen } from "../lib/captures.mjs";

test("the every-block fixture is examples/every-block, and no other report stands in for it", () => {
	const cwd = mkdtempSync(join(tmpdir(), "captures-"));
	try {
		mkdirSync(join(cwd, "examples/first-report"), { recursive: true });
		writeFileSync(join(cwd, "examples/first-report/report.rmd"), "# A report\n");
		assert.throws(() => everyBlockFixture(cwd), /every-block fixture examples\/every-block\/report\.rmd is missing/);
		assert.throws(() => typographySpecimen(cwd), /typography specimen .* is missing/);
		mkdirSync(join(cwd, "examples/every-block"), { recursive: true });
		writeFileSync(join(cwd, "examples/every-block/report.rmd"), "# Every block\n");
		assert.equal(everyBlockFixture(cwd), "examples/every-block");
	} finally {
		rmSync(cwd, { recursive: true, force: true });
	}
});
