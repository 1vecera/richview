// The WebGL report runtime in a real browser (ARC-05, FEA-CH-39 to FEA-CH-56, FEA-CH-64,
// DEL-74). Every example and chart form mounts at desktop and phone widths, fills the box core
// reserved, and is read through the geometry model (`window.__richview.charts()`, DEL-72):
// pointer and keyboard readouts, legend hide and restore, Table, CSV, PNG, one Expand modal,
// zoom and print, with no script error. RICHVIEW_VERIFY_BROWSER picks the engine, so the
// engines-smoke step runs this suite in Firefox and WebKit too.
import assert from 'node:assert/strict';
import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { join } from 'node:path';
import { PNG } from 'playwright-core/lib/utilsBundle';
import { RUNTIME_WIDTHS, harness, root, watchErrors } from './browser-harness.ts';
import { FORMS, figures, reports, settle } from './report-runtime.ts';
import { nameButtonContract } from './chart-interaction-checks.ts';
import { runtimeCases } from '../tools/rv-gate/lib/runtime-cases.mts';
import type { Locator } from 'playwright';
import type { ChartModel, Point } from './report-model.ts';

interface RuntimeView {
  name: string;
  width: number;
  charts: number;
  legendSummaries?: Awaited<ReturnType<typeof histogramLegend>>;
  contextRecovery?: Awaited<ReturnType<typeof contextRecovery>>;
  readoutLayout?: Awaited<ReturnType<typeof compactReadouts>>;
  touchReadouts?: Awaited<ReturnType<typeof touchReadouts>>;
  selectionContours?: Awaited<ReturnType<typeof selectionContours>>;
  [actions: `actions${number}`]: string[];
  [zoom: `zoom${number}`]: Point;
}
interface TouchTrace {
  type: string;
  id?: number;
  x?: number;
  y?: number;
  wait?: number;
  primary?: boolean;
}
type SurfaceCorners = Record<string, number[]>;
interface Payload {
  block: { controls?: unknown[] };
  columns: string[];
  rows: unknown[];
}

declare global {
  interface Window {
    __rvRecovery: { context: null | WeakRef<WebGL2RenderingContext>; created: number };
    __rvPayloadReads: number;
  }
}


const h = await harness('report-runtime');
let inputs: Awaited<ReturnType<typeof reports>> | undefined;
let caseScope: ReturnType<typeof runtimeCases>['scope'] | undefined;
const cases: RuntimeView[] = [];
const errors: string[] = [];
const failures: string[] = [];
const completedViews = new Set();
const check = async (label: string, fn: () => unknown) => {
  try {
    await fn();
  } catch (error) {
    failures.push(`${label}: ${(error as Error).message}`);
  }
};
const near = (a: number, b: number, slack = 1.01) => Math.abs(a - b) <= slack;

/** The payload core embedded for figure `index` (TYP-36). */
const payloadOf = (page: import('playwright').Page, index: number) => page.evaluate(i => JSON.parse(document.querySelectorAll('figure[data-rv-chart]')[i].querySelector('script[data-rv-payload]')!.textContent!) as Payload, index);
/** Visible readout text, or null. */
const readoutOf = (figure: Locator) => figure.locator('.rv-chart__readout').evaluate(node => ((node as HTMLElement).hidden ? null : node.textContent));
const modelOf = (page: import('playwright').Page, index: number) => page.evaluate(i => window.__richview!.charts()[i], index);
/**
 * Whether each mounted chart is drawn on its frame's surface, the paper's in print and the
 * screen's otherwise (FEA-CH-67, STY-21). Per chart: the colour the model clears the canvas with,
 * and the canvas's corner pixels, against the frame's background. `corners` returns which corners
 * show the surface; given the screen's, those corners must show it again (a treemap or an icicle
 * covers all four, and then the model speaks alone). `off` lists each chart that is not.
 */
