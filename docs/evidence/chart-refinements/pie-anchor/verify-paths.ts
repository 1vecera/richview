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
const fixtures: PieFixture[] = JSON.parse(readFileSync('tmp/pie-anchor/fixtures.json', 'utf8'));
const targets: Record<string, [string[], string[]]> = {
  name: [['', 'North'], ['North']], field: [['', 'North'], ['North']],
  'explicit-empty-name': [[''], ['']], 'explicit-empty-field': [[''], ['']],
  'conflicting-aliases': [['North', 'South'], ['North', 'South']],
  'empty-and-named': [['', 'North'], ['', 'North']], 'no-alias': [[''], []],
  'unmatched-name': [[''], []], 'matching-aliases': [['North'], ['North']],
  'color-field': [['', 'North'], ['North']], 'numeric-alias': [['', '0'], ['0']],
  'field-is-name': [['North'], ['North']], 'zero-empty': [['', 'North'], ['North']],
  facets: [['', 'North'], ['North']],
};
const shape = (svg: Element) => {
  const marks = [...svg.querySelectorAll('.rv-pie')];
  return { marks: marks.map(p => ({ row: p.getAttribute('data-row'), fill: p.getAttribute('fill'), center: p.getAttribute('data-center'), angles: p.getAttribute('data-angles'), radius: p.getAttribute('data-r'), path: p.getAttribute('d') })),
    outlineRows: [...svg.querySelectorAll('path[stroke-width="4"]')].map(p => marks.findIndex(m => m.getAttribute('d') === p.getAttribute('d'))),
    notes: [...svg.querySelectorAll('.rv-annotation')].map(t => t.textContent),
  };
};
const pages = new Map<string, string>(), errors: string[] = [];
const server = createServer((req, res) => {
  const path = new URL(req.url!, 'http://localhost').pathname;
  if (path === '/favicon.ico') { res.writeHead(204); res.end(); return; }
  if (pages.has(path)) { res.writeHead(200, { 'content-type': 'text/html' }); res.end(pages.get(path)); return; }
  const file = resolve(dist, '.' + path);
  if (!file.startsWith(dist + sep) || !existsSync(file)) { res.writeHead(404); res.end(); return; }
  res.writeHead(200, { 'content-type': ({ '.js': 'text/javascript', '.wasm': 'application/wasm', '.css': 'text/css', '.woff2': 'font/woff2' } as Record<string, string>)[extname(file)] || 'application/octet-stream' }); res.end(readFileSync(file));
});
await new Promise<void>(done => server.listen(0, '127.0.0.1', done));
const base = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
const browser = await chromium.launch({ executablePath: '/usr/bin/chromium', headless: true });
const manifest = { head: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: wt, encoding: 'utf8' }).trim(),
  wasm_sha256: createHash('sha256').update(readFileSync(join(dist, 'richview_web_bg.wasm'))).digest('hex'), browser: browser.version(), phase: values.phase, records: [] as object[], errors };
