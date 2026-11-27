// Scored probes of the chart runtime's behaviour around a reader: whether charts mount without
// moving the page (mount continuity), whether a tap on a phone shows a readout that stays, and
// whether series emphasis is released when the pointer leaves. Scroll entries are paced in
// pacing.ts.
import type { Browser } from 'playwright';
import type { ReviewViewport } from './outputs.ts';
import { contextOptions } from './outputs.ts';
import { init, probe } from './probes.ts';

const sleep = (ms: number) => new Promise<void>(accept => setTimeout(accept, ms));

const heights = () => [...document.querySelectorAll('figure[data-rv-chart]')].map(figure => ({ id: figure.id, height: figure.getBoundingClientRect().height }));

// The page without JavaScript (each figure holds its notice at the final aspect ratio) against
// the live page: height change of the report's first chart when the runtime first sizes its
// canvas, and every figure's height change once `settled()` resolves. The page without
// JavaScript draws no chart, so there is no picture to compare pixels against.
export async function mountContinuity(browser: Browser, view: ReviewViewport, url: string) {
  const still = await browser.newContext(contextOptions(view, { javaScriptEnabled: false }));
  let before;
  try {
    const page = await still.newPage();
    await page.goto(url, { waitUntil: 'load', timeout: 60000 });
    await page.evaluate(() => document.fonts.ready);
    before = await page.evaluate(heights);
  } finally {
    await still.close();
  }
  if (!before[0]) return { error: 'no chart' };
  const live = await browser.newContext(contextOptions(view));
  await live.addInitScript(init);
  try {
    const page = await live.newPage();
    await page.goto(url, { waitUntil: 'load', timeout: 60000 });
    const ready = await probe(page, 'settle');
    if (!ready.charts) return { error: 'no chart' };
    const mounts = await page.evaluate(() => window.__review.mounts);
    const first = mounts[before[0].id];
    if (!first) return { error: `${before[0].id} did not mount` };
    const after = await page.evaluate(heights);
    const deltas = before.map(b => ({ id: b.id, dh: Math.round(((after.find(a => a.id === b.id)?.height ?? b.height) - b.height) * 10) / 10 }));
    return {
      chart: before[0].id, mount_ms: +first.at.toFixed(1), settled_ms: ready.settled_ms,
      first_frame: { dh: Math.round((first.height - before[0].height) * 10) / 10 },
      settled: { figures: deltas.length, summed_abs_dh: Math.round(deltas.reduce((sum, d) => sum + Math.abs(d.dh), 0) * 10) / 10, changed: deltas.filter(d => d.dh !== 0) },
    };
  } finally {
    await live.close();
  }
}

// A tap on a representative mark at phone width: the readout appears within 100 ms, is still
// there at 1500 ms, and a second tap on the same mark releases it.
export async function tap(browser: Browser, view: ReviewViewport, url: string, id: string) {
  const context = await browser.newContext(contextOptions(view));
  await context.addInitScript(init);
  try {
    const page = await context.newPage();
    await page.goto(url, { waitUntil: 'load', timeout: 60000 });
    await probe(page, 'settle');
    await probe(page, 'center', id);
    await sleep(300);
    const target = await probe(page, 'target', id);
    if (!target) return { error: 'no mark to tap' };
    const start = Date.now();
    await page.touchscreen.tap(target.x, target.y);
    const at = async (ms: number) => { await sleep(Math.max(0, ms - (Date.now() - start))); return probe(page, 'readout', id); };
    const shown = { ms100: await at(100), ms1500: await at(1500) };
    await page.touchscreen.tap(target.x, target.y);
    await sleep(700);
    const released = (await probe(page, 'readout', id)) === 0;
    return { mark: target.mark, shown, released, ok: shown.ms100 > 0 && shown.ms1500 > 0.9 && released };
  } finally {
    await context.close();
  }
}

// Emphasis from a legend entry is released when the pointer leaves it: hovering the first legend
// entry dims the other series (a visible share of the canvas changes), and moving away within
// 400 ms restores the canvas to the pixels it had before.
export async function focusRelease(browser: Browser, view: ReviewViewport, url: string, id: string) {
  const context = await browser.newContext(contextOptions(view));
  await context.addInitScript(init);
  try {
    const page = await context.newPage();
    await page.goto(url, { waitUntil: 'load', timeout: 60000 });
    await probe(page, 'settle');
    await probe(page, 'center', id);
    await sleep(300);
    const point = await page.evaluate(id => {
      const entry = document.getElementById(id)?.querySelector('.rv-chart__legend-entry[data-rv-series]');
      if (!entry) return null;
      const r = entry.getBoundingClientRect();
      return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
    }, id);
    if (!point) return { error: 'no legend entry' };
    await page.mouse.move(2, 2);
    await sleep(400);
    await probe(page, 'snapshot', id);
    await page.mouse.move(point.x, point.y, { steps: 4 });
    await sleep(400);
    const focused = await probe(page, 'differs', id);
    await page.mouse.move(2, 2, { steps: 4 });
    await sleep(500);
    const after = await probe(page, 'differs', id);
    return { via: 'legend', focused, after, ok: focused > 0.002 && after < 0.0005 };
  } finally {
    await context.close();
  }
}