const surfaces = (page: import('playwright').Page, screen?: SurfaceCorners) => page.evaluate(screen => {
  const probe = document.createElement('canvas');
  probe.width = probe.height = 1;
  const pc = probe.getContext('2d', { willReadFrequently: true });
  const rgba = (color: string|CanvasGradient|CanvasPattern) => {
    pc!.clearRect(0, 0, 1, 1);
    pc!.fillStyle = color;
    pc!.fillRect(0, 0, 1, 1);
    return [...pc!.getImageData(0, 0, 1, 1).data];
  };
  const near = (a: number[], b: number[]) => a.every((v, i) => Math.abs(v - b[i]) <= 3);
  const corners: SurfaceCorners = {};
  const off = [];
  for (const model of window.__richview!.charts()) {
    const figure = document.getElementById(model.figure);
    const canvas = figure?.querySelector('.rv-chart__canvas');
    if (!(canvas as HTMLCanvasElement | HTMLImageElement | HTMLVideoElement)?.width || !(canvas as HTMLCanvasElement | HTMLImageElement | HTMLVideoElement).height || (canvas as HTMLElement).style.display === 'none') continue;
    const surface = rgba(getComputedStyle(figure!.querySelector('.rv-chart__frame')!).backgroundColor);
    const cleared = model.geometry.background.map(v => Math.round(v * 255));
    const ctx = (canvas as HTMLCanvasElement).getContext('2d');
    const [w, h] = [(canvas as HTMLCanvasElement | HTMLImageElement | HTMLVideoElement).width, (canvas as HTMLCanvasElement | HTMLImageElement | HTMLVideoElement).height];
    const pixels = [[1, 1], [w - 2, 1], [1, h - 2], [w - 2, h - 2]].map(([x, y]) => [...ctx!.getImageData(x, y, 1, 1).data]);
    corners[model.figure] = pixels.flatMap((pixel, i) => (near(pixel, surface) ? [i] : []));
    const expected = screen?.[model.figure] ?? [];
    if (!near(cleared, surface) || expected.some(i => !corners[model.figure].includes(i)))
      off.push(`${model.figure}: frame ${surface}, model ${cleared}, corners ${pixels.map(p => p.join(',')).join(' | ')}`);
  }
  return { corners, off };
}, screen);
/** A chart at the report's full width under a paragraph that sets the text column (FEA-CH-67). */
const WIDE = `---\ntitle: Wide chart\nversion: "1"\n---\n\nA paragraph sets the text column, which a chart that opts into the report's full width leaves.\n\n\`\`\`chart\n${JSON.stringify({
  type: 'line', title: 'Sales by region', wide: true, encoding: { x: 'month', y: 'sales', color: 'region' },
  data: { inline: ['2024-01', '2024-02', '2024-03', '2024-04'].flatMap((month, m) => [{ month, region: 'North', sales: 40 + m * 7 }, { month, region: 'South', sales: 30 + m * 5 }]) },
})}\n\`\`\`\n`;
/** Hand-drawn tells series apart by hatch, dash and marker; its legends must show them (FEA-CH-57). */
const LEGEND_CUES = `---\ntitle: Legend cues\nversion: "1"\nstyles: [hand-drawn]\n---\n\n${[
  { type: 'bar', title: 'Attention by kind', encoding: { x: 'pair', y: 'share', color: 'kind' },
    data: { inline: ['Google', 'Wikipedia'].flatMap((pair, p) => ['Products', 'Companies', 'Chief executives'].map((kind, k) => ({ pair, kind, share: 10 + 15 * k + 4 * p }))) } },
  { type: 'scatter', title: 'Points by group', encoding: { x: 'x', y: 'y', color: 'group' },
    data: { inline: Array.from({ length: 24 }, (_, k) => ({ x: k, y: (k * 7) % 11, group: ['North', 'South', 'East'][k % 3] })) } },
].map(chart => `\`\`\`chart\n${JSON.stringify(chart)}\n\`\`\`\n`).join('\n')}`;
/** Values equal to real series names remain text, while the outside names stay buttons. */
const NUMERIC_SERIES_NAMES = "---\ntitle: Numeric series names\nversion: \"1\"\n---\n\n```chart\n{\"type\":\"line\",\"title\":\"Numeric series names\",\"style\":{\"ratio\":1},\"axes\":{\"y\":{\"min\":-100,\"max\":100}},\"encoding\":{\"x\":\"x\",\"y\":\"v\",\"color\":\"s\"},\"data\":{\"inline\":[{\"x\":0,\"v\":12,\"s\":\"18\"},{\"x\":1,\"v\":18,\"s\":\"18\"},{\"x\":0,\"v\":42,\"s\":\"36\"},{\"x\":1,\"v\":36,\"s\":\"36\"}]}}\n```\n";
/** Safari's layout box for an A4 page: 559 by 806 points inside its margins, laid out 1.25 times wider. */
const PAPER = { width: 699, height: 1007 };
/**
 * What a browser's own print lays out (FEA-CH-67, FEA-RP-34): Safari, Chrome and Firefox fire
 * `beforeprint` while the screen's layout stands and then lay the page out for paper with no
 * script in between, so each chart stays as drawn. Dispatches `beforeprint`, freezes every canvas
 * into an image of the same size and inline style, drops the scripts and lays the copy out in
 * print media on `PAPER`. Lists each chart not drawn as wide as its printed column: wider reaches
 * past the text and, past the page, makes a browser shrink every page to fit; narrower is a chart
 * laid out while its part of the page was hidden, such as a tab not chosen.
 */
const printedAsDrawn = async (page: import('playwright').Page) => {
  const html = await page.evaluate(() => {
    window.dispatchEvent(new Event('beforeprint'));
    const drawn = [...document.querySelectorAll('.rv-chart__canvas')];
    const copy = document.documentElement.cloneNode(true);
    (copy as Element).querySelectorAll('.rv-chart__canvas').forEach((canvas, i) => {
      const image = document.createElement('img');
      image.src = (drawn[i] as HTMLCanvasElement).toDataURL();
      image.className = canvas.className;
      image.setAttribute('style', canvas.getAttribute('style') ?? '');
      canvas.replaceWith(image);
    });
    (copy as Element).querySelectorAll('script').forEach(script => script.remove());
    return `<!doctype html>${(copy as Element).outerHTML}`;
  });
  const paper = await page.context().browser()!.newPage({ viewport: PAPER });
  try {
    await paper.emulateMedia({ media: 'print' });
    await paper.setContent(html, { waitUntil: 'load' });
    return await paper.evaluate(() => [...document.querySelectorAll('figure[data-rv-chart]')].flatMap(figure => {
      const column = figure.querySelector('.rv-chart__frame')?.getBoundingClientRect().width;
      const drawn = figure.querySelector('img.rv-chart__canvas')?.getBoundingClientRect().width;
      return Math.abs(drawn! - column!) > 1 ? [`${figure.id}: drawn ${Math.round(drawn!)} px wide in a ${Math.round(column!)} px column`] : [];
    }));
  } finally {
    await paper.close();
  }
};
/** The figures whose canvas changes when the runtime redraws for arrived fonts, read after `settled()`. */
const fontsRedraw = (page: import('playwright').Page) => page.evaluate(async () => {
  const pixels = () => [...document.querySelectorAll('figure[data-rv-chart] .rv-chart__canvas')].map(c => ((c as HTMLCanvasElement | HTMLImageElement | HTMLVideoElement).width && (c as HTMLCanvasElement | HTMLImageElement | HTMLVideoElement).height ? (c as HTMLCanvasElement).toDataURL() : ''));
  const before = pixels();
  document.fonts.dispatchEvent(new Event('loadingdone'));
  await window.__richview!.settled();
  const after = pixels();
  return before.flatMap((b, i) => (b === after[i] ? [] : [i]));
});
/** Fresh chart state without transferring unconsumed geometry. */
const stateOf = (page: import('playwright').Page, index: number) => page.evaluate(i => {
  const { target, hidden, zoom, expanded, state } = window.__richview!.charts()[i];
  return { target, hidden, zoom, expanded, state };
}, index);

/** Retain a weak handle to the real runtime context without adding a production hook. */
function trackContext() {
  const get = HTMLCanvasElement.prototype.getContext, seen = new WeakSet();
  window.__rvRecovery = { context: null, created: 0 };
  HTMLCanvasElement.prototype.getContext = function (this: HTMLCanvasElement, kind: string, ...args: [options?: unknown]) {
    const context = get.call(this, kind, ...args);
    if (kind === 'webgl2' && context && !seen.has(context)) {
      seen.add(context);
      window.__rvRecovery.context = new WeakRef(context as WebGL2RenderingContext);
      window.__rvRecovery.created++;
    }
    return context;
  } as typeof get;
}

/** Restore the real context without drawing during loss and retain the exact visible frame. */
async function contextRecovery(page: import('playwright').Page, width: number) {
  const figure = page.locator('figure[data-rv-chart]').first(), frame = figure.locator('.rv-chart__frame');
  const canvas = figure.locator('canvas.rv-chart__canvas');
  const painted = () => canvas.evaluate(c => {
    const pixels = (c as HTMLCanvasElement).getContext('2d')!.getImageData(0, 0, (c as HTMLCanvasElement | HTMLImageElement | HTMLVideoElement).width, (c as HTMLCanvasElement | HTMLImageElement | HTMLVideoElement).height).data;
    return pixels.some((value, i) => value !== pixels[i % 4]);
  });
  await frame.scrollIntoViewIfNeeded();
  await page.mouse.move(1, 1);
  await frame.evaluate(node => {
    for (const key of ['+', 'Home']) node.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true }));
  });
  await settle(page, 45_000, { models: false });
  const beforeState = await stateOf(page, 0), beforeReadout = await readoutOf(figure);
  assert.ok(beforeState.zoom && beforeState.target && beforeReadout, 'zoomed chart has a selected source readout');
  assert.equal(await painted(), true, 'the scene is painted before loss');
  const before = await frame.screenshot({ animations: 'allow' });
  writeFileSync(join(h.dir, `context-before-${width}.png`), before);
  const recovery = await page.evaluate(async () => {
    const proof = window.__rvRecovery, gl = proof.context?.deref();
    if (!gl) throw new Error('Runtime WebGL context was not captured');
    const extension = gl.getExtension('WEBGL_lose_context');
    if (!extension) throw new Error('Real context-loss extension is unavailable');
    const event = (name: string) => new Promise<void>((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error(`${name} did not arrive`)), 15_000);
      gl.canvas.addEventListener(name, () => { clearTimeout(timer); resolve(); }, { once: true });
    });
    const lost = event('webglcontextlost');
    extension.loseContext();
    await lost;
    // Restoration is allowed after the lost event finishes. No input, refresh, model
    // read or animation frame here may draw while the context is lost.
    await new Promise(resolve => setTimeout(resolve, 0));
    const restored = event('webglcontextrestored');
    extension.restoreContext();
    await restored;
    return { created: proof.created, sameContext: proof.context!.deref() === gl };
  });
  await settle(page, 45_000, { models: false });
  const after = await frame.screenshot({ animations: 'allow' });
  writeFileSync(join(h.dir, `context-after-${width}.png`), after);
  const pixels = (image: Buffer<ArrayBufferLike>) => createHash('sha256').update(PNG.sync.read(image).data).digest('hex');
  assert.deepEqual(recovery, { created: 1, sameContext: true });
  assert.equal(await canvas.isVisible(), true);
  assert.deepEqual(await stateOf(page, 0), beforeState);
  assert.equal(await readoutOf(figure), beforeReadout);
  assert.equal(pixels(after), pixels(before), 'restoration retains every visible frame pixel');
  await frame.evaluate(node => node.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true })));
  await settle(page, 45_000, { models: false });
  assert.notEqual(await readoutOf(figure), beforeReadout, 'the recovered chart responds to the next source observation');
  assert.equal(await painted(), true, 'the next draw uses valid GPU resources');
  return { ...recovery, pixels: pixels(after), readout: beforeReadout, zoom: beforeState.zoom };
}
/** Touch keeps a source readout until a valid dismissal, including around chart controls. */
async function touchReadouts(url: string) {
  const page = await h.browser.newPage({ viewport: { width: 390, height: 844 }, hasTouch: true, reducedMotion: 'reduce' });
  watchErrors(page, errors);
  try {
    await page.goto(url);
    await settle(page);
    const figure = page.locator('figure[data-rv-chart]').first(), frame = figure.locator('.rv-chart__frame');
    const selected = () => stateOf(page, 0);
    const point = async () => {
      await frame.scrollIntoViewIfNeeded();
      return page.evaluate(() => {
        const m = window.__richview!.charts()[0], a = m.geometry.panels[0].observations[1].area;
        const r = document.querySelector('.rv-chart__frame')!.getBoundingClientRect();
        return { x: r.left + a!.x + a!.w / 2, y: r.top + a!.y + a!.h / 2 };
      });
    };
    const tapMark = async () => { const p = await point(); await page.touchscreen.tap(p.x, p.y); };
    const remains = async (label: string) => {
      assert.deepEqual((await selected()).target, target, label);
      assert.equal(await readoutOf(figure), text, label);
    };
    await tapMark();
    const { target } = await selected(), text = await readoutOf(figure);
    assert.ok(target && text, 'real touch selects a source observation');
    await page.waitForTimeout(450);
    await remains('lifting keeps the readout beyond the mouse exit hold');
    await tapMark();
    assert.equal((await selected()).target, null, 'a second tap dismisses');
    await tapMark();
    await frame.press('Escape');
    assert.equal(await readoutOf(figure), null, 'Escape dismisses touch selection');
    await tapMark();
    // Real taps on chart actions, the expanded title and Close remain inside one chart.
    await figure.locator('.rv-chart__action--table').tap();
    await remains('Table is inside the chart');
    await figure.locator('.rv-chart__action--table').tap();
    await figure.locator('.rv-chart__action--expand').tap();
    await page.locator('.rv-chart-dialog__title').tap();
    await remains('the expanded dialog belongs to the original chart');
    await page.locator('.rv-chart-dialog__close').tap();
    await remains('Close is inside the expanded chart');
    await point();
    // Pointer traces exercise gestures the cross-engine Touchscreen API cannot send.
    const trace = (events: TouchTrace[]) => page.evaluate(async events => {
      for (const { type, id = 91, x = 1, y = 1, wait = 0, primary = true } of events) {
        if (wait) await new Promise(resolve => setTimeout(resolve, wait));
        if (type === 'blur') { window.dispatchEvent(new Event('blur')); continue; }
        const node = document.elementFromPoint(x, y) ?? document;
        node.dispatchEvent(new PointerEvent(type, { bubbles: true, pointerType: 'touch', pointerId: id, isPrimary: primary, clientX: x, clientY: y }));
      }
    }, events);
    const down = { type: 'pointerdown' }, up = { type: 'pointerup' };
    const gestures: [string, TouchTrace[]][] = [
      ['drag', [down, { type: 'pointermove', x: 18 }, { ...up, x: 18 }]],
      ['out-and-back drag', [down, { type: 'pointermove', x: 18 }, up]],
      ['tap slop boundary', [down, { ...up, x: 9 }]],
      ['long press', [down, { ...up, wait: 550 }]],
      ['cancelled touch', [down, { type: 'pointercancel' }, up]],
      ['orphan release', [up]],
      ['multiple fingers', [down, { ...down, id: 92, primary: false }, { ...up, id: 92, primary: false }, up]],
      ['window loses focus', [down, { type: 'blur' }, up]],
    ];
    for (const [label, events] of gestures) {
      await trace(events);
      await remains(label + ' does not dismiss');
    }
    const edge = await figure.evaluate(node => { const r = node.getBoundingClientRect(); return { x: r.right, y: r.top + 2 }; });
    await trace([{ ...down, x: edge.x - 2, y: edge.y }, { ...up, x: edge.x + 2, y: edge.y }]);
    await remains('a touch beginning inside the chart does not dismiss it');
    await trace([{ ...down, x: edge.x + 2, y: edge.y }, { ...up, x: edge.x - 2, y: edge.y }]);
    await remains('a touch ending inside the chart does not dismiss it');
    await page.mouse.click(1, 1);
    await remains('mouse input retains the existing pinned behavior');
    await page.touchscreen.tap(1, 1);
    assert.equal((await selected()).target, null, 'a real tap outside the chart dismisses');
    assert.equal(await readoutOf(figure), null, 'outside dismissal hides the readout');
    await tapMark();
    await remains('a new tap still selects the same source values');
    // A roomy modal leaves a real backdrop tap target. Both copies must clear, while its
    // own controls and heading stay inside even though they lie outside the source figure.
    await page.setViewportSize({ width: 1440, height: 900 });
    await settle(page);
    await figure.locator('.rv-chart__action--expand').tap();
    const dialog = page.locator('.rv-chart-dialog'), expanded = dialog.locator('.rv-chart--expanded');
    await settle(page);
    const canvas = await expanded.locator('.rv-chart__canvas').boundingBox();
    let expandedText = null;
    for (const y of [.25, .5, .75]) {
      for (const x of [.25, .5, .75]) {
        await page.touchscreen.tap(canvas!.x + canvas!.width * x, canvas!.y + canvas!.height * y);
        expandedText = await readoutOf(expanded);
        if (expandedText) break;
      }
      if (expandedText) break;
    }
    assert.ok(expandedText, 'the expanded chart has its own selected readout');
    await dialog.locator('.rv-chart-dialog__title').tap();
    await expanded.locator('.rv-chart__action--table').tap();
    assert.equal(await readoutOf(expanded), expandedText, 'expanded controls preserve its readout');
    await remains('expanded controls preserve the covered source selection');
    const backdrop = await dialog.evaluate(node => {
      const r = node.getBoundingClientRect();
      return [{ x: 1, y: 1 }, { x: innerWidth - 1, y: innerHeight - 1 }].find(p => p.x < r.left || p.x > r.right || p.y < r.top || p.y > r.bottom);
    });
    assert.ok(backdrop, 'the roomy dialog leaves a visible backdrop');
    await page.touchscreen.tap(backdrop.x, backdrop.y);
    assert.equal(await readoutOf(expanded), null, 'a real backdrop tap clears the expanded readout');
    assert.equal(await readoutOf(figure), null, 'a real backdrop tap also clears the covered source');
    assert.equal(await dialog.evaluate(node => (node as HTMLDialogElement).open), true, 'outside dismissal leaves the modal open');
    return { target, text, gestures: gestures.map(([label]) => label), outside: 'dismissed', expandedText };
  } finally {
    await page.close();
  }
}

