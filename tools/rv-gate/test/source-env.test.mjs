// Host reason (ARC-03): prove hotfix checks cannot inherit a probe's binary, site or engine.
import test from "node:test";
import assert from "node:assert/strict";
import {SOURCE_OVERRIDES} from "../lib/source-env.mjs";
import {hotfixEnvironment} from "../lib/hotfix-run.mjs";

test("hotfix uses the common clean source environment and its explicit Chromium budget",t=>{
	const before=Object.fromEntries(SOURCE_OVERRIDES.map(name=>[name,process.env[name]]));
	t.after(()=>{for(const name of SOURCE_OVERRIDES)if(before[name]===undefined)delete process.env[name];else process.env[name]=before[name];});
	for(const name of SOURCE_OVERRIDES)process.env[name]=`probe-${name}`;
	const env=hotfixEnvironment("/shared/target");
	for(const name of SOURCE_OVERRIDES)if(!["RICHVIEW_VERIFY_BROWSER","RICHVIEW_VERIFY_WORKERS"].includes(name))assert.equal(env[name],undefined,name);
	assert.equal(env.RICHVIEW_VERIFY_BROWSER,"chromium");assert.equal(env.RICHVIEW_VERIFY_WORKERS,"2");assert.equal(env.CARGO_TARGET_DIR,"/shared/target");
	for(const name of SOURCE_OVERRIDES)assert.equal(process.env[name],`probe-${name}`,`${name} in caller`);
});
