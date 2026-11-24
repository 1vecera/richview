// Host reason (ARC-03): sample settled Wasm chart pixels after real pointer and keyboard input.
import assert from 'node:assert/strict';
import palettes from './fixtures/selection-ring-colours.json' with { type: 'json' };

const report = (style, chart) => `---\ntitle: Selection rings\nstyles: [${style}]\n---\n\n\`\`\`chart\n${JSON.stringify(chart)}\n\`\`\`\n`;
export const paletteReports = Object.fromEntries(Object.entries(palettes).map(([style, { marks }]) => [
  'rings-' + style, report(style, {
    type: 'bar', title: 'Series colours', axes: { y: { min: 0, max: 20 } },
    encoding: { x: 'label', y: 'value', color: 'group' },
    data: { inline: marks.map((_, i) => ({ label: 'All', value: 11.3, group: `Series ${i + 1}` })) },
  }),
]));
export const reducedReport = report('scientific', {
  type: 'scatter', title: 'Two neighbours beside the axis', encoding: { x: 'x', y: 'y' }, data: { csv: 'points.csv' },
});
export const reducedCsv = ['x,y', '0,41.23', '0.00001,41.23',
  ...Array.from({ length: 10001 }, (_, i) => `${80 + i / 1000},${i % 101}`)].join('\n');
export const bubbleReport = report('scientific', {
  type: 'packed-bubble', title: 'A large bubble and an omitted bubble', encoding: { label: 'name', y: 'value' },
  data: { inline: [{ name: 'Large', value: 1000000 }, { name: 'Small', value: 1 }] },
});

