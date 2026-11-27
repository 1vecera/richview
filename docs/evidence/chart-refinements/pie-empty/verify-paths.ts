import type { CapturedRender, PieChart, PieFixture, RecordedRenderer } from '../capture-types.ts';
import type { Locator } from 'playwright';
import type { AddressInfo } from 'node:net';
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'node:fs';
import { createServer } from 'node:http';
import { resolve, join, extname, sep } from 'node:path';
import { pathToFileURL } from 'node:url';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { parseArgs } from 'node:util';

const { values } = parseArgs({ options: { worktree: { type: 'string' }, out: { type: 'string' }, phase: { type: 'string' } } });
const wt = resolve(values.worktree!), out = resolve(values.out!), dist = join(wt, 'dist/web');
mkdirSync(out, { recursive: true });
const { chromium } = await import(pathToFileURL(join(wt, 'node_modules/playwright/index.mjs')).href) as typeof import('playwright');
const { default: rust } = await import(pathToFileURL(join(wt, 'packages/core/dist/index.js')).href) as { default: RecordedRenderer };
const wanted = ['zero', 'missing', 'negative', 'invalid', 'mixed', 'facets', 'positive-with-zeros', 'height-60'].map(s => 'pie-empty-' + s);
const fixtures = ['fixtures.json', 'supplement-fixtures.json'].flatMap(file => JSON.parse(readFileSync(join('tmp/pie-empty', file), 'utf8'))).filter(f => wanted.includes(f.id));
const pages = new Map<string, string>(), errors: string[] = [];
const server = createServer((req, res) => {
  const path = new URL(req.url!, 'http://localhost').pathname;
  if (path === '/favicon.ico') { res.writeHead(204); res.end(); return; }
  if (pages.has(path)) { res.writeHead(200, { 'content-type': 'text/html' }); res.end(pages.get(path)); return; }
  const file = resolve(dist, '.' + path);
  if (!file.startsWith(dist + sep) || !existsSync(file)) { res.writeHead(404); res.end(); return; }
  const type = ({ '.js': 'text/javascript', '.wasm': 'application/wasm', '.css': 'text/css', '.woff2': 'font/woff2' } as Record<string, string>)[extname(file)] || 'application/octet-stream';
  res.writeHead(200, { 'content-type': type }); res.end(readFileSync(file));
});
await new Promise<void>(done => server.listen(0, '127.0.0.1', done));
const base = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
const browser = await chromium.launch({ executablePath: '/usr/bin/chromium', headless: true });
const manifest = { head: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: wt, encoding: 'utf8' }).trim(),
  wasm_sha256: createHash('sha256').update(readFileSync(join(dist, 'richview_web_bg.wasm'))).digest('hex'), browser: browser.version(), phase: values.phase, records: [] as object[], errors };
