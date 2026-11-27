// Host reason (ARC-03): inspect a small-multiple chart's painted canvas in a real browser.
import assert from 'node:assert/strict';
import { harness } from './browser-harness.ts';
import { reports, settle } from './report-runtime.ts';

// A system face whose wide capitals exceed the engine's estimate. The title still fits
// its panel. Its last capitals expose a text texture that ends before the painted ink.
const title = 'WWWW WWWW WWWW WWWW';
const source = `---
title: Facet titles
styles: [air, facet-font.rstyle.yaml]
---

\`\`\`chart
type: bar
title: Sales by location
encoding: { x: month, y: sales, facet: location }
data:
  inline:
    - { month: Apr, sales: 1500, location: '${title}' }
    - { month: May, sales: 400, location: '${title}' }
    - { month: Apr, sales: 900, location: East }
    - { month: May, sales: 600, location: East }
\`\`\`
`;

const h = await harness('facet-text');
let inputs;
try {
  inputs = await reports(h.dir, {
    only: [],
    extra: { facets: source },
    files: { facets: { 'facet-font.rstyle.yaml': 'fonts:\n  mono: serif\n' } },
  });
  for (const width of [1280, 720, 390]) {
    const page = await h.browser.newPage({ viewport: { width, height: 900 }, deviceScaleFactor: 2, reducedMotion: 'reduce' });
    try {
      await page.goto(`${inputs.base}/facets/report.html`);
      await settle(page);
      const { family, panels } = await page.evaluate(() => {
        const canvas = document.querySelector<HTMLCanvasElement>('.rv-chart__canvas')!;
        const { canvas: { dpr }, geometry } = window.__richview!.charts()[0];
        const { theme }: { theme: { dataFont: string } } = JSON.parse(document.querySelector('script[data-rv-payload]')!.textContent!);
        const ctx = canvas.getContext('2d')!;
        const measure = document.createElement('canvas').getContext('2d')!;
        const panels = geometry.panels.map(panel => {
          const label = panel.items.find(item => item.kind === 'text' && item.role === 'panel-title');
          if (label?.kind !== 'text') throw new Error('The panel requires a painted facet title');
          measure.font = `${label.weight} ${label.size}px ${theme.dataFont}`;
          const ink = measure.measureText(label.text);
          const y = Math.floor((label.y - ink.actualBoundingBoxAscent) * dpr);
          const h = Math.ceil((ink.actualBoundingBoxAscent + ink.actualBoundingBoxDescent + 1) * dpr);
          const x = Math.round(label.x * dpr);
          const w = Math.floor(panel.frame.w * dpr);
          const { data } = ctx.getImageData(x, y, w, h);
          let last = 0;
          for (let i = 0; i < data.length; i += 4) {
            if (data[i + 3] > 127 && data[i] < 128) last = Math.max(last, (i / 4) % w);
          }
          return { title: label.text, panel: panel.frame.w, measured: label.bounds.w, expected: ink.actualBoundingBoxRight, painted: (last + 1) / dpr };
        });
        return { family: theme.dataFont, panels };
      });
      assert.equal(family, 'serif', 'the facet fixture must render the wide serif data face');
      assert.deepEqual(panels.map(panel => panel.title), [title, 'East']);
      assert.ok(panels[0].expected > panels[0].measured + 10, 'the wide face must exercise ink beyond the model box');
      for (const panel of panels) {
        assert.ok(panel.expected < panel.panel, `the fixture title fits in its panel at ${width}px`);
        assert.ok(Math.abs(panel.painted - panel.expected) <= 3, `${width}px: "${panel.title}" paints ${panel.painted}px of ${panel.expected}px`);
      }
    } finally {
      await page.close();
    }
  }
  console.log(`PASS facet titles paint their complete browser ink at 1280, 720 and 390 px in ${h.engine}`);
} finally {
  try { await inputs?.close(); } finally { await h.close(); }
}
