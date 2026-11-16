// Interaction feedback, zoom, the overview strip and entrances in the WebGL report runtime
// (FEA-CH-42, FEA-CH-49 to FEA-CH-53). Reads drawn pixels and the geometry model frame by
// frame: emphasis fades in about 150 ms and out in about 250 ms, the hover dot grows and a
// scatter contour fades in about 120 ms, a hovered pie slice keeps its place and is outlined, drag-zoom and reset glide
// for about 600 ms with the plot's left edge fixed and no readout, wheel zoom lands at once, the
// readout holds about 250 ms after the pointer leaves, the overview strip follows the zoom and
// drives it, and touch pinches and pans only in the expanded view. Reduced motion lands every
// change at once. RICHVIEW_VERIFY_BROWSER picks the engine.
import assert from 'node:assert/strict';
import { harness, installWasmClock, watchErrors } from './browser-harness.mjs';
import { reports } from './report-runtime.mjs';

const fence = value => `\n\`\`\`chart\n${JSON.stringify(value)}\n\`\`\`\n`;
const months = Array.from({ length: 24 }, (_, i) => `${2023 + Math.floor(i / 12)}-${String(i % 12 + 1).padStart(2, '0')}-01`);
const lines = months.flatMap((month, i) => ['Exports', 'Imports'].map((flow, k) => ({ month, flow, value: 40 + i * 2 + k * 25 + (i % 5) * 3 })));
const points = Array.from({ length: 30 }, (_, i) => ({ x: i, y: (i * 37) % 23, weight: i + 1, group: i % 2 ? 'Odd' : 'Even' }));
const bars = ['North', 'South', 'East'].flatMap((region, r) => ['2024', '2025'].map((year, y) => ({ region, year, sales: 30 + r * 12 + y * 20 })));
const FIXTURE = '---\ntitle: Feedback\nversion: "1"\n---\n\nInteraction feedback.\n'
  + fence({ type: 'line', title: 'Trade by month', encoding: { x: 'month', y: 'value', color: 'flow' }, data: { inline: lines } })
  + fence({ type: 'bar', title: 'Sales by region', encoding: { x: 'region', y: 'sales', color: 'year' }, data: { inline: bars } })
  + fence({ type: 'scatter', title: 'Points', encoding: { x: 'x', y: 'y', size: 'weight', color: 'group' }, data: { inline: points } })
  + fence({ type: 'pie', title: 'Shares', encoding: { x: 'region', y: 'sales' }, data: { inline: bars.filter(b => b.year === '2024') } })
  + fence({ type: 'rose', title: 'Rain', encoding: { x: 'month', y: 'mm' }, data: { inline: Array.from({ length: 12 }, (_, i) => ({ month: `M${String(i + 1).padStart(2, '0')}`, mm: 30 + i * 8 })) } });
const DECAL_FIXTURE = '---\ntitle: Marker decals\nversion: "1"\nstyles: [scientific, decals.rstyle.yaml]\n---\n'
  + fence({ type: 'scatter', title: 'Small marker shapes', encoding: { x: 'x', y: 'y', size: 'weight', color: 'group' }, data: { inline: Array.from({ length: 6 }, (_, i) => ({ x: i * 10, y: i * 2 + 1, weight: i + 1, group: `Group ${i}` })) } });
const LINE = 0, BAR = 1, SCATTER = 2, PIE = 3, ROSE = 4;
const FRAME_MS = 1000 / 60;

const h = await harness('chart-feedback');
const inputs = await reports(h.dir, { only: ['calendar', 'combo', 'dumbbell', 'gauge', 'lollipop', 'sankey', 'waffle', 'streamgraph'], extra: { feedback: FIXTURE, decals: DECAL_FIXTURE }, files: { decals: { 'decals.rstyle.yaml': 'chart:\n  decals: true\n' } } });
const errors = [];
const failures = [];
const evidence = {};
const check = async (label, fn) => {
  try {
    evidence[label] = await fn();
  } catch (error) {
    failures.push(`${label}: ${error.message}`);
  }
};

async function open(width, { reduce = false } = {}) {
  const context = await h.browser.newContext({ viewport: { width, height: 1000 }, reducedMotion: reduce ? 'reduce' : 'no-preference', hasTouch: false });
  const page = await context.newPage();
  watchErrors(page, errors, `${width}${reduce ? ' reduce' : ''}`);
  await page.goto(`${inputs.base}/feedback/report.html`);
  await page.waitForFunction(() => window.__richview);
  await page.evaluate(() => window.__richview.settled());
  await installWasmClock(page);
  return { context, page };
}

const settled = page => page.evaluate(() => window.__richview.settled());
const model = (page, k) => page.evaluate(async k => { await window.__richview.settled(); return window.__richview.charts()[k]; }, k);
const canvasBox = async (page, k) => {
  const canvas = page.locator('figure[data-rv-chart]').nth(k).locator('canvas.rv-chart__canvas');
  await canvas.scrollIntoViewIfNeeded();
  await page.evaluate(() => window.__richview.settled());
  return canvas.boundingBox();
};

/**
 * Run `act` in the page, then record `probe` on every animation frame for `ms`; returns
 * `[{t, value}]` with `t` in milliseconds since the act. Both are function bodies over `k`.
 */
const frames = (page, k, act, probe, ms) => page.evaluate(async ({ k, act, probe, ms }) => {
  const figure = document.querySelectorAll('figure[data-rv-chart]')[k];
  const canvas = figure.querySelector('canvas.rv-chart__canvas');
  const run = new Function('figure', 'canvas', 'k', act);
  const read = new Function('figure', 'canvas', 'k', probe);
  const out = [];
  const start = performance.now();
  run(figure, canvas, k);
  while (performance.now() - start < ms) {
    await new Promise(accept => requestAnimationFrame(accept));
    out.push({ t: performance.now() - start, value: read(figure, canvas, k) });
  }
  return out;
}, { k, act, probe, ms });

/** A baseline is one real frame, independent of how long setting up its probe took. */
const sampleFrame = (page, k, probe) => page.evaluate(async ({ k, probe }) => {
  const figure = document.querySelectorAll('figure[data-rv-chart]')[k];
  const canvas = figure.querySelector('canvas.rv-chart__canvas');
  const read = new Function('figure', 'canvas', 'k', probe);
  await new Promise(accept => requestAnimationFrame(accept));
  return read(figure, canvas, k);
}, { k, probe });

/**
 * A virtual clock for motion that must be measured exactly: `performance.now` and
 * `requestAnimationFrame` are replaced by a clock the test steps, so an animation's first frame
 * is the one its start time gives, however late the machine is. `thaw` hands pending frames back
 * to the real loop.
 */
const freeze = page => page.evaluate(() => {
  const clock = { vt: performance.now(), queue: [], raf: window.requestAnimationFrame.bind(window), now: performance.now.bind(performance) };
  window.__clock = clock;
  window.requestAnimationFrame = callback => { clock.queue.push(callback); return clock.queue.length; };
  performance.now = () => clock.vt;
});
const thaw = page => page.evaluate(() => {
  const clock = window.__clock;
  window.requestAnimationFrame = clock.raf;
  performance.now = clock.now;
  clock.queue.splice(0).forEach(callback => clock.raf(callback));
  delete window.__clock;
});
/** Step the frozen clock a frame at a time for `ms`, reading `probe` after each frame is drawn. */
const step = (page, k, probe, ms) => page.evaluate(({ k, probe, ms }) => {
  const clock = window.__clock;
  const figure = document.querySelectorAll('figure[data-rv-chart]')[k];
  const canvas = figure.querySelector('canvas.rv-chart__canvas');
  const read = new Function('figure', 'canvas', 'k', probe);
  const out = [];
  const start = clock.vt;
  while (clock.vt - start < ms) {
    clock.vt += 1000 / 60;
    for (const callback of clock.queue.splice(0)) callback(clock.vt);
    out.push({ t: clock.vt - start, value: read(figure, canvas, k) });
  }
  return out;
}, { k, probe, ms });

/** The drawn pixel at CSS point `p` of chart `k`'s canvas, as `[r, g, b]`. */
const PIXEL = `
  const ctx = canvas.getContext('2d');
  const scale = canvas.width / canvas.clientWidth;
  const at = window.__probe;
  const d = ctx.getImageData(Math.round(at[0] * scale), Math.round(at[1] * scale), 1, 1).data;
  return [d[0], d[1], d[2]];`;
