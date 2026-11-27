import type { CapturedRender, RecordedRenderer } from '../capture-types.ts';
import type { AddressInfo } from 'node:net';
import assert from 'node:assert/strict';
import {createServer} from 'node:http';
import {createHash} from 'node:crypto';
import {readFileSync,writeFileSync,mkdirSync,existsSync} from 'node:fs';
import {resolve,join,extname,sep} from 'node:path';
import {pathToFileURL} from 'node:url';
import {parseArgs} from 'node:util';
const {values}=parseArgs({options:{worktree:{type:'string'},out:{type:'string'}}});
assert(values.worktree&&values.out,'Pass --worktree BUILT_WORKTREE --out OUTPUT_DIRECTORY');
const wt=resolve(values.worktree!),out=resolve(values.out!),dist=join(wt,'dist/web');mkdirSync(out,{recursive:true});
const {chromium}=await import(pathToFileURL(join(wt,'node_modules/playwright/index.mjs')).href) as typeof import('playwright');
const {default:core}=await import(pathToFileURL(join(wt,'packages/core/dist/index.js')).href) as { default: RecordedRenderer };
const hash=(b: string | Buffer)=>createHash('sha256').update(b).digest('hex'),pages=new Map<string, string>();
const server=createServer((req,res)=>{
 const path=new URL(req.url!,'http://localhost').pathname;
 if(path==='/favicon.ico'){res.writeHead(204);res.end();return;}
 if(pages.has(path)){res.writeHead(200,{'content-type':'text/html'});res.end(pages.get(path));return;}
 const file=resolve(dist,'.'+path);if(!file.startsWith(dist+sep)||!existsSync(file)){res.writeHead(404);res.end();return;}
 const mime=({'.js':'text/javascript','.wasm':'application/wasm','.css':'text/css','.woff2':'font/woff2'} as Record<string, string>)[extname(file)]||'application/octet-stream';
 res.writeHead(200,{'content-type':mime,'cache-control':'no-store'});res.end(readFileSync(file));
});
await new Promise<void>(done=>server.listen(0,'127.0.0.1',done));
const base='http://127.0.0.1:'+(server.address() as AddressInfo).port,browser=await chromium.launch({headless:true,executablePath:'/usr/bin/chromium'}),records=[];
try{for(const [id,theme,width]of [['positive-percent','scientific',240],['signed-percent','sleek',320]] as const){
 const source=readFileSync(new URL(id+'.rmd',import.meta.url),'utf8');
 const context=await browser.newContext({viewport:{width,height:900},deviceScaleFactor:2,reducedMotion:'reduce'}),page=await context.newPage(),errors: string[]=[];
 page.on('pageerror',e=>errors.push(e.message));page.on('console',m=>{if(m.type()==='error')errors.push(m.text());});
 const output: CapturedRender = JSON.parse(core.render_source(source,JSON.stringify({width,styles:[theme],runtime_url:base+'/richview_web',report_folder:{root:id,source:'report.rmd',files:['report.rmd','reference.json']}})));
 pages.set('/'+id,output.html);await page.goto(base+'/'+id,{waitUntil:'networkidle'});await page.evaluate(()=>document.fonts.ready);
 const figure=page.locator('figure.rv-chart');await figure.scrollIntoViewIfNeeded();await page.waitForFunction(()=>document.querySelector('.rvc')?.hasAttribute('data-rvc-diagnostics'));
 await page.evaluate(async()=>{await window.__richview!.settled();await new Promise(done=>requestAnimationFrame(()=>requestAnimationFrame(done)));});
 const details=await figure.evaluate(n=>({diagnostics:JSON.parse(n.querySelector('.rvc')!.getAttribute('data-rvc-diagnostics')!),overflow:document.documentElement.scrollWidth>innerWidth+1,svg:n.querySelector('.rvc-svg')!.outerHTML}));
 assert.deepEqual(errors,[]);assert.deepEqual(details.diagnostics.checks,[]);assert(!details.overflow);
 const image=join(out,id+'.png');await figure.screenshot({path:image});records.push({id,theme,viewport_width:width,source_sha256:hash(source),image_sha256:hash(readFileSync(image)),...details});await context.close();
}}finally{await browser.close();server.closeAllConnections();await new Promise<void>(done=>server.close(() => done()));}
writeFileSync(join(out,'capture.json'),JSON.stringify({worktree:wt,browser:browser.version(),wasm_sha256:hash(readFileSync(join(dist,'richview_web_bg.wasm'))),records},null,2)+'\n');
console.log('Captured both endpoint examples with clean measured labels.');
