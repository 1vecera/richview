// Render either built checkout; never synthesize or restyle the product chart.
import assert from 'node:assert/strict';
import { readFileSync, mkdirSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';
const [worktree, phase, destination] = process.argv.slice(2);
assert(worktree && ['before', 'after'].includes(phase) && destination,
  'node reproduce.mjs WORKTREE before|after OUTPUT_DIRECTORY');
const wt = resolve(worktree), out = resolve(destination);
mkdirSync(out, { recursive: true });
const { chromium } = await import(pathToFileURL(wt + '/node_modules/playwright/index.mjs'));
const { default: core } = await import(pathToFileURL(wt + '/packages/core/dist/index.js'));
const { serveStatic } = await import(pathToFileURL(wt + '/scripts/browser-harness.mjs'));
let html = '';
const server = await serveStatic({ '/': wt + '/dist/web' }, { route(path, url, response) {
  if (path === '/favicon.ico') { response.writeHead(204); response.end(); return true; }
  if (path === '/evidence') { response.writeHead(200, { 'content-type': 'text/html' }); response.end(html); return true; }
} });
const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM || '/usr/bin/chromium' });
const settle = page => page.evaluate(async () => {
  await document.fonts.ready;
  await window.__richview.settled();
  await new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done)));
  await Promise.allSettled(document.getAnimations().filter(a => a.playState === 'running'
    && a.effect?.getComputedTiming().iterations !== Infinity).map(a => a.finished));
  await new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done)));
});
try {
  for (const [name, id, theme, width, motion] of [
    ['line', 'line-crossing', 'scientific', 390, 'no-preference'],
    ['area', 'area-crossing-strings', 'sleek', 320, 'reduce'],
  ]) {
    const context = await browser.newContext({ viewport: { width, height: 1000 }, deviceScaleFactor: 2, reducedMotion: motion });
    const page = await context.newPage();
    const source = readFileSync(new URL(name + '.rmd', import.meta.url), 'utf8');
    html = JSON.parse(core.render_source(source, JSON.stringify({ width, styles: [theme],
      runtime_url: server.base + '/richview_web',
      report_folder: { root: id, source: 'report.rmd', files: ['report.rmd'] },
    }))).html;
    await page.goto(server.base + '/evidence', { waitUntil: 'networkidle' });
    await settle(page);
    const figure = page.locator('figure.rv-chart').first();
    await figure.scrollIntoViewIfNeeded();
    await figure.locator('.rvc-stage').first().press('Equal');
    await settle(page);
    await figure.screenshot({ path: out + '/' + name + '-' + phase + '.png' });
    await context.close();
  }
} finally { await browser.close(); await server.close(); }
