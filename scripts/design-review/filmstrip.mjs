// Motion frames of a chart canvas on the real clock. The runtime emits no per-frame event, so
// the page reads the chart's canvas on every animation frame (`__review.film`) and keeps, for
// each time of interest, the first frame at or after it with its actual time. Frames are the
// canvas only, taken straight from the page, so they do not depend on the screenshot rate or
// on how much of the figure is in view. A chart enters when half of it is visible, so a chart
// below the first screen is scrolled to the middle of the viewport: time 0 is that moment,
// within a frame of the entrance starting. A chart on the first screen keeps its painted state.
import { PNG } from 'playwright-core/lib/utilsBundle';
import { watchErrors } from '../browser-harness.mjs';
import { contextOptions } from './outputs.mjs';
import { init, probe } from './probes.mjs';

export const ENTRANCE_MS = [0, 80, 160, 240, 360, 480, 640, 800, 1000, 1400];
export const POINTER_MS = [0, 50, 100, 150, 250, 400];
export const SETTLED_MS = 3000;
const sleep = ms => new Promise(accept => setTimeout(accept, ms));
const png = base64 => Buffer.from(base64, 'base64');

async function livePage(browser, view, url, options) {
  const context = await browser.newContext(contextOptions(view, options));
  await context.addInitScript(init);
  const page = await context.newPage();
  const errors = [];
  watchErrors(page, errors);
  await page.goto(url, { waitUntil: 'load', timeout: 60000 });
  // The runtime starts after the module and Wasm load; its charts are mounted at once.
  await page.waitForFunction(() => document.fonts.status === 'loaded' && (!!window.__richview || !document.querySelector('figure[data-rv-chart]')), null, { timeout: 60000 });
  return { context, page, errors };
}
// Wait until a chart has finished its entrance and any glide, without finishing it early.
async function entered(page, id, timeout = 8000) {
  for (const end = Date.now() + timeout; Date.now() < end; await sleep(50)) {
    if (!(await probe(page, 'pending')).includes(id)) { await sleep(150); return true; }
  }
  return false;
}

// Entrance frames, the settled frame and a pointer sequence on one representative mark.
// Phones get a tap; wider viewports a mouse hover.
export async function chartMotion(browser, view, url, chart) {
  const { context, page, errors } = await livePage(browser, view, url);
  try {
    const waiting = (await probe(page, 'pending')).includes(chart.id);
    const entrance = (await page.evaluate(([id, targets]) => { window.__review.center(id); return window.__review.film(id, targets, 'now'); }, [chart.id, ENTRANCE_MS]))
      .map(f => ({ t: f.t, target: f.target, readout: f.readout, png: png(f.png) }));
    if (entrance.length < ENTRANCE_MS.length) throw new Error(`${chart.id}: ${entrance.length} of ${ENTRANCE_MS.length} entrance frames`);
    if (!(await entered(page, chart.id))) throw new Error(`${chart.id} did not settle`);
    const settled = png(await probe(page, 'canvasPng', chart.id));
    const target = await probe(page, 'target', chart.id);
    const pointer = [];
    if (target) {
      const frames = page.evaluate(([id, targets]) => window.__review.film(id, targets, 'input'), [chart.id, POINTER_MS]);
      if (view.mobile) await page.touchscreen.tap(target.x, target.y);
      else await page.mouse.move(target.x, target.y);
      for (const f of await frames) pointer.push({ t: f.t, target: f.target, readout: f.readout, png: png(f.png) });
    }
    return { entrance, pointer, settled, entered_while_waiting: waiting, target: target && { ...target, input: view.mobile ? 'tap' : 'hover' }, errors };
  } finally {
    await context.close();
  }
}

// With prefers-reduced-motion, the first drawn frame of the page top and of each chart must
// equal its settled frame: nothing is waiting to enter, and nothing moves.
export async function reducedMotion(browser, view, url, charts) {
  const { context, page, errors } = await livePage(browser, view, url, { reducedMotion: 'reduce' });
  try {
    await sleep(100);
    const top = await page.screenshot();
    const first = {}, failed = {};
    for (const chart of charts) {
      try {
        await probe(page, 'center', chart.id);
        await page.evaluate(() => new Promise(accept => requestAnimationFrame(() => requestAnimationFrame(accept))));
        first[chart.id] = png(await probe(page, 'canvasPng', chart.id));
      } catch (error) { failed[chart.id] = error.message.split('\n')[0]; }
    }
    await sleep(1500);
    const result = {};
    for (const chart of charts) {
      let settled = null;
      try { await probe(page, 'center', chart.id); await sleep(40); settled = png(await probe(page, 'canvasPng', chart.id)); } catch (error) { failed[chart.id] ??= error.message.split('\n')[0]; }
      result[chart.id] = { settled, ...compare(first[chart.id], settled), ...(failed[chart.id] && { error: failed[chart.id] }) };
    }
    await page.evaluate(() => scrollTo({ top: 0, behavior: 'instant' }));
    await sleep(100);
    return { charts: result, page_top: compare(top, await page.screenshot()), errors };
  } finally {
    await context.close();
  }
}

// Pixels differ when any channel differs by more than two levels; smaller differences
// come from compositing a layer that holds a finished animation's fill.
export function compare(a, b) {
  if (!a || !b) return { equal: false, diff_pixels: null };
  if (a.equals(b)) return { equal: true, diff_pixels: 0, diff_ratio: 0, max_delta: 0 };
  const x = PNG.sync.read(a), y = PNG.sync.read(b);
  if (x.width !== y.width || x.height !== y.height) return { equal: false, diff_pixels: null, sizes: [[x.width, x.height], [y.width, y.height]] };
  let diff = 0, max = 0;
  for (let i = 0; i < x.data.length; i += 4) {
    const delta = Math.max(Math.abs(x.data[i] - y.data[i]), Math.abs(x.data[i + 1] - y.data[i + 1]), Math.abs(x.data[i + 2] - y.data[i + 2]), Math.abs(x.data[i + 3] - y.data[i + 3]));
    max = Math.max(max, delta);
    if (delta > 2) diff++;
  }
  return { equal: diff === 0, diff_pixels: diff, diff_ratio: +(diff / (x.width * x.height)).toFixed(5), max_delta: max };
}
