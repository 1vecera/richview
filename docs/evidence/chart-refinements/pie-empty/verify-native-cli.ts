import type { GeneratedReport, PieFixture } from '../capture-types.ts';
import { chromium } from 'playwright';
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { execFileSync, spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { createHash } from 'node:crypto';
import assert from 'node:assert/strict';

const fixtures = ['fixtures.json', 'supplement-fixtures.json'].flatMap(file => JSON.parse(readFileSync('tmp/pie-empty/' + file, 'utf8')) as PieFixture[]);
const wanted = ['mixed', 'no-title', 'facets', 'height-60', 'positive-control', 'positive-with-zeros'].map(s => 'pie-empty-' + s);
const reports = [];
for (const item of fixtures.filter(f => wanted.includes(f.id))) for (const theme of ['scientific', 'brutalist']) {
  const dir = resolve('tmp/pie-empty/native-reports', item.id + '-' + theme); mkdirSync(dir, { recursive: true });
  writeFileSync(dir + '/report.rmd', item.source);
  const execution = spawnSync('target/release/richview', ['generate', dir + '/report.rmd', '--style', theme, '--public-dir', resolve('dist/web'), '--output', 'report.html', '--format', 'json'], { encoding: 'utf8' });
  const result: GeneratedReport = JSON.parse(execution.stdout);
  assert.equal(execution.status, result.diagnostics.some(d => d.level === 'error') ? 1 : 0, item.id + ' CLI respects source diagnostics');
  const empty = item.id === 'pie-empty-facets' ? [true, false] : [!item.id.includes('positive')];
  reports.push({ id: item.id, theme, empty, exit: execution.status, ...result });
}
writeFileSync('tmp/pie-empty/native-reports.json', JSON.stringify(reports, null, 2));
const browser = await chromium.launch({ executablePath: '/usr/bin/chromium', headless: true });
const manifest = { head: execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim(), native_sha256: createHash('sha256').update(readFileSync('target/release/richview')).digest('hex'), wasm_sha256: createHash('sha256').update(readFileSync('dist/web/richview_web_bg.wasm')).digest('hex'), browser: browser.version(), records: [] as object[], errors: [] as string[], external: [] as string[] };
const message = 'No positive values to show';
try {
  for (const width of [320, 720]) for (const live of [false, true]) {
    const context = await browser.newContext({ viewport: { width, height: 1000 }, deviceScaleFactor: 2, javaScriptEnabled: live, reducedMotion: 'reduce' });
    const page = await context.newPage();
    page.on('pageerror', error => manifest.errors.push(error.message));
    await page.route('**/*', route => {
      if (/^(file|data|blob):/.test(route.request().url())) return route.continue();
      manifest.external.push(route.request().url()); return route.abort();
    });
    for (const report of reports) {
      await page.goto(pathToFileURL(report.output).href);
      await page.evaluate(() => document.fonts.ready);
      const figure = page.locator('figure.rv-chart'); await figure.scrollIntoViewIfNeeded();
      if (live) {
        await page.waitForFunction(() => !!document.querySelector('.rvc[data-rvc-diagnostics]'));
        await page.evaluate(() => window.__richview!.settled());
      }
      const charts = await figure.locator('svg:visible').evaluateAll((nodes, staticFacet) => (staticFacet ? nodes.flatMap(svg => [...svg.children].filter(n => n.tagName === 'g')) : nodes).map(svg => ({
        viewBox: svg!.getAttribute('viewBox'), texts: [...svg.querySelectorAll('text')].map(t => t.textContent), marks: svg.querySelectorAll('.rv-mark').length,
        pies: [...svg.querySelectorAll('.rv-pie')].map(p => ({ row: p.getAttribute('data-row'), d: p.getAttribute('d') })),
        diagnostics: svg.closest('.rvc')?.getAttribute('data-rvc-diagnostics'),
      })), !live && report.empty.length > 1);
      assert.equal(charts.length, report.empty.length);
      for (const [i, empty] of report.empty.entries()) {
        assert.equal(charts[i].texts.includes(message), empty, report.id + ' visible native note');
        if (empty) { assert.equal(charts[i].marks, 0); assert.deepEqual(charts[i].pies, []); }
        if (live) {
          const checks = JSON.parse(charts[i].diagnostics!).checks;
          const expected = ['pie-empty-mixed', 'pie-empty-positive-control'].includes(report.id)
            ? [{ kind: 'negative-pie-value', message: 'Pie slices require nonnegative values', severity: 'error' }]
            : [];
          assert.deepEqual(checks, expected, 'source errors remain visible without new glyph findings');
        }
      }
      const aria = await figure.ariaSnapshot();
      assert.equal(aria.includes(message), report.empty.some(Boolean));
      const overflow = await page.evaluate(() => document.documentElement.scrollWidth > innerWidth + 1); assert.equal(overflow, false);
      const id = `${report.id}-${report.theme}-${width}-${live ? 'live' : 'static'}`;
      const dir = 'tmp/pie-empty/native-verified/' + id; mkdirSync(dir, { recursive: true });
      await figure.screenshot({ path: dir + '/chart.png' });
      manifest.records.push({ id, width, live, theme: report.theme, charts, aria, overflow, image: dir + '/chart.png' });
      writeFileSync('tmp/pie-empty/native-cli-checks.json', JSON.stringify(manifest, null, 2));
      console.log(id + ': offline native CLI state/accessibility passed');
    }
    await context.close();
  }
  assert.deepEqual(manifest.external, []); assert.deepEqual(manifest.errors, []);
} finally { await browser.close(); }
