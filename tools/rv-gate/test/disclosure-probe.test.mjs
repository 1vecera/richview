import assert from "node:assert/strict";
import { test } from "node:test";
import { disclosures, finerCsv, finerJson, finerReport, refinedPositions } from "../disclosure-probe.mjs";

test("finerReport writes a block's inline rows and a metric's value finer, never its settings", () => {
	const source = [
		"```chart",
		"type: gauge",
		"data:",
		"  inline:",
		"    - {month: 3, share: 0.42, label: North}",
		"style:",
		"  bands:",
		"    - {from: 0, to: 50}",
		"```",
		"",
		"```metric",
		"value: 12480",
		"delta: 0.004",
		"digits: 3",
		"```",
		"",
		"Prose with 61.7 stays.",
	].join("\n");
	const { text, values } = finerReport(source);
	assert.deepEqual(values, [0.420051852, 12481.5407, 0.00400049383]);
	assert.match(text, /- \{month: 3, share: 0\.420051852, label: North\}/);
	assert.match(text, /- \{from: 0, to: 50\}/, "style bands are the author's settings");
	assert.match(text, /^digits: 3$/m);
	assert.match(text, /Prose with 61\.7 stays\./);
});

test("finerReport reads rows at their key's own indent, a block sequence YAML allows", () => {
	const { text, values } = finerReport(["```chart", "type: line", "data:", "  inline:", "  - {month: 2024-11, posts: 12603}", "```"].join("\n"));
	assert.deepEqual(values, [12604.5559]);
	assert.match(text, /^ {2}- \{month: 2024-11, posts: 12604\.5559\}$/m);
});

test("finerCsv writes measured columns finer and keeps placing columns, quoted lines and CRLF headers", () => {
	const values = [];
	const csv = finerCsv('call,ms\r\n1,191.4\r\n2,"160"\r\n3,86\r\n', values);
	assert.deepEqual(values, [191.42363, 86.0106173]);
	assert.equal(csv.split("\n")[1], "1,191.42363");
	assert.equal(csv.split("\n")[2], '2,"160"\r');
});

test("finerJson writes the numbers under measuring keys finer and keeps placing keys and arrays", () => {
	const values = [];
	const json = JSON.parse(finerJson('{"nodes":[{"id":3,"weight":12.5,"x":4}],"bounds":[1,2],"total":0}', values));
	assert.deepEqual(values, [12.5015432]);
	assert.deepEqual(json, { nodes: [{ id: 3, weight: 12.5015432, x: 4 }], bounds: [1, 2], total: 0 });
	assert.throws(() => finerJson("not json", []));
});

test("disclosures finds a finer value in an attribute or a payload and ignores media bytes", () => {
	const html = [
		'<td data-rv-sort-value="61.7076173">61.7</td>',
		'<script type="application/json" data-rv-payload>{"derived":[[69.1085,2],[71.1,3]]}</script>',
		'<img src="data:image/png;base64,AAAA/61.7076173AAAA">',
	].join("");
	const found = disclosures(html, [61.7076173, 69.1085309, 71.1087778]);
	assert.deepEqual(
		found.map((f) => [f.raw, f.where]),
		[
			["61.7076173", "attribute data-rv-sort-value"],
			["69.1085", "embedded chart data"],
		],
	);
});

test("refinedPositions takes a refined grid's row positions at their shown value or a base step, never finer", () => {
	// Income on a grid of units (66,000 steps); the base grid's step is 66,000 / 4,095 = 16.117.
	const payload = {
		columns: ["income", "region"],
		quant: { income: { min: 0, max: 66000, steps: 66000 }, life: { min: 37, max: 84, steps: 47 } },
		rows: [
			[9501, "Europe"],
			[10000, "Europe"],
			[10009, "Asia"],
			[10001, "Asia"],
		],
		shown: { income: [9501, 10000, 10000, 10000] },
	};
	const { violations, payload: rest } = refinedPositions(payload);
	assert.deepEqual(violations, [{ column: "income", row: 3, position: 10001, shown: 10000 }]);
	assert.deepEqual(rest.rows, [
		[null, "Europe"],
		[null, "Europe"],
		[null, "Asia"],
		[null, "Asia"],
	]);
	assert.equal(payload.rows[3][0], 10001, "the payload it was given stays as it was");
	const html = `<script type="application/json" data-rv-payload>${JSON.stringify(payload)}</script>`;
	assert.deepEqual(
		disclosures(html, [10001.2346, 10009.31]).map((d) => [d.kind, d.raw, d.where]),
		[["position", "10001", "rows of income, row 3"]],
		"the digit scan does not read a base step on a source value's fifth digit (10009)",
	);
});
