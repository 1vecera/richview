// node capture.ts /absolute/built/worktree /absolute/output before|after
import assert from 'node:assert/strict';
import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {pathToFileURL,fileURLToPath} from 'node:url';
import type { RecordedRenderer, RecordedBrowser, RecordedServer, CapturePair, CapturedRender } from '../capture-types.ts';
import {dirname,join,resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
const wt=resolve(process.argv[2]),out=resolve(process.argv[3]),side=process.argv[4],here=dirname(fileURLToPath(import.meta.url));
assert((side === "before" || side === "after"));mkdirSync(out,{recursive:true});
const evidence: { pairs: (CapturePair & { report_root: string })[] } = JSON.parse(readFileSync(join(here, 'verification.json'), 'utf8'));
const {default:core}: { default: RecordedRenderer }=await import(pathToFileURL(wt+'/packages/core/dist/index.js').href);
const {chromium}: RecordedBrowser=await import(pathToFileURL(wt+'/node_modules/playwright/index.mjs').href);
const {serveStatic}: RecordedServer=await import(pathToFileURL(wt+'/scripts/browser-harness.mjs').href);
let html='';
const server=await serveStatic({'/':wt+'/dist/web'},{route(p,u,r){if(p==='/review'){r.writeHead(200,{'content-type':'text/html'});r.end(html);return true;}if(p==='/favicon.ico'){r.writeHead(204);r.end();return true;}}});
const browser=await chromium.launch({headless:true,executablePath:'/usr/bin/chromium'});
const hash= (bytes: string | Buffer) =>createHash('sha256').update(bytes).digest('hex');
const proof={head:execFileSync('git',['rev-parse','HEAD'],{cwd:wt,encoding:'utf8'}).trim(),wasm:hash(readFileSync(wt+'/dist/web/richview_web_bg.wasm')),browser:browser.version(),records: [] as { id: string; png_sha256: string }[]};
try{for(const setup of evidence.pairs){
  const source=readFileSync(join(here,setup.source),'utf8');assert.equal(hash(source),setup.source_sha256);
  const report: CapturedRender = JSON.parse(core.render_source(source,JSON.stringify({width:setup.width,styles:[setup.theme],runtime_url:server.base+'/richview_web',report_folder:{root:setup.report_root,source:'report.rmd',files:['report.rmd']}})));
  html = report.html;
  const context=await browser.newContext({viewport:{width:setup.width,height:900},deviceScaleFactor:2,reducedMotion:setup.motion}),page=await context.newPage();
  await page.goto(server.base+'/review',{waitUntil:'networkidle'});
  await page.evaluate(async()=>{await document.fonts.ready;await window.__richview!.settled();await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));await Promise.allSettled(document.getAnimations().filter(a=>a.playState==='running'&&a.effect?.getComputedTiming().iterations!==Infinity).map(a=>a.finished));});
  const path=join(out,setup[side].file),png=await page.locator('figure.rv-chart').screenshot({path});
  assert.equal(hash(png),setup[side].sha256,'Use the recorded build, Chromium and bundled fonts for identical pixels');
  proof.records.push({id:setup.id,png_sha256:hash(png)});await context.close();
}}finally{await browser.close();await server.close();}
writeFileSync(join(out,'capture.json'),JSON.stringify(proof,null,2)+'\n');
