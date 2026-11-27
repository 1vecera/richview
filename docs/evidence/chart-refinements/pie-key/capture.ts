// node capture.ts /absolute/built/worktree /absolute/output
import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {pathToFileURL,fileURLToPath} from 'node:url';
import assert from 'node:assert/strict';
import type { CapturedRender, RecordedRenderer, RecordedBrowser, RecordedServer } from '../capture-types.ts';
import {dirname,join,resolve} from 'node:path';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
const [wt,out]=process.argv.slice(2).map(p=>resolve(p)),here=dirname(fileURLToPath(import.meta.url));
mkdirSync(out,{recursive:true});
const {default:core}: { default: RecordedRenderer }=await import(pathToFileURL(wt+'/packages/core/dist/index.js').href);
const {chromium}: RecordedBrowser=await import(pathToFileURL(wt+'/node_modules/playwright/index.mjs').href);
const {serveStatic}: RecordedServer=await import(pathToFileURL(wt+'/scripts/browser-harness.mjs').href);
let html='';
const server=await serveStatic({'/':wt+'/dist/web'},{route(p,u,r){if(p==='/review'){r.writeHead(200,{'content-type':'text/html'});r.end(html);return true;}if(p==='/favicon.ico'){r.writeHead(204);r.end();return true;}}});
const browser=await chromium.launch({headless:true,executablePath:'/usr/bin/chromium'});
const hash= (p: string) =>createHash('sha256').update(readFileSync(p)).digest('hex');
const records: { id: string; theme: string; width: number; selector: string; index: number; point: { x: number; y: number }; readout: string | null; source_sha256: string; png_sha256: string }[] = [];
const proof={head:execFileSync('git',['rev-parse','HEAD'],{cwd:wt,encoding:'utf8'}).trim(),wasm:hash(wt+'/dist/web/richview_web_bg.wasm'),browser:browser.version(),records};
try{for(const setup of [{id:'tiny',theme:'scientific',width:320,selector:'.rv-slice-value',index:1},{id:'wrapped',theme:'brutalist',width:720,selector:'.rv-slice-label',index:-1}]){
  const source=readFileSync(join(here,setup.id+'-source.rmd'),'utf8');
  const report: CapturedRender = JSON.parse(core.render_source(source,JSON.stringify({width:setup.width,styles:[setup.theme],runtime_url:server.base+'/richview_web',report_folder:{root:setup.id,source:'report.rmd',files:['report.rmd']}})));
  html = report.html;
  const context=await browser.newContext({viewport:{width:setup.width,height:1100},deviceScaleFactor:2,reducedMotion:'reduce'}),page=await context.newPage();
  await page.goto(server.base+'/review',{waitUntil:'networkidle'});
  const settle=()=>page.evaluate(async()=>{await document.fonts.ready;await window.__richview!.settled();await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));});
  await settle();const figure=page.locator('figure.rv-chart'),nodes=figure.locator(setup.selector),node=setup.index<0?nodes.last():nodes.nth(setup.index);
  await node.scrollIntoViewIfNeeded();const box=await node.boundingBox();assert.ok(box);
  const point={x:box.x+box.width/2,y:box.y+box.height/2};await page.mouse.move(point.x,point.y);await settle();
  const tip=figure.locator('.rvc-tip'),readout=await tip.isVisible()?await tip.textContent():null,path=join(out,setup.id+'.png');
  await figure.screenshot({path});proof.records.push({...setup,point,readout,source_sha256:createHash('sha256').update(source).digest('hex'),png_sha256:hash(path)});await context.close();
}}finally{await browser.close();await server.close();}
writeFileSync(join(out,'capture.json'),JSON.stringify(proof,null,2)+'\n');
