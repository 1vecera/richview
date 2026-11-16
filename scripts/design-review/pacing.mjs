// Real-clock pacing: requestAnimationFrame intervals, long animation frames and dropped
// frames while a chart enters and after the pointer arrives, at stated CPU throttling.
// Numbers describe this machine and headless Chromium; compare runs on the same device.
import os from 'node:os';
import { chromiumPath, quantiles } from '../browser-harness.mjs';
import { contextOptions } from './outputs.mjs';
import { init, probe } from './probes.mjs';

export const THROTTLE = [1, 4];
export const ENTRANCE_WINDOW_MS = 1400;
export const POINTER_WINDOW_MS = 500;
const sleep = ms => new Promise(accept => setTimeout(accept, ms));
const round = value => (Number.isFinite(value) ? +value.toFixed(1) : null);

function recorder() {
  const pace = window.__pace = { frames: [], loaf: [], chart: null, states: [] };
  const loop = time => {
    pace.frames.push(time);
    // The paced chart's visible share and whether it is still waiting to enter or entering
    // (the runtime lists such charts in `pending()`; it emits no per-frame event).
    const figure = pace.chart && document.getElementById(pace.chart);
    if (figure) {
      const r = figure.getBoundingClientRect();
      pace.states.push({ t: time, visible: Math.max(0, Math.min(r.bottom, innerHeight) - Math.max(r.top, 0)) / Math.max(1, r.height), pending: !!window.__richview?.pending().includes(pace.chart) });
    }
    requestAnimationFrame(loop);
  };
  requestAnimationFrame(loop);
  if (PerformanceObserver.supportedEntryTypes.includes('long-animation-frame')) {
    new PerformanceObserver(list => { for (const e of list.getEntries()) pace.loaf.push({ start: e.startTime, duration: e.duration, blocking: e.blockingDuration }); })
      .observe({ type: 'long-animation-frame', buffered: true });
  }
}

export async function deviceFacts(browser) {
  const page = await browser.newPage();
  try {
    // chrome://gpu is not served by every headless Chromium build; the renderer string below stays.
    let features = {};
    try {
      await page.goto('chrome://gpu');
      for (let i = 0; i < 50 && !Object.keys(features).length; i++) {
        features = await page.evaluate(() => {
          const root = document.querySelector('info-view')?.shadowRoot;
          const heading = [...(root?.querySelectorAll('h3') ?? [])].find(h => h.textContent.trim().startsWith('Graphics Feature Status'));
          return Object.fromEntries([...(heading?.nextElementSibling?.querySelectorAll('li') ?? [])].map(li => li.textContent.trim().replace(/^\*\s*/, '').split(/:\s+(.*)/s).slice(0, 2)));
        });
        if (!Object.keys(features).length) await sleep(100);
      }
    } catch { /* feature status unavailable */ }
    const blank = await browser.newPage();
    await blank.goto('about:blank');
    const { webgl, interval } = await blank.evaluate(async () => {
      const gl = document.createElement('canvas').getContext('webgl');
      const info = gl?.getExtension('WEBGL_debug_renderer_info');
      const times = [];
      await new Promise(done => { const loop = t => { times.push(t); if (times.length < 61) requestAnimationFrame(loop); else done(); }; requestAnimationFrame(loop); });
      const deltas = times.slice(1).map((t, i) => t - times[i]).sort((a, b) => a - b);
      return { webgl: gl ? gl.getParameter(info ? info.UNMASKED_RENDERER_WEBGL : gl.RENDERER) : null, interval: deltas[deltas.length >> 1] };
    });
    await blank.close();
    return {
      cpu: os.cpus()[0]?.model, logical_cores: os.cpus().length, memory_gb: Math.round(os.totalmem() / 2 ** 30), os: `${os.type()} ${os.release()} ${os.arch()}`,
      node: process.version, chromium: browser.version(), executable: chromiumPath() ?? 'Playwright bundled Chromium', headless: true,
      gpu: { features, webgl }, frame_interval_ms: round(interval), load_average: os.loadavg().map(round),
    };
  } finally {
    await page.close();
  }
}