/** How many pixels in a box around `window.__probe` differ from `window.__before`. */
const CHANGED = `
  const ctx = canvas.getContext('2d');
  const scale = canvas.width / canvas.clientWidth;
  const [x, y, r] = window.__probe;
  const box = ctx.getImageData(Math.round((x - r) * scale), Math.round((y - r) * scale), Math.round(2 * r * scale), Math.round(2 * r * scale)).data;
  let n = 0;
  for (let i = 0; i < box.length; i += 4) if (Math.abs(box[i] - window.__before[i]) + Math.abs(box[i + 1] - window.__before[i + 1]) + Math.abs(box[i + 2] - window.__before[i + 2]) > 30) n++;
  return n;`;
const BEFORE = `
  const ctx = canvas.getContext('2d');
  const scale = canvas.width / canvas.clientWidth;
  const [x, y, r] = window.__probe;
  window.__before = ctx.getImageData(Math.round((x - r) * scale), Math.round((y - r) * scale), Math.round(2 * r * scale), Math.round(2 * r * scale)).data;`;
const distance = (a, b) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);
const domainOf = m => m.geometry.panels[0].x.domain;

// Re-entering before the readout hold ends must resume feedback without trapping the runtime.
for (const reduce of [false, true]) {
  const { context, page } = await open(480, { reduce });
  await check(`${reduce ? 'reduced motion' : 'motion'}: return during the readout hold`, async () => {
    const beforeErrors = errors.length;
    const m = await model(page, LINE);
    const o = m.geometry.panels[0].observations.find(o => o.dots?.length);
    const box = await canvasBox(page, LINE);
    const canvas = page.locator('figure[data-rv-chart]').nth(LINE).locator('canvas.rv-chart__canvas');
    const at = { clientX: box.x + o.x, clientY: box.y + o.dots[0].at[1], pointerType: 'mouse' };
    await canvas.dispatchEvent('pointermove', at);
    await canvas.dispatchEvent('pointerleave', at);
    await canvas.dispatchEvent('pointermove', at);
    assert.equal(errors.length, beforeErrors, 'returning during the hold produces no runtime error');
    assert.ok((await model(page, LINE)).target, 'the chart still selects an observation');
    await page.locator('figure[data-rv-chart]').nth(LINE).getByRole('button', { name: 'Expand', exact: true }).click();
    await settled(page);
    assert.equal(errors.length, beforeErrors, 'the expanded chart settles without an error');
  });
  await context.close();
}

