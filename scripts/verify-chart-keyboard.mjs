// Trusted keyboard input must survive canvas redraws and responsive legend changes.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { cli, harness, inputValue, root, serveStatic, settled, settledCharts, watchErrors } from './browser-harness.mjs';

const h = await harness('chart-keyboard');
const errors = [], cases = [];
let status = 'running', failure, reportServer;
// Genuine runtime gaps stay failing assertions, collected so one gap does not hide the other cases.
const failures = [];
const expect = (label, actual, expected) => { if (actual !== expected) failures.push(`${label}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`); };

/** In the page: whether the first chart's frame shows its ring, and the text boxes, on the canvas or the page, that come within 3 px of it. */
const FRAME_RING = () => {
  const figure = document.querySelector('figure[data-rv-chart]');
  const frame = figure.querySelector('.rv-chart__frame');
  const ring = getComputedStyle(frame, '::after');
  const f = frame.getBoundingClientRect();
  const width = parseFloat(ring.borderTopWidth) || 0;
  const outer = { x: f.x + parseFloat(ring.left), y: f.y + parseFloat(ring.top), right: f.right - parseFloat(ring.right), bottom: f.bottom - parseFloat(ring.bottom) };
  const boxes = [];
  const g = window.__richview.charts()[0].geometry;
  const canvas = figure.querySelector('canvas.rv-chart__canvas').getBoundingClientRect();
  for (const item of [...g.panels.flatMap(p => p.items), ...(g.figure_items ?? [])]) {
    if (item.kind !== 'text') continue;
    const b = item.bounds, turn = (item.rotate ?? 0) * Math.PI / 180;
    const corners = [[b.x, b.y], [b.x + b.w, b.y], [b.x + b.w, b.y + b.h], [b.x, b.y + b.h]].map(([x, y]) => {
      const dx = x - item.x, dy = y - item.y;
      return [item.x + dx * Math.cos(turn) - dy * Math.sin(turn), item.y + dx * Math.sin(turn) + dy * Math.cos(turn)];
    });
    // The canvas cuts text at its edges.
    const xs = corners.map(c => c[0]), ys = corners.map(c => c[1]);
    const box = { text: item.text, x: canvas.x + Math.max(0, Math.min(...xs)), y: canvas.y + Math.max(0, Math.min(...ys)), right: canvas.x + Math.min(g.width, Math.max(...xs)), bottom: canvas.y + Math.min(g.height, Math.max(...ys)) };
    if (box.right > box.x && box.bottom > box.y) boxes.push(box);
  }
  // The page's text: the boxes of every visible element outside the frame that holds text itself.
  for (const element of figure.querySelectorAll('*')) {
    if (element.closest('.rv-chart__frame, script')) continue;
    if (![...element.childNodes].some(n => n.nodeType === Node.TEXT_NODE && n.textContent.trim())) continue;
    // Text nobody sees, such as an action's tooltip before its button is hovered.
    let hidden = false;
    for (let e = element; e && e !== figure.parentElement; e = e.parentElement) {
      const style = getComputedStyle(e);
      if (style.opacity === '0' || style.visibility === 'hidden') { hidden = true; break; }
    }
    if (hidden) continue;
    for (const r of element.getClientRects()) if (r.width && r.height) boxes.push({ text: element.textContent.trim().slice(0, 24), x: r.x, y: r.y, right: r.right, bottom: r.bottom });
  }
  const shown = frame.matches(':focus-visible') && !['none', 'normal'].includes(ring.content) && width > 0;
  // Per side, the nearest text outside the ring's band and inside it. A side keeps 3 px from
  // both; where the text on either side leaves less than 3 + 2 + 3 px, the ring sits midway
  // and crosses neither.
  const sides = [
    { side: 'top', across: [outer.x, outer.right], at: [outer.y, outer.y + width], out: b => outer.y - b.bottom, in: b => b.y - (outer.y + width), span: b => [b.x, b.right] },
    { side: 'bottom', across: [outer.x, outer.right], at: [outer.bottom - width, outer.bottom], out: b => b.y - outer.bottom, in: b => outer.bottom - width - b.bottom, span: b => [b.x, b.right] },
    { side: 'left', across: [outer.y, outer.bottom], at: [outer.x, outer.x + width], out: b => outer.x - b.right, in: b => b.x - (outer.x + width), span: b => [b.y, b.bottom] },
    { side: 'right', across: [outer.y, outer.bottom], at: [outer.right - width, outer.right], out: b => b.x - outer.right, in: b => outer.right - width - b.right, span: b => [b.y, b.bottom] },
  ];
  const crowding = [];
  if (shown) for (const s of sides) {
    let out = Infinity, inside = Infinity, crossed = null;
    for (const b of boxes) {
      const [from, to] = s.span(b);
      if (to <= s.across[0] || from >= s.across[1]) continue;
      const o = s.out(b), i = s.in(b);
      if (o >= 0) out = Math.min(out, o); else if (i >= 0) inside = Math.min(inside, i); else crossed = b.text;
    }
    const roomy = out >= 3 && inside >= 3, midway = out + inside < 6 && Math.abs(out - inside) <= 0.6;
    if (crossed !== null || !(roomy || midway)) crowding.push({ side: s.side, out: Math.round(out * 10) / 10, in: Math.round(inside * 10) / 10, crossed });
  }
  return { shown, selected: frame.hasAttribute('data-rv-selected'), width, crowding };
};

