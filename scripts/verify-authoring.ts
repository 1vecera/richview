// Actual browser appearance and portable filesystem output, all in task-local folders.
import assert from 'node:assert/strict';
import {spawn,spawnSync} from 'node:child_process';
import {cpSync,mkdirSync,readFileSync,readdirSync,writeFileSync,unlinkSync} from 'node:fs';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';
import {cli,harness,root,watchErrors} from './browser-harness.ts';
import type { Page, FrameLocator } from 'playwright';
const h=await harness('authoring');
const folder=join(h.dir,'quarterly-sales');mkdirSync(join(folder,'data'),{recursive:true});mkdirSync(join(folder,'media'));
const file=join(folder,'quarter.rmd');
const csv='month,value\nJan,100\nFeb,140\n';writeFileSync(join(folder,'data/sales.csv'),csv);
writeFileSync(join(folder,'media/logo.svg'),'<svg xmlns="http://www.w3.org/2000/svg" width="40" height="40"><rect width="40" height="40" fill="#176430"/></svg>');
const source=`---\ntitle: Same observations\ndescription: Exact values remain unchanged through style changes.\nstyles: [scientific, report.rstyle.yaml]\n---\n\n![Local logo](media/logo.svg)\n\n[Original observations](data/sales.csv)\n\n\`\`\`chart\ntype: bar\ntitle: Revenue rose in February\ndata: {csv: data/sales.csv}\nencoding: {x: month, y: value}\nstyle: {ratio: '16:9'}\n\`\`\`\n\n\`\`\`table\ndata: {csv: data/sales.csv}\ncolumns:\n  - key: month\n    label: Month\n  - key: value\n    label: Value\n    align: right\n\`\`\`\n`;
const custom=(accent: string)=>`colors:\n  background: '#fff4dc'\n  brand: '${accent}'\npalette: ['${accent}', '#167d8d']\ncorners: large\n`;
writeFileSync(file,source);writeFileSync(join(folder,'report.rstyle.yaml'),custom('#a52a2a'));
const generate=(name: string,styles: string[] = [])=>{const result=spawnSync(cli,['generate',file,'--output',name+'.html','--format','json',...styles.flatMap(s=>['--style',s])],{encoding:'utf8',timeout:60000});assert.equal(result.status,0,result.stdout+result.stderr);const output=JSON.parse(result.stdout).output;assert.equal(output,join(folder,'Rendered',name+'.html'));return output;};
const snapshot=async (scope: Page|FrameLocator)=>scope.locator('body').evaluate(async node=>{
 await node.ownerDocument.fonts.ready;
 const get=(s: string)=>s==='body'?node:node.querySelector(s);const style=(s: string)=>getComputedStyle(get(s)!);
 const text=get('.rv-report')!.cloneNode(true);(text as Element).querySelectorAll('.rv-chart, script, footer').forEach(n=>n.remove());
 const model=window.__richview?.charts()[0];const c=model?.geometry.series[0]?.color;const bar=c?{fill:`rgb(${c.slice(0,3).map(v=>Math.round(v*255)).join(', ')})`}:null;
 return {content:text.textContent!.replace(/\s+/g,' ').trim(),data:[...node.querySelectorAll('script[data-rv-payload]')].map(n=>{const p=JSON.parse(n.textContent);return {columns:p.columns,rows:p.rows};}),bg:getComputedStyle(node.ownerDocument.documentElement).getPropertyValue('--rv-bg').trim(),font:style('.rv-report__body').fontFamily,titleFont:style('h1').fontFamily,width:style('.rv-report').maxWidth,bar,loadedFonts:[...node.ownerDocument.fonts].filter(f=>f.status==='loaded').map(f=>f.family)};
});
const waitChart=async (scope: Page|FrameLocator)=>{
 const deadline=Date.now()+45000;
 for(;;){
  const remaining=deadline-Date.now();assert.ok(remaining>0,'The chart runtime did not settle within 45 seconds');
  let timer;
  try{
   await scope.locator('figure[data-rv-chart] canvas.rv-chart__canvas').waitFor({timeout:remaining});
   const left=Math.max(1,deadline-Date.now());
   await Promise.race([
    scope.locator('body').evaluate(async(_,remaining)=>{
     const end=performance.now()+remaining;
     while(!window.__richview&&performance.now()<end)await new Promise(r=>setTimeout(r,50));
     if(!window.__richview)throw new Error('The chart runtime did not start');
     await window.__richview.settled();
    },left),
    new Promise((_,reject)=>{timer=setTimeout(()=>reject(new Error('The chart runtime did not settle within 45 seconds')),left);}),
   ]);
   return;
  }catch(error){
   if(!(error as Error).message.includes('Execution context was destroyed')||Date.now()>=deadline)throw error;
  }finally{clearTimeout(timer);}
 }
};
const waitFill=async(scope: Page|FrameLocator,fill: string)=>{for(let i=0;i<450;i++){try{if(await scope.locator('body').evaluate(()=>{const c=window.__richview!.charts()[0]?.geometry.series[0]?.color;return c?`rgb(${c.slice(0,3).map(v=>Math.round(v*255)).join(', ')})`:null;})===fill)return;}catch{}await new Promise(r=>setTimeout(r,100));}throw new Error('Expected actual bar fill '+fill);};
type Snapshot = Awaited<ReturnType<typeof snapshot>>;
type View = Snapshot & { name: string };
type ResponseFailure = { url: string; status: number };
const evidence: { checks: string[]; exports: View[]; live: View[]; browser: View[]; errors: string[]; network: string[]; network_errors?: ResponseFailure[]; evidence_dir?: string }
  = {checks:[],exports:[],live:[],browser:[],errors:[],network:[]};
