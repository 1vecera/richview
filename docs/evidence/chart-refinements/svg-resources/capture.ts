import assert from 'node:assert/strict';
import { readFileSync, writeFileSync, mkdirSync, copyFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { resolve, dirname } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import type { RecordedRenderer, RecordedBrowser, RecordedServer, CapturedRender } from '../capture-types.ts';

declare const wasm_bindgen: { ChartView: new (host: HTMLElement, payload: string) => unknown };

const [worktreeArg, outputArg, label] = process.argv.slice(2);
assert(worktreeArg && outputArg && (label === "before" || label === "after"), 'capture.ts WORKTREE OUTPUT before|after');
const worktree = resolve(worktreeArg), output = resolve(outputArg), evidence = dirname(fileURLToPath(import.meta.url));
mkdirSync(output, { recursive: true });
const hash = (path: string) => createHash('sha256').update(readFileSync(path)).digest('hex');
const expected: { browser: string; files: Record<string, string>; builds: Record<'before' | 'after', { wasm: string }> } = JSON.parse(readFileSync(evidence + '/verification.json', 'utf8'));
for (const file of ['mixed-bars.rmd', 'identical-bars.rmd', 'report.rstyle.yaml']) assert.equal(hash(evidence + '/' + file), expected.files[file]);
assert.equal(hash(worktree + '/dist/web/richview_web_bg.wasm'), expected.builds[label].wasm);
const { default: core }: { default: RecordedRenderer } = await import(pathToFileURL(worktree + '/packages/core/dist/index.js').href);
const { chromium }: RecordedBrowser = await import(pathToFileURL(worktree + '/node_modules/playwright/index.mjs').href);
const { serveStatic }: RecordedServer = await import(pathToFileURL(worktree + '/scripts/browser-harness.mjs').href);
// Existing mono-mode tokens, captured from the two theme controls in the showcase.
const mono = [
  { decals:true,palette:['oklch(0.2178 0 0)','oklch(0.4091 0 0)','oklch(0.5795 0 0)','oklch(0.656 0 0)','oklch(0.656 0 0)','oklch(0.5103 0 0)','oklch(0.2178 0 0)','oklch(0.4091 0 0)'],ink:'oklch(0.1792 0.0052 67.49)',inkSoft:'oklch(0.4191 0.0084 67.59)',grid:'oklch(0.9295 0.0052 67.76)',axis:'oklch(0.8804 0.0064 75.4)',surface:'oklch(0.9858 0.0045 78.3)',accent:'oklch(0.1792 0.0052 67.49)',accentSoft:'oklch(0.9295 0.0052 67.76)',fontFamily:"'Source Code Pro', ui-monospace, monospace",ramp:['oklch(0.9491 0 0)','oklch(0.6268 0 0)','oklch(0.2809 0 0)'] },
  { decals:true,palette:Array(8).fill('oklch(0.5999 0 0)'),ink:'oklch(0.9507 0.0052 247.88)',inkSoft:'oklch(0.7201 0.0116 256.71)',grid:'oklch(0.2383 0.0147 261.65)',axis:'oklch(0.2996 0.0161 259.79)',surface:'oklch(0.1904 0.0156 261.56)',accent:'oklch(0.9507 0.0052 247.88)',accentSoft:'oklch(0.2383 0.0147 261.65)',fontFamily:"'Geist Mono', 'JetBrains Mono', ui-monospace, monospace",ramp:['oklch(0.2645 0 0)','oklch(0.6301 0 0)','oklch(0.9491 0 0)'] },
];
let html = '';
const server = await serveStatic({ '/': worktree + '/dist/web' }, { route(path, _url, response) {
  if (path === '/review') { response.writeHead(200, { 'content-type': 'text/html' }); response.end(html); return true; }
  if (path === '/favicon.ico') { response.writeHead(204); response.end(); return true; }
} });
const browser = await chromium.launch({ headless: true, executablePath: '/usr/bin/chromium' });
const errors: string[] = [], files: Record<string, string> = {};
try {
  assert.equal(browser.version(), expected.browser);
  const source = readFileSync(evidence + '/mixed-bars.rmd', 'utf8');
  const reports = ['scientific', 'sleek'].map((theme): CapturedRender => JSON.parse(core.render_source(source, JSON.stringify({ width:390,styles:[theme],runtime_url:server.base + '/richview_web',report_folder:{root:'resource-owner',source:'report.rmd',files:['report.rmd']} }))));
  html = reports[1].html;
  const payloads = reports.map((report, index) => { const p: Record<string, unknown> & { theme: Record<string, unknown> } = JSON.parse(report.html.match(/<script[^>]*data-rvc-runtime[^>]*>([\s\S]*?)<\/script>/)![1]); return { ...p, theme:{ ...p.theme, ...mono[index] } }; });
  const context = await browser.newContext({ viewport:{width:390,height:1400},deviceScaleFactor:2,reducedMotion:'reduce',hasTouch:true,acceptDownloads:true });
  const page = await context.newPage(); page.on('pageerror', error => errors.push(error.message));
  await page.goto(server.base + '/review', { waitUntil:'networkidle' });
  await page.evaluate(async payloads => {
    await document.fonts.ready; await window.__richview!.settled();
    document.querySelector('figure.rv-chart')!.remove();
    const main = document.querySelector('main')!; main.style.cssText = 'margin:0;padding:0;width:358px;max-width:none';
    for (let index = 0; index < 2; index++) { const host = document.createElement('section'); host.id = 'owner-' + index; host.style.width = '100%'; main.append(host); new wasm_bindgen.ChartView(host, JSON.stringify(payloads[index])); }
    await document.fonts.ready; await window.__richview!.settled();
    await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    await Promise.allSettled(document.getAnimations().filter(a => a.playState === 'running' && a.effect?.getComputedTiming().iterations !== Infinity).map(a => a.finished));
    await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  }, payloads);
  await page.locator('#owner-1 .rvc-stage').screenshot({ path:output + '/mixed-' + label + '.png' });
  await context.close();

  const offline = output + '/offline'; mkdirSync(offline, { recursive:true });
  copyFileSync(evidence + '/identical-bars.rmd', offline + '/report.rmd');
  copyFileSync(evidence + '/report.rstyle.yaml', offline + '/report.rstyle.yaml');
  const generated: { output: string } = JSON.parse(execFileSync(worktree + '/target/release/richview', ['generate',offline + '/report.rmd','--public-dir',worktree + '/dist/web','--output','report.html','--format','json'], { cwd:worktree,encoding:'utf8' }));
  const local = await browser.newContext({ viewport:{width:720,height:1400},deviceScaleFactor:2,reducedMotion:'reduce' });
  const reportPage = await local.newPage(); reportPage.on('pageerror', error => errors.push(error.message));
  await reportPage.route('**/*', route => /^(file|data|blob):/.test(route.request().url()) ? route.continue() : (errors.push(route.request().url()), route.abort()));
  await reportPage.goto(pathToFileURL(generated.output).href);
  await reportPage.evaluate(() => document.fonts.ready); await reportPage.waitForFunction(() => !!window.__richview);
  await reportPage.evaluate(() => window.__richview!.settled());
  await reportPage.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  await reportPage.locator('figure.rv-chart').first().evaluate(node => (node as HTMLElement).style.display = 'none');
  await reportPage.locator('figure.rv-chart').nth(1).screenshot({ path:output + '/hidden-' + label + '.png' });
  await local.close();
  assert.deepEqual(errors, []);
  for (const prefix of ['mixed', 'hidden']) { const name: string = prefix + '-' + label + '.png'; files[name] = hash(output + '/' + name); assert.equal(files[name], expected.files[name], name); }
  writeFileSync(output + '/capture.json', JSON.stringify({head:execFileSync('git',['rev-parse','HEAD'],{cwd:worktree,encoding:'utf8'}).trim(),wasm:expected.builds[label].wasm,browser:browser.version(),files,status:'PASS'}, null, 2) + '\n');
  console.log(JSON.stringify({label,files,status:'PASS'}));
} finally { await browser.close(); await server.close(); }