for (const reduce of [false, true]) {
  const { context, page } = await open(1440, { reduce });
  const tag = reduce ? 'reduced motion' : 'motion';
  await page.mouse.move(5, 5);

  await check(`${tag}: legend name turns from soft ink to ink on hover`, async () => {
    const entry = page.locator('figure[data-rv-chart]').nth(BAR).locator('button.rv-chart__legend-entry').first();
    const name = entry.locator('.rv-chart__legend-name');
    await entry.scrollIntoViewIfNeeded();
    const expected = await name.evaluate(node => {
      const probe = document.createElement('span'); node.appendChild(probe);
      probe.style.color = 'var(--rv-ink-soft)'; const soft = getComputedStyle(probe).color;
      probe.style.color = 'var(--rv-ink)'; const ink = getComputedStyle(probe).color;
      probe.remove(); return { soft, ink };
    });
    assert.notEqual(expected.soft, expected.ink, 'the style distinguishes the two inks');
    assert.equal(await name.evaluate(node => getComputedStyle(node).color), expected.soft);
    await entry.hover();
    assert.equal(await name.evaluate(node => getComputedStyle(node).color), expected.ink);
    await page.mouse.move(5, 5);
    return expected;
  });

  await check(`${tag}: emphasis dims the other series and restores them`, async () => {
    const m = await model(page, BAR);
    const bar = m.geometry.panels[0].items.find(i => i.kind === 'rect' && i.role === 'mark' && i.series === 1);
    // Automatic values can cover the centre; measure the fade on clear fill.
    const at = [bar.rect.x + bar.rect.w / 4, bar.rect.y + bar.rect.h / 4];
    await page.evaluate(at => { window.__probe = at; }, at);
    const full = await sampleFrame(page, BAR, PIXEL);
    const fill = bar.fill.color.slice(0, 3).map(v => Math.round(v * 255));
    assert.ok(distance(full, fill) < 3, `the emphasis probe reads clear bar fill: ${full} vs ${fill}`);
    const background = m.geometry.background.slice(0, 3).map(v => Math.round(v * 255));
    // Measured on a stepped clock: a late frame under load cannot hide the fade.
    await freeze(page);
    let enter, leave;
    try {
      await page.evaluate(i => document.querySelectorAll('figure[data-rv-chart]')[i].querySelector('.rv-chart__legend-entry').dispatchEvent(new PointerEvent('pointerenter')), BAR);
      enter = await step(page, BAR, PIXEL, 450);
      await page.evaluate(i => document.querySelectorAll('figure[data-rv-chart]')[i].querySelector('.rv-chart__legend-entry').dispatchEvent(new PointerEvent('pointerleave')), BAR);
      leave = await step(page, BAR, PIXEL, 500);
    } finally {
      // A throw while frozen would leave the clock stopped for every later check.
      await thaw(page);
    }
    const strength = c => distance(c, background) / distance(full, background);
    const dimmed = strength(enter.at(-1).value);
    assert.ok(Math.abs(dimmed - 0.32) < 0.06, `the other series draws at 32% on the dark page: ${dimmed.toFixed(2)}`);
    assert.ok(strength(leave.at(-1).value) > 0.97, 'leaving restores the series');
    const between = list => list.filter(f => { const s = strength(f.value); return s > 0.36 && s < 0.94; });
    // When the colour stops changing: the last frame that still differs from the final one.
    const arriveFrames = list => {
      const last = list.at(-1).value;
      const moving = list.filter(f => distance(f.value, last) > 1.5);
      return moving.length ? list.indexOf(moving.at(-1)) + 2 : 1;
    };
    const inFrames = arriveFrames(enter), outFrames = arriveFrames(leave);
    const timing = { in: inFrames * FRAME_MS, out: outFrames * FRAME_MS, in_frames: inFrames, out_frames: outFrames, between_in: between(enter).length, between_out: between(leave).length };
    if (reduce) {
      assert.equal(timing.between_in + timing.between_out, 0, `reduced motion has no in-between frames: ${JSON.stringify(timing)}`);
    } else {
      assert.ok(timing.between_in >= 2 && timing.between_out >= 2, `emphasis fades in and out: ${JSON.stringify(timing)}`);
      assert.ok(inFrames >= 7 && inFrames <= 15, `dims in about 150 ms: ${inFrames} frames`);
      assert.ok(outFrames >= 12 && outFrames <= 22, `restores in about 250 ms: ${outFrames} frames`);
      assert.ok(outFrames > inFrames, 'restoring is slower than dimming');
    }
    return timing;
  });

  await check(`${tag}: the hover dot grows in`, async () => {
    const m = await model(page, LINE);
    const o = m.geometry.panels[0].observations[10];
    const box = await canvasBox(page, LINE);
    // The probe covers the dot, its ring and its halo, which grow in together.
    await page.evaluate(([x, y]) => { window.__probe = [x, y, 13]; }, o.dots[0].at);
    await sampleFrame(page, LINE, BEFORE);
    await freeze(page);
    let grow;
    try {
      await page.mouse.move(box.x + o.x, box.y + o.dots[0].at[1]);
      grow = await step(page, LINE, CHANGED, 260);
    } finally {
      await thaw(page);
    }
    await page.mouse.move(5, 5);
    await settled(page);
    const first = grow[0].value, last = grow.at(-1).value;
    if (reduce) assert.ok(first >= last * 0.95, `reduced motion shows the full dot at once: ${first} of ${last}`);
    else {
      assert.ok(first < last * 0.97, `the dot starts small: ${first} of ${last}`);
      const full = grow.find(f => f.value >= last * 0.97)?.t;
      assert.ok(full > 50 && full < 220, `and is full in about 120 ms: ${full}`);
    }
    return { first, last };
  });

  await check(`${tag}: a scatter contour fades in without repainting its value-sized fill`, async () => {
    await page.mouse.move(5, 5);
    await settled(page);
    const m = await model(page, SCATTER);
    const o = m.geometry.panels[0].observations[7];
    const box = await canvasBox(page, SCATTER);
    await page.evaluate(([x, y]) => { window.__probe = [x, y, 14]; }, o.at);
    const original = await sampleFrame(page, SCATTER, PIXEL);
    await sampleFrame(page, SCATTER, BEFORE);
    const before = await page.evaluate(() => [...window.__before]);
    const colors = `
      const ctx = canvas.getContext('2d'), scale = canvas.width / canvas.clientWidth;
      const [x, y, r] = window.__probe;
      return [...ctx.getImageData(Math.round((x - r) * scale), Math.round((y - r) * scale), Math.round(2 * r * scale), Math.round(2 * r * scale)).data];`;
    // The browser can defer pointer delivery under load. Step only after the chart has received
    // the actual input, so the frozen clock cannot run ahead of the hover handler.
    await page.evaluate(k => {
      const canvas = document.querySelectorAll('figure[data-rv-chart]')[k].querySelector('canvas.rv-chart__canvas');
      window.__scatterPointerDelivered = new Promise(resolve => canvas.addEventListener('pointermove', resolve, { once: true }));
    }, SCATTER);
    await freeze(page);
    let fade;
    try {
      await page.mouse.move(box.x + o.at[0], box.y + o.at[1]);
      await page.evaluate(() => window.__scatterPointerDelivered);
      fade = await step(page, SCATTER, colors, 260);
    } finally {
      await thaw(page);
    }
    // Read the final pixels only after the runtime reports that its pending draws are done.
    await settled(page);
    const settledColors = await sampleFrame(page, SCATTER, colors);
    const core = await sampleFrame(page, SCATTER, PIXEL);
    await page.mouse.move(5, 5);
    await settled(page);
    assert.deepEqual(core, original, 'the original bubble fill is not repainted or enlarged');
    const lastColors = fade.at(-1).value;
    const delta = colors => colors.reduce((sum, v, i) => sum + Math.abs(v - before[i]), 0);
    const first = delta(fade[0].value), last = delta(lastColors);
    const full = fade.find(f => f.value.every((v, i) => v === lastColors[i]))?.t;
    evidence[`${tag}: scatter frame diagnostics`] = { first, last, settled: delta(settledColors), full, original, core };
    assert.deepEqual(lastColors, settledColors, 'the final animation frame agrees with the settled render');
    assert.ok(last > 0, 'the selected edge receives feedback');
    if (reduce) assert.deepEqual(fade[0].value, lastColors, 'reduced motion shows the full contour at once');
    else {
      assert.ok(first < last, `the contour fades in: ${first} → ${last}`);
      assert.ok(full > 50 && full < 220, `the contour is full in about 120 ms: ${full}`);
    }
    return { first, last, full, original, core };
  });

  await check(`${tag}: the smallest scatter point keeps its fill colour`, async () => {
    const m = await model(page, SCATTER), panel = m.geometry.panels[0];
    const point = panel.items.filter(i => i.kind === 'point' && i.role === 'mark' && i.obs != null).sort((a, b) => a.radius - b.radius)[0];
    assert.ok(point, 'the fixture has an observation-owned point');
    const o = panel.observations[point.obs], box = await canvasBox(page, SCATTER);
    // Read the fill half a radius up and left of the centre: inside the point at rest and on
    // hover. The fixture's smallest point lies on the origin, where the axis line crosses its
    // centre at rest, and a hovered point comes in front of that line (FEA-CH-53), so the
    // centre pixel itself turns from line to fill. Any repaint, recolour or cover of the
    // hovered point's fill still changes this pixel.
    await page.evaluate(at => { window.__probe = at; }, [point.center[0] - point.radius / 2, point.center[1] - point.radius / 2]);
    const before = await sampleFrame(page, SCATTER, PIXEL);
    await page.mouse.move(box.x + o.at[0], box.y + o.at[1]);
    await settled(page);
    const after = await sampleFrame(page, SCATTER, PIXEL);
    await page.mouse.move(5, 5);
    await settled(page);
    assert.deepEqual(after, before, `point radius ${point.radius} keeps its encoded fill`);
    return { radius: point.radius, before, after };
  });

  await check(`${tag}: a pie contour keeps the hovered slice in place`, async () => {
    const m = await model(page, PIE);
    const o = m.geometry.panels[0].observations[1];
    const { center, radius, start, end } = o.wedge;
    const mid = (start + end) / 2 - Math.PI / 2;
    const outside = [center[0] + (radius + 3.5) * Math.cos(mid), center[1] + (radius + 3.5) * Math.sin(mid)];
    const slice = m.geometry.series[o.series].color.slice(0, 3).map(v => Math.round(v * 255));
    await page.evaluate(at => { window.__probe = at; }, outside);
    const before = await sampleFrame(page, PIE, PIXEL);
    const edge = [center[0] + radius * Math.cos(mid), center[1] + radius * Math.sin(mid), 6];
    await page.evaluate(at => { window.__probe = at; }, edge);
    await sampleFrame(page, PIE, BEFORE);
    const box = await canvasBox(page, PIE);
    await page.mouse.move(box.x + o.at[0], box.y + o.at[1]);
    await settled(page);
    const changed = await sampleFrame(page, PIE, CHANGED);
    await page.evaluate(at => { window.__probe = at; }, outside);
    const after = await sampleFrame(page, PIE, PIXEL);
    await page.mouse.move(5, 5);
    await settled(page);
    assert.ok(distance(before, slice) > 60, 'outside the pie before hover');
    assert.ok(distance(after, slice) > 60, `the original slice fill must not move beyond the rim: ${after} vs ${slice}`);
    assert.ok(changed > 0, `the selected slice receives an outline: ${changed} changed pixels`);
    return { before, after, slice, changed };
  });

  await check(`${tag}: rose highlights the selected wedge without moving the first`, async () => {
    const m = await model(page, ROSE);
    const panel = m.geometry.panels[0], selected = panel.observations[6], first = panel.observations[0];
    const box = await canvasBox(page, ROSE);
    const pixels = () => page.evaluate(k => {
      const c = document.querySelectorAll('figure[data-rv-chart]')[k].querySelector('canvas');
      return { width: c.width, height: c.height, scale: c.width / c.clientWidth, data: [...c.getContext('2d').getImageData(0, 0, c.width, c.height).data] };
    }, ROSE);
    const before = await pixels();
    await page.mouse.move(box.x + selected.at[0], box.y + selected.at[1]);
    await settled(page);
    const after = await pixels();
    await page.mouse.move(5, 5);
    await settled(page);
    // Exclude the shared centre and compare only the angular sector of each wedge, between
    // `inner` and `outer` pixels from its rim.
    const changed = (o, inner = -Infinity, outer = 10) => {
      const w = o.wedge, mid = (w.start + w.end) / 2, half = (w.end - w.start) / 2;
      let n = 0;
      for (let y = 0; y < before.height; y++) for (let x = 0; x < before.width; x++) {
        const dx = x / before.scale - w.center[0], dy = y / before.scale - w.center[1];
        const r = Math.hypot(dx, dy), angle = (Math.atan2(dy, dx) + Math.PI / 2 + 2 * Math.PI) % (2 * Math.PI);
        const delta = Math.abs(Math.atan2(Math.sin(angle - mid), Math.cos(angle - mid)));
        if (r < 10 || r < w.radius + inner || r > w.radius + outer || delta > half + 0.02) continue;
        const i = (y * before.width + x) * 4;
        if (Math.abs(before.data[i] - after.data[i]) + Math.abs(before.data[i + 1] - after.data[i + 1]) + Math.abs(before.data[i + 2] - after.data[i + 2]) > 15) n++;
      }
      return n;
    };
    // The first wedge steps back with the others, in place: nothing is drawn past its rim.
    const untouched = changed(first, 2, 10), outlined = changed(selected);
    assert.equal(untouched, 0, `the first wedge must not move or be outlined: ${untouched} changed pixels past its rim`);
    assert.ok(outlined > 10, `the selected wedge receives a contour: ${outlined} changed pixels`);
    return { untouched, outlined };
  });

  await check(`${tag}: drag-zoom and reset glide; wheel zoom lands at once`, async () => {
    const m = await model(page, LINE);
    const plot = m.geometry.panels[0].plot;
    const box = await canvasBox(page, LINE);
    const probe = `const m = window.__richview.charts()[k]; const p = m.geometry.panels[0];
      return { domain: p.x.domain, left: p.plot.x, readout: !figure.querySelector('.rv-chart__readout').hidden };`;
    const startDomain = domainOf(m);
    const y = box.y + plot.y + plot.h / 2;
    // A point inside the window to be dragged, just under the plot's top, where no line runs.
    await page.evaluate(p => { window.__probe = p; }, [plot.x + plot.w * 0.45, plot.y + 3]);
    const empty = await sampleFrame(page, LINE, PIXEL);
    await page.mouse.move(box.x + plot.x + plot.w * 0.3, y);
    await page.mouse.down();
    await page.mouse.move(box.x + plot.x + plot.w * 0.6, y, { steps: 4 });
    // On a line chart the crosshair always has a target; the band shows anyway, and the
    // crosshair and readout let go while the drag lasts (FEA-CH-49).
    const banded = await sampleFrame(page, LINE, PIXEL);
    const during = await page.evaluate(k => window.__richview.charts()[k].target, LINE);
    assert.ok(distance(empty, banded) > 8, `the dragged window is tinted: ${empty} → ${banded}`);
    assert.equal(during, null, 'the crosshair lets go while dragging');
    // The mouse up happens inside the page so the glide is timed from it.
    const up = await frames(page, LINE, `canvas.dispatchEvent(new PointerEvent('pointerup', { clientX: ${box.x + plot.x + plot.w * 0.6}, clientY: ${y}, pointerType: 'mouse', button: 0, bubbles: true }))`, probe, 900);
    await page.mouse.up();
    await settled(page);
    const zoomed = await model(page, LINE);
    assert.ok(zoomed.zoom, 'the drag zoomed');
    const final = zoomed.zoom;
    const inBetween = up.filter(f => f.value.domain[0] > startDomain[0] + 1 && Math.abs(f.value.domain[0] - final[0]) > (final[1] - final[0]) * 0.01);
    const lefts = up.map(f => f.value.left);
    assert.ok(Math.max(...lefts) - Math.min(...lefts) < 0.5, `the plot's left edge stays put: ${Math.min(...lefts)}–${Math.max(...lefts)}`);
    const arrived = up.find(f => Math.abs(f.value.domain[0] - final[0]) <= (final[1] - final[0]) * 0.002)?.t;
    if (reduce) assert.equal(inBetween.length, 0, 'reduced motion zooms at once');
    else {
      // Under a loaded host frames drop; three in-between frames still show a glide.
      assert.ok(inBetween.length >= 3, `the zoom glides: ${inBetween.length} frames between`);
      assert.ok(arrived > 450 && arrived < 850, `in about 600 ms: ${arrived}`);
      assert.ok(inBetween.every(f => !f.value.readout), 'no readout during the glide');
    }
    // Reset glides back to the whole series.
    const reset = await frames(page, LINE, "figure.querySelector('.rv-chart__action--reset').click()", probe, 900);
    const whole = (await model(page, LINE));
    assert.equal(whole.zoom, null, 'reset clears the zoom');
    const resetBetween = reset.filter(f => f.value.domain[0] < final[0] - 1 && f.value.domain[0] > domainOf(whole)[0] + 1);
    if (!reduce) assert.ok(resetBetween.length >= 3, `the reset glides: ${resetBetween.length}`);
    // Ctrl + wheel zooms around the pointer at once.
    await page.mouse.move(box.x + plot.x + plot.w / 2, y);
    const wheel = await frames(page, LINE, `canvas.dispatchEvent(new WheelEvent('wheel', { deltaY: -100, ctrlKey: true, clientX: ${box.x + plot.x + plot.w / 2}, clientY: ${y}, bubbles: true, cancelable: true }))`, probe, 300);
    const changed = [...new Set(wheel.map(f => f.value.domain.join()))];
    assert.equal(changed.length, 1, `wheel zoom applies at once: ${changed.length} distinct windows`);
    assert.notEqual(changed[0], domainOf(whole).join(), 'and zooms');
    await page.evaluate(() => document.querySelector('figure[data-rv-chart] .rv-chart__action--reset').click());
    await settled(page);
    return { arrived, between: inBetween.length, reset: resetBetween.length };
  });

  await check(`${tag}: the readout holds about 250 ms after the pointer leaves, then fades in 100 ms`, async () => {
    const m = await model(page, LINE);
    const o = m.geometry.panels[0].observations[5];
    const box = await canvasBox(page, LINE);
    await page.mouse.move(box.x + o.x, box.y + o.at[1]);
    await page.waitForFunction(() => !document.querySelector('figure[data-rv-chart] .rv-chart__readout').hidden);
    const before = Math.floor(250 - FRAME_MS), after = Math.ceil(250 + FRAME_MS);
    await page.clock.pauseAt(await page.evaluate(() => Date.now() + 100));
    try {
      await page.locator('figure[data-rv-chart]').nth(LINE).locator('canvas.rv-chart__canvas').dispatchEvent('pointerleave', { pointerType: 'mouse' });
      await page.clock.runFor(before);
      assert.equal(await page.locator('figure[data-rv-chart]').nth(LINE).locator('.rv-chart__readout').evaluate(n => n.hidden), false, `still shown after ${before} ms`);
      await page.clock.runFor(after - before);
      const readout = page.locator('figure[data-rv-chart]').nth(LINE).locator('.rv-chart__readout');
      if (reduce) {
        assert.equal(await readout.evaluate(n => n.hidden), true, `gone by ${after} ms`);
      } else {
        assert.deepEqual(await readout.evaluate(n => [n.hidden, n.classList.contains('rv-chart__readout--leaving')]), [false, true], `fading at ${after} ms`);
        await page.clock.runFor(100 + Math.ceil(FRAME_MS));
        assert.deepEqual(await readout.evaluate(n => [n.hidden, n.classList.contains('rv-chart__readout--leaving')]), [true, false], `gone ${100 + Math.ceil(FRAME_MS)} ms later`);
      }
    } finally {
      await page.clock.resume();
    }
    await page.mouse.move(5, 5);
    const transition = await page.evaluate(() => getComputedStyle(document.querySelector('.rv-chart__readout')).transitionDuration);
    assert.equal(transition, reduce ? '0s' : '0.18s, 0.1s', 'the readout glides between observations and fades out only with motion');
    return { visible_at: before, hidden_by: after, transition };
  });

  await check(`${tag}: the overview strip follows the zoom and drives it`, async () => {
    const figure = page.locator('figure[data-rv-chart]').nth(LINE);
    // Start from the whole series, whatever an earlier check left.
    if ((await model(page, LINE)).zoom) {
      await page.evaluate(() => document.querySelector('figure[data-rv-chart] .rv-chart__action--reset').click());
      await settled(page);
    }
    assert.equal(await figure.locator('.rv-chart__overview').count(), 0, 'no strip before a zoom');
    const m = await model(page, LINE);
    const extent = m.geometry.panels[0].x.extent;
    const span = extent[1] - extent[0];
    // Keyboard zoom (+) lands at once; then set a known window by dragging.
    await figure.locator('.rv-chart__frame').focus();
    await page.keyboard.press('+');
    await settled(page);
    const strip = figure.locator('.rv-chart__overview');
    assert.equal(await strip.count(), 1, 'a zoom shows the strip');
    const handle = strip.locator('.rv-chart__overview-window');
    const read = async () => {
      const z = (await model(page, LINE)).zoom;
      const s = await strip.boundingBox(), w = await handle.boundingBox();
      return { zoom: z, left: (w.x - s.x) / s.width, width: w.width / s.width };
    };
    let state = await read();
    const whole = (await model(page, LINE)).geometry.panels[0].x;
    assert.ok(Math.abs(state.left - (state.zoom[0] - extent[0]) / span) < 0.02, `the window sits where the zoom is: ${JSON.stringify(state)}`);
    // Dragging the window pans by the same share of the series.
    const before = state.zoom;
    const w0 = await handle.boundingBox(), s0 = await strip.boundingBox();
    await page.mouse.move(w0.x + w0.width / 2, w0.y + w0.height / 2);
    await page.mouse.down();
    await page.mouse.move(w0.x + w0.width / 2 + s0.width * 0.1, w0.y + w0.height / 2, { steps: 5 });
    await page.mouse.up();
    state = await read();
    const moved = (state.zoom[0] - before[0]) / span;
    assert.ok(Math.abs(moved - 0.1 * (whole.domain[1] - whole.domain[0]) / span) < 0.03, `dragging pans: ${moved}`);
    assert.ok(Math.abs((state.zoom[1] - state.zoom[0]) - (before[1] - before[0])) < span * 1e-6, 'and keeps the width');
    // Keys on the window: → pans a tenth of the window, − zooms out, Home resets.
    await handle.focus();
    // + zooms in around the window's centre, leaving room on both sides.
    const wide = state.zoom;
    await page.keyboard.press('+');
    await page.keyboard.press('+');
    state = await read();
    assert.ok(state.zoom[1] - state.zoom[0] < (wide[1] - wide[0]) * 0.7, '+ zooms in');
    const k0 = state.zoom;
    await page.keyboard.press('ArrowRight');
    state = await read();
    assert.ok(Math.abs((state.zoom[0] - k0[0]) - (k0[1] - k0[0]) * 0.1) < (k0[1] - k0[0]) * 0.02, '→ pans a tenth of the window');
    await page.keyboard.press('-');
    state = await read();
    assert.ok(state.zoom[1] - state.zoom[0] > (k0[1] - k0[0]) * 1.2, '− zooms out');
    const valuetext = await handle.getAttribute('aria-valuetext');
    assert.match(valuetext, /^From \d{4}-\d{2}-\d{2} to \d{4}-\d{2}-\d{2}$/);
    await page.keyboard.press('Home');
    await settled(page);
    assert.equal((await model(page, LINE)).zoom, null, 'Home resets');
    assert.equal(await strip.count(), 0, 'and the strip goes');
    assert.equal(await page.evaluate(() => document.activeElement.classList.contains('rv-chart__frame')), true, 'focus returns to the chart');
    // A press on the strip outside the window moves the window there.
    await page.keyboard.press('+');
    await settled(page);
    const s1 = await strip.boundingBox();
    await page.mouse.click(s1.x + s1.width * 0.92, s1.y + s1.height / 2);
    state = await read();
    assert.ok(state.left + state.width > 0.9, `the window moves to the press: ${JSON.stringify(state)}`);
    await page.keyboard.press('Escape');
    await figure.locator('.rv-chart__frame').focus();
    await page.keyboard.press('Escape');
    await settled(page);
    // Scatter plots zoom without a strip.
    const scatter = page.locator('figure[data-rv-chart]').nth(SCATTER);
    await scatter.locator('.rv-chart__frame').focus();
    await page.keyboard.press('+');
    await settled(page);
    assert.ok((await model(page, SCATTER)).zoom, 'the scatter zoomed');
    assert.equal(await scatter.locator('.rv-chart__overview').count(), 0, 'without a strip');
    await page.keyboard.press('Escape');
    await settled(page);
    return { valuetext };
  });
  await context.close();
}