function stats({ frames, loaf }, from, to, interval) {
  const times = frames.filter(t => t >= from && t <= to);
  const deltas = times.slice(1).map((t, i) => t - times[i]);
  let missed = 0;
  for (const d of deltas) missed += Math.max(1, Math.round(d / interval)) - 1;
  const q = quantiles(deltas);
  const long = loaf.filter(e => e.start < to && e.start + e.duration > from);
  return {
    frames: deltas.length, p50_ms: round(q.p50_ms), p95_ms: round(q.p95_ms), max_ms: round(q.max_ms),
    dropped_pct: deltas.length ? round(100 * missed / (deltas.length + missed)) : null,
    long_animation_frames: long.length, loaf_max_ms: round(Math.max(0, ...long.map(e => e.duration))), loaf_blocking_ms: round(long.reduce((sum, e) => sum + (e.blocking ?? 0), 0)),
  };
}

// Load: long animation frames from navigation until the page is idle. Entrance: after the
// page is idle, the reader scrolls the chart into view at 2 px/ms; frames are measured for
// 1400 ms from its first visible frame. The chart was waiting to enter at that frame when the
// runtime still listed it as pending (`played`), and `entrance.settled_after_ms` is when it
// stopped being pending (a chart on the first screen keeps its painted state and is measured
// from the frame its canvas first had a size). Pointer: 500 ms after the pointer arrives.
export async function pace(browser, view, url, chart, rate, interval) {
  const context = await browser.newContext(contextOptions(view));
  await context.addInitScript(init);
  await context.addInitScript(recorder);
  const page = await context.newPage();
  try {
    const cdp = await context.newCDPSession(page);
    await cdp.send('Emulation.setCPUThrottlingRate', { rate });
    await page.goto(url, { waitUntil: 'load', timeout: 120000 });
    await page.evaluate(id => { window.__pace.chart = id; }, chart.id);
    await sleep(1500 * Math.min(rate, 2));
    const load = await page.evaluate(() => window.__pace.loaf.filter(e => e.start < performance.now()).map(e => e.duration));
    const top = await page.evaluate(id => document.getElementById(id).closest('figure').getBoundingClientRect().top, chart.id);
    let start;
    if (top >= view.height) {
      await page.evaluate(([top, height]) => scrollTo({ top: Math.max(0, top - height - 200), behavior: 'instant' }), [top, view.height]);
      await sleep(800);
      start = await page.evaluate(async id => {
        const figure = document.getElementById(id).closest('figure');
        const from = scrollY, r = figure.getBoundingClientRect();
        const target = r.top + scrollY - Math.max(0, (innerHeight - r.height) / 2);
        return new Promise(done => {
          let first = null;
          const t0 = performance.now();
          const step = now => {
            const y = Math.min(target, from + (now - t0) * 2);
            scrollTo({ top: y, behavior: 'instant' });
            if (first === null && figure.getBoundingClientRect().top < innerHeight) first = now;
            if (y < target) requestAnimationFrame(step); else done(first ?? now);
          };
          requestAnimationFrame(step);
        });
      }, chart.id);
    } else {
      start = await page.evaluate(id => window.__review.mounts[id]?.at ?? null, chart.id);
      if (start === null) throw new Error(`${chart.id} did not mount`);
    }
    await sleep(ENTRANCE_WINDOW_MS + 600);
    const target = await probe(page, 'target', chart.id);
    let pointerStart = null;
    if (target) {
      pointerStart = await page.evaluate(() => performance.now());
      if (view.mobile) await page.touchscreen.tap(target.x, target.y);
      else await page.mouse.move(target.x, target.y);
      await sleep(POINTER_WINDOW_MS + 100);
    }
    const data = await page.evaluate(() => window.__pace);
    const states = data.states.filter(s => s.t >= start - 50);
    const visible = states.find(s => s.visible > 0);
    const scrolled = top >= view.height;
    const played = scrolled && !!visible?.pending && states.some(s => s.t > visible.t && !s.pending);
    const settledAt = scrolled && visible?.pending ? states.find(s => s.t > visible.t && !s.pending) : null;
    return {
      first_frame_ms: round(start), scrolled: top >= view.height,
      load: { long_animation_frames: load.length, loaf_max_ms: round(Math.max(0, ...load)) },
      entrance: {
        ...stats(data, start, start + ENTRANCE_WINDOW_MS, interval), played,
        waiting_at_first_visible: scrolled && visible ? visible.pending : null, settled_after_ms: settledAt ? round(settledAt.t - visible.t) : null,
        curve: states.filter(s => s.t <= start + ENTRANCE_WINDOW_MS).map(s => [round(s.t - start), round(s.visible * 100), s.pending ? 1 : 0]),
      },
      pointer: pointerStart === null ? null : { input: view.mobile ? 'tap' : 'hover', ...stats(data, pointerStart, pointerStart + POINTER_WINDOW_MS, interval) },
    };
  } finally {
    await context.close();
  }
}
