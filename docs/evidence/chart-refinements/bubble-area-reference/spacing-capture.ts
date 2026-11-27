import type { CapturedRender, RecordedRenderer, RecordedServer } from '../capture-types.ts';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { dirname, resolve } from 'node:path';

// Reproduce the separate Firefox spacing supplement, not the original Chromium pair.
const packet = dirname(fileURLToPath(import.meta.url));
const [worktree, output, side] = process.argv.slice(2);
assert(worktree && output && (side === 'before' || side === 'after'), 'worktree output before|after');
const wt = resolve(worktree), out = resolve(output);
const proof = JSON.parse(readFileSync(packet + '/spacing-verification.json', 'utf8'));
const sha = (bytes: string | Buffer) => createHash('sha256').update(bytes).digest('hex');
const source = readFileSync(packet + '/spacing-wrapped.rmd', 'utf8');
assert.equal(sha(source), proof.pair.source_sha256);
assert.equal(sha(readFileSync(wt + '/dist/web/richview_web_bg.wasm')), proof[side].web_wasm_sha256);
assert.equal(sha(readFileSync(wt + '/packages/core/dist/rust_bg.wasm')), proof[side].node_wasm_sha256);
mkdirSync(out, { recursive: true });
const { firefox } = await import(pathToFileURL(wt + '/node_modules/playwright/index.mjs').href) as typeof import('playwright');
const { default: rust } = await import(pathToFileURL(wt + '/packages/core/dist/index.js').href) as { default: RecordedRenderer };
const { serveStatic } = await import(pathToFileURL(wt + '/scripts/browser-harness.mjs').href) as RecordedServer;
let html = '';
const server = await serveStatic({ '/': wt + '/dist/web' }, { route(path, url, res) {
  if (path === '/favicon.ico') { res.writeHead(204); res.end(); return true; }
  if (path === '/spacing') { res.writeHead(200, { 'content-type': 'text/html' }); res.end(html); return true; }
} });
const browser = await firefox.launch({ headless: true });
try {
  assert.equal(browser.version(), proof.browser_version);
  const rendered: CapturedRender = JSON.parse(rust.render_source(source, JSON.stringify({
    width: 240, styles: ['brutalist'], data: {}, runtime_url: server.base + '/richview_web',
    report_folder: { root: 'wrapped-caption', source: 'report.rmd', files: ['report.rmd', 'reference.json'] },
  })));
  html = rendered.html;
  const context = await browser.newContext({ viewport: { width: 240, height: 900 }, deviceScaleFactor: 2, reducedMotion: 'reduce' });
  const page = await context.newPage(), errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.goto(server.base + '/spacing', { waitUntil: 'networkidle' });
  await page.evaluate(() => document.fonts.ready);
  const figure = page.locator('figure.rv-chart');
  await figure.scrollIntoViewIfNeeded();
  await page.waitForFunction(() => [...document.querySelectorAll('.rvc')].some(n => n.hasAttribute('data-rvc-diagnostics')));
  await page.evaluate(async () => { await window.__richview!.settled(); await new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r))); });
  const measured = await figure.evaluate(node => {
    const svg = node.querySelector('.rvc-svg'), key = svg!.querySelector('.rvc-size-key')!;
    const texts = [...key.querySelectorAll('text')].map(n => { const b = n.getBoundingClientRect(); return { text: n.textContent, y: b.y, height: b.height }; });
    return { viewBox: svg!.getAttribute('viewBox'), texts,
      gaps: texts.slice(1).map((n, i) => n.y - texts[i].y - texts[i].height),
      diagnostics: JSON.parse(node.querySelector('.rvc')!.getAttribute('data-rvc-diagnostics')!),
      overflow: document.documentElement.scrollWidth > innerWidth + 1 };
  });
  assert.deepEqual(errors, []); assert.equal(measured.overflow, false);
  assert.equal(measured.texts.map(n => n.text).join(''), 'Circle area: number_of_active_customers_in_the_reporting_period16');
  assert(side === 'after' ? Math.min(...measured.gaps) > 0 : Math.min(...measured.gaps) < 0);
  const image = out + '/spacing-wrapped-' + side + '.png';
  await figure.screenshot({ path: image });
  const actual = sha(readFileSync(image));
  assert.equal(actual, proof.pair[side].sha256, 'The original complete figure must reproduce byte-for-byte');
  writeFileSync(out + '/verification.json', JSON.stringify({ side, image, sha256: actual, browser: browser.version(), measured, errors }, null, 2) + '\n');
  await context.close();
  console.log(JSON.stringify({ side, sha256: actual, minimum_client_gap: Math.min(...measured.gaps) }));
} finally { await browser.close(); await server.close(); }