/** Selection retains node names and follows financial strokes rather than their hit bucket. */
async function selectionContours(page: import('playwright').Page, name: string, { input = 'keyboard' } = {}) {
  const figure = page.locator('figure[data-rv-chart]').first(), frame = figure.locator('.rv-chart__frame');
  await frame.scrollIntoViewIfNeeded();
  const model = await modelOf(page, 0), panel = model.geometry.panels[0];
  const labels = new Set(panel.items.filter((i): i is Extract<typeof i, { kind: 'text' }> => i.kind === 'text' && i.role === 'label').map(i => i.text));
  let indices = [Math.min(4, panel.observations.length - 1)];
  if (name === 'packed-bubble') {
    const k = panel.observations.findIndex(o => labels.has(o.readout.heading));
    assert.ok(k >= 0, 'the fixture has a drawn bubble name to preserve');
    indices = [k];
  } else if (name === 'sunburst') {
    const root = panel.observations[0], child = panel.observations.findIndex(o => o.series === root.series && o.wedge!.inner >= root.wedge!.radius * 1.8);
    assert.ok(child >= 0, 'the fixture has a same-series descendant beyond the inner ring');
    indices = [...(labels.has(root.readout.heading) ? [0] : []), child];
  }
  const results = [];
  for (const k of indices) {
    await page.mouse.move(1, 1);
    await frame.press('Escape');
    await settle(page);
    const regions = await page.evaluate(({ k, name, input }) => {
      const model = window.__richview!.charts()[0], panel = model.geometry.panels[0], o = panel.observations[k];
      if (name === 'sunburst' || name === 'packed-bubble') {
        const names = [o.readout.heading.split(' › ').at(-1), ...(name === 'sunburst' ? [panel.observations[0].readout.heading] : [])];
        const regions = [...new Set(names)].flatMap(name => {
          const t = panel.items.find((i): i is Extract<typeof i, { kind: 'text' }> => i.kind === 'text' && i.role === 'label' && i.text === name);
          if (!t) return []; // A name dropped for density has no painted pixels to preserve.
          const a = (t.rotate ?? 0) * Math.PI / 180, c = Math.cos(a), s = Math.sin(a), b = t.bounds;
          const corners = [[b.x, b.y], [b.x + b.w, b.y], [b.x, b.y + b.h], [b.x + b.w, b.y + b.h]]
            .map(([x, y]) => [t.x + (x - t.x) * c - (y - t.y) * s, t.y + (x - t.x) * s + (y - t.y) * c]);
          const xs = corners.map(p => p[0]), ys = corners.map(p => p[1]);
          return [{ name, box: [Math.min(...xs), Math.min(...ys), Math.max(...xs) - Math.min(...xs), Math.max(...ys) - Math.min(...ys)], unchanged: true }];
        });
        if (name === 'sunburst' && k !== 0) {
          const parent = panel.items.find((i): i is Extract<typeof i, { kind: 'area' }> => i.kind === 'area' && i.role === 'mark' && i.obs === 0);
          const points = parent!.path.cmds.flatMap(c => c.c === 'z' ? [] : c.c === 'c' ? [c.p, c.c1, c.c2] : [c.p]), xs = points.map(p => p[0]), ys = points.map(p => p[1]);
          regions.push({ name: 'unselected same-series parent', box: [Math.min(...xs), Math.min(...ys), Math.max(...xs) - Math.min(...xs), Math.max(...ys) - Math.min(...ys)], unchanged: true });
        }
        return regions;
      }
      const a = o.area;
      const lines = panel.items.filter((i): i is Extract<typeof i, { kind: 'line' }> => i.kind === 'line' && i.role === 'mark' && i.series === o.series
        && i.path.cmds.every(c => c.c === 'z' || (c.p[0] >= a!.x - .5 && c.p[0] <= a!.x + a!.w + .5 && c.p[1] >= a!.y - .5 && c.p[1] <= a!.y + a!.h + .5)));
      if (!lines.length) throw Error('the selected period has original line strokes');
      const segments = lines.map(i => i.path.cmds.flatMap(c => c.c === 'z' ? [] : [c.p]));
      const distance = ([x, y]: Point, [a, b]: Point[]) => {
        const dx = b[0] - a[0], dy = b[1] - a[1], t = Math.max(0, Math.min(1, ((x - a[0]) * dx + (y - a[1]) * dy) / (dx * dx + dy * dy || 1)));
        return Math.hypot(x - a[0] - t * dx, y - a[1] - t * dy);
      };
      const testPoints = [[a!.x, a!.y + a!.h / 2], [a!.x + a!.w, a!.y + a!.h / 2], [a!.x + a!.w / 2, a!.y], [a!.x + a!.w / 2, a!.y + a!.h]] satisfies Point[];
      const candidates = testPoints.filter(([x, y]) => x > panel.plot.x + 2 && x < panel.plot.x + panel.plot.w - 2 && y > panel.plot.y + 2 && y < panel.plot.y + panel.plot.h - 2)
        .map(p => ({ p, distance: Math.min(...segments.map(s => distance(p, s))) })).sort((a, b) => b.distance - a.distance);
      if (!candidates.length || candidates[0].distance < 6) throw Error('the hit bucket has an edge away from the true strokes');
      const [x, y] = candidates[0].p, [u, v] = segments[0], dx = v[0] - u[0], dy = v[1] - u[1], length = Math.hypot(dx, dy);
      // Probe both sides of the actual wick. Keyboard focus sits beyond the hover band.
      // Centre the sample on it so subpixel changes in chart width cannot miss the ring.
      const reach = input === 'keyboard' ? 4.5 : 1;
      const side = (n: number) => ({
        name: `feedback on side ${n} of the wick`,
        box: [(u[0] + v[0]) / 2 - dy / length * reach * n - 2, (u[1] + v[1]) / 2 + dx / length * reach * n - 2, 4, 4],
        unchanged: false,
      });
      return [
        { name: 'empty hit-bucket edge', box: [x - .5, y - .5, 1, 1], unchanged: true },
        side(-1),
        side(1),
      ];
    }, { k, name, input });
    const pixels = () => page.evaluate(regions => {
      const m = window.__richview!.charts()[0], canvas = document.querySelector('.rv-chart__canvas'), ctx = (canvas! as HTMLCanvasElement).getContext('2d'), dpr = m.canvas.dpr;
      return regions.map(({ box }) => {
        const [x, y, w, h] = box, left = Math.max(0, Math.floor(x * dpr)), top = Math.max(0, Math.floor(y * dpr));
        const width = Math.min((canvas! as HTMLCanvasElement | HTMLImageElement | HTMLVideoElement).width - left, Math.max(1, Math.ceil((x + w) * dpr) - left));
        const height = Math.min((canvas! as HTMLCanvasElement | HTMLImageElement | HTMLVideoElement).height - top, Math.max(1, Math.ceil((y + h) * dpr) - top));
        return [...ctx!.getImageData(left, top, width, height).data];
      });
    }, regions);
    const before = await pixels();
    if (input === 'keyboard') {
      await frame.press('Home');
      for (let i = 0; i < panel.observations.length && (await modelOf(page, 0)).target?.observation !== k; i++) await frame.press('ArrowRight');
    } else {
      const point = await frame.evaluate((node, k) => {
        const m = window.__richview!.charts()[0], o = m.geometry.panels[0].observations[k], r = node.getBoundingClientRect();
        const p = o.area ? [o.area.x + o.area.w / 2, o.area.y + o.area.h / 2] : o.at;
        return { x: r.left + p[0], y: r.top + p[1] };
      }, k);
      if (input === 'touch') await page.touchscreen.tap(point.x, point.y);
      else await page.mouse.move(point.x, point.y);
    }
    await settle(page);
    assert.deepEqual((await modelOf(page, 0)).target, { panel: 0, observation: k });
    const after = await pixels();
    for (let i = 0; i < regions.length; i++) {
      const same = createHash('sha256').update(Buffer.from(before[i])).digest('hex') === createHash('sha256').update(Buffer.from(after[i])).digest('hex');
      assert.equal(same, regions[i].unchanged, regions[i].name + (regions[i].unchanged ? ' stays painted exactly' : ' receives visible contour feedback'));
    }
    results.push({ k, input, regions: regions.map(({ name, unchanged }) => ({ name, unchanged })) });
    await frame.press('Escape');
  }
  return results;
}

