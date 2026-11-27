// Tests for the npm advisory check of the dependency gate (ARC-68).

import assert from "node:assert/strict";
import test from "node:test";
import { ACCEPTED, advisoriesIn, checkReport } from "../npm-advisories.mts";

const advisory = (name: string, ghsa: string, severity = "high") => ({ source: 1, name, url: `https://github.com/advisories/${ghsa}`, severity, title: `${name} problem`, range: "<9" });

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
	assert.equal(checkReport(report, []).length, 2, "nothing is accepted without a rule");
	assert.equal(checkReport(report, [{ ghsa: "GHSA-xcx6-jx8p-x3h5", package: "undici" }]).length, 2);
});

test("the default list accepts picomatch's ReDoS for picomatch only, and each entry says why and when it goes", () => {
	const picomatch = advisory("picomatch", "GHSA-fc5q-39xc-vjpf");
	const report = {
		vulnerabilities: {
			picomatch: { via: [picomatch] },
			tinyglobby: { via: ["picomatch"] },
			"@vscode/vsce": { via: ["tinyglobby"] },
		},
	};
	assert.deepEqual(checkReport(report), []);
	const elsewhere = { vulnerabilities: { micromatch: { via: [advisory("micromatch", "GHSA-fc5q-39xc-vjpf")] } } };
	assert.equal(checkReport(elsewhere).length, 1, "the same id filed against another package still fails");
	for (const rule of ACCEPTED) {
		assert.match(rule.ghsa, /^GHSA-[0-9a-z]{4}-[0-9a-z]{4}-[0-9a-z]{4}$/);
		assert.ok(rule.package && rule.reason && rule.drop, `${rule.ghsa} names its package, reason and drop condition`);
	}
});

test("a clean report passes and a failed audit fails", () => {
	assert.deepEqual(checkReport({ vulnerabilities: {} }), []);
	assert.equal(checkReport({ error: { code: "ENOTFOUND", summary: "registry unreachable" } }).length, 1);
});