const settled = page => page.evaluate(() => window.__richview.settled());
const target = page => page.evaluate(() => window.__richview.charts()[0].target?.observation);
const hex = rgb => '#' + rgb.slice(0, 3).map(v => Math.round(255 * v).toString(16).padStart(2, '0')).join('');
const rgb = hex => hex.slice(1).match(/../g).map(v => parseInt(v, 16));
function luminance(colour) {
  const [r, g, b] = colour.map(v => (v /= 255) <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}
async function openChart(page, url) {
  await page.goto(url);
  await page.waitForFunction(() => window.__richview?.charts()[0]?.geometry.panels.length);
  const frame = page.locator('.rv-chart__frame').first(), canvas = frame.locator('canvas.rv-chart__canvas');
  await frame.scrollIntoViewIfNeeded();
  await settled(page);
  const geometry = await page.evaluate(() => window.__richview.charts()[0].geometry);
  return { frame, canvas, geometry, panel: geometry.panels[0] };
}
async function selectKey(page, frame, index, maxSteps) {
  await frame.press('Home');
  for (let i = 0; i < maxSteps && await target(page) !== index; i++) await frame.press('ArrowRight');
  await settled(page);
  assert.equal(await target(page), index);
  assert.ok(await frame.evaluate(f => f.hasAttribute('data-rv-selected')));
}
async function pointAt(page, canvas, width, [x, y]) {
  const box = await canvas.boundingBox(), scale = box.width / width;
  await page.mouse.move(box.x + x * scale, box.y + y * scale);
  await settled(page);
}
async function pixels(canvas, width, points) {
  return canvas.evaluate((c, { width, points }) => {
    const ctx = c.getContext('2d'), scale = c.width / width;
    return points.map(([x,y]) => [...ctx.getImageData(Math.floor(x * scale), Math.floor(y * scale), 1, 1).data].slice(0, 3));
  }, { width, points });
}
const distance = (a, b) => Math.max(...a.map((v, i) => Math.abs(v - b[i])));

/** Measure the middle of the sharp horizontal band, clear paper, and the original bar fill.
 * At 2× the band is 2.5 device pixels wide. The pixel containing its centre is fully covered.
 * Native sRGB and WebGL blending can differ by up to two 8-bit channel levels. */
export async function verifyPalette(page, base, style, capture) {
  const { marks, rings, paper } = palettes[style];
  const { frame, canvas, geometry, panel } = await openChart(page, base + '/rings-' + style + '/report.html');
  assert.ok(await canvas.evaluate((c, width) => c.width / width >= 1.99, geometry.width));
  const bars = panel.items.filter(m => m.kind === 'rect' && m.role === 'mark');
  const results = [];
  for (const [i, colour] of marks.entries()) {
    const bar = bars.find(m => hex(m.fill.color) === colour);
    assert.ok(bar, style + ' ' + colour);
    const { x, y, w, h } = bar.rect, cx = x + w / 2;
    const top = y - (bar.stroke?.width ?? 0) / 2;
    const probes = [[cx, top - 2.125], [cx, top - 16], [cx, y + h / 3]];
    assert.ok(top - 16 > panel.plot.y, 'the sample has clear paper inside the plot');
    await page.mouse.move(1, 1);
    await frame.press('Escape');
    await settled(page);
    const idle = await pixels(canvas, geometry.width, probes);
    let pointer;
    for (const input of ['pointer', 'keyboard']) {
      if (input === 'pointer') await pointAt(page, canvas, geometry.width, [cx, y + h / 2]);
      else await selectKey(page, frame, bar.obs, bars.length);
      assert.equal(await target(page), bar.obs);
      const [ring, background, fill] = await pixels(canvas, geometry.width, probes);
      const label = `${style} slot ${i + 1} ${input}`;
      assert.ok(distance(background, rgb(paper)) <= 2, label + ' clear paper ' + background);
      assert.deepEqual(fill, idle[2], label + ' bar fill');
      assert.ok(distance(ring, rgb(rings[i])) <= 2, label + ' ring ' + ring + ' expected ' + rings[i]);
      const a = luminance(ring), b = luminance(background);
      const contrast = (Math.max(a,b) + 0.05) / (Math.min(a,b) + 0.05);
      assert.ok(contrast >= 3, label + ' measured ' + contrast);
      if (pointer) assert.deepEqual(ring, pointer, label + ' matches pointer pixels');
      else pointer = ring;
      results.push({ slot: i + 1, input, ring, paper: background, contrast });
      if (capture) await capture({ canvas, width: geometry.width, bar, slot: i + 1, input });
    }
  }
  return results;
}

export async function verifyOmitted(page, base, kind) {
  const { frame, canvas, geometry, panel } = await openChart(page, base + '/' + kind + '/report.html');
  const scatter = kind === 'reduced-scatter';
  const owns = new Set(panel.items.filter(m => m.role === 'mark').map(m => m.obs));
  const index = panel.observations.findIndex((o, j) => !owns.has(j) && (!scatter || Math.abs(o.at[0] - panel.plot.x) < 1));
  assert.ok(index >= 0, kind + ' has an omitted observation');
  const o = panel.observations[index], [x, y] = o.at;
  const r = Math.max(o.radius, 4) + 2.125;
  if (!scatter) {
    const parent = panel.items.find(m => m.kind === 'point' && m.role === 'mark');
    assert.ok(parent);
    const d = Math.hypot(x - parent.center[0], y - parent.center[1]);
    assert.ok(Math.abs(d - parent.radius) < 0.01, 'the omitted bubble sits on the packed circle');
    assert.ok(Math.min(x, y, geometry.width - x, geometry.height - y) >= 19, 'the whole cue fits the canvas');
  }
  const probes = Array.from({ length: 8 }, (_, i) => [
    x + r * Math.cos(i * Math.PI / 4), y + r * Math.sin(i * Math.PI / 4),
  ]);
  const before = await pixels(canvas, geometry.width, probes);
  let pointer;
  if (scatter) {
    // Just to the right of the two nearby centres selects the omitted right-hand row.
    await pointAt(page, canvas, geometry.width, [x + 1, y]);
    assert.equal(await target(page), index);
    pointer = await pixels(canvas, geometry.width, probes);
  } else {
    assert.equal(o.radius, 0, 'the skipped bubble has no pointer hit area');
  }
  await selectKey(page, frame, index, 3);
  const keyed = await pixels(canvas, geometry.width, probes);
  // The parent's own fill can match part of the ring. The four quadrants must each show
  // a changed pixel in the sharp band, including both quadrants outside the scatter axis.
  for (let i = 0; i < probes.length; i += 2) {
    assert.ok([i, i + 1].some(j => distance(keyed[j], before[j]) > 12), kind + ' has an arc in quadrant ' + i / 2);
  }
  if (pointer) assert.deepEqual(keyed, pointer, kind + ' uses identical pixels for both inputs');
  const readout = page.locator('.rv-chart__readout').first();
  assert.ok(await readout.isVisible());
  assert.ok((await readout.innerText()).includes(o.readout.heading));
  if (!scatter) {
    const box = await readout.boundingBox(), surface = await canvas.boundingBox();
    const scale = surface.width / geometry.width;
    const left = surface.x + x * scale, top = surface.y + y * scale;
    const dx = Math.max(box.x - left, left - box.x - box.width, 0);
    const dy = Math.max(box.y - top, top - box.y - box.height, 0);
    assert.ok(Math.hypot(dx, dy) < Math.hypot(surface.x - left, surface.y - top) / 2,
      'the readout is near the packed anchor');
  }
  return { index, at: o.at, pointer, keyed };
}

export async function verifySizedMarker(page, base, shape) {
  const { frame, canvas, geometry, panel } = await openChart(page, base + '/decals/report.html');
  const m = panel.items.find(m => m.kind === 'point' && m.role === 'mark' && m.shape === shape);
  assert.ok(m);
  const [x,y] = m.center, radius = m.radius * 1.25, band = m.stroke.width / 2 + 2.125;
  // Sample inside the old circular backing, along the arm or the middle of the slanted facet.
  const at = shape === 'plus' ? [x + radius * 0.48, y + band]
    : [x + radius / 2 + band / Math.SQRT2, y + radius / 2 + band / Math.SQRT2];
  assert.ok(Math.hypot(at[0]-x, at[1]-y) < radius, shape + ' probe inside the backing');
  await pointAt(page, canvas, geometry.width, m.center);
  assert.equal(await target(page), m.obs);
  const [pointer] = await pixels(canvas, geometry.width, [at]);
  const i = palettes['hand-drawn'].marks.indexOf(hex(m.stroke.color));
  assert.ok(i >= 0);
  assert.ok(distance(pointer, rgb(palettes['hand-drawn'].rings[i])) <= 12, shape + ' sharp band ' + pointer);
  await selectKey(page, frame, m.obs, panel.observations.length);
  assert.deepEqual(await pixels(canvas, geometry.width, [at]), [pointer]);
  return { shape, pointer };
}

export const ringStyles = Object.keys(palettes);
