// Host reason (ARC-03): test figure-expansion case selection without a browser or build.
import test from "node:test";
import assert from "node:assert/strict";
import vm from "node:vm";
import {FIGURE_CASES,FIGURE_THEMES,FIGURE_WIDTHS,figureBuildThemes,figureCases,verifyFigureCases} from "../lib/figure-cases.mts";
import { readTestSource } from "./test-source.mts";

const requested=["expanded-regression","theme/scientific/420"];
const env={RICHVIEW_VERIFY_PARENT:"affected-figure-expand",RICHVIEW_VERIFY_CASES:JSON.stringify(requested)};
test("unfiltered figure selection preserves the original theme order and evidence bytes",()=>{
	const selection=figureCases({env:{}});assert.equal(selection.cases,FIGURE_CASES);assert.equal(figureBuildThemes(selection),FIGURE_THEMES);
	const before={engine:"chromium",browser:"fixture",cases:[{theme:"sleek",width:1440}],errors:[]};
	const after={...before,...(selection.scope&&{case_scope:selection.scope})};assert.equal(JSON.stringify(after),JSON.stringify(before));
	assert.deepEqual(FIGURE_THEMES,["sleek","scientific","brutalist","air","hand-drawn"]);assert.deepEqual(FIGURE_WIDTHS,[1440,420,320]);
});
test("affected figure cases keep source order and generate only needed themes",()=>{
	const selection=figureCases({env});assert.ok(selection.scope);assert.deepEqual(selection.scope.selected,["theme/scientific/420","expanded-regression"]);
	assert.deepEqual(figureBuildThemes(selection),["sleek","scientific"]);
	const isolated=figureCases({env:{...env,RICHVIEW_VERIFY_CASES:'["theme/air/320"]'}});assert.deepEqual(figureBuildThemes(isolated),["air"]);
	for(const bad of [{RICHVIEW_VERIFY_CASES:'["expanded-regression"]'},{...env,RICHVIEW_VERIFY_CASES:'["theme/missing/320"]'},{...env,RICHVIEW_VERIFY_CASES:'["expanded-regression","expanded-regression"]'},{...env,RICHVIEW_VERIFY_CASES:'[]'}]) assert.throws(()=>figureCases({env:bad}));
});
test("partial figure evidence proves the selected viewports and shared regression",()=>{
	const scope=figureCases({env}).scope;
	assert.ok(scope);
	const report: {status: string; engine: string; errors: string[]; case_scope: typeof scope; completed: string[]; cases: {theme: string; width: number; contracted: number; expanded: number; turned: boolean; lifecycle: {sameCanvas: boolean; charts: number}}[]; phone: {type: string}[]; upright: object; tableInView?: object}={status:"passed",engine:"chromium",errors:[],case_scope:scope,completed:[...scope.selected],cases:[{theme:"scientific",width:420,contracted:300,expanded:800,turned:true,lifecycle:{sameCanvas:true,charts:0}}],phone:[{type:"line"}],upright:{},tableInView:{}};
	assert.deepEqual(verifyFigureCases(report,requested),{...scope,completed:scope.selected});
	const mutations: ((fixture: typeof report) => unknown)[] = [r=>r.completed.pop(),r=>r.cases[0].width=320,r=>r.cases[0].lifecycle.sameCanvas=false,r=>delete r.tableInView,r=>r.case_scope.cohort.pop(),r=>r.engine="webkit",r=>r.errors.push("browser error")];
	for(const mutate of mutations) {const bad=structuredClone(report);mutate(bad);assert.throws(()=>verifyFigureCases(bad,requested));}
	assert.throws(()=>verifyFigureCases(report,[]));
});
test("the shipping themed loop visits only selected theme and viewport cases",()=>{
	const source=readTestSource(new URL("../../../scripts/verify-figure-expand.ts",import.meta.url));
	const start=source.indexOf("  for (const theme of themes) for (const width of FIGURE_WIDTHS) {"),end=source.indexOf("    await page.",start);
	assert.ok(start>=0&&end>start);const loop=source.slice(start,end)+"visited.push(`theme/${theme}/${width}`); }";
	const scenarios: [NodeJS.ProcessEnv, string[]][] = [[{},FIGURE_CASES.filter(key=>key.startsWith("theme/"))],[env,["theme/scientific/420"]]];
	for(const [input,expected] of scenarios){
		const selection=figureCases({env:input}),visited:string[]=[];
		vm.runInNewContext(loop,{themes:FIGURE_THEMES,FIGURE_WIDTHS,visited,wants:(key: string)=>selection.cases.includes(key)});
		assert.deepEqual(visited,expected);
	}
});
