// Host reason (ARC-03): test figure-expansion case selection without a browser or build.
import test from "node:test";
import assert from "node:assert/strict";
import {readFileSync} from "node:fs";
import vm from "node:vm";
import {FIGURE_CASES,FIGURE_THEMES,FIGURE_WIDTHS,figureBuildThemes,figureCases,verifyFigureCases} from "../lib/figure-cases.mjs";

const requested=["expanded-regression","theme/scientific/420"];
const env={RICHVIEW_VERIFY_PARENT:"affected-figure-expand",RICHVIEW_VERIFY_CASES:JSON.stringify(requested)};
test("unfiltered figure selection preserves the original theme order and evidence bytes",()=>{
	const selection=figureCases({env:{}});assert.equal(selection.cases,FIGURE_CASES);assert.equal(figureBuildThemes(selection),FIGURE_THEMES);
	const before={engine:"chromium",browser:"fixture",cases:[{theme:"sleek",width:1440}],errors:[]};
	const after={...before,...(selection.scope&&{case_scope:selection.scope})};assert.equal(JSON.stringify(after),JSON.stringify(before));
	assert.deepEqual(FIGURE_THEMES,["sleek","scientific","brutalist","air","hand-drawn"]);assert.deepEqual(FIGURE_WIDTHS,[1440,420,320]);
});
test("affected figure cases keep source order and generate only needed themes",()=>{
	const selection=figureCases({env});assert.deepEqual(selection.scope.selected,["theme/scientific/420","expanded-regression"]);
	assert.deepEqual(figureBuildThemes(selection),["sleek","scientific"]);
	const isolated=figureCases({env:{...env,RICHVIEW_VERIFY_CASES:'["theme/air/320"]'}});assert.deepEqual(figureBuildThemes(isolated),["air"]);
	for(const bad of [{RICHVIEW_VERIFY_CASES:'["expanded-regression"]'},{...env,RICHVIEW_VERIFY_CASES:'["theme/missing/320"]'},{...env,RICHVIEW_VERIFY_CASES:'["expanded-regression","expanded-regression"]'},{...env,RICHVIEW_VERIFY_CASES:'[]'}]) assert.throws(()=>figureCases({env:bad}));
});
test("partial figure evidence proves the selected viewports and shared regression",()=>{
	const scope=figureCases({env}).scope;
	const report={status:"passed",engine:"chromium",errors:[],case_scope:scope,completed:[...scope.selected],cases:[{theme:"scientific",width:420,contracted:300,expanded:800,turned:true,lifecycle:{sameCanvas:true,charts:0}}],phone:[{type:"line"}],upright:{},tableInView:{}};
	assert.deepEqual(verifyFigureCases(report,requested),{...scope,completed:scope.selected});
	for(const mutate of [r=>r.completed.pop(),r=>r.cases[0].width=320,r=>r.cases[0].lifecycle.sameCanvas=false,r=>delete r.tableInView,r=>r.case_scope.cohort.pop(),r=>r.engine="webkit",r=>r.errors.push("browser error")]) {const bad=structuredClone(report);mutate(bad);assert.throws(()=>verifyFigureCases(bad,requested));}
	assert.throws(()=>verifyFigureCases(report,[]));
});
test("the shipping themed loop visits only selected theme and viewport cases",()=>{
	const source=readFileSync(new URL("../../../scripts/verify-figure-expand.mjs",import.meta.url),"utf8");
	const start=source.indexOf("  for (const theme of themes) for (const width of FIGURE_WIDTHS) {"),end=source.indexOf("    await page.",start);
	assert.ok(start>=0&&end>start);const loop=source.slice(start,end)+"visited.push(`theme/${theme}/${width}`); }";
	for(const [input,expected] of [[{},FIGURE_CASES.filter(key=>key.startsWith("theme/"))],[env,["theme/scientific/420"]]]){
		const selection=figureCases({env:input}),visited=[];
		vm.runInNewContext(loop,{themes:FIGURE_THEMES,FIGURE_WIDTHS,visited,wants:key=>selection.cases.includes(key)});
		assert.deepEqual(visited,expected);
	}
});
