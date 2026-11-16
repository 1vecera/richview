// S27-780: invalid policy changes must fail before they can broaden the crate graph.
import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { policyFindings } from "../policy.mjs";
const policy = () => JSON.parse(readFileSync(new URL("../rules.json", import.meta.url), "utf8"));
const codes = rules => policyFindings(rules).map(finding => finding.code);

test("the current explicit policy is well formed and acyclic", () => {
	assert.deepEqual(policyFindings(policy()), []);
});

test("a self edge or reverse edge cannot turn the policy into a cycle", () => {
	let rules = policy();
	rules.allowed["richview-chart"].push("richview-core");
	assert.ok(codes(rules).includes("dependency.policy.cycle"));
	rules = policy();
	rules.allowed["richview-core"].push("richview-core");
	assert.ok(codes(rules).includes("dependency.policy.cycle"));
});

test("unknown schemas, rule keys, crates and malformed edge lists fail closed", () => {
	for (const mutate of [
		rules => rules.schema = "interface-v2",
		rules => rules.allowEverything = true,
		rules => rules.allowed["richview-app"].push("missing-crate"),
		rules => rules.allowed["richview-app"] = "richview-core",
		rules => rules.allowed["richview-app"].push("richview-core"),
	]) {
		const rules = policy(); mutate(rules);
		assert.ok(policyFindings(rules).length, JSON.stringify(rules));
	}
});

test("engine and core purity rules enumerate normal, build and dev dependencies", () => {
	for (const mutate of [
		rules => delete rules.pure["richview-core"],
		rules => rules.pure["richview-chart"].normal = "serde",
		rules => rules.pure["richview-core"].unknown = [],
		rules => rules.pure["not-a-crate"] = {normal:[],build:[],dev:[]},
	]) {
		const rules = policy(); mutate(rules);
		assert.ok(policyFindings(rules).length);
	}
});

test("indirect contracts name existing components and a unique public contract ID", () => {
	const triple = {consumer:"richview-core",producer:"richview-chart",contract:"chart.registry"};
	const rules = policy(); rules.indirect.push(triple);
	assert.deepEqual(policyFindings(rules), []);
	for (const entry of [{...triple,contract:""},{...triple,producer:"unknown"},{...triple,privatePath:"src/internal.rs"}]) {
		const changed=policy();changed.indirect.push(entry);
		assert.ok(policyFindings(changed).length);
	}
	rules.indirect.push(triple);
	assert.ok(policyFindings(rules).length);
});