/** In the page: the share of probes along the middle of a mark ring, 3 px past the selected mark's contour, that are drawn in the link colour, and the mark's centre in the viewport. */
const MARK_RING = () => {
  const figure = document.querySelector('figure[data-rv-chart]');
  const frame = figure.querySelector('.rv-chart__frame');
  const canvas = figure.querySelector('canvas.rv-chart__canvas');
  const model = window.__richview.charts()[0];
  const t = model.target;
  if (!t) return { ringed: false, share: 0, at: [0, 0] };
  const panel = model.geometry.panels[t.panel], o = panel.observations[t.observation];
  const around = (c, r) => Array.from({ length: 8 }, (_, k) => [c[0] + r * Math.cos(k * Math.PI / 4), c[1] + r * Math.sin(k * Math.PI / 4)]);
  const probes = [];
  let center = o.at;
  if (panel.targeting === 'nearest-x') for (const d of o.dots.length ? o.dots : [{ at: o.at }]) probes.push(...around(d.at, 11));
  else for (const m of panel.items.filter(i => i.kind !== 'text' && i.role === 'mark' && i.obs === t.observation)) {
    if (m.kind === 'point') { center = m.center; probes.push(...around(m.center, m.radius + (m.stroke ? m.stroke.width / 2 : 0) + 4 + 3)); }
    if (m.kind === 'rect') {
      const r = m.rect, d = 0.75 + 3;
      center = [r.x + r.w / 2, r.y + r.h / 2];
      probes.push([r.x + r.w / 2, r.y - d], [r.x + r.w / 2, r.y + r.h + d], [r.x - d, r.y + r.h / 2], [r.x + r.w + d, r.y + r.h / 2]);
    }
  }
  const swatch = document.createElement('canvas').getContext('2d');
  const style = getComputedStyle(frame);
  swatch.fillStyle = style.getPropertyValue('--rv-link').trim() || style.getPropertyValue('--rv-accent').trim();
  swatch.fillRect(0, 0, 1, 1);
  const link = swatch.getImageData(0, 0, 1, 1).data;
  const ctx = canvas.getContext('2d'), scale = canvas.width / canvas.clientWidth;
  const plot = panel.plot;
  const inside = ([x, y]) => x >= plot.x - 8 && y >= plot.y - 8 && x <= plot.x + plot.w + 8 && y <= plot.y + plot.h + 8;
  // The ring is 2 px wide on a fractional grid: the nearest of each probe's 3 by 3 pixels.
  const hits = probes.filter(inside).map(([x, y]) => {
    const d = ctx.getImageData(Math.round(x * scale) - 1, Math.round(y * scale) - 1, 3, 3).data;
    let nearest = Infinity;
    for (let i = 0; i < d.length; i += 4) nearest = Math.min(nearest, Math.hypot(d[i] - link[0], d[i + 1] - link[1], d[i + 2] - link[2]));
    return nearest < 40;
  });
  const share = hits.length ? hits.filter(Boolean).length / hits.length : 0;
  const box = canvas.getBoundingClientRect();
  return { ringed: share >= 0.75, share, probes: hits.length, at: [box.x + center[0], box.y + center[1]] };
};

