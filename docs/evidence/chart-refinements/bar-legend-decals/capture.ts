import {readFileSync,mkdirSync} from 'node:fs';
import {resolve,dirname} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
import type { CapturedRender, RecordedRenderer, RecordedBrowser, RecordedServer } from '../capture-types.ts';

const [worktree,output]=process.argv.slice(2);
if(!worktree||!output)throw Error('Usage: node capture.ts BUILT_WORKTREE OUTPUT');
const wt=resolve(worktree),out=resolve(output),here=dirname(fileURLToPath(import.meta.url));
mkdirSync(out,{recursive:true});
const {default:core}: { default: RecordedRenderer }=await import(pathToFileURL(wt+'/packages/core/dist/index.js').href);
const {chromium}: RecordedBrowser=await import(pathToFileURL(wt+'/node_modules/playwright/index.mjs').href);
const {serveStatic}: RecordedServer=await import(pathToFileURL(wt+'/scripts/browser-harness.mjs').href);
let html='';
const server=await serveStatic({'/':wt+'/dist/web'},{route(p,u,r){
  if(p==='/review'){r.writeHead(200,{'content-type':'text/html'});r.end(html);return true;}
  if(p==='/favicon.ico'){r.writeHead(204);r.end();return true;}
}});
const browser=await chromium.launch({headless:true,executablePath:'/usr/bin/chromium'});
try{
  for(const [name,id,theme,width] of [['six','bar-legend-craft-grouped','sleek',320],['nine','bar-legend-craft-nine','scientific',720]] as const){
    const report: CapturedRender = JSON.parse(core.render_source(readFileSync(here+'/'+name+'.rmd','utf8'),JSON.stringify({
      width,styles:[theme,JSON.stringify({chart:{decals:true,palette:Array(8).fill('oklch(0.5999 0 0)')}})],
      runtime_url:server.base+'/richview_web',report_folder:{root:id,source:'report.rmd',files:['report.rmd']}
    })));
    html = report.html;
    const context=await browser.newContext({viewport:{width,height:1800},deviceScaleFactor:2,reducedMotion:'reduce'});
    const page=await context.newPage();
    await page.goto(server.base+'/review',{waitUntil:'networkidle'});
    await page.evaluate(async()=>{
      await document.fonts.ready;await window.__richview!.settled();
      await Promise.allSettled(document.getAnimations().filter(a=>a.playState==='running'&&a.effect?.getComputedTiming().iterations!==Infinity).map(a=>a.finished));
      await new Promise(r=>requestAnimationFrame(()=>requestAnimationFrame(r)));
    });
    await page.locator('figure.rv-chart').screenshot({path:out+'/'+name+'.png'});
    await context.close();
  }
}finally{await browser.close();await server.close();}
