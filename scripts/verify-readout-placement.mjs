// Host reason (ARC-03): verify chart readout placement through real browser input.
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { loadavg } from 'node:os';
import { join } from 'node:path';
import { harness, root, watchErrors } from './browser-harness.mjs';
import { reports, settle } from './report-runtime.mjs';

// RV_PLACEMENT_STYLES, _WIDTHS and _PAGES narrow a run while developing; the gate runs them all.
const only = (name, all) => process.env[name] ? process.env[name].split(',').map(item => all.find(value => String(value) === item) ?? item) : all;
const styles = only('RV_PLACEMENT_STYLES', ['sleek', 'air', 'scientific', 'brutalist', 'hand-drawn']);
const widths = only('RV_PLACEMENT_WIDTHS', [1280, 390]).map(Number);
const pages = only('RV_PLACEMENT_PAGES', ['types', 'openai-vs-anthropic', 'touch-point', 'dense']);
// Dense charts for hover timing only (PRF-25): one line of 5,000 days, three of 2,000 days and
// 3,000 points, from a seeded walk so every run draws the same data.
const seeded = seed => () => { seed = (seed + 0x6d2b79f5) | 0; let t = Math.imul(seed ^ (seed >>> 15), 1 | seed); t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t; return ((t ^ (t >>> 14)) >>> 0) / 4294967296; };
const day = i => new Date(Date.UTC(2012, 0, 1 + i)).toISOString().slice(0, 10);
const walk = (random, n, from) => { let value = from; return Array.from({ length: n }, () => (value = Math.max(1, value + (random() - 0.5) * 2)).toFixed(2)); };
const denseFiles = (() => {
  const random = seeded(986);
  const one = walk(random, 5000, 40).map((value, i) => `${day(i)},${value}`);
  const three = ['North', 'South', 'West'].flatMap((region, r) => walk(random, 2000, 50 + 10 * r).map((value, i) => `${day(i)},${region},${value}`));
  const points = Array.from({ length: 3000 }, (_, i) => `${(random() * 100).toFixed(2)},${(random() * 60).toFixed(2)},${['North', 'South', 'West'][i % 3]}`);
  return { 'one.csv': `day,price\n${one.join('\n')}\n`, 'three.csv': `day,region,value\n${three.join('\n')}\n`, 'scatter.csv': `x,y,region\n${points.join('\n')}\n` };
})();
const dense = '---\ntitle: Dense hover\n---\n' + [
  'type: line\ntitle: One series of 5,000 days\ndata:\n  csv: one.csv\nencoding: {x: day, y: price}',
  'type: line\ntitle: Three series of 2,000 days\ndata:\n  csv: three.csv\nencoding: {x: day, y: value, color: region}',
  'type: scatter\ntitle: Three thousand points\ndata:\n  csv: scatter.csv\nencoding: {x: x, y: y, color: region}',
].map(body => `\n\`\`\`chart\n${body}\n\`\`\`\n`).join('');
// Hover timing (PRF-25), per chart: main-thread time from a pointer move to the end of the next
// frame, without the wait for that frame, and whether that frame already shows the readout for
// the observation the move reached. The limits are always judged, beside a full run's other
// suites too, so they catch a freeze and nothing else: a busy host moves the slowest hovers, not
// the median. Each chart's line carries the 1-minute load it was timed under and, where its
// slowest move took longer than a frame, whether that time went into the handler or the frame.
// A chart that misses a limit while the 1-minute load is over 8, the perf step's threshold, is
// re-timed once over the same moves against the same limits; both are recorded and the re-time
// decides.
const HOVER_P50 = 16.7, HOVER_P95 = 50, HOVER_MAX = 250, FIRST_FRAME = 0.95, RETIME_LOAD = 8;
// The fewest moves a median is judged from: a chart of one observation is hovered once, and one
// searched place of 20 to 30 ms is a slow frame, which the 95th percentile and the maximum bound.
const MEDIAN_MOVES = 5;
const timings = [];
// A listener before the chart's own (capture) and one after it (bubble): the first opens a
// record and asks for the next frame before the chart can, the second asks after it; a task
// posted from the second frame callback runs once that frame is painted.
const installTiming = page => page.evaluate(() => {
  if (window.__rvHover) return;
  const hover = window.__rvHover = { current: null, chart: 0 };
  addEventListener('pointermove', event => {
    if (!event.isTrusted) return;
    const record = hover.current = { start: performance.now() };
    requestAnimationFrame(() => { record.frame ??= performance.now(); });
  }, { capture: true });
  addEventListener('pointermove', event => {
    const record = hover.current;
    if (!event.isTrusted || !record || record.handled) return;
    record.handled = performance.now();
    requestAnimationFrame(() => {
      const channel = new MessageChannel();
      channel.port1.onmessage = () => {
        record.end = performance.now();
        const model = window.__richview.charts()[hover.chart], readout = document.getElementById(model.figure).querySelector('.rv-chart__readout');
        record.target = model.target;
        record.text = readout.hidden ? null : readout.querySelector('.rv-chart__readout-head')?.textContent ?? readout.textContent;
        record.searched = readout.hasAttribute('data-rv-placement-fallback');
      };
      channel.port2.postMessage(0);
    });
  });
});
// Move the pointer and time the move; the record says what the first frame after it showed.
const timedMove = async (page, chart, x, y) => {
  await page.evaluate(index => { window.__rvHover.chart = index; window.__rvHover.current = null; }, chart);
  await page.mouse.move(x, y);
  return (await page.waitForFunction(() => window.__rvHover.current?.end !== undefined && window.__rvHover.current, null, { timeout: 5000 })).jsonValue();
};
// What the readout settled on, to compare with what the first frame showed.
const settledReadout = (page, chart) => page.evaluate(index => {
  const model = window.__richview.charts()[index], readout = document.getElementById(model.figure).querySelector('.rv-chart__readout');
  return { target: model.target, text: readout.hidden ? null : readout.querySelector('.rv-chart__readout-head')?.textContent ?? readout.textContent };
}, chart);
const timeDense = async (page, chart, model, figure, label, width) => {
  const box = await figure.locator('canvas.rv-chart__canvas').boundingBox();
  const panel = model.geometry.panels[0], sx = box.width / model.geometry.width, sy = box.height / model.geometry.height;
  const time = timing(label);
  for (let i = 0; i < 24; i++) {
    // A shared line is targeted by x alone; points by their own place.
    const o = panel.observations[Math.floor((i + 0.5) * panel.observations.length / 24)];
    const [px, py] = panel.targeting === 'nearest-x' ? [panel.plot.x + panel.plot.w * (i + 0.5) / 24, panel.plot.y + panel.plot.h / 2] : o.at;
    const x = box.x + px * sx, y = box.y + py * sy;
    if (x < 0 || y < 0 || x >= width || y >= 900) continue;
    try { await time.at(page, chart, x, y); } catch (error) { fail(`${label}: hover timing failed: ${error.message.split('\n')[0]}`); }
  }
  await retimeUnderLoad(page, chart, time);
};
const quantile = (sorted, q) => sorted[Math.min(sorted.length - 1, Math.max(0, Math.ceil(q * sorted.length) - 1))];
// A chart's hover statistics, and what they miss of the limits (empty when they meet them all).
const hoverStats = (entry) => {
  const sorted = [...entry.ms].sort((a, b) => a - b), p50 = quantile(sorted, 0.5), p95 = quantile(sorted, 0.95), max = sorted.at(-1), first = entry.first / sorted.length;
  const missed = (sorted.length >= MEDIAN_MOVES && p50 > HOVER_P50) || p95 > HOVER_P95 || max > HOVER_MAX || first < FIRST_FRAME;
  return { sorted, p50, p95, max, first, missed };
};
// Re-time a chart once over the same moves when it missed a limit under a 1-minute load over 8.
const retimeUnderLoad = async (page, chart, time) => {
  const entry = time.entry;
  if (!entry.ms.length || !hoverStats(entry).missed || Math.max(entry.load, loadavg()[0]) <= RETIME_LOAD) return;
  const again = timing(`${entry.label}, re-timed under load`);
  entry.retimed = again.entry;
  again.entry.retime = true;
  for (const [x, y] of entry.moves) {
    try { await again.at(page, chart, x, y); } catch (error) { fail(`${again.entry.label}: hover timing failed: ${error.message.split('\n')[0]}`); }
  }
  await page.keyboard.press('Escape');
};
const timing = (label) => {
  const entry = { label, ms: [], moves: [], first: 0, load: loadavg()[0] };
  timings.push(entry);
  const time = async (page, chart, record) => {
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    const settled = await settledReadout(page, chart);
    const handler = record.handled - record.start, frame = record.end - record.frame;
    entry.ms.push(handler + frame);
    if (!entry.slowest || handler + frame > entry.slowest.handler + entry.slowest.frame) entry.slowest = { handler, frame, observation: record.target?.observation, searched: record.searched };
    if (JSON.stringify(record.target) === JSON.stringify(settled.target) && record.text === settled.text && settled.text !== null) entry.first++;
  };
  time.entry = entry;
  // Move to (x, y), remembering the move for a re-time, and time it.
  time.at = async (page, chart, x, y) => {
    entry.moves.push([x, y]);
    await time(page, chart, await timedMove(page, chart, x, y));
  };
  return time;
};
const curated = new Set(['anscombe', 'gapminder', 'german-electricity', 'global-temperature', 'keeling-curve', 'nightingale', 'old-faithful', 'playfair', 'titanic']);
const chartPages = readdirSync(join(root, 'docs/public/charts')).filter(name => name.endsWith('.md') && !curated.has(name.slice(0, -3)));
let types = '---\ntitle: Every chart type\n---\n';
for (const name of chartPages) {
  const lines = readFileSync(join(root, 'docs/public/charts', name), 'utf8').split('\n');
  const start = lines.indexOf('```chart');
  const end = lines.indexOf('```', start + 1);
  if (start >= 0 && end > start) types += `\n${lines.slice(start, end + 1).join('\n')}\n`;
}
const touchPoint = `---\ntitle: Touch point\n---\n\n\`\`\`chart\n${JSON.stringify({ type: 'scatter', title: 'Touch point', encoding: { x: 'x', y: 'y' }, data: { inline: [{ x: 1, y: 1 }] } })}\n\`\`\`\n`;
// Subtract later painted faces so only truly hidden polygonal observations are exempt from
// pointer comparison; keyboard still visits and checks every observation.
const signedArea = points => points.reduce((sum, p, i) => {
  const q = points[(i + 1) % points.length]; return sum + p[0] * q[1] - q[0] * p[1];
}, 0) / 2;
const clipHalfPlane = (poly, a, b, sign, inside) => {
  const distance = p => sign * ((b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]));
  const keep = d => inside ? d >= -1e-8 : d <= 1e-8;
  const out = [];
  for (let i = 0; i < poly.length; i++) {
    const p = poly[i], q = poly[(i + 1) % poly.length], dp = distance(p), dq = distance(q);
    if (keep(dp)) out.push(p);
    if (keep(dp) !== keep(dq)) {
      const t = dp / (dp - dq); out.push([p[0] + t * (q[0] - p[0]), p[1] + t * (q[1] - p[1])]);
    }
  }
  return out.length >= 3 && Math.abs(signedArea(out)) > 1e-7 ? out : [];
};
const subtractConvex = (poly, cover) => {
  const sign = Math.sign(signedArea(cover)), outside = [];
  let inside = poly;
  for (let i = 0; i < cover.length && inside.length; i++) {
    const a = cover[i], b = cover[(i + 1) % cover.length];
    const piece = clipHalfPlane(inside, a, b, sign, false);
    if (piece.length) outside.push(piece);
    inside = clipHalfPlane(inside, a, b, sign, true);
  }
  return outside;
};
const triangulate = points => {
  const sign = Math.sign(signedArea(points)), remaining = points.slice(), triangles = [];
  const cross = (a, b, p) => sign * ((b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]));
  while (remaining.length > 3) {
    let ear = -1;
    for (let i = 0; i < remaining.length; i++) {
      const a = remaining[(i + remaining.length - 1) % remaining.length], b = remaining[i], c = remaining[(i + 1) % remaining.length];
      if (cross(a, b, c) <= 1e-8) continue;
      if (remaining.some((p, j) => j !== i && j !== (i + 1) % remaining.length && j !== (i + remaining.length - 1) % remaining.length && cross(a, b, p) >= -1e-8 && cross(b, c, p) >= -1e-8 && cross(c, a, p) >= -1e-8)) continue;
      triangles.push([a, b, c]); ear = i; break;
    }
    if (ear < 0) return [];
    remaining.splice(ear, 1);
  }
  if (remaining.length === 3 && Math.abs(signedArea(remaining)) > 1e-7) triangles.push(remaining);
  return triangles;
};
const fullyOccludedFace = (model, panelIndex, observation) => {
  const items = model.geometry.panels[panelIndex].items;
  const raw = items.flatMap((item, order) => {
    if (item.kind !== 'area' || item.role !== 'mark' || !Number.isInteger(item.obs)) return [];
    const commands = item.path.cmds;
    if (commands.some(cmd => !['m', 'l', 'z'].includes(cmd.c)) || commands.filter(cmd => cmd.c === 'm').length !== 1) return [];
    return [{ obs: item.obs, order, opaque: item.fill.color[3] >= 0.999, points: commands.filter(cmd => cmd.c === 'm' || cmd.c === 'l').map(cmd => cmd.p) }];
  });
  const selected = raw.filter(face => face.obs === observation);
  if (!selected.length) return false;
  for (const face of selected) {
    let visible = triangulate(face.points);
    if (!visible.length) return false;
    for (const cover of raw.filter(other => other.order > face.order && other.obs !== observation && other.opaque)) {
      const triangles = triangulate(cover.points);
      if (!triangles.length) continue;
      for (const triangle of triangles) {
        visible = visible.flatMap(piece => subtractConvex(piece, triangle));
        if (!visible.length) break;
      }
      if (!visible.length) break;
    }
    if (visible.length) return false;
  }
  return true;
};
const coincidentKeyboardOnly = (model, panelIndex, observation) => {
  const panel = model.geometry.panels[panelIndex], target = panel.observations[observation];
  if (target.area || target.wedge || !target.radius) return false;
  const marks = panel.items.map((item, order) => ({ item, order })).filter(({ item }) => item.kind === 'point' && item.role === 'mark' && Number.isInteger(item.obs));
  const own = marks.filter(({ item }) => item.obs === observation).at(-1);
  if (!own) return false;
  return marks.some(({ item, order }) => {
    const other = panel.observations[item.obs];
    return order > own.order && other && !other.area && !other.wedge && other.at[0] === target.at[0] && other.at[1] === target.at[1]
      && Math.max(other.radius, 24) >= Math.max(target.radius, 24);
  });
};
const h = await harness('readout-placement');
const errors = [], failures = [], occluded = [], coincident = [], notDrawn = [], unavoidable = [];
// What a readout gives up when nothing clears, from what it keeps longest (P1b): after the title,
// the toolbar, a kept legend or key, its own mark and whole numbers (P1c), these.
const tiers = ['key', 'subtitle', 'legend', 'note', 'text'];
// The families whose marks are tiles among tiles (P1c).
const TILED = new Set(['pivot', 'heatmap', 'calendar', 'treemap', 'bin2d', 'hexbin', 'waffle', 'icicle']);
const givenUp = Object.fromEntries(tiers.map(tier => [tier, []]));
const fail = message => { failures.push(message); console.error(message); };
let observations = 0, fallbacks = 0, dockedOverNote = 0, besideTiles = 0;
// A readout never cuts a number (P1c): a text with a digit is either clear of it or wholly under it.
// Cuts are counted per readout and by where the number sits; a cut is unavoidable only when no place
// that keeps the title, the toolbar and the mark as the readout keeps them cuts none.
const cutReadouts = [], cutUnavoidable = [], cutWhere = {};
let cutNumbers = 0, textCovered = 0, labelsTouched = 0;
// A wide chart's readout keeps 6 px from a label it does not cover; a phone's readout covers each
// entry of a legend it stands in for wholly or not at all, widening its surface where it must; a
// bar's readout on a wide chart takes a place over no other mark while one stays with the bar,
// and otherwise lies on bars that stepped back before any at full strength (P1c).
const LABEL_GAP = 6;
let labelsAbutted = 0, widened = 0, barsOverMarks = 0, barsOverStrong = 0;
const pointFor = o => {
  if (o.wedge) {
    const w = o.wedge, angle = (w.start + w.end) / 2, radius = (w.inner + w.radius) / 2;
    return [w.center[0] + Math.sin(angle) * radius, w.center[1] - Math.cos(angle) * radius];
  }
  return o.at;
};
// Rectangle boundaries partition all possible positions into cells with the same overlap set.
// Shrinking the mark boxes makes this a conservative proof: if even their interiors cannot be
// avoided, the painted marks cannot be avoided either.
const clearPositionExists = (sample, obstacles, accept = () => true) => {
  const r = sample.readout, f = sample.figure, v = sample.viewport, c = sample.canvas;
  let minX = Math.max(0, f.x) + 8, maxX = Math.min(v.w, f.x + f.w) - 8 - r.w;
  let minY = Math.max(0, f.y) + 8, maxY = Math.min(v.h, f.y + f.h) - 8 - r.h;
  // A wide chart's readout stays in its room, the visible canvas 8 px in, as the runtime's search does.
  if (c.w >= 480) {
    minX = Math.max(minX, c.x + 8 * sample.sx); maxX = Math.min(maxX, c.x + c.w - 8 * sample.sx - r.w);
    minY = Math.max(minY, c.y + 8 * sample.sy); maxY = Math.min(maxY, c.y + c.h - 8 * sample.sy - r.h);
  }
  if (maxX < minX || maxY < minY) return false;
  const xs = [minX, maxX], ys = [minY, maxY];
  for (const o of obstacles) { xs.push(o.x - r.w, o.x + o.w); ys.push(o.y - r.h, o.y + o.h); }
  const overlaps = (a, b) => a.x < b.x + b.w - 1e-6 && b.x < a.x + a.w - 1e-6 && a.y < b.y + b.h - 1e-6 && b.y < a.y + a.h - 1e-6;
  for (const x of xs.filter(x => x >= minX && x <= maxX)) for (const y of ys.filter(y => y >= minY && y <= maxY)) {
    const candidate = { x, y, w: r.w, h: r.h };
    if (obstacles.every(obstacle => !overlaps(candidate, obstacle)) && accept(candidate)) return candidate;
  }
  return null;
};
// The painted pixels of the selected mark inside a place, from the mask a sample carries.
const paintedCount = ({ left, top, w: W, h: H, bits }) => {
  const bytes = Buffer.from(bits, 'base64'), sums = new Int32Array((W + 1) * (H + 1));
  for (let y = 0; y < H; y++) for (let x = 0; x < W; x++) {
    const i = y * W + x;
    sums[(y + 1) * (W + 1) + x + 1] = ((bytes[i >> 3] >> (i & 7)) & 1) + sums[y * (W + 1) + x + 1] + sums[(y + 1) * (W + 1) + x] - sums[y * (W + 1) + x];
  }
  const at = (value, most) => Math.min(most, Math.max(0, Math.round(value)));
  return place => {
    const x0 = at(place.x - left, W), x1 = at(place.x + place.w - left, W), y0 = at(place.y - top, H), y1 = at(place.y + place.h - top, H);
    return sums[y1 * (W + 1) + x1] - sums[y0 * (W + 1) + x1] - sums[y1 * (W + 1) + x0] + sums[y0 * (W + 1) + x0];
  };
};
const contains = (outer, inner) => inner.x >= outer.x - 0.5 && inner.y >= outer.y - 0.5 && inner.x + inner.w <= outer.x + outer.w + 0.5 && inner.y + inner.h <= outer.y + outer.h + 0.5;
const meetsBox = (a, b) => a.x < b.x + b.w - 0.02 && b.x < a.x + a.w - 0.02 && a.y < b.y + b.h - 0.02 && b.y < a.y + a.h - 0.02;
// The numbers a place cuts: those it overlaps without holding all of.
const cutsAt = (sample, place) => sample.numbers.filter(number => meetsBox(place, number) && !contains(place, number));
// A place in the readout's room, clear of the obstacles and accepted, that cuts no number. Every
// number adds the four edges at which a readout clears or wholly covers it, so if such a place
// exists, one lies on these coordinates.
const uncutPositionExists = (sample, obstacles, accept = () => true) => {
  const r = sample.readout, f = sample.figure, v = sample.viewport, c = sample.canvas;
  let minX = Math.max(0, f.x) + 8, maxX = Math.min(v.w, f.x + f.w) - 8 - r.w;
  let minY = Math.max(0, f.y) + 8, maxY = Math.min(v.h, f.y + f.h) - 8 - r.h;
  if (c.w >= 480) {
    minX = Math.max(minX, c.x + 8 * sample.sx); maxX = Math.min(maxX, c.x + c.w - 8 * sample.sx - r.w);
    minY = Math.max(minY, c.y + 8 * sample.sy); maxY = Math.min(maxY, c.y + c.h - 8 * sample.sy - r.h);
  }
  if (maxX < minX || maxY < minY) return null;
  const xs = [minX, maxX], ys = [minY, maxY];
  for (const o of obstacles) { xs.push(o.x - r.w, o.x + o.w); ys.push(o.y - r.h, o.y + o.h); }
  for (const n of sample.numbers) { xs.push(n.x - r.w, n.x + n.w, n.x, n.x + n.w - r.w); ys.push(n.y - r.h, n.y + n.h, n.y, n.y + n.h - r.h); }
  const overlaps = (a, b) => a.x < b.x + b.w - 1e-6 && b.x < a.x + a.w - 1e-6 && a.y < b.y + b.h - 1e-6 && b.y < a.y + a.h - 1e-6;
  const inRange = (value, min, max) => value >= min - 1e-9 && value <= max + 1e-9;
  for (const x of xs.filter(x => inRange(x, minX, maxX))) for (const y of ys.filter(y => inRange(y, minY, maxY))) {
    const candidate = { x, y, w: r.w, h: r.h };
    if (obstacles.every(obstacle => !overlaps(candidate, obstacle)) && accept(candidate) && !cutsAt(sample, candidate).length) return candidate;
  }
  return null;
};
const check = (sample, label) => {
  assert.ok(sample.target && sample.readout, `${label}: missing selection or readout`);
  const { readout: r, mark: o, canvas: c, viewport: v } = sample;
  assert.ok(r.x >= -0.5 && r.y >= -0.5 && r.x + r.w <= v.w + 0.5 && r.y + r.h <= v.h + 0.5, `${label}: outside viewport`);
  const intersects = (a, b) => a.x < b.x + b.w - 0.02 && b.x < a.x + a.w - 0.02 && a.y < b.y + b.h - 0.02 && b.y < a.y + a.h - 0.02;
  assert.ok(r.x >= sample.figure.x - 0.5 && r.y >= sample.figure.y - 0.5 && r.x + r.w <= sample.figure.x + sample.figure.w + 0.5 && r.y + r.h <= sample.figure.y + sample.figure.h + 0.5, `${label}: outside figure`);
  for (const text of sample.keep.fixed) assert.ok(!intersects(r, text), `${label}: covers the title, the toolbar, a key or a legend it does not stand in for`);
  // A readout taller than its content was squeezed; only one as tall as the visible figure scrolls.
  assert.ok(sample.content.scroll <= sample.content.client + 1 || sample.content.client >= sample.content.room - 1, `${label}: shorter than its content`);
  const page = a => ({ x: c.x + a.x * sample.sx, y: c.y + a.y * sample.sy, w: a.w * sample.sx, h: a.h * sample.sy });
  const grow = (a, by) => ({ x: a.x - by, y: a.y - by, w: a.w + 2 * by, h: a.h + 2 * by });
  // Boxes holding the mark and its 4 px halo: a place clear of them clears both.
  const halo = (o.dots?.length ? o.dots.map(dot => ({ x: dot.at[0] - 6, y: dot.at[1] - 6, w: 12, h: 12 }))
    : o.wedge ? [{ x: o.wedge.center[0] - o.wedge.radius, y: o.wedge.center[1] - o.wedge.radius, w: 2 * o.wedge.radius, h: 2 * o.wedge.radius }]
      : sample.markBounds ?? [o.area ?? { x: o.at[0] - o.radius, y: o.at[1] - o.radius, w: 2 * o.radius, h: 2 * o.radius }]).map(a => grow(page(a), 4 * sample.sx));
  let markCovered = false;
  if (c.w < 480) {
    assert.ok(['dock-below', 'dock-above', 'plot-band', 'hollow', 'fallback'].includes(sample.placement), `${label}: unknown narrow placement`);
    if (sample.placement === 'dock-below' || sample.placement === 'dock-above') assert.ok(!intersects(r, sample.plot), `${label}: phone dock covers the plot`);
    if (sample.placement === 'plot-band' || sample.placement === 'hollow') assert.ok(r.y >= sample.plot.y - 0.5 && r.y + r.h <= sample.plot.y + sample.plot.h + 0.5, `${label}: band outside plot`);
  }
  if (o.wedge) {
    const w = o.wedge;
    for (let x = r.x; x <= r.x + r.w; x += 2) for (let y = r.y; y <= r.y + r.h; y += 2) {
      const dx = (x - c.x) / sample.sx - w.center[0], dy = (y - c.y) / sample.sy - w.center[1];
      const angle = ((Math.atan2(dx, -dy) - w.start) % (2 * Math.PI) + 2 * Math.PI) % (2 * Math.PI);
      assert.ok(Math.hypot(dx, dy) < w.inner || Math.hypot(dx, dy) > w.radius || angle > w.end - w.start, `${label}: covers wedge`);
    }
  } else if (sample.gaugeMarkCovered !== null) {
    // A gauge or alluvial readout hides some of its painted mark only when no place inside the
    // figure clears both it and the title, the toolbar and any kept legend or key.
    if (sample.gaugeMarkCovered) {
      assert.ok(sample.fallback && !sample.gaugeClear, `${label}: covers selected painted mark (${JSON.stringify(sample.gaugeClear)} clears it)`);
      markCovered = true;
      const entry = `${label} (${o.readout.heading}): mark overlap unavoidable`;
      unavoidable.push(entry); console.log(entry);
    }
    // A gauge's value and its name in the middle read with the mark and stay uncut.
    if (sample.kind === 'gauge') for (const item of sample.items.filter(item => item.role === 'label')) {
      assert.ok(!intersects(r, page(item.bounds)), `${label}: covers the gauge's value or its name`);
    }
  } else {
    const boxes = sample.markBounds ?? (o.dots?.length
      ? o.dots.map(dot => ({ x: dot.at[0] - 4, y: dot.at[1] - 4, w: 8, h: 8 }))
      : [o.area ?? { x: o.at[0] - o.radius, y: o.at[1] - o.radius, w: 2 * o.radius, h: 2 * o.radius }]);
    assert.ok(boxes.length, `${label}: selected mark has no geometry`);
    const marks = boxes.map(page);
    // A point mark is a disc: test the readout against the circle, not its bounding square.
    const disc = !sample.markBounds && !o.dots?.length && !o.area && o.radius > 0;
    const hitsDisc = () => {
      const cx = c.x + o.at[0] * sample.sx, cy = c.y + o.at[1] * sample.sy, radius = o.radius * sample.sx;
      const nx = Math.min(Math.max(cx, r.x), r.x + r.w), ny = Math.min(Math.max(cy, r.y), r.y + r.h);
      return Math.hypot(nx - cx, ny - cy) < radius - 0.02;
    };
    if (disc ? hitsDisc() : marks.some(mark => intersects(r, mark))) {
      markCovered = true;
      const inner = o.dots?.length
        ? o.dots.flatMap(dot => [0.25, 0.5, 0.75, 0.9, 0.99, 0.999].map(fraction => {
          const dx = 6 * fraction, dy = Math.sqrt(36 - dx * dx);
          return { x: c.x + (dot.at[0] - dx) * sample.sx, y: c.y + (dot.at[1] - dy) * sample.sy, w: 2 * dx * sample.sx, h: 2 * dy * sample.sy };
        }))
        : sample.markProofBounds
          ? sample.markProofBounds.map(a => ({ x: c.x + a.x * sample.sx, y: c.y + a.y * sample.sy, w: a.w * sample.sx, h: a.h * sample.sy }))
          : marks.map(m => ({ x: m.x + m.w * 0.15, y: m.y + m.h * 0.15, w: m.w * 0.7, h: m.h * 0.7 }));
      const provable = sample.fallback && (o.dots?.length || sample.markBounds) && !clearPositionExists(sample, [...sample.keep.fixed, ...inner]);
      assert.ok(provable, `${label}: covers selected mark`);
      const entry = `${label} (${o.readout.heading}): mark overlap unavoidable`;
      unavoidable.push(entry); console.log(entry);
    }
  }
  // Each kept text it covers must have been given up for want of a place clearing it together
  // with everything kept longer. Once the readout covers its mark, a place counts only if it
  // hides no more of the mark, as the runtime measures it: each dot by its 12 px square, any other
  // mark by its painted parts or its box.
  const markBoxes = (o.dots?.length ? o.dots.map(dot => ({ x: dot.at[0] - 6, y: dot.at[1] - 6, w: 12, h: 12 }))
    : sample.paintedBoxes ?? sample.markBounds ?? (o.area ? [o.area] : o.radius > 0 ? [{ x: o.at[0] - o.radius, y: o.at[1] - o.radius, w: 2 * o.radius, h: 2 * o.radius }] : [])).map(page);
  const shared = (a, b) => Math.max(0, Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x)) * Math.max(0, Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y));
  // A gauge's ring, a ribbon, a venn disc or lens has empty space in its boxes, where the runtime
  // measures the painted mark: there a place hides no more only if it hides fewer painted pixels
  // by more than the pixels along its own edges, which the two measures can differ by.
  const paintedIn = sample.painted && paintedCount(sample.painted);
  const hidden = place => markBoxes.reduce((sum, b) => sum + shared(place, b), 0);
  const noMoreHidden = markCovered ? place => hidden(place) <= hidden(r) + 0.5 : undefined;
  // For the place that would cut no number, the painted measure where there is one: by boxes a
  // gauge's readout over the end of its ring hides more than one over the middle of a band.
  const noMorePainted = markCovered && paintedIn ? place => paintedIn(place) + r.w + r.h <= paintedIn(r) : noMoreHidden;
  const kept = [...sample.keep.fixed, ...(markCovered ? [] : halo)];
  // On a wide chart a readout keeps a gap from a label it does not cover; standing nearer is
  // the first thing a fallback gives up. The gauge's hollow and a phone's docks stay flush.
  const gap = c.w >= 480 && sample.placement !== 'hollow' ? LABEL_GAP * sample.sx : 0;
  // Keeping numbers uncut ranks right after the mark: a place that would clear a text by cutting
  // more numbers than the readout does is no alternative.
  const cut = cutsAt(sample, r);
  const keepsNumbers = place => (!noMoreHidden || noMoreHidden(place)) && cutsAt(sample, place).length <= cut.length;
  const docked = c.w < 480 && (sample.placement === 'dock-below' || sample.placement === 'dock-above');
  // Among tiles every place covers some number, so the amount of text must not send a tile's
  // readout away: a place beside the tile, 12 px off either side, that cuts no number wins over
  // any farther place, and least text decides only between the two sides (P1c, Chart design
  // 16 Nov 08:00). The other side, where it is a place at all, must give up no less.
  const tile = TILED.has(sample.kind) && c.w >= 480 && !markCovered && !cut.length && o.area ? page(o.area) : null;
  const sideGap = 12 * sample.sx;
  const besideTile = tile && r.y < tile.y + tile.h && tile.y < r.y + r.h
    && (Math.abs(r.x - (tile.x + tile.w + sideGap)) < 0.6 || Math.abs(r.x + r.w - (tile.x - sideGap)) < 0.6);
  if (besideTile) {
    const mirror = { ...r, x: r.x > tile.x ? tile.x - sideGap - r.w : tile.x + tile.w + sideGap };
    const room = { x: c.x + 8 * sample.sx, y: c.y + 8 * sample.sy, w: c.w - 16 * sample.sx, h: c.h - 16 * sample.sy };
    // The runtime keeps a place inside its room to the pixel: `contains` allows half a pixel,
    // which made a side 0.4 px past the room a place the readout should have taken.
    const inRoom = mirror.x >= room.x - 0.02 && mirror.y >= room.y - 0.02 && mirror.x + mirror.w <= room.x + room.w + 0.02 && mirror.y + mirror.h <= room.y + room.h + 0.02;
    const place = inRoom && !sample.keep.fixed.some(text => intersects(mirror, text))
      && !halo.some(box => intersects(mirror, box)) && !cutsAt(sample, mirror).length;
    const given = at => tiers.map(tier => sample.keep[tier].filter(text => intersects(at, text)).reduce((sum, text) => sum + text.w * text.h, 0));
    if (place) {
      const [mine, other] = [given(r), given(mirror)];
      const first = mine.findIndex((area, i) => Math.abs(area - other[i]) > 0.5);
      // The message is built only for a real difference: with none, `first` is -1.
      if (first >= 0) assert.ok(mine[first] < other[first], `${label}: beside its tile on the side that gives up more (${tiers[first]}: ${mine[first].toFixed(0)} against ${other[first].toFixed(0)})`);
    }
    besideTiles++;
  }
  for (const tier of tiers) {
    kept.push(...sample.keep[tier]);
    if (!sample.keep[tier].some(text => intersects(r, text))) continue;
    if (besideTile) continue;
    // A phone dock may lie over the note, but only over whole lines of it: one that would end
    // inside a line widens to the content width (P1c, Chart design 16 Nov 08:00). It clears
    // every other kept text.
    if (docked) {
      assert.equal(tier, 'note', `${label}: phone dock covers the ${tier}`);
      const halvedLines = sample.noteLines.filter(line => meetsBox(r, line) && !contains(r, line));
      assert.ok(!halvedLines.length, `${label}: phone dock ends inside ${halvedLines.length} line(s) of the note (readout ${JSON.stringify(r)}; lines ${JSON.stringify(halvedLines)})`);
      dockedOverNote++;
      continue;
    }
    const clear = clearPositionExists(sample, kept, keepsNumbers);
    assert.ok(!clear, `${label}: covers the ${tier} although a place clears it (${JSON.stringify(clear)}; readout ${JSON.stringify(r)}${markCovered ? ', its mark covered' : ''})`);
    const entry = `${label} (${o.readout.heading}): gave up the ${tier}`;
    givenUp[tier].push(entry); console.log(entry);
  }
  assert.deepEqual(sample.scroll, sample.before, `${label}: moved the page`);
  const near = [...sample.keep.key, ...sample.keep.text].filter(text => !intersects(r, text) && intersects(grow(r, gap - 0.05), text));
  if (gap && near.length) {
    assert.ok(sample.fallback, `${label}: ${near.length} label(s) nearer than ${LABEL_GAP} px beside a clear place`);
    labelsAbutted += near.length;
  }
  if (sample.widened) widened++;
  // A cut entry of a legend the readout stands in for is judged with the cut numbers below, which
  // count it: it fails where a place cuts none, and is listed as unavoidable where none does. A
  // phone figure can leave no such place: Hand-drawn's legend lies between the axis labels and the
  // title row, and the toolbar's box reaches half a pixel into its last line.
  if (sample.bar) {
    const over = marks => marks.map(page).filter(mark => intersects(r, mark)).length;
    const [strong, others] = [over(sample.others.filter(mark => mark.strong)), over(sample.others)];
    // One of the usual places may lie on bars that stepped back when no free place stays with
    // the bar; never on one at full strength. Only a plain bar chart's series say which those are.
    if (c.w >= 480 && sample.kind === 'bar') assert.ok(sample.fallback || !strong, `${label}: a bar's readout lies on ${strong} other mark(s) at full strength in one of its usual places`);
    if (others) barsOverMarks++;
    if (strong) barsOverStrong++;
  }
  if (sample.fallback) {
    fallbacks++;
    const texts = tiers.flatMap(tier => sample.keep[tier]);
    textCovered += texts.reduce((sum, text) => sum + shared(r, text), 0);
    labelsTouched += texts.filter(text => intersects(r, text)).length;
  }
  observations++;
  if (cut.length) {
    cutNumbers += cut.length;
    for (const number of cut) cutWhere[`${number.where}${docked ? ' (dock)' : ''}`] = (cutWhere[`${number.where}${docked ? ' (dock)' : ''}`] ?? 0) + 1;
    // A phone dock lies over the note as it always did. A note is a paragraph: the dock hides
    // part of it whether or not a digit stands in it, so it is counted here and not judged.
    if (docked && cut.every(number => number.where === 'note')) return;
    const where = `${label} (${o.readout.heading}): cuts ${cut.map(number => `'${number.text}' (${number.where})`).join(', ')}`;
    const place = uncutPositionExists(sample, [...sample.keep.fixed, ...(markCovered ? [] : halo)], noMorePainted);
    if (place) {
      cutReadouts.push(where);
      assert.fail(`${where} although a place cuts none (${JSON.stringify(place)}; readout ${JSON.stringify(r)}${docked ? ', docked' : ''})`);
    }
    const entry = `${where}: unavoidable, no place keeping the title, toolbar${markCovered ? ' and as much of its mark' : ', mark and halo'} clear cuts none`;
    cutUnavoidable.push(entry); console.log(entry);
  }
};
const sample = (page, chart, before) => page.evaluate(({ chart, before }) => {
  const model = window.__richview.charts()[chart], figure = document.getElementById(model.figure);
  const canvas = figure.querySelector('canvas.rv-chart__canvas').getBoundingClientRect();
  const readout = figure.querySelector('.rv-chart__readout');
  const box = readout.hidden ? null : readout.getBoundingClientRect();
  const t = model.target, panel = t && model.geometry.panels[t.panel], mark = panel && panel.observations[t.observation];
  const sx = canvas.width / model.geometry.width, sy = canvas.height / model.geometry.height;
  const rect = element => { const b = element.getBoundingClientRect(); return { x: b.x, y: b.y, w: b.width, h: b.height }; };
  const shown = selector => [...figure.querySelectorAll(selector)].map(rect).filter(b => b.w > 0 && b.h > 0);
  // A shared readout naming every legend entry beside its swatch stands in for the legend.
  const legendNames = [...figure.querySelectorAll('.rv-chart__legend .rv-chart__legend-name')].map(name => name.textContent);
  const cued = box ? [...readout.querySelectorAll('.rv-chart__readout-line')].filter(row => row.querySelector('.rv-chart__cue')).map(row => row.querySelector('.rv-chart__readout-name')?.textContent) : [];
  const standsIn = panel?.targeting === 'nearest-x' && legendNames.length > 0 && legendNames.every(name => cued.includes(name));
  const legend = shown('.rv-chart__legend');
  const toPage = b => ({ x: canvas.x + b.x * sx, y: canvas.y + b.y * sy, w: b.w * sx, h: b.h * sy });
  const drawn = panel ? [...panel.items, ...(model.geometry.figure_items ?? [])] : [];
  // A drawn item's box in chart units: a path by the points that define it.
  const itemBox = item => {
    if (item.kind === 'text') return item.bounds;
    if (item.kind === 'rect') return item.rect;
    if (item.kind === 'point') return { x: item.center[0] - item.radius, y: item.center[1] - item.radius, w: 2 * item.radius, h: 2 * item.radius };
    const points = (item.path?.cmds ?? []).flatMap(command => [command.p, command.c1, command.c2].filter(Boolean));
    if (!points.length) return null;
    const xs = points.map(p => p[0]), ys = points.map(p => p[1]);
    return { x: Math.min(...xs), y: Math.min(...ys), w: Math.max(...xs) - Math.min(...xs), h: Math.max(...ys) - Math.min(...ys) };
  };
  // A key drawn in the chart, such as a gauge's bands or a colour scale, counts as a legend no
  // readout stands in for: each swatch and label is kept like the title. A gauge's key repeats
  // the band its readout names, so it gives way after the subtitle, before the gauge itself.
  const keys = drawn.filter(item => item.role === 'key').map(itemBox).filter(Boolean).map(toPage);
  const gauge = model.geometry.kind === 'gauge';
  // The entries of a legend a phone's readout stands in for, and a bar's other rectangles and
  // discs, with whether each stays at full strength while the bar is read.
  const entries = standsIn && canvas.width < 480 ? shown('.rv-chart__legend-entry') : [];
  // Each line of the note as the browser lays it out, its parts on one line joined.
  const noteLines = [];
  for (const note of figure.querySelectorAll('.rv-chart__method')) {
    const range = document.createRange(); range.selectNodeContents(note);
    for (const b of range.getClientRects()) {
      if (b.width <= 0 || b.height <= 0) continue;
      const line = noteLines.find(l => Math.min(l.y + l.h, b.bottom) - Math.max(l.y, b.top) > 0.5 * Math.min(l.h, b.height));
      if (!line) { noteLines.push({ x: b.x, y: b.y, w: b.width, h: b.height }); continue; }
      const [x0, y0, x1, y1] = [Math.min(line.x, b.left), Math.min(line.y, b.top), Math.max(line.x + line.w, b.right), Math.max(line.y + line.h, b.bottom)];
      Object.assign(line, { x: x0, y: y0, w: x1 - x0, h: y1 - y0 });
    }
  }
  const bar = ['bar', 'combo', 'pareto', 'waterfall', 'histogram', 'lollipop', 'dumbbell', 'bullet', 'gantt', 'interval', 'funnel', 'bar3d'].includes(model.geometry.kind)
    && panel?.targeting !== 'nearest-x' && !mark?.wedge && !!mark?.area;
  const series = bar ? panel.items.find(item => item.role === 'mark' && item.obs === t.observation && item.series != null)?.series : undefined;
  const others = bar ? panel.items.filter(item => item.role === 'mark' && item.obs !== t.observation && ['rect', 'point'].includes(item.kind))
    .map(item => ({ ...itemBox(item), strong: series == null || item.series == null || item.series === series })) : [];
  const keep = {
    fixed: [...shown('.rv-chart__title, .rv-chart__actions'), ...(standsIn ? [] : legend), ...(gauge ? [] : keys)],
    key: gauge ? keys : [],
    subtitle: shown('.rv-chart__subtitle'), legend: standsIn && canvas.width >= 480 ? legend : [], note: shown('.rv-chart__method'),
    text: drawn.filter(item => item.kind === 'text' && item.role !== 'key').map(item => toPage(item.bounds)),
  };
  // Every text with a digit, in the chart and in the figure's head and note (P1c).
  const digit = /\d/;
  const numbers = [
    ...drawn.filter(item => item.kind === 'text' && digit.test(item.text ?? '')).map(item => ({ ...toPage(item.bounds), text: item.text, where: item.role === 'key' ? 'key' : 'chart' })),
    ...[['.rv-chart__title', 'title'], ['.rv-chart__subtitle', 'subtitle'], ['.rv-chart__method', 'note']].flatMap(([selector, where]) =>
      [...figure.querySelectorAll(selector)].filter(element => digit.test(element.textContent)).map(element => ({ ...rect(element), text: element.textContent.trim().replace(/\s+/g, ' ').slice(0, 40), where }))),
  ].filter(box => box.w > 0 && box.h > 0);
  // An entry of a legend the readout stands in for is kept whole like a number: the search
  // counts both together.
  numbers.push(...entries.map(entry => ({ ...entry, text: 'legend entry', where: 'legend' })));
  // Axis labels, which a readout of an undrawn observation never covers either.
  const axis = panel ? panel.items.filter(item => item.kind === 'text' && item.role === 'axis-label').map(item => ({ x: canvas.x + item.bounds.x * sx, y: canvas.y + item.bounds.y * sy, w: item.bounds.w * sx, h: item.bounds.h * sy })) : [];
  const figureBox = figure.getBoundingClientRect();
  // A readout's room is where it may sit: the visible figure on a narrow chart, the visible canvas otherwise.
  const roomBox = canvas.width < 480 ? figureBox : canvas;
  const visibleHeight = Math.min(innerHeight, figureBox.bottom, roomBox.bottom) - Math.max(0, figureBox.top, roomBox.top);
  // A gauge's ring, an alluvial or chord ribbon, a venn set's disc and an overlap's lens have
  // empty space in their observation bounds. Test the painted paths at pixel resolution instead
  // of treating those corners as marks, as the runtime does (readout_place::painted_paths). A
  // chord's arc keeps the exact wedge test.
  let gaugeMarkCovered = null, gaugeClear = null, paintedBoxes = null, painted = null;
  if (['gauge', 'alluvial', 'chord', 'venn'].includes(model.geometry.kind) && !mark?.wedge && box && panel) {
    const selected = panel.items.filter(item => item.role === 'mark' && (model.geometry.kind === 'gauge' || item.obs === t.observation));
    // The selected mark painted over a page region, as an image.
    const paint = (left, top, width, height) => {
      const image = document.createElement('canvas');
      image.width = Math.max(1, Math.ceil(width)); image.height = Math.max(1, Math.ceil(height));
      const context = image.getContext('2d');
      context.translate(canvas.x - left, canvas.y - top);
      context.scale(sx, sy);
      for (const item of selected) {
        const path = new Path2D();
        if (item.kind === 'rect') path.rect(item.rect.x, item.rect.y, item.rect.w, item.rect.h);
        else if (item.kind === 'point') path.arc(item.center[0], item.center[1], item.radius, 0, 2 * Math.PI);
        else if (item.path) for (const command of item.path.cmds) {
          if (command.c === 'm') path.moveTo(...command.p);
          else if (command.c === 'l') path.lineTo(...command.p);
          else if (command.c === 'c') path.bezierCurveTo(...command.c1, ...command.c2, ...command.p);
          else if (command.c === 'z') path.closePath();
        }
        if (item.fill) context.fill(path);
        if (item.stroke) { context.lineWidth = item.stroke.width; context.stroke(path); }
      }
      return { data: context.getImageData(0, 0, image.width, image.height).data, w: image.width, h: image.height };
    };
    if (selected.length) {
      gaugeMarkCovered = paint(box.x, box.y, box.width, box.height).data.some((value, index) => index % 4 === 3 && value > 0);
      paintedBoxes = selected.map(itemBox).filter(Boolean);
      if (gaugeMarkCovered) {
        // A place 8 px inside the visible figure, and the canvas on a wide chart, clear of
        // the painted mark, a gauge's value and name, and the title, the toolbar and any kept
        // legend or key, searched on a 2 px grid with a summed-area table of the painted pixels.
        const left = Math.max(figureBox.left, 0) + 8, top = Math.max(figureBox.top, 0) + 8;
        const right = Math.min(figureBox.right, innerWidth) - 8, bottom = Math.min(figureBox.bottom, innerHeight) - 8;
        const { data, w: W, h: H } = paint(left, top, right - left, bottom - top);
        const sums = new Float64Array((W + 1) * (H + 1));
        for (let y = 0; y < H; y++) for (let x = 0; x < W; x++) {
          sums[(y + 1) * (W + 1) + x + 1] = (data[(y * W + x) * 4 + 3] > 0 ? 1 : 0) + sums[y * (W + 1) + x + 1] + sums[(y + 1) * (W + 1) + x] - sums[y * (W + 1) + x];
        }
        // The painted pixels as a bit mask, for the places a covered mark is compared at.
        const bytes = new Uint8Array(Math.ceil(W * H / 8));
        for (let i = 0; i < W * H; i++) if (data[i * 4 + 3] > 0) bytes[i >> 3] |= 1 << (i & 7);
        let binary = '';
        for (let i = 0; i < bytes.length; i += 0x8000) binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
        painted = { left, top, w: W, h: H, bits: btoa(binary) };
        const rw = Math.ceil(box.width), rh = Math.ceil(box.height), wide = canvas.width >= 480;
        const kept = [...keep.fixed, ...panel.items.filter(item => model.geometry.kind === 'gauge' && item.kind === 'text' && item.role === 'label').map(item => toPage(item.bounds))];
        const meets = (a, b) => a.x < b.x + b.w - 0.02 && b.x < a.x + a.w - 0.02 && a.y < b.y + b.h - 0.02 && b.y < a.y + a.h - 0.02;
        search: for (let y = 0; y + rh <= H; y += 2) for (let x = 0; x + rw <= W; x += 2) {
          const place = { x: left + x, y: top + y, w: box.width, h: box.height };
          if (wide && (place.x < canvas.x + 8 || place.x + place.w > canvas.right - 8 || place.y < canvas.y + 8 || place.y + place.h > canvas.bottom - 8)) continue;
          const painted = sums[(y + rh) * (W + 1) + x + rw] - sums[y * (W + 1) + x + rw] - sums[(y + rh) * (W + 1) + x] + sums[y * (W + 1) + x];
          if (painted === 0 && !kept.some(b => meets(place, b))) { gaugeClear = place; break search; }
        }
      }
    }
  }
  return { target: t, mark, kind: model.geometry.kind, items: model.geometry.kind === 'gauge' && panel ? panel.items.filter(item => item.kind === 'text') : [],
    canvas: { x: canvas.x, y: canvas.y, w: canvas.width, h: canvas.height }, sx, sy, figure: rect(figure), keep, axis, standsIn, numbers,
    entries, noteLines, bar, others, painted, widened: readout.hasAttribute('data-rv-surface'),
    content: { scroll: readout.scrollHeight, client: readout.clientHeight, room: visibleHeight - 16 * sy },
    readout: box && { x: box.x, y: box.y, w: box.width, h: box.height },
    plot: panel && { x: canvas.x + panel.plot.x * sx, y: canvas.y + panel.plot.y * sy, w: panel.plot.w * sx, h: panel.plot.h * sy },
    // These observation bounds include empty targeting space between the painted parts.
    markBounds: ['bar', 'pareto', 'waffle'].includes(model.geometry.kind) && panel ? panel.items.flatMap(item => {
      if (item.role !== 'mark' || item.obs !== t.observation) return [];
      if (item.kind === 'rect') return [item.rect];
      if (item.kind === 'point') return [{ x: item.center[0] - item.radius, y: item.center[1] - item.radius, w: 2 * item.radius, h: 2 * item.radius }];
      return [];
    }) : null,
    markProofBounds: ['bar', 'pareto', 'waffle'].includes(model.geometry.kind) && panel ? panel.items.flatMap(item => {
      if (item.role !== 'mark' || item.obs !== t.observation) return [];
      if (item.kind === 'rect') {
        const r = item.rect, inset = item.radius?.some(radius => radius > 0) ? 0.15 : 0;
        return [{ x: r.x + r.w * inset, y: r.y + r.h * inset, w: r.w * (1 - 2 * inset), h: r.h * (1 - 2 * inset) }];
      }
      if (item.kind === 'point' && ['circle', 'square'].includes(item.shape)) {
        // The runtime draws a square at 0.85 of its radius; stay inside its painted area.
        const radius = item.radius * (item.shape === 'square' ? 0.8 : 1 / Math.SQRT2);
        return [{ x: item.center[0] - radius, y: item.center[1] - radius, w: 2 * radius, h: 2 * radius }];
      }
      return [];
    }) : null,
    viewport: { w: innerWidth, h: innerHeight }, scroll: [scrollX, scrollY], before,
    fallback: readout.hasAttribute('data-rv-placement-fallback'), placement: readout.dataset.rvPlacement ?? null, gaugeMarkCovered, gaugeClear, paintedBoxes };
}, { chart, before });
try {
  for (const style of styles) {
    const inputs = await reports(join(h.dir, style), { style, only: ['openai-vs-anthropic'], extra: { types, 'touch-point': touchPoint, dense }, files: { dense: denseFiles } });
    try {
      for (const width of widths) {
        const page = await h.browser.newPage({ viewport: { width, height: 900 }, reducedMotion: 'reduce', hasTouch: width === 390 });
        watchErrors(page, errors);
        for (const name of pages) {
          await page.goto(`${inputs.base}/${name}/report.html`);
          const models = await settle(page);
          await installTiming(page);
          for (let chart = 0; chart < models.length; chart++) {
            const figure = page.locator('figure[data-rv-chart]').nth(chart), frame = figure.locator('.rv-chart__frame');
            await frame.scrollIntoViewIfNeeded();
            await frame.focus();
            const label = `${style} ${width} ${name} chart ${chart} (${models[chart].geometry.kind})`;
            // The dense charts are timed only: their thousands of observations are covered by the
            // same placement as the charts checked in full.
            if (name === 'dense') { await timeDense(page, chart, models[chart], figure, label, width); continue; }
            const time = timing(label);
            const before = await page.evaluate(() => [scrollX, scrollY]);
            // Find a point actually belonging to each mark before the comparison sweep: nearby
            // ribbons and cells can overlap the centre of an observation's bounding box.
            const pointerPoints = new Map(), undrawn = new Set();
            const canvasBox = await figure.locator('canvas.rv-chart__canvas').boundingBox();
            const scaleX = canvasBox.width / models[chart].geometry.width, scaleY = canvasBox.height / models[chart].geometry.height;
            for (let panel = 0; panel < models[chart].geometry.panels.length; panel++) {
              const items = models[chart].geometry.panels[panel].observations;
              for (let index = 0; index < items.length; index++) {
                const o = items[index], points = [pointFor(o)];
                const model = models[chart], geometry = model.geometry.panels[panel];
                // Observations with no mark geometry are retained in the data for omitted words,
                // treemap leaves and bubbles. Nearest-x columns can have no individual shape.
                if ((geometry.targeting === 'contain' && !o.area && !o.wedge && !(o.radius > 0))
                  || (geometry.targeting === 'radius' && !(o.radius > 0))) {
                  const label = `${style} ${width} ${name} chart ${chart} panel ${panel} observation ${index} (${o.readout.heading})`;
                  undrawn.add(`${panel}:${index}`); notDrawn.push(label); console.log(`${label}: not drawn, keyboard only`); continue;
                }
                // The opaque faces of these two families can be proved fully hidden without
                // hundreds of futile pointer moves. Other families retain the same proof below.
                if (['bar3d', 'surface'].includes(models[chart].geometry.kind) && fullyOccludedFace(models[chart], panel, index)) {
                  const label = `${style} ${width} ${name} chart ${chart} panel ${panel} observation ${index} (${o.readout.heading})`;
                  occluded.push(label); console.log(`${label}: fully occluded`); continue;
                }
                if (o.area) for (let x = 1; x < 12; x++) for (let y = 1; y < 12; y++) points.push([o.area.x + o.area.w*x/12, o.area.y + o.area.h*y/12]);
                // The engine also accepts points within two pixels of a filled mark. Small maps
                // and thin links may have their only reachable point in that surrounding area.
                if (o.area) for (let x = 0; x <= 24; x++) for (let y = 0; y <= 24; y++) points.push([o.area.x - 2 + (o.area.w + 4)*x/24, o.area.y - 2 + (o.area.h + 4)*y/24]);
                for (const point of points) {
                  const px = canvasBox.x + point[0]*scaleX, py = canvasBox.y + point[1]*scaleY;
                  if (px < 0 || py < 0 || px >= width || py >= 900) continue;
                  await page.mouse.move(px, py);
                  const target = await page.evaluate(i => window.__richview.charts()[i].target, chart);
                  if (target?.panel === panel && target.observation === index) { pointerPoints.set(`${panel}:${index}`, point); break; }
                }
                if (!pointerPoints.has(`${panel}:${index}`)) {
                  const label = `${style} ${width} ${name} chart ${chart} panel ${panel} observation ${index} (${o.readout.heading})`;
                  if (coincidentKeyboardOnly(models[chart], panel, index)) { coincident.push(label); console.log(`${label}: coincident, keyboard only`); }
                  else if (fullyOccludedFace(models[chart], panel, index)) { occluded.push(label); console.log(`${label}: fully occluded`); }
                  else fail(`${label}: no pointer hit point found`);
                }
              }
            }
            await page.mouse.move(5, 5);
            await page.waitForTimeout(450);
            await frame.focus();
            await page.keyboard.press('Home');
            // The keyboard reaches every observation once, drawn or not (the same rows in every style).
            const count = models[chart].geometry.panels.reduce((n, panel) => n + panel.observations.length, 0);
            const keyboard = [], visited = new Set(), walked = [];
            for (let index = 0; index < count; index++) {
              const label = `${style} ${width} ${name} chart ${chart} observation ${index}`;
              try {
                const current = await sample(page, chart, before);
                const key = current.target && `${current.target.panel}:${current.target.observation}`;
                assert.ok(key, `${label}: keyboard selected nothing`);
                assert.ok(!visited.has(key), `${label}: keyboard repeated an observation`);
                visited.add(key);
                walked.push(current.target);
                // An undrawn observation has no mark to clear; its readout still shows, page unmoved.
                // An undrawn observation has no mark to clear, but its readout is placed like any
                // other and covers no title, toolbar, legend or axis labels.
                if (undrawn.has(key)) {
                  assert.ok(current.readout, `${label}: undrawn observation shows no readout`);
                  assert.deepEqual(current.scroll, current.before, `${label}: moved the page`);
                  for (const [what, boxes] of [['title, toolbar, key or legend', [...current.keep.fixed, ...current.keep.key, ...current.keep.legend]], ['axis labels', current.axis]]) {
                    const r = current.readout, meets = b => r.x < b.x + b.w - 0.02 && b.x < r.x + r.w - 0.02 && r.y < b.y + b.h - 0.02 && b.y < r.y + r.h - 0.02;
                    assert.ok(!boxes.some(meets), `${label}: undrawn observation's readout covers the ${what}`);
                  }
                } else {
                  keyboard.push(current);
                  check(current, `${label} keyboard`);
                }
              } catch (error) { fail(error.message); }
              await page.keyboard.press('ArrowRight');
            }
            if (count) {
              try {
                await page.keyboard.press('End');
                assert.deepEqual((await sample(page, chart, before)).target, walked.at(-1), `${name} chart ${chart}: End selects the last observation`);
                await page.keyboard.press('Home');
                assert.deepEqual((await sample(page, chart, before)).target, walked[0], `${name} chart ${chart}: Home selects the first observation`);
              } catch (error) { fail(error.message); }
            }
            await page.keyboard.press('Escape');
            for (const expected of keyboard) {
              const { panel, observation: index } = expected.target;
              const point = pointerPoints.get(`${panel}:${index}`);
              if (!point) continue;
              const canvas = await figure.locator('canvas.rv-chart__canvas').boundingBox();
              if (!canvas) { fail(`${name} chart ${chart}: canvas unavailable`); continue; }
              const x = canvas.x + point[0] * canvas.width / models[chart].geometry.width;
              const y = canvas.y + point[1] * canvas.height / models[chart].geometry.height;
              const where = `${style} ${width} ${name} chart ${chart} panel ${panel} observation ${index}`;
              try { await time.at(page, chart, x, y); } catch (error) { fail(`${where}: hover timing failed: ${error.message.split('\n')[0]}`); }
              try {
                const current = await sample(page, chart, before);
                assert.deepEqual(current.target, expected.target, `${where}: pointer selected a different mark`);
                check(current, `${where} pointer`);
                assert.ok(Math.abs(current.readout.x - expected.readout.x) <= 2 && Math.abs(current.readout.y - expected.readout.y) <= 2, `${where}: pointer and keyboard placement differs`);
              } catch (error) { fail(error.message); }
            }
            await retimeUnderLoad(page, chart, time);
            if (width === 390 && name === 'touch-point') {
              try {
                const o = models[chart].geometry.panels[0].observations[0];
                const canvas = await figure.locator('canvas.rv-chart__canvas').boundingBox();
                await page.keyboard.press('Escape');
                await page.touchscreen.tap(canvas.x + (o.at[0] + 28) * canvas.width / models[chart].geometry.width, canvas.y + o.at[1] * canvas.height / models[chart].geometry.height);
                assert.ok((await sample(page, chart, before)).target, 'touch reaches a point at 28 px');
              } catch (error) { fail(`${style} touch reach: ${error.message}`); }
            }
            if (width === 390 && name === 'types' && models[chart].geometry.kind === 'bar') {
              try {
                const panel = models[chart].geometry.panels[0], o = panel.observations.find(item => item.area);
                assert.ok(o, 'bar fixture has an inspectable mark');
                const plot = panel.plot;
                const empty = Array.from({ length: 19 * 19 }, (_, i) => [plot.x + plot.w * (1 + i % 19) / 20, plot.y + plot.h * (1 + Math.floor(i / 19)) / 20])
                  .find(([x, y]) => panel.observations.every(item => !item.area || x < item.area.x - 3 || x > item.area.x + item.area.w + 3 || y < item.area.y - 3 || y > item.area.y + item.area.h + 3));
                assert.ok(empty, 'bar fixture has empty plot');
                const canvas = await figure.locator('canvas.rv-chart__canvas').boundingBox();
                const xy = p => [canvas.x + p[0] * canvas.width / models[chart].geometry.width, canvas.y + p[1] * canvas.height / models[chart].geometry.height];
                await page.keyboard.press('Escape');
                await page.touchscreen.tap(...xy(pointFor(o)));
                assert.ok((await sample(page, chart, before)).target, 'touch selects a bar');
                await page.touchscreen.tap(...xy(empty));
                assert.equal((await sample(page, chart, before)).target, null, 'empty plot tap dismisses');
              } catch (error) { fail(`${style} touch: ${error.message}`); }
            }
            console.log(`${style} ${width} ${name}: chart ${chart + 1}/${models.length}, ${observations} readouts, ${failures.length} failures`);
          }
        }
        await page.close();
      }
    } finally { await inputs.close(); }
  }
} finally { await h.close(); }
assert.deepEqual(errors, [], 'browser errors');
for (const entry of timings.filter(entry => entry.ms.length)) {
  const { sorted, p50, p95, max, first, missed } = hoverStats(entry);
  console.log(`hover ${entry.label}: p50 ${p50.toFixed(1)} ms, p95 ${p95.toFixed(1)} ms, max ${max.toFixed(1)} ms over ${sorted.length} moves; first frame ${Math.round(first * 100)}%; load ${entry.load.toFixed(1)}${max > HOVER_P50 ? `; slowest at observation ${entry.slowest.observation}: ${entry.slowest.handler.toFixed(1)} ms in the handler, ${entry.slowest.frame.toFixed(1)} ms in the frame${entry.slowest.searched ? ', a searched place' : ''}` : ''}`);
  // A chart re-timed under load is judged by its re-time, which the next line records.
  if (missed && !entry.retimed?.ms.length) fail(`${entry.label}: hover p50 ${p50.toFixed(1)} ms (at most ${HOVER_P50}), p95 ${p95.toFixed(1)} ms (at most ${HOVER_P95}), max ${max.toFixed(1)} ms (at most ${HOVER_MAX}), first frame ${Math.round(first * 100)}% (at least ${FIRST_FRAME * 100}%)`);
}
const all = timings.flatMap(entry => entry.ms).sort((a, b) => a - b);
if (all.length) console.log(`hover over ${all.length} moves in ${timings.length} charts: p50 ${quantile(all, 0.5).toFixed(1)} ms, p95 ${quantile(all, 0.95).toFixed(1)} ms, max ${all.at(-1).toFixed(1)} ms`);
if (failures.length) process.exitCode = 1;
console.log(`labels nearer than ${LABEL_GAP} px beside a fallback: ${labelsAbutted}; phone readouts widened over a legend: ${widened}; bar readouts over other marks: ${barsOverMarks}, ${barsOverStrong} over one of its own series`);
console.log(`numbers cut: ${cutNumbers} (${Object.entries(cutWhere).map(([where, n]) => `${n} ${where}`).join(', ') || 'none'}) by ${cutReadouts.length + cutUnavoidable.length} readouts, ${cutUnavoidable.length} unavoidable; per fallback readout: ${fallbacks ? (textCovered / fallbacks).toFixed(1) : 0} px² of kept text covered, ${fallbacks ? (labelsTouched / fallbacks).toFixed(2) : 0} texts touched`);
console.log(`${observations} readouts checked; ${fallbacks} least-coverage fallbacks; ${unavoidable.length} unavoidable mark overlaps; given up: ${tiers.map(tier => `${givenUp[tier].length} ${tier}`).join(', ')}; ${dockedOverNote} phone docks over the note; ${besideTiles} beside their tile; ${occluded.length} fully occluded observations; ${coincident.length} coincident, keyboard-only observations; ${notDrawn.length} not drawn, keyboard only; ${failures.length} failures`);