/** A selected readout preserves every value and uses one row only when it has one value. */
async function readoutLayout(page: import('playwright').Page) {
  return page.evaluate(() => {
    const figure = document.querySelector('figure[data-rv-chart]'), model = window.__richview!.charts()[0];
    const observation = model.geometry.panels[model.target!.panel].observations[model.target!.observation];
    const readout = figure!.querySelector('.rv-chart__readout'), head = readout!.querySelector('.rv-chart__readout-head');
    const lines = [...readout!.querySelectorAll('.rv-chart__readout-line')];
    const box = (node: Element|null) => { const r = node!.getBoundingClientRect(); return { x: r.x, y: r.y, width: r.width, height: r.height }; };
    return {
      expected: observation.readout,
      heading: head!.textContent,
      lines: lines.map(line => ({ name: line.querySelector('.rv-chart__readout-name')!.textContent, value: line.querySelector('.rv-chart__readout-value')!.textContent })),
      head: box(head), rows: lines.map(box), box: box(readout), frame: box(figure!.querySelector('.rv-chart__frame')),
      hidden: (readout! as HTMLElement).hidden, horizontalOverflow: readout!.scrollWidth > readout!.clientWidth + 1,
    };
  });
}

/** Read a short single value and a waterfall's list→single→list transitions at both widths. */
async function compactReadouts(page: import('playwright').Page, name: string) {
  const frame = page.locator('figure[data-rv-chart]').first().locator('.rv-chart__frame');
  await frame.scrollIntoViewIfNeeded();
  await page.mouse.move(1, 1);
  await frame.focus();
  const count = await page.evaluate(() => window.__richview!.charts()[0].geometry.panels.reduce((n, panel) => n + panel.observations.length, 0));
  const states = [];
  try {
    for (const meaning of name === 'waterfall' ? ['multiple', 'total', 'change'] : ['single']) {
      await page.keyboard.press('Home');
      let found = false;
      for (let step = 0; step < count; step++) {
        const names = (await readoutLayout(page)).lines.map(line => line.name);
        if (meaning === 'single' || (meaning === 'multiple' && names.length > 1)
          || (meaning === 'total' && names.length === 1 && names[0] === 'Total')
          || (meaning === 'change' && names.includes('Change'))) { found = true; break; }
        await page.keyboard.press('ArrowRight');
      }
      assert.equal(found, true, 'the requested waterfall meaning is keyboard reachable in either orientation');
      const state = await readoutLayout(page);
      assert.equal(state.hidden, false);
      assert.equal(state.heading, state.expected.heading);
      assert.deepEqual(state.lines, state.expected.lines.map(({ name, value }) => ({ name, value })));
      assert.equal(state.horizontalOverflow, false);
      assert.ok(state.box.x >= state.frame.x + 7 && state.box.x + state.box.width <= state.frame.x + state.frame.width - 7, 'all values fit inside the chart');
      // One mark reads on one line: its value and at most one derived value (FEA-CH-40).
      const second = state.expected.lines[1];
      if (state.lines.length === 1 || (state.lines.length === 2 && second.derived && second.series === undefined)) {
        assert.ok(Math.abs(state.head.y - state.rows[0].y) <= 1, 'short heading and labelled value share a baseline row');
      } else {
        assert.ok(state.rows[0].y >= state.head.y + state.head.height - 1, 'multiple values retain a separate heading');
        assert.ok(state.rows.every((r, i) => i === 0 || r.y >= state.rows[i - 1].y + state.rows[i - 1].height - 1), 'multiple values retain their list');
      }
      states.push({ heading: state.heading, values: state.lines.length, box: state.box });
    }
  } finally {
    await page.keyboard.press('Escape');
  }
  return states;
}

