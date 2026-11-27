// Host reason (ARC-03): real keyboard traversal and computed accessibility evidence, per chart type.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { cli, harness, watchErrors } from './browser-harness.ts';
import { reports, settle } from './report-runtime.ts';
import { announcement, chartTypes, requireTypes } from './chart-type-evidence.ts';
import { displayedReadout } from './chart-interaction-checks.ts';
import type { Target } from './report-model.ts';

type KeyboardEvidence = { target: Target | null; live: string | null; politeness: string | null; focused: boolean; readout: (string | null | undefined)[] | null };

const started = performance.now();
const expected = chartTypes(execFileSync(cli, ['learn', 'chart-contract'], { encoding: 'utf8' }));
const h = await harness('chart-type-keyboard');
const errors: string[] = [], results = [];
let inputs, context, failure: unknown;
try {
  inputs = await reports(h.dir, { style: 'air', only: ['every-block'], extra: {} });
  context = await h.browser.newContext({ viewport: { width: 1440, height: 900 }, reducedMotion: 'reduce' });
  const page = await context.newPage();
  watchErrors(page, errors);
  page.setDefaultTimeout(5000);
  const response = await page.goto(inputs.base + '/every-block/report.html', { waitUntil: 'load' });
  assert.equal(response!.status(), 200, 'generated every-block report loads');
  const models = await settle(page);
  const facts = await page.evaluate(() => ({
    figures: [...document.querySelectorAll('figure[data-rv-chart]')].map(figure => ({ id: figure.id,
      title: figure.querySelector('.rv-chart__title')?.textContent.trim(),
      label: figure.querySelector('.rv-chart__frame')?.getAttribute('aria-label'),
      tab_index: figure.querySelector<HTMLElement>('.rv-chart__frame')?.tabIndex })),
    focusable: document.querySelectorAll('a[href],button,input,select,textarea,summary,[tabindex]').length,
    initial: document.activeElement!.tagName,
  }));
  assert.equal(models.length, facts.figures.length, 'one runtime model per chart figure');
  const cohort = models.map(model => ({ id: model.figure, type: model.geometry.kind }));
  requireTypes(expected, cohort);
  const byId = new Map(models.map((model, index) => [model.figure, { model, ...facts.figures[index] }]));
  for (const [index, model] of models.entries()) {
    assert.equal(model.figure, facts.figures[index].id, 'model/DOM order');
    assert.ok(facts.figures[index].tab_index! >= 0, `${model.figure}: chart belongs to sequential keyboard navigation`);
  }
  assert.equal(facts.initial, 'BODY', 'Tab traversal starts at the untouched document');
  const cdp = await context.newCDPSession(page);
  const { root } = await cdp.send('DOM.getDocument', { depth: 0 });
  const { nodeIds } = await cdp.send('DOM.querySelectorAll', { nodeId: root.nodeId, selector: 'figure[data-rv-chart] .rv-chart__frame' });
  assert.equal(nodeIds.length, models.length);
  const { nodeIds: liveNodeIds } = await cdp.send('DOM.querySelectorAll', { nodeId: root.nodeId, selector: 'figure[data-rv-chart] .rv-chart__live' });
  assert.equal(liveNodeIds.length, models.length);
  const axById = new Map();
  for (const [index, nodeId] of nodeIds.entries()) {
    const { node: dom } = await cdp.send('DOM.describeNode', { nodeId });
    const { nodes } = await cdp.send('Accessibility.getPartialAXTree', { nodeId, fetchRelatives: false });
    const ax = nodes.find(node => node.backendDOMNodeId === dom.backendNodeId && !node.ignored);
    assert.ok(ax, `chart ${index} is in the accessibility tree`);
    axById.set(models[index].figure, { role: ax.role?.value, name: ax.name?.value,
      focusable: ax.properties?.find(prop => prop.name === 'focusable')?.value.value });
  }
  const visited = new Set();
  let tabs = 0;
  while (tabs++ < facts.focusable + 2 && visited.size < models.length) {
    await page.keyboard.press('Tab');
    const id = await page.evaluate(() => {
      const active = document.activeElement;
      return active!.matches('.rv-chart__frame') ? active!.closest('figure')!.id : null;
    });
    if (!id) continue;
    assert.ok(byId.has(id), `Tab reached an unknown chart ${id}`);
    assert.ok(!visited.has(id), `Tab wrapped before reaching every chart (at ${id})`);
    visited.add(id);
    const { model, title, label } = byId.get(id)!;
    const ax = axById.get(id);
    assert.equal(ax.role, 'application', `${id}: interactive chart role`);
    assert.equal(ax.focusable, true, `${id}: computed focusable`);
    assert.equal(ax.name, label, `${id}: computed accessible name`);
    assert.ok(title && ax.name.includes(title) && ax.name.includes(model.geometry.summary.replace(/\.+$/, '')), `${id}: name contains title and summary`);
    const keys = [];
    for (const key of ['Home', 'ArrowRight', 'End']) {
      await page.keyboard.press(key);
      const actual: KeyboardEvidence = await page.evaluate((id: string) => {
        const model = window.__richview!.charts().find(chart => chart.figure === id);
        const figure = document.getElementById(id), live = figure!.querySelector('.rv-chart__live');
        const readout = figure!.querySelector('.rv-chart__readout');
        return { target: model!.target, live: live!.textContent, politeness: live!.getAttribute('aria-live'),
          focused: document.activeElement === figure!.querySelector('.rv-chart__frame'),
          readout: readout && !(readout as HTMLElement).hidden ? [readout.querySelector('.rv-chart__readout-head')?.textContent,
            ...[...readout.querySelectorAll('.rv-chart__readout-line')].map(line => line.textContent)] : null };
      }, id);
      assert.ok(actual.focused, `${id}: ${key} retains keyboard focus`);
      assert.equal(actual.politeness, 'polite');
      const spoken = announcement(model.geometry, actual.target);
      assert.equal(actual.live, spoken, `${id}: ${key} announces selected observation`);
      if (key === 'Home') {
        const nodeId = liveNodeIds[models.indexOf(model)];
        const { node: dom } = await cdp.send('DOM.describeNode', { nodeId });
        const { nodes } = await cdp.send('Accessibility.getPartialAXTree', { nodeId, fetchRelatives: true });
        const live = nodes.find(node => node.backendDOMNodeId === dom.backendNodeId && !node.ignored);
        assert.ok(live?.properties?.some(prop => prop.name === 'live' && prop.value.value === 'polite'), `${id}: live region is exposed to accessibility`);
        assert.ok(nodes.some(node => live!.childIds?.includes(node.nodeId) && !node.ignored && node.role?.value === 'StaticText' && node.name?.value === spoken), `${id}: announced observation is accessible text`);
      }
      const panel = model.geometry.panels[actual.target!.panel];
      const reading = displayedReadout(panel.observations[actual.target!.observation].readout, panel.targeting, model.geometry.kind);
      assert.deepEqual(actual.readout, [reading.heading, ...reading.lines.map(line => `${line.name}${line.value}`)], `${id}: ${key} readout`);
      keys.push({ key, target: actual.target, announced: spoken });
    }
    const count = model.geometry.panels.reduce((sum, panel) => sum + panel.observations.length, 0);
    assert.ok(count > 0, `${id}: every-block chart has keyboard observations`);
    if (count > 1) assert.notDeepEqual(keys[0].target, keys[1].target, `${id}: ArrowRight moves`);
    await page.keyboard.press('Escape');
    assert.equal(await page.evaluate(id => {
      const model = window.__richview!.charts().find(chart => chart.figure === id);
      return model!.target === null && (document.getElementById(id)!.querySelector('.rv-chart__readout')! as HTMLElement).hidden;
    }, id), true, `${id}: Escape clears target/readout`);
    results.push({ id, type: model.geometry.kind, name: ax.name, tab: tabs, keys, ok: true });
  }
  assert.equal(visited.size, models.length, 'every chart is reachable through actual Tab navigation');
  await page.keyboard.press('Tab');
  assert.equal(await page.evaluate(() => document.activeElement!.matches('.rv-chart__frame')), false, 'Tab leaves the last chart');
  requireTypes(expected, results);
  assert.deepEqual(errors, [], 'no browser/runtime errors');
  await cdp.detach();
} catch (error) { failure = error; }
finally {
  try { await context?.close(); }
  finally {
    try { await inputs?.close(); }
    finally { try { await h.close(); } finally { rmSync(join(h.dir, 'reports'), { recursive: true, force: true }); } }
  }
}
const seconds = (performance.now() - started) / 1000;
const saved = h.save('types.json', { expected, results, errors, seconds, ok: !failure, failure: (failure as Error | undefined)?.stack });
const out = process.argv.indexOf('--out');
if (out >= 0) { mkdirSync(process.argv[out + 1], { recursive: true }); copyFileSync(saved, join(process.argv[out + 1], 'types.json')); }
if (failure) throw failure;
console.log(`PASS ${results.length} charts / ${expected.length} types: real Tab, computed accessible names, Home/arrows/End/Escape and exact live announcements (${seconds.toFixed(2)} s)`);