const message = 'No positive values to show';
const numeric = (value: unknown) => value !== null && value !== undefined && String(value).trim() !== '' && Number.isFinite(Number(value));
const shape = (svg: Element) => [...svg.querySelectorAll('.rv-pie')].map(p => ({ row: p.getAttribute('data-row'), fill: p.getAttribute('fill'), center: p.getAttribute('data-center'), angles: p.getAttribute('data-angles'), radius: p.getAttribute('data-r'), path: p.getAttribute('d') }));
const tipData = (tip: Locator) => tip.evaluate(n => ({ heading: n.querySelector('b')?.textContent, fields: [...n.querySelectorAll('div')].map(d => [d.querySelector('span')!.textContent, d.querySelector('strong')!.textContent]) }));
try {
  for (const motion of ['reduce', 'no-preference'] as const) for (const theme of ['scientific', 'hand-drawn']) for (const width of [320, 720]) {
    const context = await browser.newContext({ viewport: { width, height: 1000 }, deviceScaleFactor: 2, hasTouch: true, reducedMotion: motion });
    const page = await context.newPage(), exported = await context.newPage();
    const nativeContext = await browser.newContext({ viewport: { width, height: 1000 }, deviceScaleFactor: 2, javaScriptEnabled: false });
    const native = await nativeContext.newPage();
    page.on('pageerror', error => errors.push(error.message));
    for (const item of fixtures) {
      const chart: PieChart = JSON.parse(item.source.match(/```chart\n([\s\S]*?)\n```/)![1]), id = `${item.id}-${theme}-${width}-${motion}`;
      const rendered: CapturedRender = JSON.parse(rust.render_source(item.source, JSON.stringify({ width, styles: [theme], runtime_url: base + '/richview_web', report_folder: { root: item.id, source: 'report.rmd', files: ['report.rmd'] } })));
      pages.set('/' + id, rendered.html);
      await page.goto(base + '/' + id, { waitUntil: 'networkidle' }); await page.evaluate(() => document.fonts.ready);
      const figure = page.locator('figure.rv-chart').first(); await figure.scrollIntoViewIfNeeded();
      await page.waitForFunction(() => !!document.querySelector('.rvc[data-rvc-diagnostics]'));
      await page.evaluate(() => window.__richview!.settled());
      await native.goto(base + '/' + id, { waitUntil: 'networkidle' }); await native.evaluate(() => document.fonts.ready);
      await native.locator('figure.rv-chart').first().screenshot({ path: join(out, id + '-static.png') });
      const staticText = await native.locator('figure.rv-chart svg text').allTextContents();
      const nativeAria = await native.locator('figure.rv-chart').first().ariaSnapshot();
      await page.bringToFront();
      const panels = chart.encoding.facet ? [...new Set(chart.data.inline.map(r => r[chart.encoding.facet!]))] : [null];
      const views = figure.locator('.rvc[data-rvc-diagnostics]'); assert.equal(await views.count(), panels.length);
      for (const [panelIndex, panel] of panels.entries()) {
        const panelId = id + (panel ? '-' + panel : ''), view = views.nth(panelIndex), stage = view.locator('.rvc-stage'), tip = view.locator('.rvc-tip'), actions = panel ? view : figure;
        const sourceRows = chart.data.inline.filter(r => !panel || r[chart.encoding.facet!] === panel);
        const empty = !sourceRows.some(r => numeric(r.value) && Number(r.value) > 0);
        const rows = empty ? sourceRows : sourceRows.filter(r => numeric(r.value) && Number(r.value) >= 0).sort((a, b) => a.name! < b.name! ? -1 : a.name! > b.name! ? 1 : 0);
        await stage.scrollIntoViewIfNeeded(); await page.evaluate(() => window.__richview!.settled());
        const svg = view.locator('.rvc-svg'), geometry = await svg.evaluate(shape), texts = await svg.locator('text').allTextContents();
        const dimensions = await svg.evaluate(s => ({ width: (s as SVGSVGElement).viewBox.baseVal.width, height: (s as SVGSVGElement).viewBox.baseVal.height }));
        assert.equal(texts.includes(message), empty && values.phase === 'after', panelId + ' truthful visible note');
        if (empty) {
          assert.deepEqual(geometry, [], panelId + ' no invented sectors');
          assert.equal(await svg.locator('.rv-mark').count(), 0, panelId + ' no invented mark');
        }
        const liveAria = await view.ariaSnapshot();
        if (!chart.encoding.facet) assert.equal(staticText.includes(message), empty && values.phase === 'after', panelId + ' JavaScript-disabled text');
        await stage.focus(); const keyboard = [];
        for (const [i, row] of rows.entries()) {
          await stage.press(i ? 'ArrowRight' : 'Home');
          await page.evaluate(() => new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done))));
          const actual = await tipData(tip);
          assert.equal(actual.heading, String(row.name), panelId + ' source keyboard category');
          assert(actual.fields.some(([key, value]) => key === 'value' && value === (row.value == null ? 'No observation' : String(row.value))), panelId + ' exact keyboard value: ' + JSON.stringify(actual));
          keyboard.push(actual);
        }
        await stage.press('Escape'); await page.mouse.move(1, 1);
        await page.evaluate(async () => { await window.__richview!.settled(); await Promise.allSettled(document.getAnimations().filter(a => a.playState === 'running').map(a => a.finished)); });
        await tip.waitFor({ state: 'hidden', timeout: 3000 });
        if (empty) {
          const target = await svg.evaluate(s => { const r = s.getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height / 2 }; });
          await page.mouse.move(target.x, target.y);
          await page.evaluate(() => new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done))));
          assert(await tip.isHidden(), panelId + ' note has no data hit');
          await page.touchscreen.tap(target.x, target.y);
          assert(await tip.isHidden(), panelId + ' note touch has no data hit');
        }
        await page.mouse.move(1, 1); await stage.press('Escape');
        await page.evaluate(async () => { await window.__richview!.settled(); await Promise.allSettled(document.getAnimations().filter(a => a.playState === 'running').map(a => a.finished)); });
        const [download] = await Promise.all([page.waitForEvent('download'), actions.getByRole('button', { name: 'Download SVG', exact: true }).click()]);
        const exportPath = join(out, panelId + '.svg'); await download.saveAs(exportPath);
        await exported.setContent('<!doctype html><style>html,body{margin:0}svg{max-width:100%;height:auto}</style>' + readFileSync(exportPath, 'utf8')); await exported.evaluate(() => document.fonts.ready);
        const exportText = await exported.locator('svg text').allTextContents();
        assert.equal(exportText.includes(message), empty && values.phase === 'after', panelId + ' exported text');
        if (empty) assert.deepEqual(await exported.locator('svg').evaluate(shape), [], panelId + ' export no invented sectors');
        await exported.locator('svg').screenshot({ path: join(out, panelId + '-export.png') });
        await page.bringToFront();
        await actions.getByRole('button', { name: 'Table', exact: true }).click();
        const table = await view.locator('.rvc-table tbody tr').evaluateAll(nodes => nodes.map(n => [...n.querySelectorAll('td')].map(c => c.textContent))), keys = Object.keys(sourceRows[0]).sort();
        assert.deepEqual(table, sourceRows.map(row => keys.map(key => String(row[key]))), panelId + ' original table rows');
        const [csv] = await Promise.all([page.waitForEvent('download'), actions.getByRole('button', { name: 'Download CSV', exact: true }).click()]);
        const csvPath = join(out, panelId + '.csv'); await csv.saveAs(csvPath);
        const csvRows = readFileSync(csvPath, 'utf8').trimEnd().split(/\r?\n/).map(line => line.split(',').map(s => s.replace(/^"|"$/g, '').replace(/""/g, '"')));
        assert.deepEqual(csvRows, [keys, ...sourceRows.map(row => keys.map(key => row[key] == null ? '' : String(row[key])))], panelId + ' original CSV rows');
        await page.mouse.move(1, 1);
        await page.evaluate(async () => { await window.__richview!.settled(); await Promise.allSettled(document.getAnimations().filter(a => a.playState === 'running').map(a => a.finished)); });
        const activeAnimations = await page.evaluate(() => document.getAnimations().filter(a => a.playState === 'running').length); assert.equal(activeAnimations, 0);
        manifest.records.push({ id: panelId, empty, dimensions, geometry, texts, staticText, exportText, liveAria, nativeAria, keyboard, table, csvPath, exportPath, activeAnimations, native: rendered.metadata?._richview?.chart_diagnostics });
        writeFileSync(join(out, 'checks.json'), JSON.stringify(manifest, null, 2)); console.log(panelId + ': native/live/export state and source controls pass');
      }
    }
    await context.close(); await nativeContext.close();
  }
  assert.deepEqual(errors, []);
} finally { await browser.close(); server.closeAllConnections(); await new Promise<void>(done => server.close(() => done())); }