/** Only the selected frame dimensions consumed after opening a chart. */
const figureFrameOf = (page: import('playwright').Page, index: number) => page.evaluate(i => {
  const r = document.querySelectorAll('figure[data-rv-chart]')[i].querySelector('.rv-chart__frame')!.getBoundingClientRect();
  return { frame: { width: r.width, height: r.height } };
}, index);

/** The first panel with observations, and the client point of its first observation. */
function firstObservation(model: ChartModel, frame: { x: number; y: number; width: number; height: number; }|null) {
  const panelIndex = model.geometry.panels.findIndex(p => p.observations.length);
  if (panelIndex < 0) return null;
  const obs = model.geometry.panels[panelIndex].observations[0];
  const [x, y] = obs.area ? [obs.area.x + obs.area.w / 2, obs.area.y + obs.area.h / 2] : obs.at;
  return { panelIndex, obs, point: { x: frame!.x + x, y: frame!.y + y } };
}

/**
 * The first observation with its point inside the window. A chart taller than the window can
 * stand with that point above or below it: Firefox scrolls such a frame's top out of view where
 * Chromium and WebKit leave a frame that is partly in view alone, and a pointer outside the
 * window is over nothing. The page scrolls by the point, and the frame's box is read again.
 */
async function firstObservationInView(page: import('playwright').Page, frame: Locator, model: ChartModel) {
  let target = firstObservation(model, await frame.boundingBox());
  if (!target) return null;
  const height = page.viewportSize()!.height;
  if (target.point.y < 8 || target.point.y > height - 8) {
    await page.evaluate(dy => window.scrollBy(0, dy), target.point.y - height / 2);
    target = firstObservation(model, await frame.boundingBox());
  }
  return target;
}

/**
 * Each legend swatch of a decal style draws its series' cue: the static legend before the runtime
 * and the runtime's buttons, distinct within a chart, and a hidden series keeps its swatch
 * (FEA-CH-57, FEA-CH-33).
 */