await check('touch pinches and pans only in the expanded view', async () => {
  const context = await h.browser.newContext({ viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true });
  const page = await context.newPage();
  watchErrors(page, errors, 'touch');
  await page.goto(`${inputs.base}/feedback/report.html`);
  await page.waitForFunction(() => window.__richview);
  await settled(page);
  // Fingers move along the plot's own x axis. On this phone held upright the expanded chart is
  // turned a quarter clockwise (FEA-CH-68), so that axis runs down the screen.
  await page.evaluate(() => {
    window.__finger = (canvas, id, type, x) => {
      const r = canvas.getBoundingClientRect();
      const m = new DOMMatrixReadOnly(getComputedStyle(canvas.closest('dialog') ?? document.body).transform);
      const turned = Math.abs(m.b - 1) < 1e-6 && Math.abs(m.c + 1) < 1e-6;
      const y = canvas.offsetHeight / 2;
      const [clientX, clientY] = turned ? [r.right - y, r.top + x] : [r.left + x, r.top + y];
      canvas.dispatchEvent(new PointerEvent(type, { pointerId: id, pointerType: 'touch', clientX, clientY, bubbles: true, isPrimary: id === 1 || id === 7 }));
    };
  });
  const pinch = (selector, k) => page.evaluate(({ selector, k }) => {
    const canvas = document.querySelectorAll(selector)[k];
    const w = canvas.offsetWidth;
    const fire = (type, id, x) => window.__finger(canvas, id, type, x);
    fire('pointerdown', 1, w * 0.4); fire('pointerdown', 2, w * 0.5);
    fire('pointermove', 2, w * 0.55);
    for (let s = 1; s <= 5; s++) fire('pointermove', 2, w * (0.5 + 0.1 * s));
    fire('pointerup', 2, w * 0.9); fire('pointerup', 1, w * 0.4);
  }, { selector, k });
  await pinch('figure[data-rv-chart] canvas.rv-chart__canvas', LINE);
  await settled(page);
  assert.equal((await page.evaluate(() => window.__richview.charts()[0].zoom)), null, 'inline, a pinch does not zoom the chart');
  const inline = await page.evaluate(() => getComputedStyle(document.querySelector('figure[data-rv-chart] canvas.rv-chart__canvas')).touchAction);
  assert.equal(inline, 'pan-y pinch-zoom', 'inline, touch scrolls and zooms the page');
  await page.locator('figure[data-rv-chart]').nth(LINE).locator('.rv-chart__action--expand').click();
  await page.locator('dialog.rv-chart-dialog canvas.rv-chart__canvas').waitFor();
  await settled(page);
  // The expanded chart is read through its overview strip: the window's share of the series,
  // in the strip's own layout, which turns with the chart.
  const windowShare = () => page.evaluate(() => {
    const strip = document.querySelector('dialog .rv-chart__overview');
    if (!strip) return null;
    const m = new DOMMatrixReadOnly(getComputedStyle(strip.closest('dialog')).transform);
    const s = strip.getBoundingClientRect(), w = strip.querySelector('.rv-chart__overview-window').getBoundingClientRect();
    return Math.abs(m.b - 1) < 1e-6
      ? { left: (w.top - s.top) / s.height, width: w.height / s.height }
      : { left: (w.left - s.left) / s.width, width: w.width / s.width };
  });
  const turn = await page.evaluate(() => {
    const m = new DOMMatrixReadOnly(getComputedStyle(document.querySelector('dialog.rv-chart-dialog')).transform);
    const plot = document.querySelector('dialog canvas.rv-chart__canvas');
    return { matrix: [m.a, m.b, m.c, m.d].map(v => Math.round(v * 1000) / 1000), plot: [plot.offsetWidth, plot.offsetHeight] };
  });
  assert.deepEqual(turn.matrix, [0, 1, -1, 0], `a phone held upright turns the expanded chart a quarter (FEA-CH-68): ${JSON.stringify(turn)}`);
  assert.ok(turn.plot[0] > turn.plot[1], `the turned chart is laid out in landscape: ${turn.plot}`);
  assert.equal(await page.evaluate(() => getComputedStyle(document.querySelector('dialog canvas.rv-chart__canvas')).touchAction), 'none');
  assert.equal(await windowShare(), null, 'the expanded chart starts whole');
  await pinch('dialog canvas.rv-chart__canvas', 0);
  await settled(page);
  const zoomed = await windowShare();
  assert.ok(zoomed && zoomed.width < 0.5, `a pinch zooms the expanded chart: ${JSON.stringify(zoomed)}`);
  // One finger pans the zoomed chart.
  await page.evaluate(() => {
    const canvas = document.querySelector('dialog canvas.rv-chart__canvas');
    const w = canvas.offsetWidth;
    const fire = (type, x) => window.__finger(canvas, 7, type, x);
    fire('pointerdown', w * 0.6);
    for (let s = 1; s <= 6; s++) fire('pointermove', w * (0.6 - 0.05 * s));
    fire('pointerup', w * 0.3);
  });
  await settled(page);
  const panned = await windowShare();
  assert.ok(panned.left > zoomed.left + 0.02, `a drag to the left pans later: ${zoomed.left} → ${panned.left}`);
  assert.ok(Math.abs(panned.width - zoomed.width) < 0.005, 'without changing the width');
  // The overview strip turns too: a press on it centres the window where the finger lands, and
  // a drag moves it along the strip, down the screen.
  const strip = page.locator('dialog .rv-chart__overview');
  const along = async share => {
    const box = await strip.boundingBox();
    return { x: box.x + box.width / 2, y: box.y + box.height * share };
  };
  const press = await along(0.25);
  await page.touchscreen.tap(press.x, press.y);
  await settled(page);
  const centred = await windowShare();
  const aim = Math.min(Math.max(0.25, panned.width / 2), 1 - panned.width / 2);
  assert.ok(Math.abs(centred.left + centred.width / 2 - aim) < 0.03, `a press on the turned strip centres the window there: ${JSON.stringify(centred)}`);
  await page.evaluate(() => {
    const handle = document.querySelector('dialog .rv-chart__overview-window');
    const strip = handle.parentElement;
    const r = handle.getBoundingClientRect();
    const fire = (target, type, dy) => target.dispatchEvent(new PointerEvent(type, { pointerId: 9, pointerType: 'touch', clientX: r.left + r.width / 2, clientY: r.top + r.height / 2 + dy, bubbles: true, isPrimary: true }));
    fire(handle, 'pointerdown', 0);
    for (let s = 1; s <= 4; s++) fire(handle, 'pointermove', strip.clientWidth * 0.05 * s);
    fire(handle, 'pointerup', strip.clientWidth * 0.2);
  });
  await settled(page);
  const dragged = await windowShare();
  assert.ok(dragged.left > centred.left + 0.1, `a drag down the turned strip moves the window later: ${centred.left} → ${dragged.left}`);
  // A wheel pans along the plot's x axis, which runs down the turned screen: scrolling down pans
  // later, and scrolling sideways leaves the window where it is.
  const wheel = (deltaX, deltaY) => page.evaluate(({ deltaX, deltaY }) => {
    const canvas = document.querySelector('dialog canvas.rv-chart__canvas');
    const r = canvas.getBoundingClientRect();
    canvas.dispatchEvent(new WheelEvent('wheel', { deltaX, deltaY, clientX: r.left + r.width / 2, clientY: r.top + r.height / 2, bubbles: true, cancelable: true }));
  }, { deltaX, deltaY });
  await wheel(0, -120);
  await settled(page);
  const wheeled = await windowShare();
  assert.ok(wheeled.left < dragged.left - 0.01, `scrolling up the turned chart pans earlier: ${dragged.left} → ${wheeled.left}`);
  await wheel(120, 0);
  await settled(page);
  assert.ok(Math.abs((await windowShare()).left - wheeled.left) < 0.002, 'scrolling across the turned chart does not pan it');
  // Hiding a series lays the turned chart out again; it still fits the dialog.
  const series = page.locator('dialog.rv-chart-dialog [data-rv-series]').first();
  await series.click();
  await settled(page);
  const overflow = await page.evaluate(() => { const d = document.querySelector('dialog.rv-chart-dialog'); return d.scrollHeight - d.clientHeight; });
  assert.ok(overflow <= 1, `with a series hidden the turned chart still fits its dialog: ${overflow}px over`);
  // The value axis runs to the left on the turned screen. Without the production mapping: bars
  // grow rightwards from the baseline at the frame's left, so a tap a third of the way in from the
  // left lies inside North's bars (30 and 50, of a scale reaching beyond 70); a mirrored value
  // axis would put it above them. Categories run down the screen, North first.
  // A phone has no Escape key: the reader taps Close.
  await page.locator('dialog.rv-chart-dialog').getByRole('button', { name: 'Close', exact: true }).tap();
  await page.locator('dialog.rv-chart-dialog').waitFor({ state: 'hidden' });
  await settled(page);
  await page.locator('figure[data-rv-chart]').nth(BAR).locator('.rv-chart__action--expand').click();
  await page.locator('dialog.rv-chart-dialog canvas.rv-chart__canvas').waitFor();
  await settled(page);
  const frame = await page.locator('dialog .rv-chart__frame').boundingBox();
  const readout = page.locator('dialog .rv-chart__readout:not([hidden])');
  await page.touchscreen.tap(frame.x + frame.width * 0.33, frame.y + frame.height * 0.17);
  await settled(page);
  assert.match(await readout.innerText(), /^North\b/, 'a tap inside a short bar of the turned chart reads it');
  await context.close();
  return { turn, zoomed, panned, centred, dragged };
});

