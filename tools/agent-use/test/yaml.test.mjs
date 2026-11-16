import assert from "node:assert/strict";
import test from "node:test";
import { parseCsv } from "../lib/rmd.mjs";
import { compute } from "../lib/facts.mjs";
import { parseYaml } from "../lib/yaml.mjs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

test("the YAML reader handles the shapes rich fences use", () => {
	const doc = parseYaml("type: bar\ntitle: 'A # not a comment'  # comment\ndata:\n  inline:\n    - month: Jul\n      orders: 120\n    - { month: Aug, orders: 150 }\nencoding: { x: month, y: orders }\ncolumns:\n- key: a\n  label: A\nnote: |\n  one\n  two\n");
	assert.equal(doc.type, "bar");
	assert.equal(doc.title, "A # not a comment");
	assert.deepEqual(doc.data.inline, [{ month: "Jul", orders: 120 }, { month: "Aug", orders: 150 }]);
	assert.deepEqual(doc.encoding, { x: "month", y: "orders" });
	assert.deepEqual(doc.columns, [{ key: "a", label: "A" }]);
	assert.equal(doc.note, "one\ntwo");
});

test("CSV quoting and numbers", () => {
	assert.deepEqual(parseCsv('a,b\n"x, y",1.5\n,00123\n'), [{ a: "x, y", b: 1.5 }, { a: null, b: "00123" }]);
});

test("task figures compute from the task's own data", () => {
	const dir = join(dirname(fileURLToPath(import.meta.url)), "..", "tasks", "adoption-by-population");
	assert.equal(compute({ file: "data/share-by-population.csv", column: "anthropic_share_pct", agg: "max" }, dir), 70.9);
	assert.equal(compute({ op: "share", a: { file: "data/share-by-population.csv", where: { measure: "npm" }, column: "anthropic_share_pct" }, b: { file: "data/share-by-population.csv", where: { measure: "HN" }, column: "anthropic_share_pct" } }, dir) > 76, true);
});

test("Markdown pipe tables count as tables, fenced ones do not", async () => {
	const { markdownTables } = await import("../lib/rmd.mjs");
	const tables = markdownTables("| Topic | Gap |\n|---|---:|\n| Refusals | 9.9 |\n\n```\n| a | b |\n|---|---|\n| 1 | 2 |\n```\n");
	assert.equal(tables.length, 1);
	assert.deepEqual(tables[0].rows, [{ Topic: "Refusals", Gap: 9.9 }]);
});
