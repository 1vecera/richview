// Host reason (ARC-03): inspect chord readouts against rendered arc geometry in real browsers.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { harness, root, watchErrors } from './browser-harness.mjs';
import { reports, settle } from './report-runtime.mjs';

const fixture = JSON.parse(readFileSync(join(root, 'richview-chart/tests/fixtures/types/chord.json'), 'utf8'));
const paragraphs = Array.from({ length: 20 }, (_, i) => `\n\nParagraph ${i + 1} gives the page room to scroll.`).join('');
const source = `---\ntitle: Chord readouts\nstyles: [scientific]\n---\n${paragraphs}\n\n\`\`\`chart\n${JSON.stringify({ ...fixture.spec, data: { inline: fixture.rows } })}\n\`\`\`\n${paragraphs}`;
// Whether a readout box, in chart coordinates, covers any pixel of the selected arc (a 2 px grid).
const coversArc = ([x, y, w, h], wedge) => {
  for (let px = x; px <= x + w; px += 2) for (let py = y; py <= y + h; py += 2) {
    const dx = px - wedge.center[0], dy = py - wedge.center[1], radius = Math.hypot(dx, dy);
    const angle = ((Math.atan2(dx, -dy) - wedge.start) % (2 * Math.PI) + 2 * Math.PI) % (2 * Math.PI);
    if (radius >= wedge.inner && radius <= wedge.radius && angle <= wedge.end - wedge.start) return true;
  }
  return false;
};
const h = await harness('chord-readouts');
let inputs;
const failures = [], errors = [];
let cases = 0;
try {
  inputs = await reports(h.dir, { only: [], extra: { chord: source } });
  for (const width of [1280, 800, 390]) {
    const page = await h.browser.newPage({ viewport: { width, height: 900 }, reducedMotion: 'reduce' });
    watchErrors(page, errors);
    await page.goto(`${inputs.base}/chord/report.html`);
    const [model] = await settle(page);
    const figure = page.locator('figure[data-rv-chart]'), frame = figure.locator('.rv-chart__frame');
    await frame.scrollIntoViewIfNeeded();
    await frame.focus();
    await page.keyboard.press('Home');
    const seen = new Set();
    for (let i = 0; i < model.geometry.panels[0].observations.length + 4; i++) {
      const state = await page.evaluate(() => {
        const m = window.__richview.charts()[0], f = document.getElementById(m.figure);
        const c = f.querySelector('canvas.rv-chart__canvas').getBoundingClientRect();
        const r = f.querySelector('.rv-chart__readout'), box = r.getBoundingClientRect();
        const observation = m.target && m.geometry.panels[m.target.panel].observations[m.target.observation];
        const sx = m.geometry.width / c.width, sy = m.geometry.height / c.height;
        const fragments = node => { const range = document.createRange(); range.selectNodeContents(node); return [...range.getClientRects()].map(b => ({ top: b.top, bottom: b.bottom })); };
        return {
          heading: observation?.readout.heading, wedge: observation?.wedge, hidden: r.hidden,
          viewport: [box.left, box.top, innerWidth - box.right, innerHeight - box.bottom],
          clipped: (() => { for (let el = r.parentElement; el; el = el.parentElement) { const style = getComputedStyle(el), rect = el.getBoundingClientRect(); if (['hidden', 'clip', 'auto', 'scroll'].includes(style.overflowX) && (box.left < rect.left + el.clientLeft - 0.5 || box.right > rect.left + el.clientLeft + el.clientWidth + 0.5)) return true; if (['hidden', 'clip', 'auto', 'scroll'].includes(style.overflowY) && (box.top < rect.top + el.clientTop - 0.5 || box.bottom > rect.top + el.clientTop + el.clientHeight + 0.5)) return true; } return false; })(),
          rect: [(box.left - c.left) * sx, (box.top - c.top) * sy, box.width * sx, box.height * sy],
          labels: m.geometry.panels[0].items.filter(item => item.text && item.bounds).map(item => ({ text: item.text, ...item.bounds })),
          lines: [...r.querySelectorAll('.rv-chart__readout-line')].map(row => ({ name: fragments(row.querySelector('.rv-chart__readout-name')), value: fragments(row.querySelector('.rv-chart__readout-value')) })),
        };
      });
      if (state.wedge && !seen.has(state.heading)) {
        seen.add(state.heading);
        try {
          assert.equal(state.hidden, false);
          assert.ok(state.viewport.every(edge => edge >= 7.5), 'readout is outside the viewport inset');
          assert.equal(state.clipped, false, 'an ancestor clips the readout');
          const [x, y, w, h] = state.rect, wedge = state.wedge;
          assert.ok(!coversArc(state.rect, wedge), 'readout covers the selected arc');
          assert.ok(state.lines.every(line => line.name.length === 1 && line.value.length === 1 && Math.abs(line.name[0].top - line.value[0].top) < 3), 'readout row wraps');
          // A phone readout may give up other arcs' labels (FEA-CH-42), never the selected arc's own.
          const kept = width === 390 ? state.labels.filter(label => label.text === state.heading) : state.labels;
          if (width === 390) assert.equal(kept.length, 1, 'selected arc label not found');
          assert.ok(kept.every(label => x >= label.x + label.w || x + w <= label.x || y >= label.y + label.h || y + h <= label.y), 'readout covers a label');
          if (width >= 800) {
            const [W, H] = [model.geometry.width, model.geometry.height];
            assert.ok(x >= 7.5 && y >= 7.5 && x + w <= W - 7.5 && y + h <= H - 7.5, 'desktop readout leaves the visible chart');
            // Where the chart leaves room beside the circle on the arc's side, the readout stands there.
            const right = Math.sin((wedge.start + wedge.end) / 2) >= 0;
            const room = right ? W - wedge.center[0] - wedge.radius : wedge.center[0] - wedge.radius;
            if (room >= w + 8) {
              if (right) assert.ok(x >= wedge.center[0] + wedge.radius, 'right arc leaves room on the right unused');
              else assert.ok(x + w <= wedge.center[0] - wedge.radius, 'left arc leaves room on the left unused');
            }
          }
          cases++;
        } catch (error) { failures.push(`${h.engine} ${width} ${state.heading}: ${error.message}`); }
      }
      await page.keyboard.press('ArrowRight');
    }
    if (seen.size !== 5) failures.push(`${h.engine} ${width}: expected five arcs, saw ${seen.size}`);
    if (width === 390) {
      // A selection at the bottom edge must stay visible without scrolling the page.
      await frame.evaluate(node => scrollBy({ top: node.getBoundingClientRect().bottom - innerHeight + 8, behavior: 'instant' }));
      await frame.focus();
      const beforeScroll = await page.evaluate(() => scrollY);
      await page.keyboard.press('Home');
      const edge = await page.evaluate(() => {
        const m = window.__richview.charts()[0], f = document.getElementById(m.figure);
        const c = f.querySelector('canvas.rv-chart__canvas').getBoundingClientRect(), r = f.querySelector('.rv-chart__readout').getBoundingClientRect();
        const o = m.target && m.geometry.panels[m.target.panel].observations[m.target.observation];
        const sx = m.geometry.width / c.width, sy = m.geometry.height / c.height;
        return { wedge: o?.wedge, rect: [(r.left - c.left) * sx, (r.top - c.top) * sy, r.width * sx, r.height * sy], readoutTop: r.top, readoutBottom: r.bottom, viewport: innerHeight, scrollY };
      });
      try {
        assert.equal(edge.scrollY, beforeScroll, 'selection scrolls the page');
        assert.ok(edge.readoutTop >= 7.5 && edge.readoutBottom <= edge.viewport - 7.5, 'readout is outside the viewport');
        assert.ok(edge.wedge && !coversArc(edge.rect, edge.wedge), 'readout covers the selected arc');
        cases++;
      } catch (error) { failures.push(`${h.engine} ${width} viewport edge: ${error.message}`); }

      // A short viewport leaves room neither above nor below the frame. The clear overlay may
      // straddle the frame, but must remain in the viewport and away from each selected arc.
      await page.setViewportSize({ width, height: 330 });
      await settle(page);
      await frame.scrollIntoViewIfNeeded();
      await frame.evaluate(node => scrollBy({ top: node.getBoundingClientRect().top - (innerHeight - node.getBoundingClientRect().height) / 2, behavior: 'instant' }));
      await frame.focus();
      await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
      const shortScroll = await page.evaluate(() => scrollY);
      await page.keyboard.press('Home');
      const shortSeen = new Set();
      for (let i = 0; i < model.geometry.panels[0].observations.length + 4; i++) {
        const short = await page.evaluate(() => {
          const m = window.__richview.charts()[0], f = document.getElementById(m.figure), c = f.querySelector('canvas.rv-chart__canvas').getBoundingClientRect(), r = f.querySelector('.rv-chart__readout').getBoundingClientRect();
          const o = m.target && m.geometry.panels[m.target.panel].observations[m.target.observation];
          return { heading: o?.readout.heading, wedge: o?.wedge, rect: [(r.left - c.left) * m.geometry.width / c.width, (r.top - c.top) * m.geometry.height / c.height, r.width * m.geometry.width / c.width, r.height * m.geometry.height / c.height], viewport: [r.left, r.top, innerWidth - r.right, innerHeight - r.bottom], scrollY };
        });
        if (short.wedge && !shortSeen.has(short.heading)) {
          shortSeen.add(short.heading);
          try {
            assert.equal(short.scrollY, shortScroll, 'selection scrolls the page');
            assert.ok(short.viewport.every(edge => edge >= 7.99), 'overlay is outside the viewport inset');
            assert.ok(!coversArc(short.rect, short.wedge), 'overlay covers the selected arc');
            cases++;
          } catch (error) { failures.push(`${h.engine} ${width} short viewport ${short.heading}: ${error.message}`); }
        }
        await page.keyboard.press('ArrowRight');
      }
      if (shortSeen.size !== 5) failures.push(`${h.engine} ${width} short viewport: expected five arcs, saw ${shortSeen.size}`);
    }
    if (width === 800) {
      // A constrained embedding cannot clip a readout that reaches outside its figure.
      await figure.evaluate(f => { f.style.overflow = 'hidden'; f.style.paddingBottom = '150px'; });
      await frame.focus();
      await page.keyboard.press('Home');
      const clip = await figure.evaluate(f => {
        const r = f.querySelector('.rv-chart__readout').getBoundingClientRect(),
          b = f.getBoundingClientRect();
        return { readout: [r.left, r.top, r.right, r.bottom], figure: [b.left, b.top, b.right, b.bottom] };
      });
      try {
        const [l, t, r, b] = clip.readout, [fl, ft, fr, fb] = clip.figure;
        assert.ok(l >= fl && t >= ft && r <= fr && b <= fb, 'clipped by the figure');
        cases++;
      } catch (error) { failures.push(`${h.engine} ${width} clipped ancestor: ${error.message}`); }
    }
    await page.close();
  }
  assert.deepEqual(errors, []);
} finally { await inputs?.close(); await h.close(); }
if (failures.length) { console.error(failures.join('\n')); process.exit(1); }
console.log(`PASS ${cases} chord arc readouts clear their arc and labels with unwrapped rows`);
