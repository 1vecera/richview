import { readFileSync, mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { harness, settled } from '../../../../scripts/browser-harness.ts';
import type { CapturedRender, RecordedRenderer } from '../capture-types.ts';

const { default: rust }: { default: RecordedRenderer } = await import(new URL('../../../../packages/core/dist/index.js', import.meta.url).href);

const fixtures: { id: string; source: string }[] = JSON.parse(readFileSync(new URL('./fixtures.json', import.meta.url), 'utf8'));
const out = resolve('tmp/exact-readouts/reproduced');
mkdirSync(out, { recursive: true });
for (const key of ['RICHVIEW_VERIFY_URL', 'RICHVIEW_SERVER_BINARY', 'RICHVIEW_VERIFY_PUBLIC_DIR']) delete process.env[key];
const h = await harness('exact-readouts-evidence');
try {
  const context = await h.browser.newContext({
    viewport: { width: 390, height: 900 }, deviceScaleFactor: 2, reducedMotion: 'reduce',
  });
  const page = await context.newPage();
  for (const fixture of fixtures.filter(f => ['readout-currency', 'readout-percent', 'heatmap-percent'].includes(f.id))) {
    const output: CapturedRender = JSON.parse(rust.render_source(fixture.source, JSON.stringify({
      width: 390, styles: ['scientific'], runtime_url: h.base + '/richview_web',
      report_folder: { root: fixture.id, source: 'report.rmd', files: ['report.rmd'] },
    })));
    const url = `${h.base}/evidence-${fixture.id}`;
    await page.route(url, route => route.fulfill({ contentType: 'text/html', body: output.html }));
    await page.goto(url, { waitUntil: 'networkidle' });
    await page.evaluate(() => document.fonts.ready);
    const figure = page.locator('figure.rv-chart');
    await figure.scrollIntoViewIfNeeded();
    await page.waitForFunction(() => document.querySelector('.rvc')?.hasAttribute('data-rvc-diagnostics'));
    await figure.locator('.rvc-stage').press('Home');
    await settled(page);
    await figure.screenshot({ path: `${out}/${fixture.id.replace('readout-', '')}-after.png` });
  }
  await context.close();
} finally { await h.close(); }
