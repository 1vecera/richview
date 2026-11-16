import { readFileSync, mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { harness } from '../../../../scripts/browser-harness.mjs';
import rust from '../../../../packages/core/dist/index.js';

const evidence = JSON.parse(readFileSync(new URL('./verification.json', import.meta.url), 'utf8'));
const out = resolve('tmp/percent-unit/reproduced');
mkdirSync(out, { recursive: true });
for (const key of ['RICHVIEW_VERIFY_URL', 'RICHVIEW_SERVER_BINARY', 'RICHVIEW_VERIFY_PUBLIC_DIR']) delete process.env[key];
const h = await harness('percent-unit-evidence');
try {
  for (const pair of evidence.pairs) {
    const width = pair.viewport_css_px.width;
    const context = await h.browser.newContext({
      viewport: { width, height: 900 }, deviceScaleFactor: 2, reducedMotion: 'reduce',
    });
    const page = await context.newPage();
    const source = readFileSync(new URL(pair.fixture, import.meta.url), 'utf8');
    const output = JSON.parse(rust.render_source(source, JSON.stringify({
      width, styles: [pair.theme], runtime_url: h.base + '/richview_web',
      report_folder: { root: pair.id, source: 'report.rmd', files: ['report.rmd', 'reference.json'] },
    })));
    const url = `${h.base}/evidence-${pair.id}`;
    await page.route(url, route => route.fulfill({ contentType: 'text/html', body: output.html }));
    await page.goto(url, { waitUntil: 'networkidle' });
    await page.evaluate(() => document.fonts.ready);
    const figure = page.locator('figure.rv-chart');
    await figure.scrollIntoViewIfNeeded();
    await page.waitForFunction(() => [...document.querySelectorAll('.rvc')].some(n => n.hasAttribute('data-rvc-diagnostics')));
    await page.evaluate(async () => {
      await window.__richview.settled();
      await new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done)));
    });
    await figure.screenshot({ path: `${out}/${pair.after.image}` });
    await context.close();
  }
} finally { await h.close(); }