// One report per chart mode, generated by the release CLI and drawn by the report runtime.
const modes = ['numeric-line', 'category-line', 'numeric-area', 'category-bar', 'numeric-scatter', 'calendar-line', 'crowded-bar'];
const chartFor = mode => {
  const [axis, type] = mode.split('-');
  const groups = axis === 'crowded' ? Array.from({ length: 9 }, (_, i) => `Long series name ${i + 1}`) : ['Žilina ["North"]', 'South'];
  const rows = groups.flatMap((group, g) => [0, 1, 2].map(i => ({
    group,
    x: axis === 'calendar' ? `2024-0${i + 1}-01` : axis === 'category' || axis === 'crowded' ? ['First', 'Middle', 'Last'][i] : i,
    value: [2, 8, 4][i] + g * 3,
  })));
  return { type, title: 'Series keyboard control', encoding: { x: 'x', y: 'value', color: 'group' }, data: { inline: rows } };
};
const reportSource = chart => '---\ntitle: Keyboard\nversion: "1"\n---\n\n```chart\n' + JSON.stringify(chart) + '\n```\n';
const out = join(h.dir, 'reports');
for (const mode of modes) {
  mkdirSync(join(out, mode), { recursive: true });
  writeFileSync(join(out, mode, 'report.rmd'), reportSource(chartFor(mode)));
  execFileSync(cli, ['generate', join(out, mode, 'report.rmd'), '--style', 'scientific', '--format', 'json'], { cwd: root, stdio: 'pipe' });
}
reportServer = await serveStatic(Object.fromEntries(modes.map(mode => [`/${mode}/`, join(out, mode, 'Rendered')])));