let preview: import("node:child_process").ChildProcessByStdio<null, import("node:stream").Readable, import("node:stream").Readable> | undefined;
const networkErrors: { url: string; status: number; }[]=[];const responses=(p: Page)=>p.on('response',r=>{if(r.status()>=400)networkErrors.push({url:r.url(),status:r.status()});});
// The shared editor must wrap without moving its line numbers or changing the source.
async function verifyWrappedEditor(){
 for(const width of [1280,390]){
  const page=await h.browser.newPage({viewport:{width,height:900}});
  try{
   await page.goto(h.base+'/');
   await page.locator('#demo-source').scrollIntoViewIfNeeded();
   await page.waitForFunction(()=>document.querySelector('#demo-source')?.parentElement!.classList.contains('is-live'));
   const area=page.locator('#demo-source');
   const text=`first\n## ${'A long prose line with words and spaces. '.repeat(15)}\n${'| one | two | three '.repeat(20)}\nlast`;
   await area.fill(text);
   const result=await area.evaluate(el=>{
    const lines=[...el.parentElement!.querySelectorAll('.code__line')];
    if(lines.length!==4)return {lineCount:lines.length,width:el.clientWidth,scrollWidth:el.scrollWidth};
    const input=getComputedStyle(el),pre=el.parentElement!.querySelector('.code__hl');
    const rects=lines.map(line=>line.getBoundingClientRect());
    // Independently measure the start of each source line in the textarea's font and width.
    const mirror=document.createElement('div');mirror.className='code__mirror';const markers: HTMLSpanElement[]=[];
    (el as HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement).value.split('\n').forEach((line,i)=>{if(i)mirror.append('\n');const marker=document.createElement('span');marker.textContent='\u200b';mirror.append(marker,line);markers.push(marker);});
    el.parentElement!.append(mirror);
    const offsets=markers.map((marker,i)=>marker.getBoundingClientRect().top-rects[i].top);mirror.remove();
    return {width:el.clientWidth,scrollWidth:el.scrollWidth,text:(el as HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement).value,painted:el.parentElement!.querySelector('.code__hl code')!.textContent.replaceAll('\u200b',''),
     lineHeight:parseFloat(input.lineHeight),heights:rects.map(r=>r.height),gaps:rects.slice(1).map((r,i)=>r.top-rects[i].bottom),
     lineCount:lines.length,offsets,counter:getComputedStyle(lines[2],'::before').content,preWidth:pre!.clientWidth,preScrollWidth:pre!.scrollWidth};
   });
   assert.equal(result.lineCount,4,'each source line has a numbered layout row');
   assert.equal(result.text,text);assert.equal(result.painted,text.replaceAll('\n',''));
   assert.ok(result.heights[1]>result.lineHeight&&result.heights[2]>result.lineHeight,`long prose and table wrap at ${width}`);
   assert.ok(result.offsets.every(offset=>Math.abs(offset-result.offsets[0])<1),`numbers track wrapped source lines at ${width}`);
   assert.ok(result.gaps.every(gap=>Math.abs(gap)<1),`line numbers stay with their source lines at ${width}`);
   assert.ok(result.scrollWidth<=result.width&&result.preScrollWidth<=result.preWidth,`no horizontal overflow at ${width}`);
   assert.notEqual(result.counter,'none',`line numbers are present at ${width}`);
   const editAt=text.indexOf('words');await area.focus();await area.evaluate((el,at)=>(el as HTMLInputElement | HTMLTextAreaElement).setSelectionRange(at,at+5),editAt);
   await page.keyboard.type('text');assert.equal(await area.inputValue(),text.slice(0,editAt)+'text'+text.slice(editAt+5));
   const beforeInsert=await area.inputValue();await page.locator('[data-add="chart"]').click();
   assert.ok((await area.inputValue()).length>beforeInsert.length,'chart button inserts at the caret');
  }finally{await page.close();}
 }
 const playground=await h.browser.newPage({viewport:{width:1280,height:900}});
 try{
  await playground.goto(h.base+'/playground.html');
  await playground.waitForFunction(()=>document.querySelector('#source')?.parentElement!.classList.contains('is-live'));
  const area=playground.locator('#source'),text='Prose and spaces '.repeat(100);await area.fill(text);
  assert.equal(await area.inputValue(),text,'playground wrapping keeps the source');
  assert.ok(await area.evaluate(el=>el.scrollWidth<=el.clientWidth),'playground has no horizontal overflow');
 }finally{await playground.close();}
 evidence.checks.push('Shared editor wraps with aligned line numbers and no horizontal overflow at 1280 and 390; editing and chart insertion work; playground source stays intact');
}
try{
 await verifyWrappedEditor();
 const context=await h.browser.newContext({viewport:{width:1280,height:900}});
 await context.route('**/favicon.ico',route=>route.fulfill({status:204,body:''}));
 const page=await context.newPage();watchErrors(page,evidence.errors);
 await page.route('**/*',route=>{if(/^(file|data|blob):/.test(route.request().url()))return route.continue();evidence.network.push(route.request().url());return route.abort();});
 let baseline: Snapshot | undefined;
 const record=async(name: string,path: string)=>{await page.goto(pathToFileURL(path).href);await waitChart(page);const value=await snapshot(page);assert.ok(value.bar);assert.ok(value.loadedFonts.length);if(baseline){assert.equal(value.content,baseline.content);assert.deepEqual(value.data,baseline.data);}else baseline=value;await page.screenshot({path:join(h.dir,'export-'+name+'.png'),fullPage:true});evidence.exports.push({name,...value});return value;};
 const canonical=[];
 // Every builtin style, read from core's style files, reaches the rendered report (DEL-17).
 const builtins=readdirSync(join(root,'crates/richview-core/assets')).filter(n=>n.endsWith('.rstyle.yaml')).map(n=>n.replace(/\.rstyle\.yaml$/,'')).sort();
 assert.ok(builtins.length>=5,builtins.join());
 for(const name of builtins)canonical.push(await record(name,generate(name,[name])));
 assert.equal(new Set(canonical.map(v=>[v.font,v.bg,v.bar!.fill].join('|'))).size,builtins.length,'each builtin renders a distinct appearance');
 const local=await record('local',generate('local'));assert.equal(local.bar!.fill,'rgb(165, 42, 42)');
 writeFileSync(join(folder,'report.rstyle.yaml'),custom('#166534'));
 const edited=await record('local-edited',generate('local-edited'));assert.equal(edited.bar!.fill,'rgb(22, 101, 52)');
 const back=await record('switch-back',generate('switch-back',['scientific']));const scientific=canonical[builtins.indexOf('scientific')];assert.equal(back.font,scientific.font);assert.equal(back.bg,scientific.bg);assert.equal(back.bar!.fill,scientific.bar!.fill);
 // Move only Rendered: runtime, fonts, media and required local links still work.
 const portable=join(h.dir,'moved-output');cpSync(join(folder,'Rendered'),portable,{recursive:true});
 await record('moved',join(portable,'local-edited.html'));
 assert.equal(readFileSync(join(portable,'data/sales.csv'),'utf8'),csv);
 assert.ok(await page.locator('img').evaluate(img=>(img as HTMLImageElement).complete&&(img as HTMLImageElement).naturalWidth===40));
 assert.equal(await page.locator('a[href="data/sales.csv"]').getAttribute('href'),'data/sales.csv');
 await page.locator('.rv-chart__frame').focus();await page.keyboard.press('Home');await page.locator('.rv-chart__readout:not([hidden])').waitFor();assert.match(await page.locator('.rv-chart__readout').innerText(),/100/);
 assert.deepEqual(evidence.network,[]);
 evidence.checks.push('Moved Rendered HTML mounts interactive Rust chart offline; bundled fonts load, media decodes, original CSV link resolves and keyboard hover shows exact 100');
 writeFileSync(file,source.replace('styles: [scientific, report.rstyle.yaml]\n',''));unlinkSync(join(folder,'report.rstyle.yaml'));
 const defaults=await record('no-yaml',generate('no-yaml'));assert.notEqual(defaults.bar!.fill,local.bar!.fill,'removing the local style file reaches the chart');
 assert.equal(readFileSync(join(folder,'data/sales.csv'),'utf8'),csv);
 evidence.checks.push('Canonical, local YAML edit, switch-back and no-YAML exported appearance differs as expected with identical content/data');
 // Native live preview must refresh after an actual local YAML save.
 writeFileSync(file,source);writeFileSync(join(folder,'report.rstyle.yaml'),custom('#a52a2a'));
 preview=spawn(cli,['serve',file,'--port','0'],{cwd:folder,stdio:['ignore','pipe','pipe']});
 const base=await new Promise<string>((ok,fail)=>{let log='';const timer=setTimeout(()=>fail(new Error(log)),15000);preview!.once('error',fail);for(const stream of [preview!.stdout,preview!.stderr])stream.on('data',(data: string)=>{log+=data;const m=log.match(/RichView at (http:\/\/127\.0\.0\.1:\d+)/);if(m){clearTimeout(timer);ok(m[1]);}});});
 const live=await context.newPage();watchErrors(live,evidence.errors);responses(live);await live.goto(base);await waitChart(live);let liveValue=await snapshot(live);assert.equal(liveValue.bar!.fill,local.bar!.fill);evidence.live.push({name:'local',...liveValue});
 writeFileSync(join(folder,'report.rstyle.yaml'),custom('#166534'));
 await waitFill(live,'rgb(22, 101, 52)');liveValue=await snapshot(live);assert.equal(liveValue.content,baseline!.content);assert.deepEqual(liveValue.data,baseline!.data);evidence.live.push({name:'yaml-save',...liveValue});
 for(const name of ['sleek','brutalist','scientific']){
  writeFileSync(file,source.replace('styles: [scientific, report.rstyle.yaml]',`styles: [${name}]`));
  const expected=evidence.exports.find(v=>v.name===name);
  await waitFill(live,expected!.bar!.fill);
  const value=await snapshot(live);assert.equal(value.content,baseline!.content);assert.deepEqual(value.data,baseline!.data);assert.equal(value.font,expected!.font);evidence.live.push({name,...value});
 }
 writeFileSync(file,source);await waitFill(live,'rgb(22, 101, 52)');evidence.live.push({name:'switch-back-local',...await snapshot(live)});
 writeFileSync(file,source.replace('styles: [scientific, report.rstyle.yaml]\n',''));unlinkSync(join(folder,'report.rstyle.yaml'));
 await waitFill(live,defaults.bar!.fill);evidence.live.push({name:'no-yaml',...await snapshot(live)});
 await live.screenshot({path:join(h.dir,'native-live-default.png'),fullPage:true});
 evidence.checks.push('Native browser live preview observes local YAML save and source canonical switches, switch-back and no-YAML');
 // Browser authoring source/default fetches sibling YAML; the canonical selector overrides it.
 const webFolder=join(h.dir,'public/examples/style-check');mkdirSync(webFolder,{recursive:true});cpSync(folder,webFolder,{recursive:true});writeFileSync(join(webFolder,'quarter.rmd'),source);writeFileSync(join(webFolder,'report.rstyle.yaml'),custom('#a52a2a'));
 writeFileSync(join(webFolder,'report-folder.json'),JSON.stringify({root:'style-check',source:'quarter.rmd',files:['quarter.rmd','report.rstyle.yaml','data/sales.csv','media/logo.svg']}));
 // The harness serves only dist/web, so this test's example folder is answered from disk at the same origin.
 const mime: Record<string, string> = {'.rmd':'text/markdown','.yaml':'text/yaml','.csv':'text/csv','.svg':'image/svg+xml','.json':'application/json'};
 await context.route(h.base+'/examples/style-check/**',route=>{const rel=decodeURIComponent(new URL(route.request().url()).pathname.slice('/examples/style-check/'.length));const path=join(webFolder,rel);try{route.fulfill({status:200,body:readFileSync(path),headers:{'content-type':mime[path.slice(path.lastIndexOf('.'))]||'text/plain','cache-control':'no-store'}});}catch{route.fulfill({status:404,body:'Not found'});}});
 // The host's _headers file gives the runtime assets Access-Control-Allow-Origin so sandboxed previews can load them; the static harness ignores it, so apply the same entries.
 const cors=[...readFileSync(join(process.env.RICHVIEW_VERIFY_PUBLIC_DIR||join(root,'dist/web'),'_headers'),'utf8').matchAll(/^(\/\S+)\n\s+Access-Control-Allow-Origin: \*/gm)].map(m=>m[1]);
 assert.ok(cors.includes('/richview_runtime_bg.wasm'),cors.join());
 await context.route(url=>cors.includes(url.pathname),async route=>{const response=await route.fetch();await route.fulfill({response,headers:{...response.headers(),'access-control-allow-origin':'*'}});});
 const author=await context.newPage();watchErrors(author,evidence.errors);responses(author);await author.goto(h.base+'/playground?src=/examples/style-check/quarter.rmd');
 const frame=author.frameLocator('#preview');await waitChart(frame);let value=await snapshot(frame);assert.equal(value.bar!.fill,local.bar!.fill);assert.equal(value.content,baseline!.content);evidence.browser.push({name:'source-local',...value});
 writeFileSync(join(webFolder,'report.rstyle.yaml'),custom('#166534'));await author.locator('#validate').click();
 await waitFill(frame,'rgb(22, 101, 52)');evidence.browser.push({name:'yaml-edit-refresh',...await snapshot(frame)});
 // The five builtin style pills replace the source's declared styles in the sandboxed preview; the real
 // colours are the ones the CLI exported for the same style, and choosing the checked pill again returns to the source's own.
 const pill=(name: string)=>author.locator(`#playground-styles [data-theme="${name}"]`);
 assert.deepEqual(await author.locator('#playground-styles [role=radio]').evaluateAll(n=>n.map(b=>b.dataset.theme).sort()),builtins,'pills offer exactly the builtin styles');
 for(const name of ['scientific','sleek','brutalist','air','hand-drawn']){
  await pill(name).click();
  const expected=evidence.exports.find(v=>v.name===name);
  await waitFill(frame,expected!.bar!.fill);
  assert.equal(await pill(name).getAttribute('aria-checked'),'true');
  assert.equal(await author.locator('#playground-styles [aria-checked=true]').count(),1);
  value=await snapshot(frame);assert.equal(value.content,baseline!.content);assert.deepEqual(value.data,baseline!.data);assert.equal(value.font,expected!.font);assert.equal(value.bg,expected!.bg);evidence.browser.push({name,...value});
 }
 // Copy link, then a fresh load of that link restores the chosen style and the edited source. A shared draft is
 // self-contained (a link cannot carry a report folder), so this uses an inline-data draft on a fresh playground.
 await context.grantPermissions(['clipboard-read','clipboard-write'],{origin:h.base});
 const sharer=await context.newPage();watchErrors(sharer,evidence.errors);responses(sharer);await sharer.goto(h.base+'/playground');
 const draft=(await sharer.locator('#source').inputValue()).replace('Revenue rose after February','Revenue rose after February (shared)');
 await sharer.locator('#source').fill(draft);
 await sharer.locator('#playground-styles [data-theme="brutalist"]').click();
 const brutalist=evidence.exports.find(v=>v.name==='brutalist');
 await waitChart(sharer.frameLocator('#preview'));
 assert.equal((await snapshot(sharer.frameLocator('#preview'))).bg,brutalist!.bg);
 await sharer.locator('#copy-link').click();
 await sharer.locator('#pg-status').filter({hasText:'Link copied'}).waitFor();
 const link=await sharer.evaluate(()=>navigator.clipboard.readText());
 assert.ok(link.startsWith(h.base+'/playground#'),link.slice(0,80));
 await sharer.close();
 const restored=await context.newPage();watchErrors(restored,evidence.errors);responses(restored);await restored.goto(link);
 const restoredFrame=restored.frameLocator('#preview');await waitChart(restoredFrame);
 assert.equal(await restored.locator('#source').inputValue(),draft);
 assert.equal(await restored.locator('#playground-styles [aria-checked=true]').getAttribute('data-theme'),'brutalist');
 value=await snapshot(restoredFrame);assert.equal(value.bg,brutalist!.bg);assert.equal(value.font,brutalist!.font);assert.equal(value.bar!.fill,brutalist!.bar!.fill);evidence.browser.push({name:'restored-from-link',...value});
 await restored.close();
 // Daniel, 4 Nov: "the playground did not work". A draft naming a file the playground's folder lacks renders with that element
 // marked and one finding, as in the command line (FEA-RP-30), where it blanked the preview and printed JSON; Check says what it
 // found; the source box ends inside its pane, where it hid the end of every line.
 const drafter=await context.newPage();watchErrors(drafter,evidence.errors);responses(drafter);await drafter.goto(h.base+'/playground');
 await drafter.locator('#diagnostics').filter({hasText:'Rust render'}).waitFor();
 assert.ok(await drafter.evaluate(()=>document.querySelector('#source')!.getBoundingClientRect().right<=document.querySelector('.pg__source')!.getBoundingClientRect().right),'the source box ends inside its pane');
 await drafter.locator('#source').fill((await drafter.locator('#source').inputValue()).replace('## What moved','## What moved\n\n![Company logo](logo.png)'));
 await drafter.waitForFunction(()=>document.querySelector('#diagnostics')!.textContent.includes("Asset 'logo.png' is absent")&&document.querySelector('#preview')!.getAttribute('srcdoc')!=='');
 await drafter.frameLocator('#preview').getByText('Company logo').waitFor();
 assert.equal((await drafter.locator('#diagnostics').textContent())!.split("Asset 'logo.png' is absent").length-1,1,'one finding for the missing image');
 await drafter.locator('#validate').click();
 await drafter.locator('#pg-status').filter({hasText:'Checked: 1 error'}).waitFor();
 await drafter.close();
 evidence.checks.push('A draft naming an image the folder lacks renders with its alt text and one finding; Check says what it found; the source box ends inside its pane');
 evidence.checks.push('Playground pills render each builtin style in the sandboxed preview with the exported colours, and Copy link then reload restores style and source');
 // Choosing the checked pill again clears it and the source's own styles (local YAML) apply again.
 await pill('hand-drawn').click();
 assert.equal(await author.locator('#playground-styles [aria-checked=true]').count(),0);
 await author.locator('#source').fill(source);
 await waitFill(frame,'rgb(22, 101, 52)');evidence.browser.push({name:'switch-back-source',...await snapshot(frame)});
 await author.screenshot({path:join(h.dir,'browser-authoring-local.png'),fullPage:true});
 await author.locator('#source').fill(source.replace('styles: [scientific, report.rstyle.yaml]\n',''));unlinkSync(join(webFolder,'report.rstyle.yaml'));
 await waitFill(frame,defaults.bar!.fill);evidence.browser.push({name:'no-yaml',...await snapshot(frame)});
 // A retired header dataset is a finding and the rest of the draft renders, as in the command line (FEA-RP-30).
 await author.locator('#source').fill(source.replace('styles: [scientific, report.rstyle.yaml]','data: {registry: {inline: []}}'));
 await author.waitForFunction(()=>document.querySelector('#diagnostics')!.textContent.includes('frontmatter.data')&&document.querySelector('#preview')!.getAttribute('srcdoc')!=='');
 // An unusable folder clears the preview rather than retaining stale content.
 writeFileSync(join(webFolder,'report-folder.json'),JSON.stringify({root:'style-check',source:'quarter.rmd',files:['quarter.rmd','another.rmd']}));
 await author.locator('#validate').click();await author.waitForFunction(()=>document.querySelector('#diagnostics')!.textContent.includes('report.root.shared')&&document.querySelector('#preview')!.getAttribute('srcdoc')==='');
 evidence.checks.push('Browser shared-root rejection clears preview (the HTTP render API is gone, FEA-SF-17)');
 evidence.network_errors=networkErrors;assert.deepEqual(networkErrors,[]);assert.deepEqual(evidence.errors,[]);
 evidence.checks.push('Browser authoring loads custom YAML, refreshes after edits, overrides canonical styles, switches back and accepts no YAML; a retired header dataset is a finding and the draft renders');
 evidence.evidence_dir=h.dir;h.save('authoring.json',evidence);console.log(JSON.stringify({checks:evidence.checks,evidence_dir:h.dir,exports:evidence.exports.map(({name,font,bg,bar})=>({name,font,bg,bar})),errors:evidence.errors},null,2));
}finally{preview?.kill('SIGINT');await h.close();}
