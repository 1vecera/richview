// S27-780: only a complete passing API verification may authorize indirect edges.
import assert from "node:assert/strict";
import test from "node:test";
import {readFileSync} from "node:fs";
import {indirectFindings} from "../indirect.mjs";
import {dependencyFindings} from "../graph.mjs";
const policy = () => JSON.parse(readFileSync(new URL("../rules.json", import.meta.url), "utf8"));
const triple = {consumer:"richview-core",producer:"richview-chart",contract:"chart.registry"};
const report = indirect => ({schema:"interface-v1",status:"pass",findings:[],indirect,checked:["chart.registry"],changes:[]});
const codes = (value,rules=policy()) => indirectFindings(value,rules).map(f=>f.code);

test("verified explicit lower-layer public bindings pass", () => {
	const rules=policy(); rules.indirect.push(triple);
	assert.deepEqual(indirectFindings(report([triple]),rules), []);
	assert.deepEqual(indirectFindings(report([]),policy()), []);
});

test("an observed public contract needs an exact policy triple", () => {
	assert.ok(codes(report([triple])).includes("dependency.indirect.unlisted"));
	const rules=policy(); rules.indirect.push(triple);
	assert.ok(codes(report([{...triple,contract:"chart.private"}]),rules).includes("dependency.indirect.unlisted"));
});

test("listing an indirect contract cannot bypass the crate direction", () => {
	const reverse={consumer:"richview-chart",producer:"richview-core",contract:"document.api"};
	const rules=policy(); rules.indirect.push(reverse);
	assert.ok(codes(report([reverse]),rules).includes("dependency.indirect.direction"));
});

test("missing, failed, unknown or contradictory API reports fail closed", () => {
	for (const value of [undefined,null,{}, {...report([]),schema:"interface-v2"}, {...report([]),status:"warning"}, {...report([]),status:"fail"}, {...report([]),findings:[{code:"api.missing"}]}, {...report([]),findings:null}, {...report([]),indirect:null}]) {
		assert.ok(indirectFindings(value,policy()).length,JSON.stringify(value));
	}
});

test("partial failed reports cannot authorize their otherwise valid bindings", () => {
	const rules=policy();rules.indirect.push(triple);
	assert.ok(codes({...report([triple]),status:"fail",findings:[{code:"api.changed"}]},rules).includes("dependency.api.failed"));
});

test("malformed, duplicated and unknown endpoint bindings are rejected", () => {
	for (const entries of [[{}],[{...triple,privatePath:"internal.rs"}],[{...triple,producer:"missing"}],[triple,triple]]) {
		assert.ok(codes(report(entries)).includes("dependency.api.schema"));
	}
});

test("the approved ARC-07 runtime artifact binding grants no app dependency on the runtime crate", () => {
	const bindings=[{consumer:"richview-app",producer:"richview-runtime",contract:"runtime.embed-files"},{consumer:"richview-build",producer:"richview-runtime",contract:"runtime.embed-files"}];
	assert.deepEqual(indirectFindings(report(bindings),policy()),[]);
	const packages=[{name:"richview-app",targets:[{kind:["lib"]}],dependencies:[{name:"richview-runtime",kind:null}]},{name:"richview-runtime",targets:[{kind:["lib"]}],dependencies:[]}];
	assert.ok(dependencyFindings(packages,policy()).some(f=>f.code === "dependency.edge"));
});

test("the runtime artifact exception cannot authorize another contract, consumer or producer", () => {
	for(const patch of [{contract:"runtime.private"},{consumer:"richview-core"},{producer:"richview-app"},{exception:"ARC-01"}]) {
		const rules=policy(); Object.assign(rules.indirect[0],patch);
		assert.ok(codes(report([]),rules).includes("dependency.policy.schema"));
	}
	const rules=policy();delete rules.indirect[0].exception;
	assert.ok(codes(report([{consumer:"richview-app",producer:"richview-runtime",contract:"runtime.embed-files"}]),rules).includes("dependency.indirect.direction"));
});
