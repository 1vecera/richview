// Tests for the npm advisory check of the dependency gate (ARC-68).

import assert from "node:assert/strict";
import test from "node:test";
import { advisoriesIn, checkReport } from "../npm-advisories.mjs";

const advisory = (name, ghsa, severity = "high") => ({ source: 1, name, url: `https://github.com/advisories/${ghsa}`, severity, title: `${name} problem`, range: "<9" });

test("an advisory fails unless accepted for that package; transitive entries count once", () => {
	const report = {
		vulnerabilities: {
			undici: { via: [advisory("undici", "GHSA-aaaa-bbbb-cccc")] },
			miniflare: { via: ["undici"] },
			"@azure/msal-node": { via: [advisory("@azure/msal-node", "GHSA-xcx6-jx8p-x3h5")] },
			"@azure/identity": { via: ["@azure/msal-node"] },
		},
	};
	assert.equal(advisoriesIn(report).length, 2);
	const accepted = [{ ghsa: "GHSA-xcx6-jx8p-x3h5", package: "@azure/msal-node", reason: "test" }];
	const problems = checkReport(report, accepted);
	assert.equal(problems.length, 1);
	assert.match(problems[0], /^undici .*GHSA-aaaa-bbbb-cccc/);
	assert.equal(checkReport(report).length, 2, "nothing is accepted by default");
	assert.equal(checkReport(report, [{ ghsa: "GHSA-xcx6-jx8p-x3h5", package: "undici" }]).length, 2);
});

test("a clean report passes and a failed audit fails", () => {
	assert.deepEqual(checkReport({ vulnerabilities: {} }), []);
	assert.equal(checkReport({ error: { code: "ENOTFOUND", summary: "registry unreachable" } }).length, 1);
});
