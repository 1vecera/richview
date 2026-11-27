// Host reason (ARC-03): test case dispatch and proof without browser or build processes.
import test from "node:test";
import assert from "node:assert/strict";
import {mkdirSync,mkdtempSync,rmSync,writeFileSync} from "node:fs";
import {join} from "node:path";
import type {CaseKind} from "../lib/case-selection.mts";
import type {SelectionItem} from "../lib/selection.mts";
import type {runShardedCommand} from "../lib/shards.mts";
import {executeCaseCommand} from "../lib/case-executor.mts";
import {browserCases} from "../lib/browser-cases.mts";

function fixture(t: test.TestContext){mkdirSync(join(process.cwd(),"tmp"),{recursive:true});const dir=mkdtempSync(join(process.cwd(),"tmp/case-executor-"));t.after(()=>rmSync(dir,{recursive:true,force:true}));return dir;}
function selected(kind: CaseKind,keys: string[]): SelectionItem[] {return keys.map(key=>({step:"chromium",status:"run",id:`unit:${key}`,commandId:"chromium:example",caseKind:kind,caseKey:key}));}
const schedule=<T,>(work: () => T) => work();

test("only validated selected browser cases pass and the child gets an explicit filter",async t=>{
	const dir=fixture(t),selection=selected("browser",["landing-hero"]),seen: {args: string[]; env: NodeJS.ProcessEnv}[]=[];
	const runCommand=async(args: string[],env: NodeJS.ProcessEnv)=>{
		seen.push({args,env});const out=args[args.indexOf("--out")+1],scope=browserCases({env}).scope;
		assert.ok(scope);
		writeFileSync(join(out,"browser.partial.json"),JSON.stringify({status:"passed",engine:"chromium",errors:[],case_scope:scope,completed:scope.selected}));return {status:"pass" as const};
	};
	const result=await executeCaseCommand({command:{cmd:"node",args:[]},args:["script"],selection,cwd:dir,runDir:dir,env:{},signal:undefined,logPath:join(dir,"log"),parallel:1,schedule,runCommand});
	assert.equal(result.status,"pass");assert.deepEqual(result.caseUnits,[{id:"unit:landing-hero",status:"pass"}]);assert.deepEqual(result.cases?.selected,["landing-hero"]);
	assert.equal(seen[0].env.RICHVIEW_VERIFY_PARENT,"affected-browser");assert.equal(seen[0].env.RICHVIEW_VERIFY_CASES,'["landing-hero"]');
});

test("missing, stale, wrong-engine or incomplete evidence cannot pass",async t=>{
	const dir=fixture(t),selection=selected("browser",["landing-hero"]);
	for(const variant of ["missing","wrong-engine","incomplete"]){
		const result=await executeCaseCommand({command:{cmd:"node",args:[]},args:["script"],selection,cwd:dir,runDir:dir,env:{},signal:undefined,logPath:join(dir,"log"),parallel:1,schedule,runCommand:async(args: string[],env: NodeJS.ProcessEnv)=>{
			const out=args[args.indexOf("--out")+1],scope=browserCases({env}).scope;
		assert.ok(scope);
			if(variant!=="missing")writeFileSync(join(out,"browser.partial.json"),JSON.stringify({status:"passed",engine:variant==="wrong-engine"?"webkit":"chromium",errors:[],case_scope:scope,completed:variant==="incomplete"?[]:scope.selected}));
			return {status:"pass" as const};
		}});
		assert.equal(result.status,"fail",variant);assert.equal(result.caseUnits?.[0].status,"fail");
	}
});

test("fixed docs cases honor one shared permit and require matching selected evidence",async t=>{
	const dir=fixture(t),selection=selected("docs",["native","phone"]);let active=0,peak=0;
	const runShards: typeof runShardedCommand = async(_cmd,args,options)=>{
		assert.deepEqual(options.shard.cases,["native","phone"]);assert.equal(options.shard.count,5);assert.equal(args.filter((arg: string)=>arg==="--out").length,1);
		await Promise.all([1,2].map(()=>options.scheduleCommand(async()=>{active++;peak=Math.max(peak,active);await new Promise(done=>setTimeout(done,5));active--;return {status:"pass"};})));
		return {status:"pass",cases:{cohort:["native","main","desktop","phone","no-js"],selected:["native","phone"],completed:["native","phone"]}};
	};
	const result=await executeCaseCommand({command:{cmd:"node",args:[]},args:["script","--out","old"],selection,cwd:dir,runDir:dir,env:{},signal:undefined,logPath:join(dir,"log"),parallel:1,schedule,runCommand:()=>assert.fail(),runShards});
	assert.equal(result.status,"pass");assert.equal(peak,1);assert.ok(result.caseUnits?.every((item: { status: string; })=>item.status==="pass"));
});

test("a child-reported superset cannot cover unselected cases",async t=>{
	const dir=fixture(t),selection=selected("runtime",["bar"]);
	const result=await executeCaseCommand({command:{cmd:"node",args:[]},args:["script"],selection,cwd:dir,runDir:dir,env:{},signal:undefined,logPath:join(dir,"log"),parallel:2,schedule,runCommand:()=>assert.fail(),runShards:async()=>({status:"pass",cases:{cohort:["bar","line"],selected:["bar","line"],completed:["bar","line"]}})});
	assert.equal(result.status,"fail");assert.equal(result.caseUnits?.[0].status,"fail");
});