// Entrances (FEA-CH-51): the first screen keeps its painted state; a chart below it waits for
// the reader, plays once when half of it shows and never replays. No settled() call here: it
// finishes every entrance at once (FEA-CH-54).
await check('motion: a chart below the first screen enters once, when half of it shows', async () => {
  const context = await h.browser.newContext({ viewport: { width: 1440, height: 700 }, reducedMotion: 'no-preference' });
  const page = await context.newPage();
  watchErrors(page, errors, 'entrance');
  await page.goto(`${inputs.base}/feedback/report.html`);
  await page.waitForFunction(() => window.__richview && window.__richview.charts().length === 5);
  await page.waitForTimeout(400);
  const ids = await page.evaluate(() => [...document.querySelectorAll('figure[data-rv-chart]')].map(f => f.id));
  const waiting = await page.evaluate(() => window.__richview.pending());
  assert.ok(!waiting.includes(ids[LINE]), `the first-screen chart is not waiting: ${waiting}`);
  assert.ok(waiting.includes(ids[PIE]), `a chart below the first screen waits for the reader: ${waiting}`);
  // Ink: canvas pixels that differ from the canvas's corner, sampled every fourth pixel.
  const INK = `
    const ctx = canvas.getContext('2d');
    const d = ctx.getImageData(0, 0, canvas.width, canvas.height).data;
    let n = 0;
    for (let i = 0; i < d.length; i += 16) if (Math.abs(d[i] - d[0]) + Math.abs(d[i + 1] - d[1]) + Math.abs(d[i + 2] - d[2]) > 30) n++;
    return n;`;
  const enter = await frames(page, PIE, "figure.scrollIntoView({ block: 'center' })", INK, 1200);
  const last = enter.at(-1).value;
  const moving = enter.filter(f => Math.abs(f.value - last) > last * 0.01);
  const done = moving.length ? moving.at(-1).t : 0;
  assert.ok(enter[0].value < last * 0.9, `the slices start partly drawn: ${enter[0].value} of ${last}`);
  assert.ok(done > 350 && done < 1000, `and fan out in about 700 ms: ${Math.round(done)}`);
  assert.ok(!(await page.evaluate(() => window.__richview.pending())).includes(ids[PIE]), 'then the chart is settled');
  await page.evaluate(() => window.scrollTo(0, 0));
  await page.waitForTimeout(200);
  const again = await frames(page, PIE, "figure.scrollIntoView({ block: 'center' })", INK, 400);
  assert.ok(again.every(f => Math.abs(f.value - last) <= last * 0.01), `scrolling back never replays it: ${again.map(f => f.value)}`);
  await context.close();
  return { first: enter[0].value, last, done: Math.round(done) };
});

