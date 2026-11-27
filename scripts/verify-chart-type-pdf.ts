// Host reason (ARC-03): verify actual native/Wasm print-width PDF paints and selectable labels.
// Both targets share engine source and Chrome serialization. This is partial FEA-CH-75 evidence.
import assert from 'node:assert/strict';
import { AsyncLocalStorage } from 'node:async_hooks';
import { channel } from 'node:diagnostics_channel';
import { setTimeout as sleep } from 'node:timers/promises';
import { execFileSync, spawnSync, type ChildProcess } from 'node:child_process';
import { createHash } from 'node:crypto';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, readlinkSync, writeFileSync } from 'node:fs';
import { basename, dirname, extname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
import vm from 'node:vm';
import { chromium, errors, type Browser, type BrowserContext, type Route } from 'playwright';
import { launchBrowser } from '../tools/rv-gate/lib/browser-launch.mts';
import { waitForBenchmark } from '../tools/rv-gate/lib/quiet.mts';
import { paintedShapes, pageStreams, parseBlocks, parseWords, locateTitles, overlapShare } from './export-types.ts';
import { builtinStyles, chartTypes, requireTypes, matchPainted, selectableLabels, framePaints, chartPaperRoots } from './chart-type-evidence.ts';
// A process is its ids, its start time and its executable, none of which it can change while it runs: Linux's
// /proc/<pid>/exe, and macOS's ucomm, the name the kernel took from the executable at exec. ps's comm is no
// identity: on Linux it is the main thread's name, which Node renames from node to node-MainThread once it has
// started, and on both systems process.title rewrites it. null: no such process.
export function identity(pid: number){
 const linux=process.platform==='linux';
 try{const line=execFileSync('/bin/ps',['-p',String(pid),'-o','pid=,ppid=,pgid=,lstart='+(linux?'':',ucomm=')],{encoding:'utf8',timeout:1000}).trim();const m=line.match(linux?/^(\d+)\s+(\d+)\s+(\d+)\s+(.{24})$/:/^(\d+)\s+(\d+)\s+(\d+)\s+(.{24})\s+(.+)$/);if(!m)throw Error('Unrecognized own-process identity');return{pid:+m[1],ppid:+m[2],pgid:+m[3],birth:m[4],exe:linux?readlinkSync(`/proc/${pid}/exe`):m[5]};}catch(e){if((e as Error & { status?: number }).status===1||((e as Error & { code?: string }).code==='ENOENT'&&(e as Error & { syscall?: string }).syscall==='readlink'))return null;throw e;}
}
type OwnedRoot = {
 child: ChildProcess & { pid: number };
 file: string;
 detached: true;
 done: boolean;
 identity?: ReturnType<typeof identity>;
 error?: string;
 killed?: boolean;
};
// The Node spawn tracing event is not typed by diagnostics_channel.
type SpawnStart = { process: OwnedRoot['child']; options: { file: string; detached?: boolean } };
export function ownRoots(){
 const scope=new AsyncLocalStorage<OwnedRoot[]>(),spawn=channel('tracing:child_process.spawn:start'),roots: OwnedRoot[]=[];
 const observe=(message: unknown)=>{const {process:child,options}=message as SpawnStart;if(scope.getStore()!==roots||!options.detached)return;const item: OwnedRoot={child,file:options.file,detached:true,done:false};roots.push(item);child.once('spawn',()=>{try{item.identity=identity(child.pid);if(!item.identity||item.identity.ppid!==process.pid||item.identity.pgid!==child.pid)throw Error('Observed root lacks own detached identity');}catch(e){item.error=(e as Error).message;}});child.once('close',()=>item.done=true);};
 spawn.subscribe(observe);
 const absent=(item: { child: { pid: number; }; })=>{try{process.kill(-item.child.pid,0);return false;}catch(e){if((e as Error & { code?: string }).code==='ESRCH')return true;if((e as Error & { code?: string }).code==='EPERM')return false;throw e;}};
 function kill(){for(const item of roots){if(!item.child.pid||absent(item))continue;if(item.error||!item.identity)throw Error('Refusing unverified root: '+item.error);const now=identity(item.child.pid);if(!now||JSON.stringify(now)!==JSON.stringify(item.identity))throw Error('Refusing signal: own root identity changed or leader unavailable');process.kill(-item.identity.pgid,'SIGKILL');item.killed=true;}}
 async function drained(ms=1500){const until=Date.now()+ms;while(Date.now()<until){if(roots.every(item=>item.done&&absent(item)))return true;await sleep(25);}return roots.every(item=>item.done&&absent(item));}
 return{run:<T>(fn: () => T)=>scope.run(roots,fn),kill,drained,stop:()=>spawn.unsubscribe(observe),snapshot:()=>roots.map(item=>({file:item.file,identity:item.identity,error:item.error,killed:!!item.killed,closed:item.done,groupAbsent:!!item.child.pid&&absent(item)}))};
}
export async function bounded(promise: Promise<void>,ms: number){let timer: NodeJS.Timeout | undefined;try{return await Promise.race([promise,new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error('Owned close exceeded '+ms+'ms')),ms);})]);}finally{clearTimeout(timer);}}