const tipData = (tip: Locator) => tip.evaluate(n => ({ heading: n.querySelector('b')?.textContent, fields: [...n.querySelectorAll('div')].map(d => [d.querySelector('span')!.textContent, d.querySelector('strong')!.textContent]) }));
try {
  for (const motion of ['reduce', 'no-preference'] as const) for (const theme of ['scientific', 'hand-drawn']) {
    const width = 390;
    const context = await browser.newContext({ viewport: { width, height: 1100 }, deviceScaleFactor: 2, hasTouch: true, reducedMotion: motion });
    const page = await context.newPage(), exported = await context.newPage();
    const staticContext = await browser.newContext({ viewport: { width, height: 1100 }, deviceScaleFactor: 2, javaScriptEnabled: false });
    const native = await staticContext.newPage();
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
      await native.locator('figure.rv-chart').screenshot({ path: join(out, id + '-static.png') });
      const panels = chart.encoding.facet ? [...new Set(chart.data.inline.map(r => r[chart.encoding.facet!]))] : [null];
      const staticShapes = chart.encoding.facet
        ? await native.locator('figure.rv-chart svg:visible > g').evaluateAll((nodes, shapeText) => nodes.map(n => ((0, eval)('(' + shapeText + ')') as (svg: Element) => ReturnType<typeof shape>)(n)), shape.toString())
        : [await native.locator('figure.rv-chart svg:visible').evaluate(shape)];
      assert.equal(staticShapes.length, panels.length);
      const views = figure.locator('.rvc[data-rvc-diagnostics]'); assert.equal(await views.count(), panels.length);
      await page.bringToFront();
      for (const [panelIndex, panel] of panels.entries()) {
        const panelId = id + (panel ? '-' + panel : ''), view = views.nth(panelIndex), stage = view.locator('.rvc-stage'), tip = view.locator('.rvc-tip'), actions = panel ? view : figure;
        const sourceRows = chart.data.inline.filter(r => !panel || r[chart.encoding.facet!] === panel), category = chart.encoding.color || chart.encoding.x;
        const names = [...new Set(sourceRows.map(r => String(r[category])))].sort();
        const selected = panelIndex ? ['North'] : targets[item.id.replace('pie-anchor-', '')][values.phase === 'after' ? 1 : 0];
        const expectedRows = selected.map(name => names.indexOf(name));
        const verifyShape = (s: ReturnType<typeof shape>, label: string) => { assert.deepEqual(s.outlineRows, expectedRows, panelId + ' ' + label + ' outlines'); assert.deepEqual(s.notes, selected.map(() => 'Reviewed'), panelId + ' ' + label + ' notes'); };
        verifyShape(staticShapes[panelIndex], 'JavaScript-disabled');
        await stage.scrollIntoViewIfNeeded(); await page.evaluate(() => window.__richview!.settled());
        const svg = view.locator('.rvc-svg'), geometry = await svg.evaluate(shape); verifyShape(geometry, 'live');
        await stage.focus(); const keyboard = [], pointer = [], touch = [];
        for (const [index, name] of names.entries()) {
          const categoryRows = sourceRows.filter(r => String(r[category]) === name);
          const sum = categoryRows.reduce((sum, r) => sum + Number(r.value), 0);
          const expectedValue = categoryRows.length === 1 ? String(categoryRows[0].value) : Number.isInteger(sum) ? sum.toFixed(1) : String(sum);
          await stage.press(index ? 'ArrowRight' : 'Home');
          await page.evaluate(() => new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done))));
          const actual = await tipData(tip);
          assert.equal(actual.heading, name, panelId + ' keyboard category');
          assert(actual.fields.some(([key, value]) => key === 'value' && value === expectedValue), panelId + ' exact value ' + JSON.stringify(actual));
          keyboard.push(actual);
        }
        await stage.press('Escape'); await page.mouse.move(1, 1); await tip.waitFor({ state: 'hidden' });
        for (const [index, mark] of geometry.marks.entries()) {
          if (!mark.path) continue;
          const [cx, cy] = mark.center!.split(' ').map(Number), [from, to] = mark.angles!.split(' ').map(Number), radius = Number(mark.radius), angle = (from + to) / 2;
          const point = await svg.evaluate((s, p) => { const svg = s as SVGSVGElement; const q = svg.createSVGPoint(); q.x = p[0]; q.y = p[1]; const v = q.matrixTransform(svg.getScreenCTM()!); return { x: v.x, y: v.y }; }, [cx + Math.cos(angle) * radius * 0.6, cy + Math.sin(angle) * radius * 0.6]);
          await page.mouse.move(point.x, point.y); await page.evaluate(() => new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done))));
          const actual = await tipData(tip); assert.deepEqual(actual, keyboard[index], panelId + ' pointer identity'); pointer.push(actual);
          await page.touchscreen.tap(point.x, point.y); const tapped = await tipData(tip); assert.deepEqual(tapped, keyboard[index], panelId + ' touch identity'); touch.push(tapped);
          await stage.press('Escape'); await page.mouse.move(1, 1); await tip.waitFor({ state: 'hidden' });
        }
        const [download] = await Promise.all([page.waitForEvent('download'), actions.getByRole('button', { name: 'Download SVG', exact: true }).click()]);
        const exportPath = join(out, panelId + '.svg'); await download.saveAs(exportPath);
        await exported.setContent('<!doctype html><style>html,body{margin:0}svg{max-width:100%;height:auto}</style>' + readFileSync(exportPath, 'utf8')); await exported.evaluate(() => document.fonts.ready);
        const exportShape = await exported.locator('svg').evaluate(shape); verifyShape(exportShape, 'export');
        assert.deepEqual(exportShape.marks, geometry.marks, panelId + ' exported sector geometry');
        await exported.locator('svg').screenshot({ path: join(out, panelId + '-export.png') });
        await page.bringToFront();
        await actions.getByRole('button', { name: 'Table', exact: true }).click();
        const table = await view.locator('.rvc-table tbody tr').evaluateAll(nodes => nodes.map(n => [...n.querySelectorAll('td')].map(c => c.textContent))), keys = Object.keys(sourceRows[0]).sort();
        assert.deepEqual(table, sourceRows.map(row => keys.map(key => String(row[key]))), panelId + ' original table rows');
        const [csv] = await Promise.all([page.waitForEvent('download'), actions.getByRole('button', { name: 'Download CSV', exact: true }).click()]);
        const csvPath = join(out, panelId + '.csv'); await csv.saveAs(csvPath);
        const csvRows = readFileSync(csvPath, 'utf8').trimEnd().split(/\r?\n/).map(line => line.split(',').map(s => s.replace(/^"|"$/g, '').replace(/""/g, '"')));
        assert.deepEqual(csvRows, [keys, ...sourceRows.map(row => keys.map(key => String(row[key])))], panelId + ' original CSV rows');
        await page.mouse.move(1, 1); await page.evaluate(async () => { await window.__richview!.settled(); await Promise.allSettled(document.getAnimations().filter(a => a.playState === 'running').map(a => a.finished)); });
        const activeAnimations = await page.evaluate(() => document.getAnimations().filter(a => a.playState === 'running').length); assert.equal(activeAnimations, 0);
        manifest.records.push({ id: panelId, selected, geometry, staticShape: staticShapes[panelIndex], exportShape, keyboard, pointer, touch, table, csvPath, exportPath, activeAnimations, live: JSON.parse((await view.getAttribute('data-rvc-diagnostics'))!), native: rendered.metadata?._richview?.chart_diagnostics });
        writeFileSync(join(out, 'checks.json'), JSON.stringify(manifest, null, 2)); console.log(panelId + ': native/live/export targets and source controls pass');
      }
    }
    await context.close(); await staticContext.close();
  }
  assert.deepEqual(errors, []);
} finally { await browser.close(); server.closeAllConnections(); await new Promise<void>(done => server.close(() => done())); }
