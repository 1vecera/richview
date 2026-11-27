// Host reason (ARC-03): capture real report interaction frames in an isolated browser.
// Usage: node scripts/motion-sheet.ts <report.html> <chart index> <observation index> [width] [output directory]
import assert from 'node:assert/strict';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { basename, dirname, resolve } from 'node:path';
import { chromium } from 'playwright';
import type { Browser } from 'playwright';

interface MotionProfile { durationMs: number; staggerMs: number }
interface SheetClock {
  vt: number;
  queue: FrameRequestCallback[];
  timers: Map<number, { at: number; callback: () => void }>;
  nextTimer: number;
  raf: Window['requestAnimationFrame'];
  now: Performance['now'];
}
type SheetWindow = Window & { __clock: SheetClock; __sheetRest: Uint8ClampedArray };
interface SheetFrame { label: string; pixels: string; image: string }
import { serveStatic, chromiumPath } from './browser-harness.ts';
import { launchBrowser } from '../tools/rv-gate/lib/browser-launch.mts';
import { errors } from 'playwright';

const [file, chartArg, observationArg, widthArg = '1440', outputArg = 'tmp/motion-sheet'] = process.argv.slice(2);
if (!file || chartArg === undefined || observationArg === undefined) throw new Error('Expected report.html, chart index and observation index');
const chart = Number(chartArg), observation = Number(observationArg), width = Number(widthArg);
if (![chart, observation].every(Number.isSafeInteger) || chart < 0 || observation < 0 || !Number.isSafeInteger(width) || width < 320) throw new Error('Invalid chart, observation or width');
const report = resolve(file), output = resolve(outputArg);
mkdirSync(output, { recursive: true });
const server = await serveStatic({ '/': dirname(report) });
let browser: Browser | undefined;
try {
  browser = await launchBrowser(chromium, { headless: true, executablePath: chromiumPath(), args: ['--no-sandbox', ...(process.platform === 'darwin' && process.arch === 'arm64' ? ['--enable-gpu'] : [])] }, { TimeoutError: errors.TimeoutError });
  const page = await browser!.newPage({ viewport: { width, height: 1000 }, deviceScaleFactor: 1 });
  // Place the chart below the first screen before mount, so its real entrance is observable.
  await page.route(`**/${basename(report)}`, async route => {
    const html = readFileSync(report, 'utf8').replace('</head>', '<style>body { padding-top: 1200px !important }</style></head>');
    await route.fulfill({ contentType: 'text/html', body: html });
  });
  await page.goto(`${server.base}/${basename(report)}`);
  await page.waitForFunction(k => window.__richview?.charts().length! > k, chart, { timeout: 45000 });
  const figure = page.locator('figure[data-rv-chart]').nth(chart);
  const canvas = figure.locator('canvas.rv-chart__canvas');
  await page.waitForTimeout(500); // Allow the renderer and fonts to finish mounting, while offscreen.
  const info = await page.evaluate(({ chart, observation }) => {
    const model = window.__richview!.charts()[chart], panel = model.geometry.panels[0];
    const o = panel.observations[observation];
    if (!o) throw new Error(`Observation ${observation} does not exist`);
    const figure = document.querySelectorAll('figure[data-rv-chart]')[chart];
    const payload = [...figure.querySelectorAll('script[type="application/json"]')].map(s => { try { return JSON.parse(s.textContent!) as { theme?: { motionProfile?: MotionProfile } }; } catch { return null; } }).find(x => x?.theme?.motionProfile);
    if (!payload) throw new Error('Chart motion profile is missing');
    const p = payload.theme!.motionProfile!, count = model.geometry.panels.reduce((n, panel) => n + panel.items.filter(i => i.role === 'mark').length, 0);
    const scale = Math.max(p.durationMs, 1) / 500;
    const main = ({ grow: Math.max(p.durationMs, 1) + Math.min(Math.max(count - 1, 0) * Math.max(p.staggerMs, 0), 320), sweep: 700 * scale, fan: 700 * scale, settle: 400 * scale, fade: 300 * scale })[model.geometry.entrance!];
    if (main === undefined) throw new Error('Chart has no measurable entrance');
    return { at: o.at, x: o.x, targeting: panel.targeting, count: model.geometry.panels.reduce((n, p) => n + p.observations.length, 0), duration: main + 200 * scale, entrance: model.geometry.entrance, title: figure.querySelector('.rv-chart__title')?.textContent };
  }, { chart, observation });
  // The same stepped performance/RAF clock used in verify-chart-feedback.
  await page.evaluate(() => {
    const clock: SheetClock = { vt: performance.now(), queue: [], timers: new Map(), nextTimer: 1, raf: window.requestAnimationFrame.bind(window), now: performance.now.bind(performance) };
    const sheet = window as unknown as SheetWindow;
    sheet.__clock = clock;
    window.requestAnimationFrame = callback => { clock.queue.push(callback); return clock.queue.length; };
    performance.now = () => clock.vt;
    sheet.setTimeout = (callback, delay = 0, ...args: unknown[]) => { const id = clock.nextTimer++; clock.timers.set(id, { at: clock.vt + delay, callback: () => (callback as (...args: unknown[]) => void)(...args) }); return id; };
    sheet.clearTimeout = id => clock.timers.delete(id!);
  });
  const step = (ms: number) => page.evaluate(ms => {
    const clock = (window as unknown as SheetWindow).__clock, end = clock.vt + ms;
    while (clock.vt < end - 0.0001) {
      clock.vt = Math.min(end, clock.vt + 1000 / 60);
      for (const [id, timer] of clock.timers) if (timer.at <= clock.vt) { clock.timers.delete(id); timer.callback(); }
      for (const callback of clock.queue.splice(0)) callback(clock.vt);
    }
  }, ms);
  await figure.scrollIntoViewIfNeeded();
  await page.waitForTimeout(150); // IntersectionObserver is independent of the stepped RAF.
  const captures: SheetFrame[] = [], checks: [string, boolean][] = [];
  const capture = async (label: string) => {
    const pixels = await canvas.evaluate(c => (c as HTMLCanvasElement).toDataURL());
    const image = await figure.screenshot({ timeout: 5000 });
    captures.push({ label, pixels, image: image.toString('base64') });
    return pixels;
  };
  await capture('Entrance 0%');
  for (const percent of [25, 50, 75, 100]) { await step(info.duration / 4); await capture(`Entrance ${percent}%`); }
  const entranceEnd = captures.at(-1)!.pixels;
  await step(100); // Complete the runtime's settled transition.
  const rest = await capture('Rest');
  await canvas.evaluate(node => { const c = node as HTMLCanvasElement; (window as unknown as SheetWindow).__sheetRest = c.getContext('2d')!.getImageData(0, 0, c.width, c.height).data; });
  checks.push(['Entrance 100% = rest', entranceEnd === rest]);
  const box = await canvas.boundingBox();
  await page.mouse.move(box!.x + info.at[0], box!.y + info.at[1]);
  const target = await page.evaluate(k => window.__richview!.charts()[k].target, chart);
  assert.equal(target?.observation, observation, `Pointer must select observation ${observation}`);
  const hoverZero = await capture('Hover 0 ms');
  if (hoverZero === rest) {
    checks.push(['Hover 0 ms = rest', true]);
  } else if (info.targeting === 'nearest-x') {
    const difference = await page.evaluate(({ chart, x }) => {
      const figure = document.querySelectorAll('figure[data-rv-chart]')[chart];
      const canvas = figure.querySelector<HTMLCanvasElement>('canvas.rv-chart__canvas')!, bounds = canvas.getBoundingClientRect();
      const readout = figure.querySelector<HTMLElement>('.rv-chart__readout');
      const box = readout && !readout.hidden ? readout.getBoundingClientRect() : null;
      const data = canvas.getContext('2d')!.getImageData(0, 0, canvas.width, canvas.height).data;
      const before = (window as unknown as SheetWindow).__sheetRest, scale = canvas.width / canvas.clientWidth;
      let changed = 0, outside = 0;
      for (let py = 0; py < canvas.height; py++) for (let px = 0; px < canvas.width; px++) {
        const i = (py * canvas.width + px) * 4;
        if (data[i] === before[i] && data[i + 1] === before[i + 1] && data[i + 2] === before[i + 2] && data[i + 3] === before[i + 3]) continue;
        changed++;
        const cx = px / scale, cy = py / scale;
        const onCrosshair = Math.abs(cx - x) <= 2;
        const inReadout = box && bounds.left + cx >= box.left && bounds.left + cx < box.right && bounds.top + cy >= box.top && bounds.top + cy < box.bottom;
        if (!onCrosshair && !inReadout) outside++;
      }
      return { changed, outside };
    }, { chart, x: info.x });
    checks.push([`Hover 0 differs from rest: crosshair is immediate (expected); outside column/readout: ${difference.outside} pixels`, difference.changed > 0 && difference.outside === 0]);
  } else {
    checks.push(['Hover 0 ms = rest', false]);
  }
  await step(60); await capture('Hover 60 ms');
  await step(60); await capture('Hover 120 ms');
  await canvas.dispatchEvent('pointerleave', { pointerType: 'mouse' });
  // The highlight releases over 250 ms from the leave; the readout holds 300 ms, then fades 100 ms.
  await step(60); await capture('Leave 60 ms');
  await step(65); await capture('Leave 125 ms');
  await step(125); await capture('Leave 250 ms');
  await step(100); await capture('Leave 350 ms');
  await step(200); checks.push(['Leave settled = rest', await capture('Leave settled (550 ms)') === rest]);
  const frame = figure.locator('.rv-chart__frame');
  await frame.focus();
  await page.keyboard.press('Home');
  let keyed;
  for (let i = 0; i <= info.count; i++) {
    keyed = await page.evaluate(k => window.__richview!.charts()[k].target, chart);
    if (keyed?.panel === 0 && keyed.observation === observation) break;
    await page.keyboard.press('ArrowRight');
  }
  assert.ok(keyed?.panel === 0 && keyed.observation === observation, `Keyboard must select panel 0 observation ${observation}; got ${JSON.stringify(keyed)}`);
  await step(250); await capture('Keyboard select');
  const result = await page.evaluate(async frames => {
    const images = await Promise.all(frames.map(({ image: data }) => new Promise<HTMLImageElement>((resolve, reject) => { const image = new Image(); image.onload = () => resolve(image); image.onerror = reject; image.src = `data:image/png;base64,${data}`; })));
    const w = images[0].width, h = images[0].height, columns = 3, gap = 16, label = 28;
    const sheet = document.createElement('canvas'); sheet.width = columns * w + (columns + 1) * gap; sheet.height = Math.ceil(images.length / columns) * (h + label + gap) + gap;
    const ctx = sheet.getContext('2d')!; ctx.fillStyle = '#fff'; ctx.fillRect(0, 0, sheet.width, sheet.height); ctx.fillStyle = '#111'; ctx.font = '16px sans-serif';
    images.forEach((image, i) => { const x = gap + (i % columns) * (w + gap), y = gap + Math.floor(i / columns) * (h + label + gap); ctx.fillText(frames[i].label, x, y + 18); ctx.drawImage(image, x, y + label); });
    return sheet.toDataURL('image/png').split(',')[1];
  }, captures);
  const name = `chart-${chart}-observation-${observation}-${width}.png`;
  writeFileSync(resolve(output, name), Buffer.from(result, 'base64'));
  writeFileSync(resolve(output, 'index.md'), `# Motion sheet\n\n- ${name}: ${info.title}, chart ${chart}, observation ${observation}, ${width}px; ${info.entrance} entrance ${info.duration.toFixed(1)}ms.\n- Frames: entrance 0/25/50/75/100%, rest, hover 0/60/120ms, leave 60/125/250/350ms, leave settled at 550ms, keyboard selection 250ms after the key. The highlight releases over 250ms from the leave; the readout holds 300ms, then fades 100ms.\n- Canvas pixel assertions: ${checks.map(([name, ok]) => `${name}: ${ok ? 'PASS' : 'FAIL'}`).join('; ')}.\n`);
  console.log(`Wrote ${resolve(output, name)}: ${checks.map(([name, ok]) => `${name}: ${ok ? 'PASS' : 'FAIL'}`).join('; ')}`);
  assert.ok(checks.every(([, ok]) => ok), 'One or more motion equalities failed; see index.md');
} finally {
  await browser?.close();
  await server.close();
}
