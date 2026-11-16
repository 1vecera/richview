// A fixed review matrix, usable before and after every chart change. Full output stays in tmp;
// the checked-in evidence is a small comparison gallery rather than another artifact archive.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join, relative, resolve } from 'node:path';
import { parseArgs } from 'node:util';
import { chromium, errors } from 'playwright';
import { launchBrowser } from '../tools/rv-gate/lib/browser-launch.mjs';
import { chromiumPath, root, serveStatic, settledCharts, waitForBenchmark } from './browser-harness.mjs';
import { CHROMIUM_ARGS } from '../tools/rv-gate/lib/captures.mjs';
import { pool } from '../tools/rv-gate/cross-style.mjs';
import { buildFacts, produce } from './design-review/outputs.mjs';

const { values } = parseArgs({ options: {
  out: { type: 'string' }, phase: { type: 'string', default: 'after' }, baseline: { type: 'string' },
  themes: { type: 'string' }, widths: { type: 'string' }, cases: { type: 'string' },
} });
assert.ok(values.out, 'Use --out DIR [--phase before|after] [--baseline DIR]');
assert.ok(['before', 'after'].includes(values.phase));
const themes = values.themes?.split(',') ?? ['sleek', 'air', 'scientific', 'brutalist', 'hand-drawn'];
const widths = values.widths?.split(',').map(Number) ?? [1440, 768, 390, 320];
// Each case's chart count comes from a list independent of its source, so a dropped chart fails;
// the flagship's is the research pipeline's committed chart inventory.
const cases = [
  { id: 'standard', source: 'benchmarks/chart-regression/report.rmd', charts: 8 },
  { id: 'flagship', source: 'examples/openai-vs-anthropic/report.rmd', charts: JSON.parse(readFileSync(join(root, 'examples/openai-vs-anthropic/data/charts/index.json'), 'utf8')).length },
  { id: 'bar-study-01', source: 'benchmarks/chart-craft/reports/bar-01/report.rmd', charts: 1 },
].filter(item => !values.cases || values.cases.split(',').includes(item.id)).map(item => ({ ...item, output: 'report' }));
const out = resolve(values.out);
mkdirSync(out, { recursive: true });
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const manifest = { phase: values.phase, build: buildFacts(), fixture: hash(readFileSync(join(root, 'benchmarks/chart-regression/report.rmd'))), themes, widths, captures: [], errors: [] };
if (values.baseline) {
  const before = JSON.parse(readFileSync(join(resolve(values.baseline), 'manifest.json'), 'utf8'));
  assert.equal(before.fixture, manifest.fixture, 'Comparison uses the exact same chart/comment source');
  assert.deepEqual(before.themes, themes); assert.deepEqual(before.widths, widths);
  manifest.baseline = { commit: before.build.commit, path: relative(root, resolve(values.baseline)) };
}
const outputs = cases.map(item => ({ ...item, outputs: Object.fromEntries(themes.map(theme => [theme, produce(item, theme, out)])) }));
const tasks = outputs.flatMap(item => themes.flatMap(theme => widths.map(width => ({ item, theme, width })))).map((task, index) => ({ ...task, index }));
assert.ok(tasks.length, 'The visual matrix has no cases');
const records = new Array(tasks.length);
const workers = Number(process.env.RICHVIEW_VERIFY_WORKERS ?? 1);
assert.ok(Number.isSafeInteger(workers) && workers > 0, 'RICHVIEW_VERIFY_WORKERS must be a positive safe integer');
const server = await serveStatic({ '/': out }, { route(path, url, res) { if (path === '/favicon.ico') { res.writeHead(204); res.end(); return true; } } });
let browser;
const captureCase = async ({ item, theme, width, index }) => {
  const dir = join(out, item.id, `${theme}-${width}`); mkdirSync(dir, { recursive: true });
  const context = await browser.newContext({ viewport: { width, height: 1000 }, reducedMotion: 'reduce', isMobile: width < 600, hasTouch: width < 600 });
  try {
    const page = await context.newPage();
    const errors = [];
    page.on('pageerror', e => errors.push(e.message));
    page.on('console', m => { if (m.type() === 'error') errors.push(m.text()); });
    await page.goto(server.base + item.outputs[theme].url, { waitUntil: 'load' });
    await settledCharts(page, item.charts);
    const models = await page.evaluateHandle(() => window.__richview.charts());
    const record = { case: item.id, theme, width, charts: [], errors, overflow: await page.evaluate(() => document.documentElement.scrollWidth > innerWidth + 1) };
    assert.equal(await page.locator('figure[data-rv-chart]').count(), item.charts);
    for (let i = 0; i < item.charts; i++) {
      const figure = page.locator('figure[data-rv-chart]').nth(i);
      await figure.scrollIntoViewIfNeeded();
      await page.evaluate(() => window.__richview.settled());
      // Text boxes come from the chart's geometry model, the same model the canvas draws.
      const result = await figure.evaluate((element, { index, models }) => {
        const canvas = element.querySelector('canvas.rv-chart__canvas');
        const viewport = canvas.getBoundingClientRect();
        // Title, axis titles and legend share the figure's left edge.
        const inset = element.querySelector('.rv-chart__frame').getBoundingClientRect().left;
        const titleNode = element.querySelector('.rv-chart__title'), title = titleNode.getBoundingClientRect();
        const css = getComputedStyle(titleNode), measure = document.createElement('canvas').getContext('2d');
        const weight = css.fontFamily.includes('Patrick Hand') && css.getPropertyValue('font-synthesis-weight') === 'none' ? '400' : css.fontWeight;
        measure.font = `${weight} ${css.fontSize} ${css.fontFamily}`;
        const bearing = -measure.measureText(titleNode.textContent.trim()[0] || '').actualBoundingBoxLeft;
        const geometry = models[index].geometry;
        const items = [...geometry.panels.flatMap(panel => panel.items), ...(geometry.figure_items ?? [])];
        const texts = items.filter(item => item.kind === 'text' && item.bounds.w > 0 && item.bounds.h > 0);
        const upperTitles = texts.filter(t => t.role === 'panel-title' || t.role === 'axis-title' && t.y < 40 && t.anchor === 'start');
        const legend = [...element.querySelectorAll('.rv-chart__legend .rv-chart__legend-entry')].find(e => e.getBoundingClientRect().width);
        const alignment = {
          caption: title.left + bearing - inset,
          axes: upperTitles.map(t => ({ text: t.text, offset: viewport.left + t.bounds.x - inset })),
          legend: legend ? legend.getBoundingClientRect().left - inset : null,
        };
        // The engine's own tolerances (layout/checks.rs): boxes are font boxes, so text may
        // clip by 0.5 px and overlap by 1 px; rotated text is left to the engine's checks.
        const level = texts.filter(t => !t.rotate);
        const collisions = [], clips = [];
        for (let a = 0; a < level.length; a++) {
          const r = level[a].bounds;
          if (r.x < -0.5 || r.y < -0.5 || r.x + r.w > geometry.width + 0.5 || r.y + r.h > geometry.height + 0.5) clips.push(level[a].text);
          for (const b of level.slice(a + 1)) {
            const q = b.bounds;
            if (r.x < q.x + q.w - 1 && q.x < r.x + r.w - 1 && r.y < q.y + q.h - 1 && q.y < r.y + r.h - 1) collisions.push([level[a].text, b.text]);
          }
        }
        return { title: titleNode.textContent, alignment, collisions, clips, labels: texts.map(t => t.text) };
      }, { index: i, models });
      if (values.phase === 'after') {
        assert.ok(Math.abs(result.alignment.caption) < 1.5, `${result.title}: title ink follows the figure's left edge (${result.alignment.caption}px)`);
        for (const a of result.alignment.axes) assert.ok(Math.abs(a.offset) < 1.5, `${result.title}: ${a.text} shares the figure's left edge (${a.offset}px)`);
        if (result.alignment.legend !== null) assert.ok(Math.abs(result.alignment.legend) < 1.5, `${result.title}: legend inset (${result.alignment.legend}px)`);
      }
      const name = String(i + 1).padStart(2, '0');
      result.rest = relative(out, join(dir, `${name}-rest.png`));
      await figure.screenshot({ path: join(out, result.rest) });
      await figure.locator('.rv-chart__frame').focus(); await page.keyboard.press('Home');
      await page.evaluate(() => window.__richview.settled());
      const readout = figure.locator('.rv-chart__readout');
      result.tooltipText = await readout.evaluate(node => (node.hidden ? '' : node.innerText));
      assert.ok(result.tooltipText.trim(), `${result.title}: every case has a real populated readout`);
      result.tooltipBounds = await readout.evaluate(t => { const r = t.getBoundingClientRect(); return { left: r.left, right: r.right, width: r.width, overflow: t.scrollWidth > t.clientWidth + 1, viewport: innerWidth }; });
      if (values.phase === 'after') assert.ok(result.tooltipBounds.left >= -1 && result.tooltipBounds.right <= width + 1 && !result.tooltipBounds.overflow, `${result.title}: readout containment`);
      result.tooltip = relative(out, join(dir, `${name}-tooltip.png`));
      await figure.screenshot({ path: join(out, result.tooltip) });
      await page.keyboard.press('Escape');
      record.charts.push(result);
    }
    if (values.phase === 'after' && item.id === 'standard' && theme === 'sleek' && width === 390) {
      // A keyboard toggle cycle catches lost identities; the last visible series cannot be
      // hidden (FEA-CH-33).
      const figure = page.locator('figure[data-rv-chart]').nth(1);
      const button = (series, pressed) => figure.locator(`[data-rv-series="${series}"]${pressed === undefined ? '' : `[aria-pressed="${pressed}"]`}`).first();
      await button('Alpha').focus(); await page.keyboard.press('Enter');
      await button('Alpha', false).waitFor({ state: 'attached' });
      await button('Beta').focus(); await page.keyboard.press('Enter');
      await page.evaluate(() => window.__richview.settled());
      assert.equal(await button('Beta').getAttribute('aria-pressed'), 'true', 'The last visible series stays shown');
      await button('Alpha').focus(); await page.keyboard.press('Space');
      await button('Alpha', true).waitFor({ state: 'attached' });
      await page.evaluate(() => window.__richview.settled());
      assert.equal(await figure.locator('.rv-chart__name').count(), 2, 'Both line-end names return');
      assert.equal(await figure.locator('.rv-chart__legend-entry').evaluateAll(entries => entries.filter(e => e.getBoundingClientRect().width).length), 0, 'No legend beside the line-end names');
      record.keyboardRestore = true;
    }
    await page.evaluate(() => scrollTo(0, 0));
    await page.screenshot({ path: join(dir, 'page-header.png') });
    await page.screenshot({ path: join(dir, 'page.png'), fullPage: true });
    records[index] = record;
    manifest.captures = records.filter(Boolean);
    writeFileSync(join(out, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
    console.log(`${item.id} ${theme} ${width}: ${record.charts.reduce((n, c) => n + c.collisions.length + c.clips.length, 0)} label findings`);
  } finally {
    await context.close();
  }
};
try {
  // Keep the capture step's exact Chromium raster flags; only independent contexts overlap.
  await waitForBenchmark();
  browser = await launchBrowser(chromium, { executablePath: chromiumPath(), headless: true, args: CHROMIUM_ARGS }, { TimeoutError: errors.TimeoutError });
  await pool(tasks, workers, captureCase);
} finally {
  try { await browser?.close(); } finally { await server.close(); }
}
assert.equal(manifest.captures.length, tasks.length, 'Every visual matrix case completed');
manifest.errors = manifest.captures.flatMap(capture => [...capture.errors, ...(capture.overflow ? [`${capture.case} ${capture.theme} ${capture.width}: page overflow`] : [])]);
manifest.labelFindings = manifest.captures.flatMap(capture => capture.charts.flatMap(chart => chart.collisions.concat(chart.clips))).length;
manifest.status = manifest.errors.length || values.phase === 'after' && manifest.labelFindings ? 'failed' : 'passed';
writeFileSync(join(out, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
assert.equal(manifest.status, 'passed', `${manifest.errors.join('\n')}\n${manifest.labelFindings} label findings`);
console.log(`Saved ${manifest.captures.length} captures to ${out}`);
