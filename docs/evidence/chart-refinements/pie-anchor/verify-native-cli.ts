import type { GeneratedReport, PieChart, PieFixture } from '../capture-types.ts';
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { execFileSync, spawnSync } from 'node:child_process';
import { resolve, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { createHash } from 'node:crypto';
import { parseArgs } from 'node:util';
import assert from 'node:assert/strict';
const { values } = parseArgs({ options: { worktree: { type: 'string' }, out: { type: 'string' }, phase: { type: 'string' } } });
const wt = resolve(values.worktree!), out = resolve(values.out!); mkdirSync(out, { recursive: true });
const { chromium } = await import(pathToFileURL(join(wt, 'node_modules/playwright/index.mjs')).href) as typeof import('playwright');
const fixtures: PieFixture[] = JSON.parse(readFileSync('tmp/pie-anchor/fixtures.json', 'utf8'));
const wanted = ['name', 'explicit-empty-name', 'conflicting-aliases', 'no-alias', 'zero-empty', 'facets'].map(s => 'pie-anchor-' + s);
const targets: Record<string, [string[], string[]]> = { name: [['', 'North'], ['North']], 'explicit-empty-name': [[''], ['']], 'conflicting-aliases': [['North', 'South'], ['North', 'South']], 'no-alias': [[''], []], 'zero-empty': [['', 'North'], ['North']], facets: [['', 'North'], ['North']] };
const reports: (PieFixture & GeneratedReport & { theme: string; exit: number | null })[] = [];
for (const item of fixtures.filter(f => wanted.includes(f.id))) for (const theme of ['scientific', 'brutalist']) {
  const dir = join(out, item.id + '-' + theme); mkdirSync(dir, { recursive: true }); writeFileSync(dir + '/report.rmd', item.source);
  const run = spawnSync(join(wt, 'target/release/richview'), ['generate', dir + '/report.rmd', '--style', theme, '--public-dir', join(wt, 'dist/web'), '--output', 'report.html', '--format', 'json'], { encoding: 'utf8', cwd: wt });
  assert.equal(run.status, 0, run.stderr + run.stdout);
  reports.push({ id: item.id, theme, source: item.source, exit: run.status, ...(JSON.parse(run.stdout) as GeneratedReport) });
}
writeFileSync(join(out, 'native-reports.json'), JSON.stringify(reports, null, 2));
const browser = await chromium.launch({ executablePath: '/usr/bin/chromium', headless: true });
const manifest = { head: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: wt, encoding: 'utf8' }).trim(), native_sha256: createHash('sha256').update(readFileSync(join(wt, 'target/release/richview'))).digest('hex'), wasm_sha256: createHash('sha256').update(readFileSync(join(wt, 'dist/web/richview_web_bg.wasm'))).digest('hex'), browser: browser.version(), phase: values.phase, records: [] as object[], errors: [] as string[], external: [] as string[] };
try {
  for (const width of [320, 720]) for (const live of [false, true]) {
    const context = await browser.newContext({ viewport: { width, height: 1100 }, deviceScaleFactor: 2, javaScriptEnabled: live, reducedMotion: 'reduce' }); const page = await context.newPage();
    page.on('pageerror', error => manifest.errors.push(error.message));
    await page.route('**/*', route => { if (/^(file|data|blob):/.test(route.request().url())) return route.continue(); manifest.external.push(route.request().url()); return route.abort(); });
    for (const report of reports) {
      const chart: PieChart = JSON.parse(report.source.match(/```chart\n([\s\S]*?)\n```/)![1]);
      const panels = chart.encoding.facet ? [...new Set(chart.data.inline.map(r => r[chart.encoding.facet!]))] : [null];
      await page.goto(pathToFileURL(report.output).href); await page.evaluate(() => document.fonts.ready);
      const figure = page.locator('figure.rv-chart'); await figure.scrollIntoViewIfNeeded();
      if (live) { await page.waitForFunction(() => !!document.querySelector('.rvc[data-rvc-diagnostics]')); await page.evaluate(() => window.__richview!.settled()); }
      const charts = await figure.locator('svg:visible').evaluateAll((nodes, staticFacet) => (staticFacet ? nodes.flatMap(svg => [...svg.children].filter(n => n.tagName === 'g')) : nodes).map(svg => {
        const marks = [...svg.querySelectorAll('.rv-pie')];
        return { svg: svg.outerHTML, outlineRows: [...svg.querySelectorAll('path[stroke-width="4"]')].map(p => marks.findIndex(m => m.getAttribute('d') === p.getAttribute('d'))), notes: [...svg.querySelectorAll('.rv-annotation')].map(t => t.textContent), diagnostics: svg.closest('.rvc')?.getAttribute('data-rvc-diagnostics') };
      }), !live && panels.length > 1);
      assert.equal(charts.length, panels.length);
      for (const [i, panel] of panels.entries()) {
        const rows = chart.data.inline.filter(r => !panel || r.panel === panel), names = [...new Set(rows.map(r => String(r.category)))].sort();
        const selected = i ? ['North'] : targets[report.id.replace('pie-anchor-', '')][values.phase === 'after' ? 1 : 0];
        assert.deepEqual(charts[i].outlineRows, selected.map(name => names.indexOf(name)), report.id + ' offline native targets');
        assert.deepEqual(charts[i].notes, selected.map(() => 'Reviewed'));
      }
      const overflow = await page.evaluate(() => document.documentElement.scrollWidth > innerWidth + 1); assert.equal(overflow, false);
      const id = `${report.id}-${report.theme}-${width}-${live ? 'live' : 'static'}`, image = join(out, id + '.png'); await figure.screenshot({ path: image });
      manifest.records.push({ id, charts, overflow, image, aria: await figure.ariaSnapshot() }); writeFileSync(join(out, 'checks.json'), JSON.stringify(manifest, null, 2)); console.log(id + ': offline native targets passed');
    }
    await context.close();
  }
  assert.deepEqual(manifest.external, []); assert.deepEqual(manifest.errors, []);
} finally { await browser.close(); }
