// Host reason (ARC-03): test affected browser journeys without starting a browser.
import test from "node:test";
import assert from "node:assert/strict";
import vm from "node:vm";
import {BROWSER_CASES, browserCases, verifyBrowserCases} from "../lib/browser-cases.mts";
import { readTestSource } from "./test-source.mts";

const requested=["landing-hero","landing-deferred"];
const env={RICHVIEW_VERIFY_PARENT:"affected-browser",RICHVIEW_VERIFY_CASES:JSON.stringify(requested)};
test("unfiltered browser selection retains every journey and the legacy evidence shape",()=>{
	const result=browserCases({env:{}});
	assert.equal(result.cases,BROWSER_CASES);assert.equal("scope" in result,false);
	const evidence={engine:"chromium",browser:"fixture",checks:["passed"],timings:{},errors:[],capabilities:{trustedPinch:true},status:"passed"};
	const after={...evidence,...(result.scope&&{case_scope:result.scope})};
	assert.equal(JSON.stringify(after),JSON.stringify(evidence));
});
test("affected browser selection admits only explicitly named independent journeys",()=>{
	assert.deepEqual(browserCases({env}).scope,{cohort:[...BROWSER_CASES],selected:["landing-deferred","landing-hero"]});
	for(const bad of [{RICHVIEW_VERIFY_CASES:'["landing-hero"]'},{...env,RICHVIEW_VERIFY_CASES:'["not-a-journey"]'},{...env,RICHVIEW_VERIFY_CASES:'["landing-hero","landing-hero"]'},{...env,RICHVIEW_VERIFY_CASES:'[]'}]) assert.throws(()=>browserCases({env:bad}));
});
test("partial browser evidence must prove the exact requested journeys and engine",()=>{
	const scope=browserCases({env}).scope;
	assert.ok(scope);
	const report={status:"passed",engine:"chromium",errors:[] as string[],case_scope:scope,completed:[...scope.selected]};
	assert.deepEqual(verifyBrowserCases(report,requested),{...scope,completed:scope.selected});
	const mutations: ((fixture: typeof report) => unknown)[] = [r=>r.completed.pop(),r=>r.completed.push("browser-journey"),r=>r.case_scope.cohort.pop(),r=>r.status="failed",r=>r.engine="webkit",r=>r.errors.push("browser error")];
	for(const mutate of mutations){
		const bad=structuredClone(report);mutate(bad);assert.throws(()=>verifyBrowserCases(bad,requested));
	}
	assert.throws(()=>verifyBrowserCases(report,[]));
});

test("the shipping browser dispatcher executes only its selected independent journeys",async()=>{
	const source=readTestSource(new URL("../../../scripts/verify-browser.ts",import.meta.url));
	const start=source.indexOf("try {\n  if (wants('landing-deferred'))"),end=source.indexOf("  evidence.artifacts =",start);
	assert.ok(start>=0&&end>start);
	// Keep the real five dispatch branches; replace the large journey body with a marker.
	const dispatch=source.slice(start+"try {\n".length,end)+"calls.push('browser-journey'); if (selection.scope) completedCases.push('browser-journey'); }";
	const scenarios: [NodeJS.ProcessEnv, string[], string[]][] = [[{},[...BROWSER_CASES],[]],[env,["landing-deferred","landing-hero"],["landing-deferred","landing-hero"]]];
	for(const [input,expected,completed] of scenarios){
		const selection=browserCases({env:input}),calls:string[]=[],completedCases:string[]=[];
		const ctx=vm.createContext({selection,completedCases,calls,wants:(key: string)=>selection.cases.includes(key),check:()=>{},h:{browser:{},engine:"chromium",dir:"tmp"},evidence:{},
			deferredLanding:async()=>calls.push("landing-deferred"),deferredStrip:async()=>calls.push("landing-strip"),verifyRuntimeAccess:async()=>calls.push("site-preview-access"),runtimeLoaderControls:async()=>calls.push("runtime-loader"),heroClip:async()=>calls.push("landing-hero")});
		await vm.runInContext(`(async()=>{${dispatch}})()`,ctx);
		assert.deepEqual(calls,expected);assert.deepEqual(completedCases,completed);
	}
});