// Decal markers have different silhouettes, but hover must never repaint their centres.
for (const shape of ['circle', 'square', 'triangle', 'diamond', 'plus', 'cross']) {
  await check(`decal ${shape}: the original centre is preserved`, async () => {
    const context = await h.browser.newContext({ viewport: { width: 1440, height: 1000 }, reducedMotion: 'reduce' });
    try {
      const page = await context.newPage();
      watchErrors(page, errors, `decal ${shape}`);
      await page.goto(`${inputs.base}/decals/report.html`);
      await page.waitForFunction(() => window.__richview);
      await settled(page);
      const panel = (await model(page, 0)).geometry.panels[0];
      const point = panel.items.find(i => i.kind === 'point' && i.role === 'mark' && i.shape === shape && i.obs != null);
      assert.ok(point, `the fixture includes a ${shape} point`);
      const box = await canvasBox(page, 0), observation = panel.observations[point.obs];
      await page.evaluate(at => { window.__probe = at; }, point.center);
      const before = await sampleFrame(page, 0, PIXEL);
      await page.mouse.move(box.x + observation.at[0], box.y + observation.at[1]);
      await settled(page);
      assert.equal((await model(page, 0)).target?.observation, point.obs);
      const after = await sampleFrame(page, 0, PIXEL);
      assert.deepEqual(after, before, `the ${shape} centre keeps its mark or hollow interior`);
      return { radius: point.radius, before, after };
    } finally { await context.close(); }
  });
}

