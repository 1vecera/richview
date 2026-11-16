// Host reason (ARC-03): exercise actual path-to-check plans for release evidence inputs.
import test from "node:test";
import assert from "node:assert/strict";
import {execFileSync} from "node:child_process";
import {fileURLToPath} from "node:url";
import {commandId} from "../lib/selection.mjs";
import {stepsForTier} from "../lib/steps.mjs";

const root=fileURLToPath(new URL("../../..",import.meta.url));
const script=fileURLToPath(new URL("../affected-plan.mjs",import.meta.url));
const steps=stepsForTier("full","full");
const step=id=>steps.find(item=>item.id===id);
const plan=path=>execFileSync(process.execPath,[script,path],{cwd:root,encoding:"utf8",maxBuffer:8*1024*1024});
const runs=(output,id)=>assert.ok(output.split("\n").some(line=>line.startsWith(`RUN     ${id} `)||line.startsWith(`RUN     ${id} (`)),`${id} did not run`);
const runsCommand=(output,id)=>{
	const rows=output.split("\n").filter(line=>line.slice(8).startsWith(`${id} `)||line.slice(8).startsWith(`${id}:case:`));
	assert.ok(rows.length,`${id} was not in the plan`);assert.ok(rows.every(line=>line.startsWith("RUN     ")),`${id} has a non-running case`);
};
for(const path of ["package-lock.json","cloudflare/publish/package-lock.json","package.json"]) test(`${path} runs pins and both npm dependency checks`,()=>{
	const output=plan(path);runs(output,"pins");
	for(const command of step("deps").run.filter(item=>item.args.some(arg=>/npm-(?:licences|advisories)\.mjs$/.test(arg)))) runs(output,commandId(step("deps"),command));
	assert.equal(step("deps").run.filter(item=>item.args.some(arg=>/npm-(?:licences|advisories)\.mjs$/.test(arg))).length,2);
	if(path==="package-lock.json"||path==="package.json") for(const command of step("chromium").run) runsCommand(output,commandId(step("chromium"),command));
});
for(const path of [".node-version"]) test(`${path} runs pinned toolchain checks`,()=>runs(plan(path),"pins"));
test("docs/RMD-SPEC.md runs documentation matching",()=>runs(plan("docs/RMD-SPEC.md"),"docs-match"));
for(const path of ["docs/lockdown/parts/01-intent.json","docs/lockdown/map/map.json","docs/lockdown/README.md"]) test(`${path} runs the lockdown tool tests`,()=>runs(plan(path),"tool-tests"));