try {
  const page = await h.browser.newPage({ viewport: { width: 1100, height: 900 }, reducedMotion: 'reduce' });
  watchErrors(page, errors);
  const entries = page.locator('figure[data-rv-chart] button.rv-chart__legend-entry:visible');
  // The focused legend entry is named by its series, so it survives a redraw only if it is the same series.
  const focused = () => page.evaluate(() => {
    const node = document.activeElement;
    // A series control is its legend entry or, once the legend folds into line-end names, its name (FEA-CH-27).
    const series = node?.classList.contains('rv-chart__legend-entry') || node?.classList.contains('rv-chart__name');
    return { entry: series ? node.textContent.trim() : null, frame: node?.classList.contains('rv-chart__frame') || false, id: node?.id ?? null };
  });
  const redraw = async width => { await page.setViewportSize({ width, height: 900 }); await settled(page); await page.evaluate(() => window.__richview.settled()); };
  for (const width of [720, 320]) for (const mode of modes) {
    if (mode === 'crowded-bar' && width === 320) continue;
    await page.setViewportSize({ width, height: 900 });
    await page.goto(`${reportServer.base}/${mode}/report.html`);
    await settledCharts(page, 1);
    // Isolate focus-induced scrolling from the page's smooth scroll and layout anchoring.
    await page.evaluate(() => { document.documentElement.style.scrollBehavior = 'auto'; document.body.style.overflowAnchor = 'none'; });
    // Wide line and area charts label their series directly on the canvas; the legend buttons
    // (the keyboard series control) exist where the legend is shown, so operate them there.
    const item = { width, mode };
    let at = width;
    if (!await entries.count()) {
      item.direct = true;
      // A wide chart narrowing to a phone must gain its legend buttons (soft: runtime gap, see report).
      await redraw(320);
      if (!await entries.count()) failures.push(`${mode}/${width}: no legend buttons after the viewport narrowed to 320 px without a reload`);
      at = 320;
      await page.goto(`${reportServer.base}/${mode}/report.html`);
      await page.setViewportSize({ width: at, height: 900 });
      await settledCharts(page, 1);
      await page.evaluate(() => { document.documentElement.style.scrollBehavior = 'auto'; document.body.style.overflowAnchor = 'none'; });
    }
    const control = mode === 'crowded-bar' ? entries.last() : entries.first();
    const key = (await control.textContent()).trim(); assert.ok(key);
    item.key = key; cases.push(item);
    const pressed = state => page.waitForFunction(({ key, state }) =>
      [...document.querySelectorAll('figure[data-rv-chart] button.rv-chart__legend-entry')].some(node =>
        node.offsetParent && node.textContent.trim() === key && node.getAttribute('aria-pressed') === state), { key, state });

    await control.focus();
    await page.keyboard.press('Enter');
    await page.waitForFunction(() => window.__richview.charts()[0].hidden.length === 1);
    // Hiding a series must keep its legend button so it can be restored from the keyboard.
    if (!await entries.count()) {
      failures.push(`${mode}/${width}: the legend (and the keyboard way to restore the series) disappears once a series is hidden`);
      continue;
    }
    await pressed('false');
    item.hiddenFocus = await focused();
    expect(`${mode}/${width}: hiding a series retains keyboard focus`, item.hiddenFocus.entry, key);
    await control.focus();
    // No second focus call: Space must operate the same series after its marks change.
    await page.keyboard.press('Space'); await pressed('true');
    item.restoredFocus = await focused();
    expect(`${mode}/${width}: Space restores the focused series with focus kept`, item.restoredFocus.entry, key);
    assert.deepEqual(await page.evaluate(() => window.__richview.charts()[0].hidden), [], `${mode}/${width}: the chart shows the series again`);

    // A reader can scroll away from a focused chart; a responsive redraw must not pull it back.
    await page.evaluate(() => scrollTo({ top: 0, behavior: 'instant' }));
    await redraw(at === 720 ? 320 : 720);
    item.resize = { ...await focused(), scroll: await page.evaluate(() => scrollY), legend: await entries.count() };
    if (item.resize.legend) expect(`${mode}/${width}: focus follows the series while the legend stays`, item.resize.entry, key);
    else {
      // If the legend folds into direct labels, focus returns to chart inspection.
      assert.ok(item.resize.frame || item.resize.entry === key, `${mode}/${width}: focus stays in the chart after the legend changes ${JSON.stringify(item.resize)}`);
      await page.locator('figure[data-rv-chart] .rv-chart__frame').focus();
      await page.keyboard.press('Home');
      await page.locator('figure[data-rv-chart] .rv-chart__readout:not([hidden])').waitFor();
      if (mode === 'crowded-bar') assert.ok((await page.locator('figure[data-rv-chart] .rv-chart__readout').innerText()).includes('Long series name 1'));
      await page.keyboard.press('Escape');
    }
    assert.equal(item.resize.scroll, 0, 'Restoring a chart control must not scroll the document');
    await redraw(width);

    // A pending redraw leaves external focus alone.
    await page.evaluate(() => {
      const outside = document.createElement('input');
      outside.id = 'outside-chart'; outside.setAttribute('aria-label', 'Outside chart');
      document.body.prepend(outside);
    });
    // Any series control will do: a legend entry, or a line-end name where the legend folds.
    await page.locator('figure[data-rv-chart] :is(button.rv-chart__legend-entry, button.rv-chart__name):visible').first().focus();
    await page.locator('#outside-chart').focus();
    await redraw(width);
    item.externalBeforeFrame = (await focused()).id;
    assert.equal(item.externalBeforeFrame, 'outside-chart', 'A redraw leaves external focus alone');
  }

  // A keyboard selection stays while the page scrolls under a resting pointer: WebKit then sends a
  // move to the same place, which is not the reader moving the pointer (S27-860). A pointer that
  // really moves takes the selection over as before (FEA-CH-39).
  {
    await page.setViewportSize({ width: 1100, height: 900 });
    await page.goto(`${reportServer.base}/category-bar/report.html`);
    await settledCharts(page, 1);
    await page.evaluate(() => {
      document.documentElement.style.scrollBehavior = 'auto';
      document.body.style.overflowAnchor = 'none';
      const room = document.createElement('div');
      room.style.height = '1600px';
      document.body.prepend(room);
    });
    const frame = page.locator('figure[data-rv-chart] .rv-chart__frame');
    // Where the last bar sits once focus has scrolled the chart into view.
    await frame.focus();
    await page.evaluate(() => window.__richview.settled());
    const model = await page.evaluate(() => window.__richview.charts()[0]);
    const box = await page.locator('figure[data-rv-chart] .rv-chart__canvas').boundingBox();
    const observations = model.geometry.panels[0].observations;
    const last = observations.length - 1, mark = observations[last];
    const at = mark.area ? [box.x + mark.area.x + mark.area.w / 2, box.y + mark.area.y + mark.area.h / 2] : [box.x + mark.at[0], box.y + mark.at[1]];
    // From the top with focus outside, the pointer rests where that bar will come to lie.
    await page.evaluate(() => { document.activeElement?.blur(); scrollTo({ top: 0, behavior: 'instant' }); });
    await page.mouse.move(...at);
    await frame.focus();
    await page.keyboard.press('Home');
    // WebKit sends its move after the scroll ends; give it time to arrive.
    await page.waitForTimeout(400);
    await page.evaluate(() => window.__richview.settled());
    const resting = { mode: 'resting-pointer', last, kept: await page.evaluate(() => window.__richview.charts()[0].target) };
    cases.push(resting);
    expect('a keyboard selection survives the page scrolling under a resting pointer', JSON.stringify(resting.kept && [resting.kept.panel, resting.kept.observation]), JSON.stringify([0, 0]));
    await page.mouse.move(at[0] + 2, at[1] + 2);
    await page.evaluate(() => window.__richview.settled());
    resting.moved = await page.evaluate(() => window.__richview.charts()[0].target);
    expect('a pointer that moves takes the selection over', JSON.stringify(resting.moved && [resting.moved.panel, resting.moved.observation]), JSON.stringify([0, last]));
    await page.keyboard.press('Escape');
  }

  // Keyboard focus shows where it is (STY-38): the frame's ring on arrival, 3 px clear of the
  // text on the canvas and on the page, or midway where the two leave less room; an arrow key
  // moves it onto the selected mark, 2 px past the mark's contour; a pointer never draws that
  // ring; Escape brings the frame's ring back.
  for (const width of [720, 320]) for (const mode of ['numeric-line', 'category-bar', 'numeric-scatter']) {
    await page.setViewportSize({ width, height: 900 });
    await page.goto(`${reportServer.base}/${mode}/report.html`);
    await settledCharts(page, 1);
    const frame = page.locator('figure[data-rv-chart] .rv-chart__frame');
    await frame.scrollIntoViewIfNeeded();
    await frame.focus();
    await page.evaluate(() => window.__richview.settled());
    const ring = { mode: `focus-ring/${mode}`, width, arrival: await page.evaluate(FRAME_RING) };
    cases.push(ring);
    expect(`${mode}/${width}: Tab arrival shows the frame's ring`, ring.arrival.shown, true);
    expect(`${mode}/${width}: the frame's ring keeps 3 px from text ${JSON.stringify(ring.arrival.crowding)}`, ring.arrival.crowding.length, 0);
    // An inner mark: the feedback is clipped 8 px past the plot, a ring at its edge with it.
    await page.keyboard.press('Home');
    await page.keyboard.press('ArrowRight');
    await page.evaluate(() => window.__richview.settled());
    ring.selected = { frame: await page.evaluate(FRAME_RING), mark: await page.evaluate(MARK_RING) };
    expect(`${mode}/${width}: an arrow key's selection hides the frame's ring`, ring.selected.frame.shown, false);
    expect(`${mode}/${width}: the selected mark gets the ring ${JSON.stringify(ring.selected.mark)}`, ring.selected.mark.ringed, true);
    // The pointer takes over on the same mark: the contour stays, the keyboard's ring goes.
    await page.mouse.move(ring.selected.mark.at[0] + 3, ring.selected.mark.at[1] + 3);
    await page.mouse.move(ring.selected.mark.at[0], ring.selected.mark.at[1], { steps: 3 });
    await page.evaluate(() => window.__richview.settled());
    ring.pointer = { frame: await page.evaluate(FRAME_RING), mark: await page.evaluate(MARK_RING) };
    expect(`${mode}/${width}: pointer hover never shows the mark ring ${JSON.stringify(ring.pointer.mark)}`, ring.pointer.mark.share <= 0.25, true);
    await page.keyboard.press('Home');
    await page.keyboard.press('Escape');
    await page.evaluate(() => window.__richview.settled());
    ring.escaped = await page.evaluate(FRAME_RING);
    expect(`${mode}/${width}: Escape brings the frame's ring back`, ring.escaped.shown && !ring.escaped.selected, true);
    await page.mouse.move(1, 1);
  }

  // A redraw in an embedded preview must not take focus from the parent editor. The child
  // document's activeElement alone cannot establish whether it still owns focus.
  await page.setViewportSize({ width: 1100, height: 900 });
  await page.goto(`${h.base}/playground`);
  assert.equal(await page.locator('#preview').getAttribute('sandbox'), 'allow-scripts', 'the preview keeps its opaque script-only sandbox');
  const previewElement = await page.locator('#preview').elementHandle();
  const frame = await previewElement.contentFrame();
  await previewElement.dispose();
  assert.ok(frame, 'The preview iframe has a browsing context');
  await settledCharts(frame, 1);
  await inputValue(page.locator('#source'), reportSource(chartFor('category-bar')));
  await page.waitForFunction(() => document.querySelector('#preview').getAttribute('srcdoc').includes('Series keyboard control'));
  await settledCharts(frame, 1);
  const embedded = { mode: 'embedded-preview' }; cases.push(embedded);
  const childRedraw = async () => {
    await page.locator('#preview').evaluate(node => { const w = node.offsetWidth; node.style.width = `${w > 600 ? 480 : w + 160}px`; });
    await frame.locator('body').evaluate(() => new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done))));
    await frame.locator('body').evaluate(() => window.__richview.settled());
  };
  await frame.locator('button.rv-chart__legend-entry:visible').first().focus();
  assert.equal(await frame.locator('body').evaluate(() => document.hasFocus()), true);
  await page.locator('#source').focus();
  await childRedraw();
  embedded.externalBeforeFrame = await page.evaluate(() => document.activeElement?.id);
  assert.equal(embedded.externalBeforeFrame, 'source', 'An inactive preview leaves focus in the parent editor');
  assert.equal(await frame.locator('body').evaluate(() => document.hasFocus()), false);
  assert.deepEqual(errors, []);
  assert.deepEqual(failures, []);
  status = 'passed';
  console.log(`PASS ${cases.length} chart keyboard cases through hide/restore, responsive redraw, external focus and parent-editor focus`);
} catch (error) { status = 'failed'; failure = String(error); throw error; }
finally { h.save('chart-keyboard.json', { browser: h.browser.version(), status, failure, cases, errors }); await reportServer?.close(); await h.close(); }
