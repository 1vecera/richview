// Host reason (ARC-03): check readouts against the selected funnel bands in real browsers.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { harness, root, watchErrors } from './browser-harness.ts';
import { reports, settle } from './report-runtime.ts';

const fixture: { spec: Record<string, unknown>; rows: Record<string, unknown>[] } = JSON.parse(readFileSync(join(root, 'richview-chart/tests/fixtures/types/funnel.json'), 'utf8'));
const source = `---\ntitle: Funnel readouts\nstyles: [scientific]\n---\n\n\`\`\`chart\n${JSON.stringify({ ...fixture.spec, data: { inline: fixture.rows } })}\n\`\`\`\n`;
const h = await harness('funnel-readouts');
let inputs, cases = 0;
const failures: string[] = [], errors: string[] = [];
try {
  inputs = await reports(h.dir, { only: [], extra: { funnel: source } });
  for (const width of [1280, 390]) {
    const page = await h.browser.newPage({ viewport: { width, height: 900 }, reducedMotion: 'reduce' });
    watchErrors(page, errors);
    await page.goto(`${inputs.base}/funnel/report.html`);
    const [model] = await settle(page);
    const frame = page.locator('figure[data-rv-chart] .rv-chart__frame');
    await frame.scrollIntoViewIfNeeded();
    await frame.focus();
    await page.keyboard.press('Home');
    const seen = new Set();
    for (let i = 0; i < model.geometry.panels[0].observations.length + 2; i++) {
      const state = await page.evaluate(() => {
        const m = window.__richview!.charts()[0], f = document.getElementById(m.figure)!;
        const c = f.querySelector('canvas.rv-chart__canvas')!.getBoundingClientRect();
        const r = f.querySelector<HTMLElement>('.rv-chart__readout')!, b = r.getBoundingClientRect();
        const o = m.target && m.geometry.panels[m.target.panel].observations[m.target.observation];
        const sx = m.geometry.width / c.width, sy = m.geometry.height / c.height;
        return { target: m.target, heading: o?.readout.heading, area: o?.area, hidden: r.hidden, rect: [(b.left - c.left) * sx, (b.top - c.top) * sy, b.width * sx, b.height * sy] };
      });
      if (state.area && !seen.has(state.heading)) {
        seen.add(state.heading);
        try {
          assert.equal(state.hidden, false);
          const [x, y, w, h] = state.rect, a = state.area;
          assert.ok(x + w <= a.x || x >= a.x + a.w || y + h <= a.y || y >= a.y + a.h, 'readout covers the selected band');
          cases++;
        } catch (error) { failures.push(`${h.engine} ${width} ${state.heading}: ${error instanceof Error ? error.message : String(error)}`); }
      }
      await page.keyboard.press('ArrowRight');
    }
    if (seen.size !== model.geometry.panels[0].observations.length) failures.push(`${h.engine} ${width}: not every stage was selected`);
    await page.close();
  }
  assert.deepEqual(errors, []);
} finally { await inputs?.close(); await h.close(); }
if (failures.length) { console.error(failures.join('\n')); process.exit(1); }
console.log(`PASS ${cases} funnel readouts clear the selected bands`);
