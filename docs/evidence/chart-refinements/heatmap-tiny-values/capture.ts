import assert from 'node:assert/strict';
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { resolve, dirname } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import type { RecordedRenderer, RecordedBrowser, RecordedServer, CapturedRender, RecordedCaptureCase } from '../capture-types.ts';

type Build = { web: string; shared: string };

const [worktreeArg, outputArg, side] = process.argv.slice(2);
assert(worktreeArg && outputArg && (side === "before" || side === "after"), 'capture.ts WORKTREE OUTPUT before|after');
const worktree = resolve(worktreeArg), output = resolve(outputArg), evidence = dirname(fileURLToPath(import.meta.url));
mkdirSync(output, { recursive: true });
const hash = (path: string) => createHash('sha256').update(readFileSync(path)).digest('hex');
const expected: { browser: string; files: Record<string, string>; before: Build; builds: { main: Build }; cases: RecordedCaptureCase[] } = JSON.parse(readFileSync(evidence + '/verification.json', 'utf8'));
const build = side === 'before' ? expected.before : expected.builds.main;
assert.equal(hash(worktree + '/dist/web/richview_web_bg.wasm'), build.web);
assert.equal(hash(worktree + '/packages/core/dist/rust_bg.wasm'), build.shared);
const { default: core }: { default: RecordedRenderer } = await import(pathToFileURL(worktree + '/packages/core/dist/index.js').href);
const { chromium }: RecordedBrowser = await import(pathToFileURL(worktree + '/node_modules/playwright/index.mjs').href);
const { serveStatic }: RecordedServer = await import(pathToFileURL(worktree + '/scripts/browser-harness.mjs').href);
let html = '';
const server = await serveStatic({ '/': worktree + '/dist/web' }, { route(path, _url, response) {
  if (path === '/review') { response.writeHead(200, { 'content-type': 'text/html' }); response.end(html); return true; }
  if (path === '/favicon.ico') { response.writeHead(204); response.end(); return true; }
} });
const browser = await chromium.launch({ headless: true, executablePath: '/usr/bin/chromium' });
const errors: string[] = [], files: Record<string, string> = {};
try {
  assert.equal(browser.version(), expected.browser);
  for (const entry of expected.cases) {
    assert.equal(hash(evidence + '/' + entry.source), expected.files[entry.source]);
    const report: CapturedRender = JSON.parse(core.render_source(readFileSync(evidence + '/' + entry.source, 'utf8'), JSON.stringify({
      width: entry.width, styles: [entry.theme], data: {}, runtime_url: server.base + '/richview_web',
      report_folder: { root: entry.fixture, source: 'report.rmd', files: ['report.rmd', 'reference.json'] },
    })));
    html = report.html;
    const context = await browser.newContext({ viewport: { width: entry.width, height: 900 }, deviceScaleFactor: 2, reducedMotion: 'reduce' });
    const page = await context.newPage(); page.on('pageerror', error => errors.push(error.message));
    await page.goto(server.base + '/review', { waitUntil: 'networkidle' });
    await page.evaluate(() => document.fonts.ready);
    const figure = page.locator('figure.rv-chart');
    await figure.scrollIntoViewIfNeeded();
    await page.waitForFunction(() => [...document.querySelectorAll('.rvc')].some(n => n.hasAttribute('data-rvc-diagnostics')));
    await page.evaluate(async () => { await window.__richview!.settled(); await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))); });
    const name: string = entry.name + '-' + side + '.png';
    await figure.screenshot({ path: output + '/' + name });
    files[name] = hash(output + '/' + name); assert.equal(files[name], expected.files[name], name);
    await context.close();
  }
  assert.deepEqual(errors, []);
  writeFileSync(output + '/capture.json', JSON.stringify({ head: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: worktree, encoding: 'utf8' }).trim(), wasm: build.web, browser: browser.version(), files, status: 'PASS' }, null, 2) + '\n');
  console.log(JSON.stringify({ side, files, status: 'PASS' }));
} finally { await browser.close(); await server.close(); }