async function legendCues(page: import('playwright').Page, width: number, url: string) {
  const label = `legend-cues @${width}`;
  const html = await (await fetch(url)).text();
  const statics = [...html.matchAll(/<div class="rv-chart__legend[^"]*" role="group" aria-label="Series" data-rv-static>([\s\S]*?)<\/div>/g)];
  await check(`${label}: the static legends draw each series' cue`, () => {
    assert.ok(statics.length >= 2, `static legends: ${statics.length}`);
    for (const [, entries] of statics) {
      const swatches = [...entries.matchAll(/<span class="rv-chart__swatch[^"]*"[^>]*>([\s\S]*?)<\/span><span class="rv-chart__legend-name">/g)].map(m => m[1]);
      assert.ok(swatches.length >= 2 && swatches.every(sw => sw.startsWith('<svg class="rv-chart__decal"')), entries.slice(0, 300));
      assert.equal(new Set(swatches).size, swatches.length, 'two series share one swatch');
    }
  });
  const figures = page.locator('figure[data-rv-chart]');
  for (let i = 0; i < await figures.count(); i++) {
    const entries = figures.nth(i).locator('button.rv-chart__legend-entry');
    const swatches = () => entries.evaluateAll(list => list.map(e => e.querySelector('.rv-chart__swatch--cue > svg.rv-chart__decal')?.outerHTML ?? null));
    const before = await swatches();
    await check(`${label} #${i}: each legend button draws its series' cue`, () => {
      assert.ok(before.length >= 2, `entries: ${before.length}`);
      assert.ok(before.every(Boolean), JSON.stringify(before));
      assert.equal(new Set(before).size, before.length, 'two series share one swatch');
    });
    await entries.first().click();
    await page.evaluate(() => window.__richview!.settled());
    const hidden = await swatches();
    await check(`${label} #${i}: a hidden series keeps its swatch`, () => {
      assert.equal(hidden[0], before[0]);
      assert.deepEqual(hidden, before);
    });
    await entries.first().click();
    await page.evaluate(() => window.__richview!.settled());
    // Only rows naming a series use its legend cue; a series may instead be the heading.
    const frame = figures.nth(i).locator('.rv-chart__frame');
    await frame.scrollIntoViewIfNeeded();
    const target = await firstObservationInView(page, frame, await modelOf(page, i));
    await page.mouse.move(target!.point.x, target!.point.y);
    await page.evaluate(() => window.__richview!.settled());
    const readout = await page.evaluate((index: number) => {
      const model = window.__richview!.charts()[index];
      const observation = model.geometry.panels[model.target!.panel].observations[model.target!.observation];
      const figure = document.querySelectorAll('figure[data-rv-chart]')[index];
      return {
        heading: figure.querySelector('.rv-chart__readout-head')!.textContent,
        lines: observation.readout.lines,
        cues: [...figure.querySelectorAll('.rv-chart__readout-line')].map(row => row.querySelector('.rv-chart__cue--decal > svg.rv-chart__decal')?.outerHTML ?? null),
        series: model.geometry.series.map(series => series.name),
      };
    }, i);
    await check(`${label} #${i}: cues mark only series rows`, () => {
      assert.equal(readout.cues.length, readout.lines.length);
      for (const [index, line] of readout.lines.entries()) {
        assert.equal(readout.cues[index], line.names_series ? before[line.series!] : null, line.name);
      }
      if (i === 0) assert.ok(readout.lines.some(line => line.names_series), 'bar names a series in a row');
      else {
        assert.ok(readout.series.includes(readout.heading!), 'scatter names its series in the heading');
        assert.ok(readout.lines.every(line => !line.names_series), 'scatter has no repeated series row');
      }
    });
    await page.mouse.move(1, 1);
  }
}

/** Legend changes must update the spoken summary from the same exact tally as the bins. */
async function histogramLegend(page: import('playwright').Page, width: number) {
  const figure = page.locator('figure[data-rv-chart]').first();
  const history = [];
  for (const [toggle, hidden, counts, summary] of [
    [null, [], [3, 2, 1], '6 observations from 0 to 30; the fullest interval, 0 to 10, holds 3.'],
    ['B', ['B'], [0, 2, 1], '3 observations from 0 to 30; the fullest interval, 10 to 20, holds 2.'],
    // FEA-CH-33 keeps the last visible series; programmatic all-hidden is covered natively.
    ['A', ['B'], [0, 2, 1], '3 observations from 0 to 30; the fullest interval, 10 to 20, holds 2.'],
    ['B', [], [3, 2, 1], '6 observations from 0 to 30; the fullest interval, 0 to 10, holds 3.'],
    ['A', ['A'], [3, 0, 0], '3 observations from 0 to 30; the fullest interval, 0 to 10, holds 3.'],
    ['A', [], [3, 2, 1], '6 observations from 0 to 30; the fullest interval, 0 to 10, holds 3.'],
  ] as const) {
    if (toggle) {
      const button = figure.locator(`button.rv-chart__legend-entry[data-rv-series="${toggle}"]`);
      await button.click();
    }
    await page.evaluate(() => window.__richview!.settled());
    const m = await modelOf(page, 0);
    const record = {
      hidden: m.hidden,
      counts: m.geometry.panels.flatMap(p => p.observations).map(o => Number(o.readout.lines.find(l => l.name === 'Count')?.value)),
      summary: m.geometry.summary,
      name: await figure.locator('.rv-chart__frame').getAttribute('aria-label'),
    };
    history.push(record);
    await check(`histogram-legend @${width}: hidden ${hidden!.join(',') || 'none'}`, () => {
      assert.deepEqual([...record.hidden].sort(), hidden);
      assert.deepEqual(record.counts, counts, 'the near-edge observation keeps its exact bin');
      assert.equal(record.summary, summary, 'the summary counts only visible observations');
      assert.equal(record.name, `Values by group on fixed edges. ${summary!.replace(/\.$/, '')}. Arrow keys step through the data; Escape clears.`, 'the accessible name reads the same visible tally and retains its keyboard instructions');
    });
  }
  return history;
}

try {
  const extra = {
    forms: FORMS,
    'histogram-legend': readFileSync(new URL('../tools/rv-gate/fixtures/label-truth/regrid-hidden/report.rmd', import.meta.url), 'utf8'),
    'wide-chart': WIDE,
    'legend-cues': LEGEND_CUES,
    'numeric-series-names': NUMERIC_SERIES_NAMES,
  };
  const selected = runtimeCases(root, extra);
  caseScope = selected.scope;
  inputs = await reports(h.dir, { shard: true, ...selected.options });
  // Views are independent, so pages can take them from one queue (RICHVIEW_VERIFY_WORKERS=n); the default is serial because
  // GL pages in parallel under a loaded gate time out on the Expand modal.
  // A dispatched shard owns one context and keeps each selected report's two widths together.
  const workers = inputs.shard!.count > 1 ? 1 : Number(process.env.RICHVIEW_VERIFY_WORKERS ?? 1);
  assert.ok(Number.isSafeInteger(workers) && workers > 0, 'RICHVIEW_VERIFY_WORKERS must be a positive integer');
  const queue = inputs.names.flatMap(name => RUNTIME_WIDTHS.map(width => ({ name, width })));
  await Promise.all(Array.from({ length: workers }, async () => {
    const page = await h.browser.newPage({ viewport: { width: RUNTIME_WIDTHS[0], height: 900 }, reducedMotion: 'reduce', acceptDownloads: true });
    await page.addInitScript(trackContext);
    watchErrors(page, errors);
    for (let job = queue.shift(); job; job = queue.shift()) {
      const { name, width } = job;
      {
      await page.setViewportSize({ width, height: 900 });
      await page.goto(`${inputs!.base}/${name}/report.html`);
      const models = await settle(page);
      const boxes = await figures(page);
      const label = `${name} @${width}`;
      const item: RuntimeView = { name, width, charts: models.length };
      cases.push(item);
      // Settled charts are drawn in their style's faces (FEA-CH-30, FEA-RP-37): the runtime's redraw
      // for arrived fonts changes none. WebKit fires no `loadingdone` for faces CSS loads, so a chart
      // that mounted before its fonts kept a fallback face.
      const redrawn = await fontsRedraw(page);
      await check(`${label}: settled charts are drawn in their own faces`, () => assert.deepEqual(redrawn, [], `figures ${redrawn.join(', ')} change when the fonts redraw`));
      if (name === 'histogram-legend') item.legendSummaries = await histogramLegend(page, width);
      if (name === 'legend-cues') await legendCues(page, width, `${inputs!.base}/${name}/report.html`);
      if (name === 'wide-chart') {
        await check(`${label}: WebGL restoration retains its frame and source state`, async () => {
          item.contextRecovery = await contextRecovery(page, width);
        });
        // The context case changes zoom and selection; the remaining checks start from
        // the original report state even when the destructive case fails.
        await page.reload();
        await settle(page);
      }
      if (name === 'first-report' || name === 'waterfall') {
        await check(`${label}: readouts fit their values without losing meaning`, async () => {
          item.readoutLayout = await compactReadouts(page, name);
        });
      }
      if (name === 'first-report' && width === 390) {
        await check(`${label}: touch dismisses only on a valid outside tap`, async () => {
          item.touchReadouts = await touchReadouts(`${inputs!.base}/${name}/report.html`);
        });
      }
      if (['ohlc', 'sunburst', 'packed-bubble'].includes(name)) {
        await check(`${label}: selection keeps names and follows original mark contours`, async () => {
          item.selectionContours = await selectionContours(page, name);
        });
      }
      await check(`${label}: every figure mounts one chart`, () => assert.equal(models.length, boxes.length));
      for (const [i, box] of boxes.entries()) {
        const model = models[i];
        const at = `${label} #${i}`;
        await check(`${at}: settled`, () => assert.equal(model.settled, true));
        await check(`${at}: notice hidden once drawn`, () => assert.equal(box.notice, null));
        await check(`${at}: one canvas, no static chart`, () => assert.deepEqual([box.canvas, box.svg, box.img], [1, 0, 0]));
        // A chart in a closed tab panel has no box until its tab opens (checked below).
        if (box.frame!.width === 0) continue;
        await check(`${at}: only genuine series names become buttons`, async () => {
          assert.deepEqual((await nameButtonContract(page, i)).problems, []);
        });
        await check(`${at}: the frame holds the model's box`, () => {
          assert.ok(near(box.frame!.width, model.width), `${box.frame!.width} vs ${model.width}`);
          assert.ok(near(box.frame!.height, model.geometry.height), `${box.frame!.height} vs ${model.geometry.height}`);
        });
        await check(`${at}: the canvas has the model's pixels`, () => {
          assert.equal(model.canvas.width, Math.round(model.width * model.canvas.dpr));
          assert.equal(model.canvas.height, Math.round(model.geometry.height * model.canvas.dpr));
        });
        await check(`${at}: the frame names the chart and its summary`, () => {
          assert.ok(box.label!.startsWith(box.title ?? ''), box.label!);
          assert.ok(box.label!.includes(model.geometry.summary.split('.')[0]), `${box.label} lacks "${model.geometry.summary}"`);
        });
      }
      if (width !== RUNTIME_WIDTHS[0]) { completedViews.add(`${name}@${width}`); continue; }
      for (let i = 0; i < boxes.length; i++) {
        const at = `${label} #${i}`;
        const figure = page.locator('figure[data-rv-chart]').nth(i);
        const frame = figure.locator('.rv-chart__frame');
        // A chart in a closed tab panel is read after its tab opens (FEA-RP-30).
        const tab = await figure.evaluate(node => node.closest('[role=tabpanel]')?.id ?? null);
        if (tab) await page.locator(`[role=tab][aria-controls="${tab}"]`).click();
        await frame.scrollIntoViewIfNeeded();
        await page.evaluate(() => window.__richview!.settled());
        const model = await modelOf(page, i);
        const opened = await figureFrameOf(page, i);
        await check(`${at}: an opened chart fills its frame`, () => {
          assert.ok(near(opened.frame.width, model.width), `${opened.frame.width} vs ${model.width}`);
          assert.ok(near(opened.frame.height, model.geometry.height), `${opened.frame.height} vs ${model.geometry.height}`);
        });
        const target = await firstObservationInView(page, frame, model);
        // Pointer: the readout shows the observation under the pointer (FEA-CH-40).
        if (target) {
          await page.mouse.move(target.point.x, target.point.y);
          const hovered = await modelOf(page, i);
          const readout = await readoutOf(figure);
          await check(`${at}: pointer targets an observation`, () => assert.ok(hovered.target, 'no target'));
          if (hovered.target) {
            const obs = hovered.geometry.panels[hovered.target.panel].observations[hovered.target.observation];
            await check(`${at}: the readout shows the targeted observation`, () => assert.ok(readout?.includes(obs.readout.heading), `${readout} lacks ${obs.readout.heading}`));
          }
          await page.mouse.move(2, 2);
          // The readout holds about 300 ms after the pointer leaves (FEA-CH-42); settled waits it out.
          await page.evaluate(() => window.__richview!.settled());
          await check(`${at}: leaving clears the readout`, async () => assert.equal(await readoutOf(figure), null));
        }
        // Keyboard: Home, End and Escape step and clear, and announce (FEA-CH-55).
        await frame.focus();
        await page.keyboard.press('Home');
        const first = await modelOf(page, i);
        const live = await figure.locator('.rv-chart__live').textContent();
        if (target) {
          await check(`${at}: Home targets the first observation`, () => assert.ok(first.target));
          await check(`${at}: the target is announced`, () => assert.ok(live!.trim().length > 0, 'silent live region'));
          await page.keyboard.press('End');
          const last = await stateOf(page, i);
          const count = first.geometry.panels.reduce((n, p) => n + p.observations.length, 0);
          if (count > 1) await check(`${at}: End moves to another observation`, () => assert.notDeepEqual(last.target, first.target));
        }
        await page.keyboard.press('Escape');
        await check(`${at}: Escape clears the selection`, async () => assert.equal((await stateOf(page, i)).target, null));
        // Legend: an entry hides its series and restores it (FEA-CH-33, FEA-CH-63).
        const entries = figure.locator('button.rv-chart__legend-entry');
        // Pointing at an entry, or focusing it from the keyboard, emphasises its series. The rest
        // of the chart dims; a Sankey or an alluvial diagram outlines the series' ribbons and
        // nodes instead and dims nothing (FEA-CH-33, FEA-CH-52).
        if ((await entries.count()) > 1 && (await entries.first().getAttribute('aria-pressed')) === 'true') {
          const key = await entries.first().getAttribute('data-rv-series');
          const series = model.geometry.series.findIndex(s => s.key === key);
          const how = ['sankey', 'alluvial'].includes(model.geometry.kind) ? 'outline' : 'dim';
          const emphasised = async () => {
            await page.evaluate(() => window.__richview!.settled());
            return (await modelOf(page, i)).emphasis;
          };
          await entries.first().hover();
          await check(`${at}: pointing at a legend entry emphasises its series (${how})`, async () => assert.deepEqual(await emphasised(), { how, series }));
          await page.mouse.move(2, 2);
          await check(`${at}: leaving a legend entry ends the emphasis`, async () => assert.equal(await emphasised(), null));
          // A key press makes the next focus a keyboard focus.
          await page.keyboard.press('Shift');
          await entries.first().focus();
          await check(`${at}: focusing a legend entry from the keyboard emphasises its series (${how})`, async () => assert.deepEqual(await emphasised(), { how, series }));
          await entries.first().blur();
          await check(`${at}: leaving the focus of a legend entry ends the emphasis`, async () => assert.equal(await emphasised(), null));
        }
        if ((await entries.count()) > 1) {
          await entries.first().click();
          const hidden = await stateOf(page, i);
          await check(`${at}: a legend entry hides its series`, () => assert.equal(hidden.hidden.length, 1));
          await check(`${at}: the entry says it is off`, async () => assert.equal(await entries.first().getAttribute('aria-pressed'), 'false'));
          await entries.first().click();
          await check(`${at}: the entry restores its series`, async () => assert.deepEqual((await stateOf(page, i)).hidden, []));
        }
        // Actions: one labelled group (FEA-CH-43).
        const actions = figure.locator('.rv-chart__actions .rv-chart__action:visible');
        const labels = (await actions.allTextContents()).map(t => t.trim());
        item[`actions${i}`] = labels;
        await check(`${at}: labelled actions`, () => assert.deepEqual(labels, ['Table', 'Download PNG', 'CSV', 'Expand']));
        const payload = await payloadOf(page, i);
        // Table: every row the payload carries, at the author's precision (FEA-CH-45, TYP-36).
        await figure.locator('.rv-chart__action--table').click();
        const table = figure.locator('.rv-chart__table table');
        const rows = await table.locator('tbody tr').count();
        // With reader controls the Table lists the state on screen (FEA-BR-32); verify-controls
        // counts its rows per state.
        if (payload.block.controls?.length) {
          const status = await figure.locator('.rv-chart__table-status').textContent();
          await check(`${at}: the table names the state`, async () => assert.ok(status!.startsWith(`${(await stateOf(page, i)).state}:`), status!));
        } else {
          await check(`${at}: the table lists the rows`, () => assert.equal(rows, Math.min(payload.rows.length, 1000)));
        }
        await figure.locator('.rv-chart__action--table').click();
        await check(`${at}: Table toggles back to the chart`, async () => assert.equal(await table.isVisible().catch(() => false), false));
        // CSV: the referenced columns and every row (FEA-CH-45).
        const [csv] = await Promise.all([page.waitForEvent('download'), figure.locator('.rv-chart__action--csv').click()]);
        const text = readFileSync(await csv.path(), 'utf8').trim().split(/\r?\n/);
        await check(`${at}: CSV header`, () => assert.equal(text[0].replaceAll('"', ''), payload.columns.join(',')));
        await check(`${at}: CSV rows`, () => assert.equal(text.length - 1, payload.rows.length));
        await check(`${at}: CSV file name`, () => assert.match(csv.suggestedFilename(), /\.csv$/));
        // PNG: a real image of the chart (FEA-CH-46).
        const [png] = await Promise.all([page.waitForEvent('download'), figure.locator('.rv-chart__action--png').click()]);
        const bytes = readFileSync(await png.path());
        await check(`${at}: PNG download`, () => assert.deepEqual([...bytes.subarray(0, 4)], [0x89, 0x50, 0x4e, 0x47]));
        // Expand: one modal with the live chart; Escape closes it and focus returns (FEA-CH-44).
        await figure.locator('.rv-chart__action--expand').click();
        const dialog = page.locator('dialog.rv-chart-dialog[open]');
        await check(`${at}: Expand opens one modal`, async () => assert.equal(await dialog.count(), 1));
        await check(`${at}: the modal draws the chart`, async () => assert.equal(await dialog.locator('canvas').count(), 1));
        await check(`${at}: the model records the expanded view`, async () => assert.equal((await stateOf(page, i)).expanded, true));
        await check(`${at}: the modal has Close and no Expand`, async () => {
          assert.equal(await dialog.locator('.rv-chart__action--expand').count(), 0);
          assert.equal(await dialog.locator('.rv-chart-dialog__close').isVisible(), true);
        });
        // The pointer rests where Expand was, now over the modal's chart; WebKit targets what
        // lies under it, and Escape clears a selection before it closes the modal. Move it off.
        await page.mouse.move(1, 1);
        await page.evaluate(() => window.__richview!.settled());
        await dialog.locator('.rv-chart__frame').focus();
        await page.keyboard.press('Escape');
        await check(`${at}: Escape closes the modal`, async () => assert.equal(await page.locator('dialog.rv-chart-dialog[open]').count(), 0));
        await check(`${at}: focus returns to Expand`, async () => assert.equal(await page.evaluate(() => document.activeElement?.classList.contains('rv-chart__action--expand')), true));
        // Zoom: keys narrow a numeric or time axis; Reset zoom appears; Escape resets (FEA-CH-49).
        await frame.focus();
        // The payload script holds every row: a zoom step never reads it. The focus ring's scan
        // of the figure's text once copied the whole payload into Wasm on every step.
        await figure.evaluate(node => {
          const read = Object.getOwnPropertyDescriptor(Node.prototype, 'textContent')!.get;
          window.__rvPayloadReads = 0;
          for (const n of [node.querySelector('script[data-rv-payload]'), ...node.querySelector('script[data-rv-payload]')!.childNodes]) {
            Object.defineProperty(n, 'textContent', { configurable: true, get() { window.__rvPayloadReads++; return read!.call(this); } });
          }
        });
        await page.keyboard.press('+');
        const zoomed = await modelOf(page, i);
        const payloadReads = await figure.evaluate(node => {
          for (const n of [node.querySelector('script[data-rv-payload]'), ...node.querySelector('script[data-rv-payload]')!.childNodes]) delete (n! as { textContent?: string | null }).textContent;
          return window.__rvPayloadReads;
        });
        await check(`${at}: a zoom step does not read the payload`, () => assert.equal(payloadReads, 0));
        if (zoomed.zoom) {
          item[`zoom${i}`] = zoomed.zoom;
          await check(`${at}: Reset zoom joins the group`, async () => assert.equal(await figure.locator('.rv-chart__action--reset').isVisible(), true));
          // Pan: Shift-drag and horizontal scroll move the window and keep its width.
          const plot = zoomed.geometry.panels[0].plot;
          const box = await frame.boundingBox();
          const y = box!.y + plot.y + plot.h / 2;
          await page.keyboard.down('Shift');
          await page.mouse.move(box!.x + plot.x + plot.w * 0.7, y);
          await page.mouse.down();
          await page.mouse.move(box!.x + plot.x + plot.w * 0.4, y, { steps: 4 });
          await page.mouse.up();
          await page.keyboard.up('Shift');
          const dragged = (await stateOf(page, i)).zoom;
          const width = (w: number[]|null) => w![1] - w![0];
          await check(`${at}: Shift-drag pans`, () => {
            assert.ok(dragged && dragged[0] > zoomed.zoom![0], `${JSON.stringify(dragged)} vs ${JSON.stringify(zoomed.zoom)}`);
            assert.ok(Math.abs(width(dragged) - width(zoomed.zoom)) < width(zoomed.zoom) * 1e-6);
          });
          await page.mouse.move(box!.x + plot.x + plot.w / 2, y);
          await page.mouse.wheel(-120, 0);
          // Firefox delivers the wheel event after `wheel()` resolves; wait for the pan to land.
          let scrolled = (await stateOf(page, i)).zoom;
          for (let tries = 0; tries < 20 && scrolled && scrolled[0] === dragged![0]; tries++) {
            await page.waitForTimeout(50);
            scrolled = (await stateOf(page, i)).zoom;
          }
          await check(`${at}: horizontal scroll pans`, () => assert.ok(scrolled && scrolled[0] < dragged![0], `${JSON.stringify(scrolled)} vs ${JSON.stringify(dragged)}`));
          // A zoomed chart opens zoomed, with Reset zoom in the modal's group, and keeps the zoom
          // and the button after the modal closes (FEA-CH-43, FEA-CH-44).
          const before = (await stateOf(page, i)).zoom;
          await figure.locator('.rv-chart__action--expand').click();
          const modal = page.locator('dialog.rv-chart-dialog[open]');
          await check(`${at}: the zoomed modal offers Reset zoom`, async () => assert.equal(await modal.locator('.rv-chart__action--reset').isVisible(), true));
          await modal.locator('.rv-chart-dialog__close').click();
          await check(`${at}: the zoom and Reset zoom survive the modal`, async () => {
            assert.deepEqual((await stateOf(page, i)).zoom, before);
            assert.equal(await figure.locator('.rv-chart__action--reset').isVisible(), true);
          });
          // Leave the plot so no hovered observation takes the first Escape.
          await page.mouse.move(2, 2);
          await frame.focus();
          await page.keyboard.press('Escape');
          await check(`${at}: Escape resets the zoom`, async () => assert.equal((await stateOf(page, i)).zoom, null));
          // A scatter zooms along y on a window of its own: a rectangle narrows both axes, draws and
          // reads only the points inside, and Escape returns both (FEA-CH-49).
          if (zoomed.geometry.kind === 'scatter' && zoomed.geometry.panels[0].x?.y) {
            await check(`${at}: a rectangle zooms a scatter along both axes`, async () => {
              const whole = (await modelOf(page, i)).geometry.panels[0];
              const full = whole.x?.domain;
              const yFull = whole.x?.y?.domain;
              assert.ok(full && yFull, 'a scatter reports a window along each axis');
              const here = await frame.boundingBox();
              assert.ok(here, 'the chart has a box');
              const at = (fx: number, fy: number) => [here.x + whole.plot.x + whole.plot.w * fx, here.y + whole.plot.y + whole.plot.h * fy];
              const [x0, y0] = at(0.2, 0.25);
              const [x1, y1] = at(0.65, 0.7);
              await page.mouse.move(x0, y0);
              await page.mouse.down();
              await page.mouse.move(x1, y1, { steps: 6 });
              await page.mouse.up();
              await page.evaluate(() => window.__richview!.settled());
              const model = await modelOf(page, i);
              const panel = model.geometry.panels[0];
              const x = panel.x?.domain;
              const y = panel.x?.y?.domain;
              assert.ok(x && y, 'the zoomed chart reports both windows');
              assert.ok(model.zoom, 'the chart is zoomed');
              assert.ok(x[1] - x[0] < (full[1] - full[0]) * 0.8, `x ${x} within ${full}`);
              assert.ok(y[1] - y[0] < (yFull[1] - yFull[0]) * 0.8, `y ${y} within ${yFull}`);
              // The points read are those the dragged rectangle held, give or take the edge's pixels.
              const held = (grow: number) => whole.observations.filter(o => o.at[0] >= whole.plot.x + whole.plot.w * 0.2 - grow && o.at[0] <= whole.plot.x + whole.plot.w * 0.65 + grow && o.at[1] >= whole.plot.y + whole.plot.h * 0.25 - grow && o.at[1] <= whole.plot.y + whole.plot.h * 0.7 + grow).length;
              assert.ok(panel.observations.length >= held(-3) && panel.observations.length <= held(3), `${panel.observations.length} points read, the rectangle held ${held(-3)} to ${held(3)} of ${whole.observations.length}`);
              const inside = panel.observations.every(o => o.at[0] >= panel.plot.x - 1 && o.at[0] <= panel.plot.x + panel.plot.w + 1 && o.at[1] >= panel.plot.y - 1 && o.at[1] <= panel.plot.y + panel.plot.h + 1);
              assert.ok(inside, 'every point read stands inside the plot');
              await page.mouse.move(2, 2);
              await frame.focus();
              await page.keyboard.press('Escape');
              await page.evaluate(() => window.__richview!.settled());
              const back = await modelOf(page, i);
              assert.equal(back.zoom, null);
              assert.deepEqual(back.geometry.panels[0].x?.y?.domain, yFull);
            });
          }
        }
      }
      // Print draws every chart at twice the pixels (FEA-CH-67), on the paper's surface: a dark
      // style's chart prints in its light counterpart's theme, as the page around it does (STY-21).
      const onScreen = await surfaces(page);
      await check(`${label}: every chart is drawn on the screen's surface`, () => assert.deepEqual(onScreen.off, []));
      await check(`${label}: printed as drawn, every chart fills its column on paper`, async () => assert.deepEqual(await printedAsDrawn(page), []));
      await page.emulateMedia({ media: 'print' });
      await page.evaluate(() => window.dispatchEvent(new Event('beforeprint')));
      const printed = await page.evaluate(() => window.__richview!.charts());
      await check(`${label}: print redraws sharp`, () => assert.ok(printed.every(c => c.canvas.dpr >= 2), JSON.stringify(printed.map(c => c.canvas))));
      await check(`${label}: print draws every chart on the paper's surface`, async () => assert.deepEqual((await surfaces(page, onScreen.corners)).off, []));
      await page.emulateMedia({ media: 'screen' });
      await page.evaluate(() => window.dispatchEvent(new Event('afterprint')));
      await check(`${label}: after printing every chart is back on the screen's surface`, async () => assert.deepEqual((await surfaces(page, onScreen.corners)).off, []));
      completedViews.add(`${name}@${width}`);
      }
    }
    await page.close();
  }));
  cases.sort((a, b) => a.name.localeCompare(b.name) || b.width - a.width);
  await check('every selected report completes both widths', () => assert.deepEqual([...completedViews].sort(), inputs!.names.flatMap((name: string) => RUNTIME_WIDTHS.map(width => `${name}@${width}`)).sort()));
  await check('no script errors', () => assert.deepEqual(errors, []));
  assert.deepEqual(failures, []);
  console.log(`PASS ${cases.length} report views in ${h.engine}: mount, reserved box, pointer, keyboard, legend, Table, CSV, PNG, Expand, zoom and print`);
} finally {
  try {
    const shard = inputs?.shard;
    const completed = inputs?.names.filter(name => RUNTIME_WIDTHS.every(width => completedViews.has(`${name}@${width}`))) ?? [];
    const results = completed.map(name => ({ caseKey: name, ok: !errors.length && !failures.some(failure => failure.startsWith(`${name} @`)), views: cases.filter(view => view.name === name) }));
    const name = (shard?.count ?? 0) > 1 ? `report-runtime-${shard!.index}-of-${shard!.count}.json` : 'report-runtime.json';
    const saved = h.save(name, { engine: h.engine, browser: h.browser.version(), cases, errors, failures, shard, ...(caseScope && { case_scope: caseScope }), completed, results });
    const outAt = process.argv.indexOf('--out');
    if (outAt >= 0 && process.argv[outAt + 1]) { mkdirSync(process.argv[outAt + 1], { recursive: true }); copyFileSync(saved, join(process.argv[outAt + 1], name)); }
  } finally {
    try { await inputs?.close(); } finally { await h.close(); }
  }
}
