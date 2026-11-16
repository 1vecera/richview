// Host reason (ARC-03): prove report selection and evidence without starting a browser.
import test from "node:test";
import assert from "node:assert/strict";
import {mkdirSync,mkdtempSync,rmSync,writeFileSync} from "node:fs";
import {join} from "node:path";
import {runtimeCases} from "../lib/runtime-cases.mjs";
import {verificationShard} from "../lib/shards.mjs";

function fixture(t) {
	mkdirSync(join(process.cwd(),"tmp"),{recursive:true});const root=mkdtempSync(join(process.cwd(),"tmp/runtime-cases-"));
	t.after(()=>rmSync(root,{recursive:true,force:true}));
	for(const name of ["alpha","zeta"]) {const dir=join(root,"examples",name);mkdirSync(dir,{recursive:true});writeFileSync(join(dir,"report.rmd"),"# report");}
	return root;
}

test("unfiltered runtime input options and shard bytes stay unchanged",t=>{
	const root=fixture(t), extra={forms:"forms source","wide-chart":"wide source"}, before={shard:true,extra};
	const selection=runtimeCases(root,extra,{env:{}}), after={shard:true,...selection.options};
	assert.equal(selection.options.extra,extra);assert.equal(JSON.stringify(after),JSON.stringify(before));assert.equal("scope" in selection,false);
	const names=["alpha","zeta",...Object.keys(extra)];
	const previous=process.env.RICHVIEW_VERIFY_SHARD;t.after(()=>{if(previous===undefined)delete process.env.RICHVIEW_VERIFY_SHARD;else process.env.RICHVIEW_VERIFY_SHARD=previous;});delete process.env.RICHVIEW_VERIFY_SHARD;
	assert.equal(JSON.stringify(verificationShard(names,{key:name=>name})), '{"cases":["alpha","zeta","forms","wide-chart"],"shard":{"index":0,"count":1,"cohort":["alpha","zeta","forms","wide-chart"],"selected":["alpha","zeta","forms","wide-chart"]}}');
});

test("admitted runtime filtering retains the full source inventory and only selected examples and fixtures",t=>{
	const root=fixture(t),extra={forms:"forms source","wide-chart":"wide source"};
	const result=runtimeCases(root,extra,{env:{RICHVIEW_VERIFY_PARENT:"affected-runtime",RICHVIEW_VERIFY_CASES:'["wide-chart","alpha"]'}});
	assert.deepEqual(result,{options:{only:["alpha","wide-chart"],extra:{"wide-chart":"wide source"}},scope:{cohort:["alpha","zeta","forms","wide-chart"],selected:["alpha","wide-chart"]}});
});

test("runtime case filtering rejects missing admission and unknown or duplicate keys",t=>{
	const root=fixture(t),extra={forms:"forms source"};
	for(const env of [{RICHVIEW_VERIFY_CASES:'["alpha"]'}, {RICHVIEW_VERIFY_PARENT:"affected-runtime",RICHVIEW_VERIFY_CASES:'["missing"]'}, {RICHVIEW_VERIFY_PARENT:"affected-runtime",RICHVIEW_VERIFY_CASES:'["alpha","alpha"]'}, {RICHVIEW_VERIFY_PARENT:"affected-runtime",RICHVIEW_VERIFY_CASES:'[]'}]) assert.throws(()=>runtimeCases(root,extra,{env}));
});
