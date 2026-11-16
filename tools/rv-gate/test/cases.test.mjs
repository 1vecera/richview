// Host reason (ARC-03): prevent case selection from silently weakening a complete check.
import test from "node:test";
import assert from "node:assert/strict";
import {verificationShard} from "../lib/shards.mjs";
import {filterCases, forbidInheritedCases} from "../lib/cases.mjs";

test("no case filter leaves the original array and output shape unchanged", () => {
	const cases = [{id:"bar"},{id:"line"}];
	assert.deepEqual(filterCases(cases,{key:value=>value.id,env:{}}),{cases});
	assert.equal(filterCases(cases,{key:value=>value.id,env:{}}).cases,cases);
});
test("an admitted filter selects exact keys in source order and retains the authoritative inventory", () => {
	const cases = [{id:"bar"},{id:"line"},{id:"pie"}];
	assert.deepEqual(filterCases(cases,{key:value=>value.id,env:{RICHVIEW_VERIFY_PARENT:"affected-interaction",RICHVIEW_VERIFY_CASES:'["pie","bar"]'}}),
		{cases:[cases[0],cases[2]],scope:{cohort:["bar","line","pie"],selected:["bar","pie"]}});
});
test("unadmitted, malformed, duplicate, unknown or empty selections fail closed", () => {
	const cases=["bar","line"];
	for (const env of [{RICHVIEW_VERIFY_CASES:'["bar"]'}, {RICHVIEW_VERIFY_PARENT:"affected-interaction",RICHVIEW_VERIFY_CASES:'bad'}, {RICHVIEW_VERIFY_PARENT:"affected-interaction",RICHVIEW_VERIFY_CASES:'[]'}, {RICHVIEW_VERIFY_PARENT:"affected-interaction",RICHVIEW_VERIFY_CASES:'["bar","bar"]'}, {RICHVIEW_VERIFY_PARENT:"affected-interaction",RICHVIEW_VERIFY_CASES:'["unknown"]'}])
		assert.throws(()=>filterCases(cases,{key:value=>value,env}));
});

test("full and launch invocations refuse an inherited filter", () => {
	for(const [command,phase] of [["full","prelaunch"],["prod","prelaunch"],["fast","launch"],["affected","launch"]]) assert.throws(()=>forbidInheritedCases(command,phase,{RICHVIEW_VERIFY_CASES:'["bar"]'}),/cannot be set/);
	assert.doesNotThrow(()=>forbidInheritedCases("affected","prelaunch",{}));
	assert.doesNotThrow(()=>forbidInheritedCases("full","launch",{}));
});

test("the no-filter chart shard report bytes equal the previous selection", () => {
	const cases=[{id:"0:bar"},{id:"1:line"},{id:"2:pie"}], key=value=>value.id;
	const before=verificationShard(cases,{key});
	const filtered=filterCases(cases,{key,env:{}});
	const after=verificationShard(filtered.cases,{key});
	assert.equal(JSON.stringify(after),JSON.stringify(before));
	assert.equal("scope" in filtered,false);
});


test("each case-aware suite requires its own dispatcher admission", () => {
	const env={RICHVIEW_VERIFY_PARENT:"affected-docs",RICHVIEW_VERIFY_CASES:'["desktop"]'};
	assert.deepEqual(filterCases(["main","desktop"],{key:value=>value,env,parent:"affected-docs"}).scope.selected,["desktop"]);
	assert.throws(()=>filterCases(["main","desktop"],{key:value=>value,env}),/dispatcher/);
});