// Each contained observation receives feedback on every owned mark, never its hit rectangle.
for (const [name, index] of [['feedback', 0], ['dumbbell', 0], ['combo', 2], ['waffle', 1], ['lollipop', 0], ['gauge', 0]]) {
  await check(`contained ${name}: feedback follows the observation's marks`, async () => {
    const context = await h.browser.newContext({ viewport: { width: 1440, height: 1000 }, reducedMotion: 'no-preference' });
    try {
      const page = await context.newPage();
      watchErrors(page, errors, name);
      await page.goto(`${inputs.base}/${name}/report.html`);
      await page.waitForFunction(() => window.__richview);
      await settled(page);
      const chart = name === 'feedback' ? BAR : 0;
      const m = await model(page, chart), panel = m.geometry.panels[0], o = panel.observations[index];
      const owned = panel.items.filter(i => i.role === 'mark' && i.obs === index);
      const box = await canvasBox(page, chart);
      await page.evaluate(k => {
        const c = document.querySelectorAll('figure[data-rv-chart]')[k].querySelector('canvas');
        window.__markBefore = c.getContext('2d').getImageData(0, 0, c.width, c.height).data;
      }, chart);
      await freeze(page);
      await page.mouse.move(box.x + o.at[0], box.y + o.at[1]);
      const unchanged = await page.evaluate(k => {
        const c = document.querySelectorAll('figure[data-rv-chart]')[k].querySelector('canvas');
        return c.getContext('2d').getImageData(0, 0, c.width, c.height).data.every((value, i) => value === window.__markBefore[i]);
      }, chart);
      assert.ok(unchanged, 'hover starts at rest instead of flashing a hit box');
      await step(page, chart, 'return 0;', 200);
      assert.equal((await model(page, chart)).target?.observation, index, 'the pointer selected the intended observation');
      const result = await page.evaluate(({ chart, owned }) => {
        const c = document.querySelectorAll('figure[data-rv-chart]')[chart].querySelector('canvas');
        const before = window.__markBefore, after = c.getContext('2d').getImageData(0, 0, c.width, c.height).data, scale = c.width / c.clientWidth;
        const changes = owned.filter(i => i.kind === 'rect' || i.kind === 'point').map(mark => {
          const r = mark.kind === 'rect' ? mark.rect : { x: mark.center[0] - mark.radius, y: mark.center[1] - mark.radius, w: mark.radius * 2, h: mark.radius * 2 };
          let n = 0;
          const x0 = Math.max(0, Math.floor((r.x - 4) * scale)), x1 = Math.min(c.width, Math.ceil((r.x + r.w + 4) * scale));
          const y0 = Math.max(0, Math.floor((r.y - 4) * scale)), y1 = Math.min(c.height, Math.ceil((r.y + r.h + 4) * scale));
          for (let y = y0; y < y1; y++) for (let x = x0; x < x1; x++) {
            const j = (y * c.width + x) * 4;
            if (Math.abs(before[j] - after[j]) + Math.abs(before[j + 1] - after[j + 1]) + Math.abs(before[j + 2] - after[j + 2]) > 15) n++;
          }
          return n;
        });
        const pointCentres = owned.filter(i => i.kind === 'point').map(point => {
          const x = Math.round(point.center[0] * scale), y = Math.round(point.center[1] * scale), i = (y * c.width + x) * 4;
          return { before: [...before.slice(i, i + 4)], after: [...after.slice(i, i + 4)] };
        });
        return { changes, pointCentres, identical: after.every((value, i) => value === before[i]) };
      }, { chart, owned });
      if (name === 'gauge') {
        assert.ok(o.area, 'the gauge has a pointer area');
        const sample = await page.evaluate(({ chart, at, area }) => {
          const c = document.querySelectorAll('figure[data-rv-chart]')[chart].querySelector('canvas');
          const before = window.__markBefore, after = c.getContext('2d').getImageData(0, 0, c.width, c.height).data, scale = c.width / c.clientWidth;
          const changed = (x, y) => {
            x = Math.round(x * scale); y = Math.round(y * scale);
            if (x < 0 || y < 0 || x >= c.width || y >= c.height) return false;
            const i = (y * c.width + x) * 4;
            return Math.abs(before[i] - after[i]) + Math.abs(before[i + 1] - after[i + 1]) + Math.abs(before[i + 2] - after[i + 2]) > 15;
          };
          const center = [area.x + area.w / 2, area.y + area.w / 2], delta = [at[0] - center[0], at[1] - center[1]], length = Math.hypot(...delta);
          let bandChanges = 0;
          for (let d = -40; d <= 40; d++) for (let dx = -1; dx <= 1; dx++) for (let dy = -1; dy <= 1; dy++)
            if (changed(at[0] + d * delta[0] / length + dx, at[1] + d * delta[1] / length + dy)) bandChanges++;
          let cornerChanges = 0;
          for (const x of [area.x, area.x + area.w]) for (const y of [area.y, area.y + area.h])
            for (let dx = -2; dx <= 2; dx++) for (let dy = -2; dy <= 2; dy++) if (changed(x + dx, y + dy)) cornerChanges++;
          return { bandChanges, cornerChanges };
        }, { chart, at: o.at, area: o.area });
        assert.ok(sample.bandChanges > 0, 'the selected gauge band receives a contour');
        assert.equal(sample.cornerChanges, 0, 'the gauge hit rectangle stays invisible');
        return sample;
      }
      assert.ok(owned.length > 0, 'the observation owns its painted marks');
      const { changes, pointCentres } = result;
      assert.ok(pointCentres.every(p => p.before.every((v, i) => v === p.after[i])), `point centres keep their original fill: ${JSON.stringify(pointCentres)}`);
      assert.ok(changes.length && changes.every(n => n > 0), `every owned shape receives a contour: ${changes}`);
      return { marks: owned.length, changes };
    } finally { await context.close(); }
  });
}

await check('sankey: node and ribbon contours follow their painted edges', async () => {
  const context = await h.browser.newContext({ viewport: { width: 1440, height: 1000 }, reducedMotion: 'reduce' });
  try {
    const page = await context.newPage();
    watchErrors(page, errors, 'sankey');
    await page.goto(`${inputs.base}/sankey/report.html`);
    await page.waitForFunction(() => window.__richview);
    await settled(page);
    const results = [];
    for (const index of [2, 9]) {
      await page.mouse.move(5, 5);
      await settled(page);
      const panel = (await model(page, 0)).geometry.panels[0];
      const observation = panel.observations[index];
      const mark = panel.items.find(item => item.role === 'mark' && item.obs === index);
      assert.ok(mark, `observation ${index} owns a shape`);
      let point;
      if (mark.kind === 'area') {
        const [a, b] = mark.path.cmds;
        assert.equal(b.c, 'c', 'ribbon edge starts with a cubic curve');
        point = [0, 1].map(axis => (a.p[axis] + 3 * b.c1[axis] + 3 * b.c2[axis] + b.p[axis]) / 8);
      } else {
        assert.equal(mark.kind, 'rect');
        point = [mark.rect.x - 3, mark.rect.y + mark.rect.h / 2];
      }
      const box = await canvasBox(page, 0);
      const pixels = () => page.evaluate(point => {
        const canvas = document.querySelector('figure[data-rv-chart] canvas');
        const scale = canvas.width / canvas.clientWidth;
        return [...canvas.getContext('2d').getImageData(Math.round((point[0] - 1) * scale), Math.round((point[1] - 4) * scale), Math.round(2 * scale), Math.round(8 * scale)).data];
      }, point);
      const before = await pixels();
      await page.mouse.move(box.x + observation.at[0], box.y + observation.at[1]);
      await settled(page);
      assert.equal((await model(page, 0)).target?.observation, index, 'the intended mark is selected');
      const after = await pixels();
      const difference = after.reduce((sum, value, i) => sum + Math.abs(value - before[i]), 0);
      assert.ok(difference > 0, `observation ${index} changes pixels on its actual shape, not just its hit box`);
      results.push({ index, difference });
    }
    return results;
  } finally { await context.close(); }
});

