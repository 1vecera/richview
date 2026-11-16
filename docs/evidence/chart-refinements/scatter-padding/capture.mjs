import assert from 'node:assert/strict';
import { readFileSync, mkdirSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { createHash } from 'node:crypto';
import { harness } from '../../../../scripts/browser-harness.mjs';

const wt = resolve(process.argv[2] || '.'), out = resolve(process.argv[3] || 'tmp/scatter-padding/reproduced');
const side = process.argv[4] || 'after';
assert(['before', 'after'].includes(side));
const evidence = JSON.parse(readFileSync(new URL('./verification.json', import.meta.url), 'utf8'));
const { default: rust } = await import(pathToFileURL(join(wt, 'packages/core/dist/index.js')));
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
delete process.env.RICHVIEW_VERIFY_URL;
process.env.RICHVIEW_SERVER_BINARY = join(wt, 'target/release/richview-server');
process.env.RICHVIEW_VERIFY_PUBLIC_DIR = join(wt, 'dist/web');
mkdirSync(out, { recursive: true });
const h = await harness('scatter-padding-evidence');
try {
  for (const pair of evidence.pairs) {
    const context = await h.browser.newContext({ viewport: { width: pair.width, height: 900 }, deviceScaleFactor: 2, reducedMotion: 'reduce' });
    const page = await context.newPage(), source = readFileSync(new URL(pair.source, import.meta.url), 'utf8');
    assert.equal(hash(source), pair.source_sha256);
    const output = JSON.parse(rust.render_source(source, JSON.stringify({ width: pair.width, styles: [pair.theme], runtime_url: h.base + '/richview_web', report_folder: { root: pair.id, source: 'report.rmd', files: ['report.rmd', 'reference.json'] } })));
    const url = `${h.base}/evidence-${pair.id}`;
    await page.route(url, route => route.fulfill({ contentType: 'text/html', body: output.html }));
    await page.goto(url, { waitUntil: 'networkidle' });
    await page.evaluate(() => document.fonts.ready);
    const figure = page.locator('figure.rv-chart');
    await figure.scrollIntoViewIfNeeded();
    await page.waitForFunction(() => !!document.querySelector('.rvc[data-rvc-diagnostics]'));
    await page.evaluate(async () => { await window.__richview.settled(); await new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done))); });
    const png = await figure.screenshot({ path: join(out, pair[side].file) });
    assert.equal(hash(png), pair[side].sha256, `${pair.id}: use the recorded browser, fonts and build for identical pixels`);
    await context.close();
  }
} finally { await h.close(); }
