// Host reason (ARC-03): check selected wedge readouts in real browsers at report widths.
import assert from 'node:assert/strict';
import { harness, watchErrors } from './browser-harness.mjs';
import { reports, settle } from './report-runtime.mjs';

const styles = ['sleek', 'air', 'scientific', 'brutalist', 'hand-drawn'];
const data = [
  { name: 'Coding', value: 3.1 }, { name: 'Review', value: 1.4 },
  { name: 'Meetings', value: 1.8 }, { name: 'Operations', value: 0.9 },
];
const fence = chart => `\n\`\`\`chart\n${JSON.stringify(chart)}\n\`\`\`\n`;
const source = style => `---\ntitle: Wedge readouts\nstyles: [${style}]\n---\n`
  + ['pie', 'rose'].map(type => fence({ type, title: type, encoding: { x: 'name', y: 'value' }, data: { inline: data } })).join('')
  + fence({ type: 'pie', title: 'One slice', encoding: { x: 'name', y: 'value' }, data: { inline: [data[0]] } })
  + fence({ type: 'pie', title: 'Long label', encoding: { x: 'name', y: 'value' }, data: { inline: [{ name: 'Long label '.repeat(30), value: 99 }, { name: 'Other', value: 1 }] } });
const h = await harness('wedge-readouts');
let inputs;
const failures = [], errors = [];
let count = 0;
try {
  inputs = await reports(h.dir, { only: [], extra: Object.fromEntries(styles.map(style => [style, source(style)])) });
  for (const style of styles) for (const width of [1280, 390]) {
    const page = await h.browser.newPage({ viewport: { width, height: 900 }, reducedMotion: 'reduce' });
    watchErrors(page, errors);
    await page.goto(`${inputs.base}/${style}/report.html`);
    const models = await settle(page);
    for (let chart = 0; chart < models.length; chart++) {
      const figure = page.locator('figure[data-rv-chart]').nth(chart);
      const frame = figure.locator('.rv-chart__frame');
      await frame.scrollIntoViewIfNeeded();
      await frame.focus();
      await page.keyboard.press('Home');
      const n = models[chart].geometry.panels[0].observations.length;
      for (let observation = 0; observation < n; observation++) {
        const sample = await page.evaluate(i => {
          const model = window.__richview.charts()[i];
          const figure = document.getElementById(model.figure);
          const canvas = figure.querySelector('canvas.rv-chart__canvas').getBoundingClientRect();
          const readout = figure.querySelector('.rv-chart__readout');
          const box = readout.getBoundingClientRect();
          const wedge = model.target && model.geometry.panels[model.target.panel].observations[model.target.observation].wedge;
          const sx = model.geometry.width / canvas.width, sy = model.geometry.height / canvas.height;
          return { target: model.target, hidden: readout.hidden, wedge, scale: [sx, sy], origin: [canvas.left, canvas.top], rect: [(box.left - canvas.left) * sx, (box.top - canvas.top) * sy, box.width * sx, box.height * sy] };
        }, chart);
        try {
          assert.ok(sample.target && !sample.hidden && sample.wedge, JSON.stringify(sample));
          const [x, y, w, h] = sample.rect, wedge = sample.wedge;
          // The area of the selected wedge a box of the readout's size at (bx, by) covers, sampled
          // every `step` chart pixels.
          const covered = (bx, by, step) => {
            let n = 0;
            for (let px = bx; px <= bx + w; px += step) for (let py = by; py <= by + h; py += step) {
              const dx = px - wedge.center[0], dy = py - wedge.center[1];
              const angle = ((Math.atan2(dx, -dy) - wedge.start) % (2 * Math.PI) + 2 * Math.PI) % (2 * Math.PI);
              const radius = Math.hypot(dx, dy);
              if (radius >= wedge.inner && radius <= wedge.radius && angle <= wedge.end - wedge.start) n++;
            }
            return n * step * step;
          };
          const label = `${style} ${width} ${models[chart].geometry.kind} observation ${observation}`;
          if (chart !== 3 || observation !== 0) assert.equal(covered(x, y, 2), 0, `${label} covers its wedge`);
          else {
            // The 99% slice fills the canvas, so no place clears it (FEA-CH-42; Chart design's
            // ruling of 14 Nov). The readout stays in the figure at its natural height, clear of
            // the title, the toolbar and any key, and no place it may take covers less of the slice.
            const room = await figure.evaluate(f => {
              const box = b => ({ x: b.x, y: b.y, w: b.width, h: b.height });
              const canvas = f.querySelector('canvas.rv-chart__canvas').getBoundingClientRect(), whole = f.getBoundingClientRect();
              const r = f.querySelector('.rv-chart__readout');
              const inset = b => {
                const left = Math.max(b.left, whole.left, 0) + 8, top = Math.max(b.top, whole.top, 0) + 8;
                const right = Math.min(b.right, whole.right, innerWidth) - 8, bottom = Math.min(b.bottom, whole.bottom, innerHeight) - 8;
                return { x: left, y: top, w: right - left, h: bottom - top };
              };
              const shown = selector => [...f.querySelectorAll(selector)].map(e => box(e.getBoundingClientRect())).filter(b => b.w > 0 && b.h > 0);
              // A key drawn in the chart counts as a legend the readout keeps clear (FEA-CH-42).
              const model = window.__richview.charts().find(m => m.figure === f.id);
              const sx = canvas.width / model.geometry.width, sy = canvas.height / model.geometry.height;
              const keys = [...model.geometry.panels.flatMap(p => p.items), ...(model.geometry.figure_items ?? [])].filter(item => item.role === 'key').map(item => {
                const b = item.kind === 'text' ? item.bounds : item.kind === 'rect' ? item.rect : null;
                return b && { x: canvas.left + b.x * sx, y: canvas.top + b.y * sy, w: b.w * sx, h: b.h * sy };
              }).filter(Boolean);
              // Where a readout may sit, 8 px in: the visible figure on a narrow chart, the visible
              // canvas otherwise (f614f4ec).
              return { room: inset(canvas.width < 480 ? whole : canvas), readout: box(r.getBoundingClientRect()),
                head: shown('.rv-chart__title, .rv-chart__actions'), legend: shown('.rv-chart__legend'), keys, scroll: r.scrollHeight, client: r.clientHeight };
            });
            const meets = (a, b) => a.x < b.x + b.w - 0.02 && b.x < a.x + a.w - 0.02 && a.y < b.y + b.h - 0.02 && b.y < a.y + a.h - 0.02;
            const r = room.readout, a = room.room;
            assert.ok(r.x >= a.x - 0.5 && r.y >= a.y - 0.5 && r.x + r.w <= a.x + a.w + 0.5 && r.y + r.h <= a.y + a.h + 0.5,
              `${label}: the long readout leaves its room (${JSON.stringify(r)} in ${JSON.stringify(a)})`);
            assert.ok(room.scroll <= room.client + 1 || room.client >= a.h - 1, `${label}: the long readout is shorter than its content`);
            const met = [...room.head, ...room.keys].find(text => meets(r, text));
            assert.ok(!met, `${label}: the long readout covers the title, the toolbar or a key (${JSON.stringify(met)}; readout ${JSON.stringify(r)})`);
            // Every place of the same size it may take that keeps the title, the toolbar, the legend
            // and any key clear, on a 4 px grid; the runtime's choice may trail the best by 2% of its area.
            const [sx, sy] = sample.scale, [ox, oy] = sample.origin, kept = [...room.head, ...room.legend, ...room.keys];
            let best = Infinity;
            for (let px = a.x; px + r.w <= a.x + a.w + 0.5; px += 4) for (let py = a.y; py + r.h <= a.y + a.h + 0.5; py += 4) {
              if (kept.some(text => meets({ x: px, y: py, w: r.w, h: r.h }, text))) continue;
              best = Math.min(best, covered((px - ox) * sx, (py - oy) * sy, 4));
            }
            const own = covered(x, y, 4);
            assert.ok(own <= best + 0.02 * w * h, `${label}: the long readout covers ${own.toFixed(0)} px² of its slice where ${best.toFixed(0)} px² was possible`);
            if (room.scroll > room.client + 1) {
              await page.keyboard.press('PageDown');
              assert.ok(await figure.locator('.rv-chart__readout').evaluate(e => e.scrollTop) > 0, `${label}: Page Down scrolls the long readout`);
            }
          }
          count++;
        } catch (error) { failures.push(error.message); }
        await page.keyboard.press('ArrowRight');
      }
    }
    await page.close();
  }
  assert.deepEqual(errors, []);
} finally { await inputs?.close(); await h.close(); }
if (failures.length) { console.error(failures.join('\n')); process.exit(1); }
console.log(`PASS ${count} wedge readouts clear the selected mark, or cover least of a slice no place clears`);