await check('contained fallback: the hit outline fades in', async () => {
  const context = await h.browser.newContext({ viewport: { width: 1440, height: 1000 }, reducedMotion: 'no-preference' });
  try {
    const page = await context.newPage();
    watchErrors(page, errors, 'calendar');
    await page.goto(`${inputs.base}/calendar/report.html`);
    await page.waitForFunction(() => window.__richview);
    await settled(page);
    const o = (await model(page, 0)).geometry.panels[0].observations[0];
    const box = await canvasBox(page, 0);
    await page.evaluate(() => { const c = document.querySelector('figure[data-rv-chart] canvas'); window.__fallbackBefore = c.getContext('2d').getImageData(0, 0, c.width, c.height).data; });
    const delta = () => page.evaluate(() => {
      const c = document.querySelector('figure[data-rv-chart] canvas'), a = c.getContext('2d').getImageData(0, 0, c.width, c.height).data, b = window.__fallbackBefore;
      return a.reduce((sum, value, i) => sum + Math.abs(value - b[i]), 0);
    });
    await freeze(page);
    await page.mouse.move(box.x + o.at[0], box.y + o.at[1]);
    const first = await delta();
    await step(page, 0, 'return 0;', 60); const middle = await delta();
    await step(page, 0, 'return 0;', 60); const last = await delta();
    assert.equal(first, 0, `no box flashes at the beginning: ${first}`);
    assert.ok(middle > 0 && middle < last, `the fallback outline fades in: ${first}, ${middle}, ${last}`);
    return { first, middle, last };
  } finally { await context.close(); }
});

await check('streamgraph: keep the crosshair and readout without layer dots', async () => {
  const context = await h.browser.newContext({ viewport: { width: 1440, height: 1000 }, reducedMotion: 'reduce' });
  try {
    const page = await context.newPage();
    watchErrors(page, errors, 'streamgraph');
    await page.goto(`${inputs.base}/streamgraph/report.html`);
    await page.waitForFunction(() => window.__richview);
    await settled(page);
    const m = await model(page, 0), panel = m.geometry.panels[0], o = panel.observations[5];
    assert.ok(o.dots.length > 0, 'the fixture has layer dot positions');
    const box = await canvasBox(page, 0);
    await page.evaluate(() => { const c = document.querySelector('figure[data-rv-chart] canvas'); window.__streamBefore = c.getContext('2d').getImageData(0, 0, c.width, c.height).data; });
    // Above every layer, so that no layer is under the pointer and none steps back.
    await page.mouse.move(box.x + o.at[0], box.y + panel.plot.y + 2);
    await settled(page);
    assert.equal((await model(page, 0)).target?.observation, 5);
    assert.equal(await page.locator('figure[data-rv-chart] .rv-chart__readout').isVisible(), true, 'the readout remains visible');
    const result = await page.evaluate(({ o, plot }) => {
      const c = document.querySelector('figure[data-rv-chart] canvas'), before = window.__streamBefore;
      const after = c.getContext('2d').getImageData(0, 0, c.width, c.height).data, scale = c.width / c.clientWidth;
      const changed = (x, y) => { const i = (y * c.width + x) * 4; return Math.abs(before[i] - after[i]) + Math.abs(before[i + 1] - after[i + 1]) + Math.abs(before[i + 2] - after[i + 2]) > 15; };
      let dots = 0, crosshair = 0;
      for (const dot of o.dots) {
        for (let y = Math.max(0, Math.floor((dot.at[1] - 9) * scale)); y < Math.min(c.height, Math.ceil((dot.at[1] + 9) * scale)); y++) {
          for (let x = Math.max(0, Math.floor((dot.at[0] - 9) * scale)); x < Math.min(c.width, Math.ceil((dot.at[0] + 9) * scale)); x++) {
            if (Math.abs(x / scale - o.x) > 3 && changed(x, y)) dots++;
          }
        }
      }
      for (let y = Math.max(0, Math.floor(plot.y * scale)); y < Math.min(c.height, Math.ceil((plot.y + plot.h) * scale)); y++) {
        for (let x = Math.max(0, Math.floor((o.x - 2) * scale)); x < Math.min(c.width, Math.ceil((o.x + 2) * scale)); x++) if (changed(x, y)) crosshair++;
      }
      return { dots, crosshair };
    }, { o, plot: panel.plot });
    assert.equal(result.dots, 0, `no layer markers appear: ${result.dots} changed pixels`);
    assert.ok(result.crosshair > 0, 'the crosshair remains visible');
    const dots = [...o.dots].sort((a, b) => a.at[1] - b.at[1]);
    assert.ok(dots.length >= 2, 'keyboard can choose between layers');
    const pixels = () => page.evaluate(points => {
      const c = document.querySelector('figure[data-rv-chart] canvas'), scale = c.width / c.clientWidth;
      return points.map(p => [...c.getContext('2d').getImageData(Math.round(p.at[0] * scale), Math.round(p.at[1] * scale), 1, 1).data]);
    }, dots);
    const frame = page.locator('figure[data-rv-chart] .rv-chart__frame');
    await page.mouse.move(5, 5);
    await frame.focus();
    await page.keyboard.press('Home');
    for (let i = 0; i < 5; i++) await page.keyboard.press('ArrowRight');
    await settled(page);
    const keyboardTop = await pixels();
    await page.mouse.move(box.x + dots[0].at[0], box.y + dots[0].at[1]);
    await settled(page);
    const pointerTop = await pixels();
    assert.deepEqual(keyboardTop, pointerTop, 'keyboard and pointer emphasise the same top layer');
    await page.mouse.move(5, 5);
    await frame.focus();
    await page.keyboard.press('Home');
    for (let i = 0; i < 5; i++) await page.keyboard.press('ArrowRight');
    await page.keyboard.press('ArrowDown');
    await settled(page);
    const keyboardLower = await pixels();
    await page.mouse.move(box.x + dots[1].at[0], box.y + dots[1].at[1]);
    await settled(page);
    assert.deepEqual(keyboardLower, await pixels(), 'Down selects the next layer like the pointer');
    return result;
  } finally { await context.close(); }
});


// FEA-CH-41: a hover is not the first tap of a touch selection.
for (const reduce of [false, true]) {
  await check(`touch ${reduce ? 'reduced' : 'motion'}: the first tap pins a hovered mark and the second clears it`, async () => {
    const context = await h.browser.newContext({ viewport: { width: 390, height: 900 }, hasTouch: true, reducedMotion: reduce ? 'reduce' : 'no-preference' });
    try {
      const page = await context.newPage();
      watchErrors(page, errors, `touch ${reduce}`);
      await page.goto(`${inputs.base}/feedback/report.html`);
      const figure = page.locator('figure[data-rv-chart]').nth(BAR);
      const canvas = figure.locator('canvas.rv-chart__canvas');
      await canvas.scrollIntoViewIfNeeded();
      await page.waitForFunction(k => window.__richview?.charts()[k]?.settled, BAR);
      const box = await canvas.boundingBox();
      const first = await page.evaluate(k => window.__richview.charts()[k].geometry.panels.flatMap(p => p.observations).find(o => o.area), BAR);
      const point = { x: box.x + first.area.x + first.area.w / 2, y: box.y + first.area.y + first.area.h / 2 };
      const readout = figure.locator('.rv-chart__readout');
      const state = () => page.evaluate(k => {
        const m = window.__richview.charts()[k];
        return { target: m.target, pinned: m.pinned };
      }, BAR);
      await page.mouse.move(point.x, point.y);
      await figure.locator('.rv-chart__readout:not([hidden])').waitFor();
      const hovered = await state(), text = await readout.textContent();
      assert.equal(hovered.pinned, false, 'Mouse hover must be distinct from a pinned touch selection');
      await page.touchscreen.tap(point.x, point.y);
      const tapped = await state();
      assert.equal(tapped.pinned, true, 'A first touch on a hovered mark pins it instead of clearing it');
      assert.deepEqual(tapped.target, hovered.target);
      assert.equal(await readout.getAttribute('hidden'), null);
      assert.equal(await readout.textContent(), text);
      await page.touchscreen.tap(point.x, point.y);
      const cleared = await state();
      assert.equal(cleared.pinned, false);
      assert.equal(cleared.target, null);
      assert.notEqual(await readout.getAttribute('hidden'), null);
      return { hovered, tapped, cleared, text };
    } finally { await context.close(); }
  });
}

await inputs.close();
h.save('chart-feedback.json', { evidence, errors, failures });
await h.close();
if (errors.length) failures.push(...errors.map(e => `page error: ${e}`));
if (failures.length) {
  console.error(failures.join('\n'));
  process.exit(1);
}
console.log(`PASS chart feedback: entrances, emphasis, hover dot, scatter and pie contours, zoom glides, readout hold, overview strip and expanded touch, with motion and reduced`);