const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');
const hash=(bytes: string | Buffer)=>createHash('sha256').update(bytes).digest('hex');
// Match the existing export reader's capacity: pencil paths inflate substantially in QDF.
const PDF_BUFFER = 256 << 20;
const run=(cmd: string,args: readonly string[])=>execFileSync(cmd,args,{encoding:'utf8',maxBuffer:PDF_BUFFER});
const attrs=(s: string)=>Object.fromEntries([...s.matchAll(/([\w:-]+)="([^"]*)"/g)].map(m=>[m[1],m[2]]));
const entities: Record<string, string> = {'&amp;':'&','&lt;':'<','&gt;':'>','&quot;':'"','&apos;':"'",'&#39;':"'"};
const decode=(s: string)=>s.replace(/<[^>]*>/g,'').replace(/&#(?:x([0-9a-f]+)|(\d+));/gi,(_,h,n)=>String.fromCodePoint(parseInt(h??n,h?16:10))).replace(/&(?:amp|lt|gt|quot|apos|#39);/g,e=>entities[e]);
type Shape = ReturnType<typeof paintedShapes>[number];
const clipBox=(shape: Shape)=>{
  if(!shape.clips.length)return null;
  const boxes=shape.clips.map(clip=>{
    assert.ok(clip.commands.length && clip.commands.every((c: { c: string; })=>['m','l','c','z'].includes(c.c)), 'explicit vector chart clip');
    // Bezier control bounds conservatively associate a clipped mark with its frame.
    // Every control and clipping rule remains exact in the paint signature.
    const points=clip.commands.flatMap(c=>c.c==='z'?[]:c.c==='c'?[c.c1,c.c2,c.p]:[c.p]);
    assert.ok(points.length && points.every(p=>p.length===2 && p.every(Number.isFinite)), 'finite clip controls');
    const xs=points.map(p=>p[0]),ys=points.map(p=>p[1]);
    return{left:Math.min(...xs),right:Math.max(...xs),top:Math.min(...ys),bottom:Math.max(...ys)};
  });
  return{left:Math.max(...boxes.map(b=>b.left)),right:Math.min(...boxes.map(b=>b.right)),top:Math.max(...boxes.map(b=>b.top)),bottom:Math.min(...boxes.map(b=>b.bottom))};
};
type PdfValue = null | boolean | number | string | PdfValue[] | PdfDictionary;
type PdfDictionary = { [key: string]: PdfValue };
type QpdfObject = { value?: PdfDictionary; stream?: { dict: PdfDictionary } };
type QpdfJson = { pages: { object: string; images: { name: string }[] }[]; qpdf: [unknown, ...Record<string, QpdfObject>[]] };
type TextRecord = { left: number; top: number; width: number; height: number; family: string; size: number; color: string; text: string };
function readPdf(pdf: string){
  const qdf=execFileSync('qpdf',['--qdf','--object-streams=disable',pdf,'-'],{maxBuffer:PDF_BUFFER}).toString('latin1');
  const bbox=run('pdftotext',['-bbox-layout',pdf,'-']);
  const pages=parseBlocks(bbox), words=parseWords(bbox);
  const streams=pageStreams(qdf);assert.equal(streams.length,pages.length);
  const json: QpdfJson=JSON.parse(run('qpdf',['--json','--json-key=pages','--json-key=qpdf',pdf]));
  const objects: Record<string, QpdfObject>=Object.assign({},...json.qpdf.slice(1));
  const deref=(v: PdfValue): PdfValue | undefined=>typeof v==='string'&&/^\d+ \d+ R$/.test(v)?objects['obj:'+v]?.value??objects['obj:'+v]?.stream?.dict:v;
  const canonical=(v: PdfValue | undefined,seen=new Set<string>()): PdfValue=>{
    if(typeof v==='string'&&/^\d+ \d+ R$/.test(v)){assert.ok(!seen.has(v),'cyclic paint resource');assert.ok(!objects['obj:'+v]?.stream,'unsupported stream-valued paint resource');return canonical(deref(v),new Set([...seen,v]));}
    if(Array.isArray(v))return v.map(x=>canonical(x,seen));
    if(v&&typeof v==='object')return Object.fromEntries(Object.keys(v).sort().map(k=>[k,canonical((v as PdfDictionary)[k],seen)]));
    assert.notEqual(v,undefined,'unresolved paint resource');return v!;
  };
  const pageDict=(i: number)=>deref(json.pages[i].object) as PdfDictionary;
  const resources=(i: number)=>{let page=pageDict(i);for(let depth=0;depth<20;depth++){if(page['/Resources'])return deref(page['/Resources']) as PdfDictionary;page=deref(page['/Parent']) as PdfDictionary;assert.ok(page,'page resources');}throw Error('page resource chain');};
  const shapes=streams.map((stream,i)=>{
    const states=(deref(resources(i)['/ExtGState'])??{}) as PdfDictionary;
    return paintedShapes(stream,pages[i].height).map(shape=>({...shape,paintState:shape.paintState.map(s=>{
      if(['SCN','scn'].includes(s.op))assert.ok(!s.args.some(arg=>arg.startsWith('/')),'unsupported pattern paint');
      if(['CS','cs'].includes(s.op))assert.ok(['/DeviceGray','/DeviceRGB','/DeviceCMYK'].includes(s.args[0]),'unsupported named colourspace');
      if(s.op!=='gs')return s;assert.equal(s.args.length,1);assert.ok(states[s.args[0]],'actual ExtGState resource');
      return{...s,resolved:canonical(states[s.args[0]])};
    })}));
  });
  const xml=run('pdftohtml',['-xml','-i','-noroundcoord','-fontfullname','-zoom','1','-stdout',pdf]);
  const fonts=new Map<string, Record<string, string>>();const text: TextRecord[][]=[];
  for(const m of xml.matchAll(/<page ([^>]+)>([\s\S]*?)<\/page>/g)){
    const page=+attrs(m[1]).number;
    for(const f of m[2].matchAll(/<fontspec ([^>]+)\/>/g)){const a=attrs(f[1]);fonts.set(a.id,a);}
    text[page-1]=[...m[2].matchAll(/<text ([^>]+)>([\s\S]*?)<\/text>/g)].map(t=>{
      const a=attrs(t[1]),font=fonts.get(a.font);assert.ok(font,'resolved selectable-text font');
      return{left:+a.left,top:+a.top,width:+a.width,height:+a.height,family:font.family.replace(/^[A-Z]{6}\+/,''),size:+font.size,color:font.color,text:decode(t[2])};
    });
  }
  const programs=[];
  for(const obj of Object.values(objects)){
    const v=obj.value;if(v?.['/Type']!=='/FontDescriptor'||!v['/FontFile2'])continue;
    const bytes=execFileSync('qpdf',['--show-object='+(v['/FontFile2'] as string).split(' ')[0],'--filtered-stream-data',pdf],{maxBuffer:8<<20});
    programs.push({name:(v['/FontName'] as string).replace(/^\/[A-Z]{6}\+/,''),sha256:hash(bytes)});
  }
  programs.sort((a,b)=>a.name.localeCompare(b.name));
  return{pdf,pages,shapes,text,words,programs,images:json.pages.map(p=>p.images),pdf_sha256:hash(readFileSync(pdf))};
}

function inspect(pdf: ReturnType<typeof readPdf>, { payloads, cohort, sources }: ReturnType<typeof sourceEvidence>){
  const titles=locateTitles(pdf.pages,payloads.map(p=>p.block.title));assert.ok(titles.every(Boolean),'all chart titles selectable');
  const papers=chartPaperRoots(pdf.shapes,sources);
  const charts=[],claimed=new Set();
  for(const[i,source]of sources.entries()){
    const place=titles[i],paper=papers[i];
    assert.equal(paper.page,place!.page,cohort[i].id+' paper and selectable caption share a page');
    const shapes=pdf.shapes[paper.page-1];
    const root=paper.shape,frame=clipBox(root);assert.ok(frame,'actual SVG frame clip');
    const rootIndex=shapes.indexOf(root);
    const marks=shapes.filter((s,j)=>{
      if(j<rootIndex)return false;
      if(s.kind==='path'&&s.source.every(op=>JSON.stringify(op.matrix)===JSON.stringify(root.matrix)))return true;
      const box=clipBox(s);if(!box)return false;
      // Skia can tighten a clip to a mark and lower strokes under another CTM. The actual
      // clip centre associates it with a frame; no coordinate is rounded for paint equality.
      const x=(box.left+box.right)/2,y=(box.top+box.bottom)/2;
      return x>=frame.left&&x<=frame.right&&y>=frame.top&&y<=frame.bottom&&s.kind==='path'&&s.left<=frame.right&&s.left+s.width>=frame.left&&s.top<=frame.bottom&&s.top+s.height>=frame.top;
    });
    assert.ok(marks.every((s: { kind: string; })=>s.kind==='path'),cohort[i].id+' contains unsupported/raster drawing');
    assert.ok(marks.length>1||cohort[i].type==='wordcloud',cohort[i].id+' needs vector marks beyond paper');
    for(const mark of marks){const key=place!.page+':'+shapes.indexOf(mark);assert.ok(!claimed.has(key),'one native painted path cannot serve two charts');claimed.add(key);}
    const wordRecords=pdf.text[place!.page-1].filter((t: { left: number; width: number; top: number; height: number; })=>{const x=t.left+t.width/2,y=t.top+t.height/2;return x>=frame.left&&x<frame.right&&y>=frame.top&&y<frame.bottom;});
    const selectableWords=pdf.words[place!.page-1].filter((t: { left: number; width: number; top: number; height: number; })=>{const x=t.left+t.width/2,y=t.top+t.height/2;return x>=frame.left&&x<frame.right&&y>=frame.top&&y<frame.bottom;});
    if(source.words.length)assert.ok(wordRecords.length,cohort[i].id+' actual selectable chart text');
    if(cohort[i].type==='wordcloud')for(const word of source.words)assert.equal(selectableWords.filter(t=>t.text===word).length,1,'wordcloud actual selectable word '+word);
    charts.push({...cohort[i],page:place!.page,root:{left:root.left,top:root.top,width:root.width,height:root.height,matrix:root.matrix},frame,source,marks,text:wordRecords,words:selectableWords});
  }
  const unsupported=[];
  for(const[page,shapes]of pdf.shapes.entries())for(const shape of shapes.filter(s=>s.kind!=='path')){
    assert.equal(shape.op,'Do','unsupported drawing never silently accepted');
    assert.ok(pdf.images[page].some(image=>image.name===shape.args[0]),'external paint is an actual Image resource');
    const[a,b,c,d,e,f]=shape.matrix,points=[[e,f],[a+e,b+f],[c+e,d+f],[a+c+e,b+d+f]],xs=points.map(p=>p[0]),ys=points.map(p=>pdf.pages[page].height-p[1]);
    const image={left:Math.min(...xs),top:Math.min(...ys),width:Math.max(...xs)-Math.min(...xs),height:Math.max(...ys)-Math.min(...ys)};
    assert.ok(charts.filter(chart=>chart.page===page+1).every(chart=>overlapShare({left:chart.frame.left,top:chart.frame.top,width:chart.frame.right-chart.frame.left,height:chart.frame.bottom-chart.frame.top},image)===0),'no external raster overlaps a chart frame');
    unsupported.push({page:page+1,name:shape.args[0],resolved:'authored Image outside every chart',box:image});
  }
  return{charts,external_images:unsupported};
}

function svgRuns(svg: string) {
  const runs = [], groups = [0];
  const angle = (tag: string) => [...(attrs(tag).transform ?? '').matchAll(/rotate\(\s*([+-]?[\d.]+)/g)].reduce((sum, m) => sum + Number(m[1]), 0);
  for (const m of svg.matchAll(/<\/g\s*>|<g\b[^>]*>|<text\b([^>]*)>([\s\S]*?)<\/text>/g)) {
    if (m[0].startsWith('</g')) { assert.ok(groups.length > 1, 'balanced SVG groups'); groups.pop(); }
    else if (m[0].startsWith('<g')) groups.push(groups.at(-1)! + angle(m[0]));
    else runs.push({ text: decode(m[2]), angle: groups.at(-1)! + angle(m[1]) });
  }
  assert.equal(groups.length, 1, 'balanced SVG groups');
  return runs;
}

type StyleEvidence = {
  style: string; native_html: string; native_pdf: string; reference_html: string; reference_pdf: string;
  native_html_sha256: string; reference_html_sha256: string;
};
type Payload = { id: string; block: { type: string; title: string }; columns: unknown; rows: unknown; shown: unknown; quant: unknown; text: unknown };
function sourceEvidence(row: StyleEvidence, expected: readonly string[]) {
  const html = readFileSync(row.native_html, 'utf8');
  const payloads: Payload[] = [...html.matchAll(/<script type="application\/json" data-rv-payload>([\s\S]*?)<\/script>/g)].map(m => JSON.parse(m[1]));
  const cohort = payloads.map(p => ({ id: p.id, type: p.block.type }));
  requireTypes(expected, cohort);
  const figures = [...html.matchAll(/<figure class="rv-chart" id="([^"]*)" data-rv-chart>([\s\S]*?)<\/figure>/g)];
  assert.deepEqual(figures.map(m => m[1]), cohort.map(c => c.id), 'native interactive figure/payload identities');
  const printHtml = readFileSync(row.reference_html, 'utf8');
  const printFigures = [...printHtml.matchAll(/<figure class="rv-chart rv-chart--print" id="([^"]*)">[\s\S]*?<div class="rv-chart__frame"[^>]*>(<svg\b[\s\S]*?<\/svg>)/g)];
  assert.deepEqual(printFigures.map(m => m[1]), cohort.map(c => c.id), 'native interactive and Wasm print figure identities');
  const sources = printFigures.map((m, i) => {
    const svg = m[2], a = attrs(svg.slice(0, svg.indexOf('>'))), body = svg.replace(/<defs>[\s\S]*?<\/defs>/g, '');
    const tags = [...body.matchAll(/<([a-zA-Z][\w:-]*)\b/g)].map(m => m[1]);
    assert.ok(tags.every(tag => ['svg', 'rect', 'path', 'circle', 'text', 'tspan', 'g', 'line', 'polyline', 'polygon', 'ellipse'].includes(tag)), 'supported print SVG primitive vocabulary');
    const counts = Object.fromEntries([...new Set(tags)].map(tag => [tag, tags.filter(t => t === tag).length]));
    const runs = svgRuns(body), frame = figures[i][2].match(/<div class="rv-chart__frame"[^>]*>/)?.[0];
    assert.ok(frame && decode(attrs(frame)['aria-label']).includes(decode(a['aria-label']).replace(/\.+$/, '')), cohort[i].id + ' interactive and print data summary');
    assert.ok(Number.isFinite(+a.width) && +a.width > 0 && Number.isFinite(+a.height) && +a.height > 0);
    return { width: +a.width, height: +a.height, counts, runs, words: runs.map(run => run.text) };
  });
  const data = payloads.map(p => Object.fromEntries((['id', 'block', 'columns', 'rows', 'shown', 'quant', 'text'] as const).map(k => [k, p[k]])));
  return { payloads, cohort, sources, data };
}

function compareStyle(row: StyleEvidence, expected: readonly string[]) {
  const started = performance.now(), source = sourceEvidence(row, expected);
  const native = readPdf(row.native_pdf), reference = readPdf(row.reference_pdf);
  const a = inspect(native, source), b = inspect(reference, source), failures = [];
  const chartFamilies = [...new Set(b.charts.flatMap(c => c.text.map(t => t.family)))].sort();
  const nativeFonts = native.programs.filter(f => chartFamilies.includes(f.name)), referenceFonts = reference.programs.filter(f => chartFamilies.includes(f.name));
  assert.deepEqual(nativeFonts.map(f => f.name).sort(), chartFamilies, 'every actual chart font has an embedded TrueType program');
  try { assert.deepEqual(nativeFonts, referenceFonts); } catch { failures.push({ contract: 'chart embedded font programs differ' }); }
  const results = a.charts.map((actual, i) => {
    const wanted = b.charts[i], paints = matchPainted(wanted.marks, actual.marks);
    const nativeLabels = selectableLabels(source.sources[i].runs, actual.words.map(t => t.text));
    const referenceLabels = selectableLabels(source.sources[i].runs, wanted.words.map(t => t.text));
    let text = true; try { assert.deepEqual(actual.text, wanted.text); } catch { text = false; }
    const mapping = actual.page === wanted.page && JSON.stringify(actual.root.matrix) === JSON.stringify(wanted.root.matrix);
    const ok = paints.ok && text && mapping && nativeLabels.ok && referenceLabels.ok;
    if (!ok) failures.push({ id: actual.id, paints: paints.ok, text, mapping, nativeLabels, referenceLabels, missing: paints.missing, extra: paints.extra, reordered: paints.reordered, unsupported: paints.unsupported });
    return { id: actual.id, type: actual.type, page: actual.page, root: actual.root, source_svg: actual.source.counts, painted_marks: actual.marks.length, unique_matches: paints.matches.length, text_records: actual.text.length, labels: nativeLabels, ok };
  });
  const allNative = framePaints(native.shapes, a.charts), allReference = framePaints(reference.shapes, b.charts);
  const lastChartPage = Math.max(...a.charts.map(c => c.page), ...b.charts.map(c => c.page));
  const completePaints = Array.from({ length: lastChartPage }, (_, i) => {
    const paths = allNative.pages[i] ?? [], expectedPaths = allReference.pages[i] ?? [];
    if (!paths.length && !expectedPaths.length) return { page: i + 1, paths: 0, ok: true };
    const match = matchPainted(expectedPaths, paths);
    if (!match.ok) failures.push({ page: i + 1, contract: 'all frame-intersecting paints', missing: match.missing, extra: match.extra, reordered: match.reordered, unsupported: match.unsupported });
    return { page: i + 1, paths: paths.length, matches: match.matches.length, ok: match.ok };
  });
  return { style: row.style, scope: 'A4/default state/one every-block instance per type (two heatmaps), print-width marks', cohort: requireTypes(expected, results), native_pdf_sha256: native.pdf_sha256, reference_pdf_sha256: reference.pdf_sha256, chart_fonts: nativeFonts, external_images: a.external_images, frame_paint_classification: allNative.classification, complete_frame_paints: completePaints, results, failures, seconds: (performance.now() - started) / 1000, ok: !failures.length };
}

type FileDigest = { bytes: number; sha256: string };
type BuildManifest = { schema: number; mode: string; inputs: { dirty: boolean; commit: string }; outputs: Record<string, FileDigest> };
function bundleRecords(bundle?: string) {
  const manifestFile = bundle ? join(bundle, 'build-manifest.json') : join(root, 'target/release/richview-build-manifest.json');
  const manifest: BuildManifest = JSON.parse(readFileSync(manifestFile, 'utf8'));
  assert.equal(manifest.schema, 1); assert.equal(manifest.mode, 'full'); assert.equal(manifest.inputs.dirty, false);
  const cli = bundle ? join(bundle, 'richview') : resolve(process.env.RICHVIEW_BIN ?? process.env.RICHVIEW_CLI ?? join(root, 'target/release/richview'));
  const web = bundle ? join(bundle, 'dist/web') : resolve(process.env.RICHVIEW_DIST ?? join(root, 'dist/web'));
  const records: Record<string, FileDigest> = {};
  for (const [key, expected] of Object.entries(manifest.outputs)) {
    let file;
    if (key.startsWith('checkout:dist/web/')) file = join(web, key.slice('checkout:dist/web/'.length));
    else if (['target:release/richview', 'target:release/richview-editor', 'target:release/richview-sql-worker'].includes(key)) file = join(dirname(cli), key.slice('target:release/'.length));
    if (file) records[file] = expected;
  }
  for (const file of [cli, join(web, 'richview_site.js'), join(web, 'richview_site_bg.wasm'), join(web, 'richview-fonts.css')]) assert.ok(records[file], 'manifest binds ' + file);
  return { cli, web, commit: manifest.inputs.commit, manifest_file: manifestFile, manifest_sha256: hash(readFileSync(manifestFile)), records };
}

function verifyBundle(binding: ReturnType<typeof bundleRecords>) {
  assert.equal(hash(readFileSync(binding.manifest_file)), binding.manifest_sha256, 'build manifest unchanged');
  for (const [file, expected] of Object.entries(binding.records)) {
    const bytes = readFileSync(file);
    assert.equal(bytes.length, expected.bytes, 'build output size ' + file);
    assert.equal(hash(bytes), expected.sha256, 'build output SHA ' + file);
  }
}

function inventory(folder: string) {
  const files: string[] = [];
  const walk = (dir: string, prefix = '') => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      if (entry.name === 'Rendered' || entry.name.startsWith('.')) continue;
      const name = prefix + entry.name;
      if (entry.isDirectory()) walk(join(dir, entry.name), name + '/'); else files.push(name);
    }
  };
  walk(folder); return files.sort();
}

type Options = { out?: string; source?: string; bundle?: string; browser?: string; 'prepare-only'?: boolean; prepared?: string; evidence?: string };
type NativeRun = { style: string; seconds: number; browser: unknown; pages: number };
type ReferenceRun = { style: string; seconds: number; settled: { images: { failed: unknown[] } }; floated: unknown; errors: string[] };
type PdfEvidence = {
  version: number; binding: ReturnType<typeof bundleRecords>; builtins: string[]; expected: string[];
  input: string; input_hashes: Record<string, string>; styles: StyleEvidence[]; prepared_at: string; producer_sha256: string;
  capture?: {
    seconds: number; browser: string; native_runs: NativeRun[]; reference_runs: ReferenceRun[];
    reference_browser_instances: number; parallel_reference_pages: number; max_active_browser_roots: number;
    native_host_source_sha256: string; producer_sha256: string;
  };
};
async function prepare(options: Options, out: string): Promise<PdfEvidence> {
  const binding = bundleRecords(options.bundle && resolve(options.bundle)); verifyBundle(binding);
  const builtins = builtinStyles(run(binding.cli, ['learn', 'style']));
  const expected = chartTypes(run(binding.cli, ['learn', 'chart-contract']));
  const input = join(out, 'input'), original = resolve(options.source ?? join(root, 'examples/every-block/report.rmd'));
  mkdirSync(input, { recursive: true });
  cpSync(dirname(original), input, { recursive: true, filter: file => basename(file) !== 'Rendered' && !basename(file).startsWith('.') });
  assert.equal(basename(original), 'report.rmd', 'the fixture uses its original source filename');
  const files = inventory(input), source = readFileSync(join(input, 'report.rmd'), 'utf8'), data: Record<string, string> = {};
  const mime: Record<string, string> = { '.png': 'image/png', '.jpg': 'image/jpeg', '.jpeg': 'image/jpeg', '.webp': 'image/webp', '.svg': 'image/svg+xml', '.gif': 'image/gif', '.mp4': 'video/mp4', '.webm': 'video/webm', '.mp3': 'audio/mpeg', '.wav': 'audio/wav' };
  const inputHashes = Object.fromEntries(files.map(file => [file, hash(readFileSync(join(input, file)))]));
  for (const file of files.filter(file => file !== 'report.rmd')) {
    const bytes = readFileSync(join(input, file)), type = mime[extname(file)];
    data[file] = type ? `data:${type};base64,${bytes.toString('base64')}` : bytes.toString('utf8');
  }
  const context = vm.createContext({ WebAssembly, TextEncoder, TextDecoder, URL, Request, Response, fetch, console, setTimeout, clearTimeout, wasm_bytes: readFileSync(join(binding.web, 'richview_site_bg.wasm')) });
  await vm.runInContext(readFileSync(join(binding.web, 'richview_site.js'), 'utf8') + '\nwasm_bindgen({module_or_path:wasm_bytes});', context);
  const api: { provide_font_catalog(css: string): boolean; render_source(source: string, options: string): string } = vm.runInContext('wasm_bindgen', context);
  assert.equal(api.provide_font_catalog(readFileSync(join(binding.web, 'richview-fonts.css'), 'utf8')), true);
  const styles = [];
  for (const style of builtins) {
    const result: { html: string; diagnostics: { level: string }[] } = JSON.parse(api.render_source(source, JSON.stringify({ styles: [style], output: 'pdf', data, report_folder: { root: 'every-block', source: 'report.rmd', files } })));
    assert.ok(!result.diagnostics.some((d: { level: string; }) => d.level === 'error'), style + ' Wasm print errors');
    const referenceHtml = join(out, `reference-${style}.html`); writeFileSync(referenceHtml, result.html);
    const name = `native-${style}.html`;
    const native = JSON.parse(run(binding.cli, ['generate', join(input, 'report.rmd'), '--style', style, '-o', name, '--format', 'json']));
    assert.equal(native.ok, true, style + ' native interactive generation');
    const row = { style, native_html: join(input, 'Rendered', name), native_pdf: join(input, 'Rendered', `native-${style}.pdf`), reference_html: referenceHtml, reference_pdf: join(out, `reference-${style}.pdf`), reference_html_sha256: hash(readFileSync(referenceHtml)), native_html_sha256: hash(readFileSync(join(input, 'Rendered', name))) };
    sourceEvidence(row, expected); styles.push(row);
  }
  const evidence = { version: 1, binding, builtins, expected, input, input_hashes: inputHashes, styles, prepared_at: new Date().toISOString(), producer_sha256: hash(readFileSync(fileURLToPath(import.meta.url))) };
  verifyBundle(binding); writeFileSync(join(out, 'evidence.json'), JSON.stringify(evidence, null, 2));
  return evidence;
}

function verifyInputs(evidence: PdfEvidence) {
  verifyBundle(evidence.binding);
  assert.deepEqual(inventory(evidence.input), Object.keys(evidence.input_hashes).sort(), 'the complete original folder inventory');
  for (const [file, digest] of Object.entries(evidence.input_hashes)) assert.equal(hash(readFileSync(join(evidence.input, file))), digest, 'source/data input unchanged ' + file);
  for (const row of evidence.styles) {
    assert.equal(hash(readFileSync(row.native_html)), row.native_html_sha256);
    assert.equal(hash(readFileSync(row.reference_html)), row.reference_html_sha256);
  }
}

async function capture(evidence: PdfEvidence, options: Options, out: string) {
  verifyInputs(evidence);
  const browserPath = resolve(options.browser ?? process.env.RICHVIEW_BROWSER ?? (existsSync('/Applications/Google Chrome.app/Contents/MacOS/Google Chrome') ? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' : chromium.executablePath()));
  const nativeSource = run('git', ['show', evidence.binding.commit + ':crates/richview-app/src/export_pdf.rs']);
  const hostScript = (name: string) => { const match = nativeSource.match(new RegExp('const ' + name + ': &str = r#"([\\s\\S]*?)"#;')); assert.ok(match, 'actual frozen native host ' + name); return match[1]; };
  const settle = hostScript('SETTLE'), float = hostScript('FLOAT'), nativeRuns: NativeRun[] = [];
  const started = performance.now();
  for (const row of evidence.styles) {
    await waitForBenchmark();
    const start = performance.now();
    const result = spawnSync(evidence.binding.cli, ['generate', join(evidence.input, 'report.rmd'), '--style', row.style, '-o', basename(row.native_pdf), '--to', 'pdf', '--browser', browserPath, '--format', 'json'], { encoding: 'utf8', timeout: 180000, maxBuffer: 16 << 20 });
    writeFileSync(join(out, `native-${row.style}.json`), result.stdout || ''); writeFileSync(join(out, `native-${row.style}.stderr`), result.stderr || '');
    for (const line of String(result.stderr ?? '').split('\n')) if (line.startsWith('RICHVIEW_BROWSER_RELAUNCH ')) console.error(line);
    assert.equal(result.status, 0, row.style + ' actual native PDF process');
    const value: { ok: boolean; browser: unknown; pages: number } = JSON.parse(result.stdout); assert.equal(value.ok, true, row.style + ' actual native PDF');
    nativeRuns.push({ style: row.style, seconds: (performance.now() - start) / 1000, browser: value.browser, pages: value.pages });
  }
  await waitForBenchmark();
  let browser: Browser | undefined;
  let context: BrowserContext | undefined;
  const referenceRuns: ReferenceRun[] = [], owned = ownRoots(), phaseStart = Date.now();
  const closure: {
    phase: string; status: string; cap_ms: number; work_ms: number; emergency_ms: number;
    close_error?: string; closed?: boolean; cleanup_error?: string; roots?: ReturnType<typeof owned.snapshot>;
    hashes?: string; hash_error?: string; elapsed_ms?: number;
  } = { phase: 'pdf-reference', status: 'running', cap_ms: 55000, work_ms: 40000, emergency_ms: 48000 };
  let expired = false;
  let finishing: Promise<void> | undefined;
  const finish = (force = false) => finishing ??= (async () => {
    if (!force) try { await bounded((async () => { try { await context?.close(); } finally { await browser?.close(); } })(), 3000); } catch (error) { closure.close_error = String(error); }
    try { owned.kill(); closure.closed = await owned.drained(1500); } catch (error) { closure.cleanup_error = String(error); closure.closed = false; }
    try { closure.roots = owned.snapshot(); } finally { owned.stop(); }
    try { verifyInputs(evidence); closure.hashes = 'all bound inputs unchanged'; } catch (error) { closure.hash_error = String(error); }
    closure.elapsed_ms = Date.now() - phaseStart;
    closure.status = expired || closure.elapsed_ms >= 55000 ? 'no-verdict/cap' : closure.closed && !closure.hash_error ? 'closed' : 'incomplete';
    writeFileSync(join(out, 'reference-closure.json'), JSON.stringify(closure, null, 2));
    clearTimeout(absolute); clearTimeout(emergency); clearTimeout(work);
    assert.equal(closure.status, 'closed', 'reference phase closes and verifies inside its absolute cap');
  })();
  const stop = () => { expired = true; void browser?.close().catch(() => {}); };
  const force = () => { expired = true; void finish(true).finally(() => process.exit(124)); };
  const absolute = setTimeout(force, 55000), emergency = setTimeout(force, 48000), work = setTimeout(stop, 40000);
  try {
    verifyInputs(evidence);
    await (async () => {
    const launched: Browser = await owned.run(() => launchBrowser(chromium, { headless: true, executablePath: browserPath, args: ['--no-sandbox', '--disable-gpu'] }, { TimeoutError: errors.TimeoutError }));
    browser = launched;
    context = await launched.newContext({ viewport: { width: 658, height: 964 }, deviceScaleFactor: 1, reducedMotion: 'reduce', colorScheme: 'light' });
    await context.route('**/*', (route: Route) => /^(file:|data:|blob:)/.test(route.request().url()) ? route.continue() : route.abort('blockedbyclient'));
    const workers = Math.min(evidence.styles.length, Number(process.env.RICHVIEW_VERIFY_WORKERS ?? 2));
    assert.ok(Number.isSafeInteger(workers) && workers > 0, 'positive reserved reference page workers');
    let next = 0, stopped = false;
    const print = async (row: StyleEvidence) => {
      const page = await context!.newPage(), problems: string[] = [], start = performance.now();
      page.on('pageerror', error => problems.push(String(error)));
      try {
        await page.emulateMedia({ media: 'print', reducedMotion: 'reduce', colorScheme: 'light' });
        await page.goto(pathToFileURL(row.reference_html).href, { waitUntil: 'load' });
        const settled = await page.evaluate<ReferenceRun['settled']>(settle); assert.deepEqual(settled.images.failed, []);
        const floated = await page.evaluate(float), cdp = await context!.newCDPSession(page);
        const pdf = await cdp.send('Page.printToPDF', { printBackground: true, preferCSSPageSize: true, displayHeaderFooter: false, marginTop: 0, marginBottom: 0, marginLeft: 0, marginRight: 0, transferMode: 'ReturnAsBase64', generateTaggedPDF: true, generateDocumentOutline: true });
        assert.ok(pdf.data); assert.deepEqual(problems, []);
        writeFileSync(row.reference_pdf, Buffer.from(pdf.data, 'base64'));
        referenceRuns.push({ style: row.style, seconds: (performance.now() - start) / 1000, settled, floated, errors: problems });
      } finally { await page.close(); }
    };
    const results = await Promise.allSettled(Array.from({ length: workers }, async () => {
      while (!stopped && next < evidence.styles.length) {
        const row = evidence.styles[next++];
        try { await print(row); } catch (error) { stopped = true; throw error; }
      }
    }));
    for (const result of results) if (result.status === 'rejected') throw result.reason;
    assert.equal(expired, false, 'reference work completed before its cap');
    evidence.capture = { seconds: (performance.now() - started) / 1000, browser: launched.version(), native_runs: nativeRuns, reference_runs: referenceRuns, reference_browser_instances: 1, parallel_reference_pages: workers, max_active_browser_roots: 1, native_host_source_sha256: hash(nativeSource), producer_sha256: hash(readFileSync(fileURLToPath(import.meta.url))) };
    })();
  } finally {
    await finish();
  }
  writeFileSync(join(out, 'evidence.json'), JSON.stringify(evidence, null, 2));
}

function compareAll(evidence: PdfEvidence, out: string) {
  verifyInputs(evidence);
  const builtins = builtinStyles(run(evidence.binding.cli, ['learn', 'style'])), expected = chartTypes(run(evidence.binding.cli, ['learn', 'chart-contract']));
  assert.deepEqual(evidence.builtins, builtins); assert.deepEqual(evidence.expected, expected);
  assert.equal(new Set(evidence.styles.map(row => row.style)).size, evidence.styles.length, 'unique styles');
  const missing = builtins.filter((style: string) => !evidence.styles.some((row: { style: string; }) => row.style === style)), extra = evidence.styles.filter((row: { style: string; }) => !builtins.includes(row.style)).map(row => row.style);
  const results = evidence.styles.map(row => {
    const result = compareStyle(row, expected);
    writeFileSync(join(out, `style-${row.style}.json`), JSON.stringify(result, null, 2));
    return result;
  });
  const data = evidence.styles.map(row => sourceEvidence(row, expected).data);
  for (const current of data.slice(1)) assert.deepEqual(current, data[0], 'all styles preserve the same author data/readout payload');
  const proof = { at: new Date().toISOString(), build: evidence.binding.commit, styles: builtins, missing, extra, results, limits: ['Native and Wasm share the richview-core/richview-chart engine source and Chrome serializer; this proves print-width parity, not independent engine correctness or screen pixel equivalence.', 'A4/default author state/every-block; one instance per type, two heatmaps. Other pages, control states, user fonts and physical VoiceOver are outside this check.', 'Ordinary labels require the exact SVG word multiset. Arbitrary-angle labels require the non-space character multiset. Separate 2026-10-30 probes of native Air build 51c9db0e, SHA-256 3f3c388d7d1d8b4a9190823a30c8d951cd168621e774c8d9690807ece6c4cd84, page 31 (retained evidence on the build host), verify six whole labels with macOS 26.3 PDFKit (build 1451.3.3) find and PDFium 156.0.8076.0 whole-word Find/selection ranges; pdf.js core 5.6.205 extracts six whole items. Chrome 156.0.8078.76 text-fragment find plus trusted selection verifies five labels; Sales target disambiguation and Firefox viewer UI remain unproved. pdftotext 26.04.0 fragments the six labels into 19 word boxes while preserving all 43 non-space characters. These scoped reader observations are not assertions of this step; other styles and Acrobat/Evince/Okular UI are untested.', 'PDF text geometry is compared at Poppler returned precision (six decimal point coordinates and integer font-size metadata), with exact embedded chart font bytes; raw Tf/Tm are not independently decoded.', 'Printed numbers are not independently judged against the source data at author precision or against Table values; label-truth currently judges screen text only.', 'Native interactive source/cohort/readout payload identity is asserted. Actual keyboard/live announcements remain the separate chart-type-keyboard check.'], ok: !missing.length && !extra.length && results.every(row => row.ok) };
  writeFileSync(join(out, 'types.json'), JSON.stringify(proof, null, 2));
  return proof;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
const started = performance.now();
const { values: options } = parseArgs({ options: { out: { type: 'string' }, source: { type: 'string' }, bundle: { type: 'string' }, browser: { type: 'string' }, 'prepare-only': { type: 'boolean' }, prepared: { type: 'string' }, evidence: { type: 'string' } } });
mkdirSync(join(root, 'tmp/verification'), { recursive: true });
const out = resolve(options.out ?? (options.prepared || options.evidence ? dirname((options.prepared ?? options.evidence)!) : mkdtempSync(join(root, 'tmp/verification/chart-type-pdf-'))));
mkdirSync(out, { recursive: true });
let evidence: PdfEvidence | undefined;
try {
  evidence = options.prepared || options.evidence ? JSON.parse(readFileSync(resolve((options.prepared ?? options.evidence)!), 'utf8')) : await prepare(options, out);
  if (options['prepare-only']) console.log(JSON.stringify({ prepared: join(out, 'evidence.json'), styles: evidence!.builtins, build: evidence!.binding.commit }));
  else {
    if (!options.evidence) await capture(evidence!, options, out);
    const proof = { ...compareAll(evidence!, out), seconds: (performance.now() - started) / 1000 };
    writeFileSync(join(out, 'types.json'), JSON.stringify(proof, null, 2));
    console.log(JSON.stringify({ ok: proof.ok, styles: proof.results.map(row => ({ style: row.style, charts: row.cohort.charts, paths: row.results.reduce((n, c) => n + c.painted_marks, 0), failures: row.failures })), seconds: proof.seconds }));
    if (!proof.ok) process.exitCode = 1;
  }
} catch (error) {
  let after;
  if (evidence) try { verifyInputs(evidence); after = 'all bound files unchanged after failure'; } catch (bindingError) { after = String(bindingError); }
  writeFileSync(join(out, 'failure.json'), JSON.stringify({ at: new Date().toISOString(), error: String(error), stack: (error as Error).stack, binding_after_failure: after, seconds: (performance.now() - started) / 1000 }, null, 2));
  console.error(error); process.exitCode = 1;
}

}
