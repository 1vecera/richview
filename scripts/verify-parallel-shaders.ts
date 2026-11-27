// Host reason (ARC-03): exercise report WebGL shader readiness in isolated real browsers.
// Run after richview-build: RICHVIEW_VERIFY_BROWSER=chromium node scripts/verify-parallel-shaders.ts
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { chromium, firefox, webkit, errors, type Browser, type Download } from 'playwright';
import { PNG } from 'playwright-core/lib/utilsBundle';
import { chromiumPath, root, waitForBenchmark, watchErrors } from './browser-harness.ts';
import { reports } from './report-runtime.ts';
import { launchBrowser } from '../tools/rv-gate/lib/browser-launch.mts';

type ShaderMode = 'pending' | 'fallback' | 'failure' | 'no-frames';
interface ShaderState {
  mode: ShaderMode;
  released: boolean;
  events: string[];
  context: WebGL2RenderingContext | null;
  contexts: WebGL2RenderingContext[];
  pollContexts: WebGL2RenderingContext[];
  fail: boolean;
  blockedFrames?: number;
}
type DrawImageArgs = [CanvasImageSource, ...number[]];

declare global {
  interface Window {
    __rvShader: ShaderState;
    __lostShaderContext: WebGL2RenderingContext;
    __lostShaderExtension: WEBGL_lose_context;
    __replacementShaderContext: WebGL2RenderingContext;
    __readyLostContext: WebGL2RenderingContext;
    __shaderSettleProbe: { done: boolean; error: null | string };
    __rvViewports: { width: number; height: number; canvasHeight: number; }[];
    __rvFirstLine: null | string;
    __rvGrowthViewports: { w: number; h: number; canvasHeight: number; }[];
    __shaderSettled: boolean;
    __releaseShaderFonts: () => void;
    __blockedFrameRan: boolean;
    __noRestoreSettled: boolean;
    __readyLossSettled: boolean;
  }
}


const chart = { type: 'line', title: 'Shader readiness', encoding: { x: 'x', y: 'y' }, data: { inline: [{ x: 0, y: 1 }, { x: 1, y: 4 }, { x: 2, y: 2 }] } };
const source = `---\ntitle: Shader readiness\nversion: "1"\n---\n\n\`\`\`chart\n${JSON.stringify(chart)}\n\`\`\`\n`;
const hatchSource = `---\ntitle: Hatch stability\nversion: "1"\nstyles: [hand-drawn]\n---\n\n${[
  { type: 'bar', title: 'Attention by kind', encoding: { x: 'pair', y: 'share', color: 'kind' },
    data: { inline: ['Google', 'Wikipedia'].flatMap((pair, p) => ['Products', 'Companies', 'Chief executives'].map((kind, k) => ({ pair, kind, share: 10 + 15 * k + 4 * p }))) } },
  { type: 'scatter', title: 'Points by group', encoding: { x: 'x', y: 'y', color: 'group' },
    data: { inline: Array.from({ length: 24 }, (_, k) => ({ x: k, y: (k * 7) % 11, group: ['North', 'South', 'East'][k % 3] })) } },
].map(value => `\`\`\`chart\n${JSON.stringify(value)}\n\`\`\`\n`).join('\n')}`;
// Keep a real unhatched line and a taller chart after it: the pair that exposed viewport
// rasterization drift. Every flagship chart is 16:9 now, so on a desktop none is taller than the
// line. The grouped bars stand in for the 4:3 heatmap the flagship lost, at that heatmap's ratio.
// If the flagship titles change, update these selectors to the corresponding charts.
const flagship = readFileSync(join(root, 'examples/openai-vs-anthropic/report.rmd'), 'utf8');
const flagshipCharts = [...flagship.matchAll(/^```chart\n[\s\S]*?^```$/gm)].map(match => match[0]);
const viewportLine = flagshipCharts.find(value => value.includes('Developer talk flipped to Anthropic in mid-2025'));
const viewportBars = flagshipCharts.find(value => value.includes('Anthropic draws complaints about reliability'));
assert.ok(viewportLine && viewportBars, 'the named flagship charts exist');
const viewportTall = viewportBars.replace("style: {ratio: '16:9'}", "style: {ratio: '4:3'}");
assert.notEqual(viewportTall, viewportBars, 'the grouped bars take the 4:3 ratio');
const viewportSource = `---\ntitle: Viewport stability\nstyles: [scientific, report.rstyle.yaml]\n---\n\n${viewportLine}\n\n${viewportTall}\n`;
const viewportStyle = readFileSync(join(root, 'examples/openai-vs-anthropic/report.rstyle.yaml'), 'utf8');

// The extension is simulated even on drivers without it. Every other WebGL call still reaches
// the real context. Holding completion makes a premature blocking status read observable.
function instrument(mode: ShaderMode) {
  const gl = WebGL2RenderingContext.prototype;
  const { getExtension, getProgramParameter, getShaderParameter, getUniformLocation, compileShader, linkProgram, getProgramInfoLog, drawArrays } = gl;
  const getContext = HTMLCanvasElement.prototype.getContext;
  const state: ShaderState = window.__rvShader = { mode, released: mode === 'fallback', events: [], context: null, contexts: [], pollContexts: [], fail: mode === 'failure' };
  if (mode === 'no-frames') {
    const frame = window.requestAnimationFrame.bind(window);
    let blocked = 0;
    // Drop frames requested during shader compilation, as a hidden document can. Once released,
    // new frames work, but callbacks dropped while pending can never fire retroactively.
    window.requestAnimationFrame = callback => {
      if (state.released) return frame(callback);
      state.blockedFrames = ++blocked;
      return 2_000_000_000 - blocked;
    };
    // Initial layout runs synchronously; suppress ResizeObserver notifications in this case so
    // its separate rAF-based relayout cannot mask whether shader polling has a timer fallback.
    window.ResizeObserver = class { observe() {} unobserve() {} disconnect() {} };
  }
  HTMLCanvasElement.prototype.getContext = function (this: HTMLCanvasElement, kind: string, ...args: [options?: unknown]) {
    const context = getContext.call(this, kind, ...args);
    if (kind === 'webgl2' && context) {
      const webgl = context as WebGL2RenderingContext;
      state.context = webgl;
      if (!state.contexts.includes(webgl)) state.contexts.push(webgl);
    }
    return context;
  } as typeof getContext;
  gl.getExtension = function (this: WebGL2RenderingContext, name: string) {
    if (name === 'KHR_parallel_shader_compile') {
      state.events.push('extension');
      return mode === 'fallback' ? null : { COMPLETION_STATUS_KHR: 0x91b1 };
    }
    return getExtension.call(this, name);
  } as typeof getExtension;
  gl.compileShader = function (shader) { state.events.push('compile'); return compileShader.call(this, shader); };
  gl.linkProgram = function (program) { state.events.push('link'); return linkProgram.call(this, program); };
  gl.drawArrays = function (...args) { if (!state.released) state.events.push('early-draw'); return drawArrays.apply(this, args); };
  gl.getShaderParameter = function (shader, name) {
    if (name === this.COMPILE_STATUS) state.events.push(state.released ? 'shader-status' : 'early-shader-status');
    return getShaderParameter.call(this, shader, name);
  };
  gl.getProgramParameter = function (program, name) {
    if (name === 0x91b1) { state.events.push('poll'); state.pollContexts.push(this); return this.isContextLost() || state.released; }
    if (name === this.LINK_STATUS) {
      state.events.push(state.released ? 'link-status' : 'early-link-status');
      if (state.fail && state.released) return false;
    }
    return getProgramParameter.call(this, program, name);
  };
  gl.getUniformLocation = function (program, name) {
    state.events.push(state.released ? 'uniform' : 'early-uniform');
    return getUniformLocation.call(this, program, name);
  };
  gl.getProgramInfoLog = function (program) {
    return state.fail ? 'Injected link failure' : getProgramInfoLog.call(this, program);
  };
}

async function open(browser: import('playwright').Browser, url: string, mode: ShaderMode, faults?: string[]) {
  const context = await browser.newContext({ viewport: { width: 1000, height: 800 }, reducedMotion: 'reduce', acceptDownloads: true });
  const page = await context.newPage();
  if (faults) watchErrors(page, faults);
  await page.addInitScript(instrument, mode);
  await page.goto(url, { waitUntil: 'domcontentloaded' });
  await page.waitForFunction(() => window.__richview && window.__rvShader.context, null, { timeout: 60_000, polling: 50 });
  return { context, page };
}

const noticeVisible = (page: import('playwright').Page) => page.locator('figure[data-rv-chart] .rv-chart__notice').evaluateAll(nodes => nodes.some(node => !(node as HTMLElement).hidden && getComputedStyle(node).display !== 'none'));
async function painted(page: import('playwright').Page) {
  return page.locator('figure[data-rv-chart] canvas.rv-chart__canvas').first().evaluate(c => {
    if (!(c as HTMLCanvasElement | HTMLImageElement | HTMLVideoElement).width || !(c as HTMLCanvasElement | HTMLImageElement | HTMLVideoElement).height) return false;
    const p = (c as HTMLCanvasElement).getContext('2d')!.getImageData(0, 0, (c as HTMLCanvasElement | HTMLImageElement | HTMLVideoElement).width, (c as HTMLCanvasElement | HTMLImageElement | HTMLVideoElement).height).data;
    return p.some((value, i) => value !== p[i % 4]);
  });
}

async function assertPng(png: Download) {
  const bytes = readFileSync(await png.path());
  assert.match(png.suggestedFilename(), /\.png$/);
  assert.deepEqual([...bytes.subarray(0, 8)], [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  const image = PNG.sync.read(bytes);
  assert.ok(image.width > 0 && image.height > 0, 'PNG decodes to a nonempty image');
  assert.ok(image.data.some((value, i) => value !== image.data[i % 4]), 'PNG contains the chart');
}

function assertSamePixels(delayed: string, fallback: string, label = 'shader paths') {
  const decode = (url: string) => PNG.sync.read(Buffer.from(url.slice(url.indexOf(',') + 1), 'base64'));
  const actual = decode(delayed), expected = decode(fallback);
  assert.equal(actual.width, expected.width);
  assert.equal(actual.height, expected.height);
  let changed = 0, maxDelta = 0;
  for (let i = 0; i < actual.data.length; i += 4) {
    let pixelDelta = 0;
    for (let channel = 0; channel < 4; channel++) pixelDelta = Math.max(pixelDelta, Math.abs(actual.data[i + channel] - expected.data[i + channel]));
    if (pixelDelta) changed++;
    maxDelta = Math.max(maxDelta, pixelDelta);
  }
  // PNG metadata can differ even when every decoded pixel is identical.
  assert.equal(changed, 0, `${label} differ in ${changed} pixels, maximum channel delta ${maxDelta}/255`);
}

async function assertNoWebglError(page: import('playwright').Page, label: string) {
  const result = await page.evaluate(() => {
    const gl = window.__rvShader.context;
    return gl && { lost: gl.isContextLost(), error: gl.getError(), noError: gl.NO_ERROR };
  });
  assert.ok(result, `${label}: no captured WebGL context`);
  assert.equal(result.lost, false, `${label}: captured WebGL context must be live`);
  assert.equal(result.error, result.noError, `${label}: WebGL error 0x${result.error.toString(16)}`);
}

async function settle(page: import('playwright').Page) {
  await page.evaluate(() => {
    window.__shaderSettleProbe = { done: false, error: null };
    window.__richview!.settled().then(
      () => { window.__shaderSettleProbe.done = true; },
      error => { window.__shaderSettleProbe.error = String(error); window.__shaderSettleProbe.done = true; },
    );
  });
  await page.waitForFunction(() => window.__shaderSettleProbe.done, null, { timeout: 35_000 });
  const error = await page.evaluate(() => window.__shaderSettleProbe.error);
  assert.equal(error, null, `settled() rejected: ${error}`);
}

async function waitForPoll(page: import('playwright').Page) {
  try {
    await page.waitForFunction(() => window.__rvShader.events.includes('poll'), null, { timeout: 10_000, polling: 50 });
  } catch {
    const events = await page.evaluate(() => window.__rvShader.events);
    assert.fail(`No nonblocking shader completion poll; WebGL calls: ${JSON.stringify(events)}`);
  }
}

// A taller second chart expands the shared WebGL canvas. Redrawing the first must not shift its
// hatch as the buffer's viewport origin moves; this is visible even when fonts are unchanged.
async function hatchAfterBufferGrowth(browser: import('playwright').Browser, url: string) {
  const context = await browser.newContext({ viewport: { width: 1280, height: 900 }, reducedMotion: 'reduce' });
  const page = await context.newPage();
  try {
    // Await directly: a polling animation frame can redraw the chart before the comparison.
    const settleDirectly = (redraw: boolean) => page.evaluate(async redraw => {
      if (redraw) document.fonts.dispatchEvent(new Event('loadingdone'));
      let timer;
      try {
        await Promise.race([
          window.__richview!.settled(),
          new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('chart did not settle')), 35_000); }),
        ]);
      } finally { clearTimeout(timer); }
    }, redraw);
    await page.addInitScript(() => {
      window.__rvViewports = [];
      const original = WebGL2RenderingContext.prototype.viewport;
      WebGL2RenderingContext.prototype.viewport = function (x, y, width, height) {
        window.__rvViewports.push({ width, height, canvasHeight: this.canvas.height });
        return original.call(this, x, y, width, height);
      };
    });
    await page.goto(url);
    await page.waitForFunction(() => window.__richview);
    await settleDirectly(false);
    await page.setViewportSize({ width: 390, height: 900 });
    await page.goto(url);
    await page.waitForFunction(() => window.__richview);
    await settleDirectly(false);
    const before = await page.evaluate(() => ({
      models: window.__richview!.charts(),
      viewports: window.__rvViewports,
      canvas: (document.querySelector('figure[data-rv-chart] .rv-chart__canvas')! as HTMLCanvasElement).toDataURL(),
    }));
    assert.equal(before.models.length, 2);
    const [first, second] = before.models.map(model => model.canvas);
    assert.ok(second.height > first.height, 'the second chart is taller than the hatched chart');
    const firstDraw = before.viewports.findIndex((v: { width: number; height: number; canvasHeight: number; }) => v.width === first.width && v.height === first.height && v.canvasHeight === first.height);
    assert.ok(firstDraw >= 0 && before.viewports.slice(firstDraw + 1).some((v: { canvasHeight: number; }) => v.canvasHeight >= second.height), 'the shared WebGL buffer grows after the hatched chart draws');
    await settleDirectly(true);
    const after = await page.locator('figure[data-rv-chart] .rv-chart__canvas').first().evaluate(canvas => (canvas as HTMLCanvasElement).toDataURL());
    assertSamePixels(before.canvas, after, 'the chart before and after the buffer grows');
  } finally { await context.close(); }
}

// Compare the first unhatched line while the shared buffer is still small with its redraw after
// a taller chart grows that buffer. Load every face before releasing the simulated shader poll so
// font arrival cannot mask the viewport-origin difference.
async function lineAfterBufferGrowth(browser: import('playwright').Browser, url: string) {
  for (const width of [1280, 390]) {
    const context = await browser.newContext({ viewport: { width, height: 900 }, reducedMotion: 'reduce' });
    const page = await context.newPage();
    try {
      await page.addInitScript(instrument, 'pending' as const);
      await page.addInitScript(() => {
        window.__rvFirstLine = null;
        window.__rvGrowthViewports = [];
        const copy = CanvasRenderingContext2D.prototype.drawImage;
        CanvasRenderingContext2D.prototype.drawImage = function (this: CanvasRenderingContext2D, ...args: DrawImageArgs) {
          const result = Reflect.apply(copy, this, args), source = args[0];
          if (!window.__rvFirstLine && source instanceof HTMLCanvasElement
              && this.canvas.matches('figure[data-rv-chart] canvas.rv-chart__canvas')
              && source.height === this.canvas.height) {
            window.__rvFirstLine = this.canvas.toDataURL();
          }
          return result;
        };
        const viewport = WebGL2RenderingContext.prototype.viewport;
        WebGL2RenderingContext.prototype.viewport = function (x, y, w, h) {
          window.__rvGrowthViewports.push({ w, h, canvasHeight: this.canvas.height });
          return viewport.call(this, x, y, w, h);
        };
      });
      await page.goto(url, { waitUntil: 'domcontentloaded' });
      await page.waitForFunction(() => window.__richview && window.__rvShader.context, null, { timeout: 60_000 });
      const data = await page.evaluate(async () => {
        await Promise.all([...document.fonts].map(face => face.load()));
        await document.fonts.ready;
        window.__rvShader.released = true;
        await window.__richview!.settled();
        const models = window.__richview!.charts();
        const first = window.__rvFirstLine;
        const viewports = [...window.__rvGrowthViewports];
        document.fonts.dispatchEvent(new Event('loadingdone'));
        await window.__richview!.settled();
        return { models, first, viewports, after: (document.querySelector('figure[data-rv-chart] canvas.rv-chart__canvas')! as HTMLCanvasElement).toDataURL() };
      });
      assert.equal(data.models.length, 2);
      const [line, taller] = data.models.map(model => model.canvas);
      assert.ok(taller.height > line.height, `${width}: the second chart is taller`);
      assert.ok(data.first, `${width}: captured the line before buffer growth`);
      const firstDraw = data.viewports.findIndex(v => v.w === line.width && v.h === line.height && v.canvasHeight === line.height);
      assert.ok(firstDraw >= 0 && data.viewports.slice(firstDraw + 1).some(v => v.canvasHeight >= taller.height), `${width}: the buffer grows after the line draws`);
      assertSamePixels(data.first, data.after, `${width}: line before and after shared-buffer growth`);
    } finally { await context.close(); }
  }
}

async function pending(browser: import('playwright').Browser, url: string, mode: ShaderMode = 'pending') {
  const { context, page } = await open(browser, url, mode);
  try {
    await waitForPoll(page);
    await page.evaluate(async () => { await document.fonts.ready; window.__shaderSettled = false; window.__richview!.settled().then(() => { window.__shaderSettled = true; }); });
    await page.waitForTimeout(200);
    const state = await page.evaluate(() => ({ events: [...window.__rvShader.events], pending: window.__richview!.pending(), settled: window.__shaderSettled, modelSettled: window.__richview!.charts()[0]?.settled }));
    assert.equal(state.events.filter(event => event === 'compile').length, 4, 'both programs submit both shaders');
    assert.equal(state.events.filter(event => event === 'link').length, 2, 'both programs link before polling');
    assert.deepEqual(state.events.filter(event => event.startsWith('early-')), [], 'no status read, uniform lookup or drawing before completion');
    assert.ok(state.pending.length, 'settled() reports shader preparation as pending');
    assert.equal(state.settled, false, 'settled() stays pending while the shader is compiling');
    assert.equal(state.modelSettled, false, 'the chart model is unsettled during compilation');
    assert.equal(await noticeVisible(page), false, 'pending compilation is not treated as WebGL failure');
    const dialogs: string[] = [], downloads: Download[] = [];
    page.on('dialog', dialog => { dialogs.push(dialog.message()); void dialog.dismiss(); });
    page.on('download', download => downloads.push(download));
    await page.locator('figure[data-rv-chart] .rv-chart__action--png').first().click();
    await page.waitForTimeout(200);
    assert.deepEqual(dialogs, [], 'requesting PNG while pending must not report a drawing failure');
    assert.equal(downloads.length, 0, 'PNG waits until the shader is ready');
    const requested = page.waitForEvent('download', { timeout: 30_000 });
    await page.evaluate(() => {
      // A first font cycle is already complete. Hold a new cycle across the shader-ready draw
      // so the original settled() waiter must follow the new FontFaceSet.ready promise.
      const fonts = document.fonts;
      const prototype = Object.getPrototypeOf(fonts);
      const status = Object.getOwnPropertyDescriptor(prototype, 'status')!.get;
      const ready = Object.getOwnPropertyDescriptor(prototype, 'ready')!.get;
      let release: (value: unknown) => void;
      const next = new Promise(resolve => { release = resolve; });
      let held = true;
      Object.defineProperty(fonts, 'status', { configurable: true, get: () => held ? 'loading' : status!.call(fonts) });
      Object.defineProperty(fonts, 'ready', { configurable: true, get: () => held ? next : ready!.call(fonts) });
      window.__releaseShaderFonts = () => { held = false; release(fonts); };
    });
    await page.evaluate(() => { window.__rvShader.released = true; });
    const png = await requested;
    assert.equal(await page.evaluate(() => window.__shaderSettled), false, 'settled() waits for fonts started by the shader-ready draw');
    await page.evaluate(() => window.__releaseShaderFonts());
    await page.waitForFunction(() => window.__shaderSettled, null, { timeout: 30_000 });
    assert.equal(await page.evaluate(() => window.__richview!.charts()[0]?.settled), true, 'the chart model settles after compilation');
    await assertPng(png);
    await page.waitForTimeout(300);
    assert.deepEqual(dialogs, [], 'requesting PNG must not show a failure dialog');
    assert.equal(downloads.length, 1, 'one pending request produces one download');
    assert.equal(await painted(page), true, 'chart paints after compilation');
    assert.equal(await noticeVisible(page), false);
    return await page.locator('figure[data-rv-chart] canvas.rv-chart__canvas').first().evaluate(c => (c as HTMLCanvasElement).toDataURL());
  } finally { await context.close(); }
}

async function withoutAnimationFrames(browser: import('playwright').Browser, url: string) {
  const { context, page } = await open(browser, url, 'no-frames');
  try {
    await waitForPoll(page);
    // The second poll cannot be an in-flight frame: every frame since page startup was dropped.
    await page.waitForFunction(() => window.__rvShader.events.filter((event: string) => event === 'poll').length >= 2, null, { timeout: 10_000, polling: 50 });
    await page.evaluate(async () => {
      await document.fonts.ready;
      window.__shaderSettled = false;
      window.__richview!.settled().then(() => { window.__shaderSettled = true; });
      window.requestAnimationFrame(() => { window.__blockedFrameRan = true; });
    });
    await page.waitForTimeout(200);
    const held = await page.evaluate(() => ({
      blockedFrames: window.__rvShader.blockedFrames,
      blockedFrameRan: !!window.__blockedFrameRan,
      events: [...window.__rvShader.events],
      pending: window.__richview!.pending(),
      settled: window.__shaderSettled,
      modelSettled: window.__richview!.charts()[0]?.settled,
    }));
    assert.ok(held.blockedFrames! > 0 && !held.blockedFrameRan, 'animation frames were requested but never delivered');
    assert.deepEqual(held.events.filter(event => event.startsWith('early-')), [], 'a timer poll does not synchronously read or draw pending shaders');
    assert.ok(held.pending.length, 'the chart remains pending without animation frames');
    assert.equal(held.settled, false, 'settled() waits while no animation frame fires');
    assert.equal(held.modelSettled, false);
    assert.equal(await noticeVisible(page), false);
    await page.evaluate(() => { window.__rvShader.released = true; });
    await page.waitForFunction(() => window.__shaderSettled, null, { timeout: 30_000, polling: 50 });
    assert.equal(await page.evaluate(() => window.__richview!.charts()[0]?.settled), true);
    assert.equal(await page.evaluate(() => !!window.__blockedFrameRan), false, 'dropped callbacks never fire after release');
    assert.equal(await painted(page), true, 'the timer fallback finishes compilation and draws');
    assert.equal(await noticeVisible(page), false);
  } finally { await context.close(); }
}

async function fallback(browser: import('playwright').Browser, url: string) {
  const { context, page } = await open(browser, url, 'fallback');
  try {
    await settle(page);
    assert.equal(await painted(page), true, 'a browser without the extension still draws');
    const events = await page.evaluate(() => window.__rvShader.events);
    assert.ok(events.includes('link-status'), 'fallback checks link status');
    assert.ok(!events.includes('poll'), 'fallback does not poll unsupported extension');
    // Match the delayed path's export and redraw before comparing the chart surface.
    const [png] = await Promise.all([
      page.waitForEvent('download', { timeout: 30_000 }),
      page.locator('figure[data-rv-chart] .rv-chart__action--png').first().click(),
    ]);
    await assertPng(png);
    await page.waitForTimeout(300);
    return await page.locator('figure[data-rv-chart] canvas.rv-chart__canvas').first().evaluate(c => (c as HTMLCanvasElement).toDataURL());
  } finally { await context.close(); }
}

async function failure(browser: import('playwright').Browser, url: string) {
  const { context, page } = await open(browser, url, 'failure');
  try {
    await waitForPoll(page);
    assert.equal(await noticeVisible(page), false);
    await page.evaluate(() => { window.__rvShader.released = true; });
    await page.waitForFunction(() => [...document.querySelectorAll('.rv-chart__notice')].some(node => !(node as HTMLElement).hidden && getComputedStyle(node).display !== 'none'), null, { timeout: 30_000 });
    await settle(page);
    assert.equal(await noticeVisible(page), true, 'a link failure produces the unavailable notice');
  } finally { await context.close(); }
}

async function contextLoss(browser: import('playwright').Browser, url: string) {
  const faults: string[] = [];
  const { context, page } = await open(browser, url, 'pending', faults);
  try {
    await waitForPoll(page);
    const supported = await page.evaluate(() => Boolean(window.__rvShader.context!.getExtension('WEBGL_lose_context')));
    if (!supported) {
      assert.deepEqual(faults, [], 'no console or page errors before the unsupported recovery case');
      return 'unsupported';
    }
    await page.evaluate(async () => {
      const gl = window.__rvShader.context!, ext = gl.getExtension('WEBGL_lose_context');
      const event = (name: string) => new Promise<void>((resolve, reject) => {
        const timer = setTimeout(() => reject(new Error(`${name} did not arrive`)), 15_000);
        gl.canvas.addEventListener(name, () => { clearTimeout(timer); resolve(); }, { once: true });
      });
      const lost = event('webglcontextlost');
      ext!.loseContext(); await lost;
      // Leave at least two polling opportunities while the extension reports completion on
      // the lost context; that result must not be mistaken for a usable program.
      await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      const restored = event('webglcontextrestored');
      ext!.restoreContext(); await restored;
    });
    await page.waitForFunction(() => window.__rvShader.events.filter((event: string) => event === 'link').length >= 4, null, { timeout: 30_000 });
    const early = await page.evaluate(() => window.__rvShader.events.filter((event: string) => event.startsWith('early-')));
    assert.deepEqual(early, [], 'a lost pending context never triggers a synchronous status query');
    await page.evaluate(() => { window.__rvShader.released = true; });
    await settle(page);
    assert.equal(await painted(page), true, 'the restored context draws');
    assert.equal(await noticeVisible(page), false);
    // Let deferred redraws and browser-delivered console events follow the restored frame.
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    await page.waitForTimeout(200);
    await assertNoWebglError(page, 'restored context');
    assert.deepEqual(faults, [], 'context loss and restoration produce no console or page errors');
    return 'restored';
  } finally { await context.close(); }
}

async function contextLossWithoutRestore(browser: import('playwright').Browser, url: string) {
  const faults: string[] = [];
  const { context, page } = await open(browser, url, 'pending', faults);
  try {
    await waitForPoll(page);
    const supported = await page.evaluate(() => Boolean(window.__rvShader.context!.getExtension('WEBGL_lose_context')));
    if (!supported) {
      assert.deepEqual(faults, [], 'no console or page errors before the unsupported recovery case');
      return 'unsupported';
    }
    await page.evaluate(async () => {
      await document.fonts.ready;
      window.__noRestoreSettled = false;
      window.__richview!.settled().then(() => { window.__noRestoreSettled = true; });
    });
    // Let startup observers drain so the replacement is driven by the lost-context fallback,
    // not an unrelated resize or input event.
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    await page.waitForTimeout(200);
    await page.evaluate(async () => {
      const gl = window.__rvShader.context!;
      window.__lostShaderContext = gl;
      const ext = gl.getExtension('WEBGL_lose_context');
      window.__lostShaderExtension = ext!;
      const lost = new Promise<void>((resolve, reject) => {
        const timer = setTimeout(() => reject(new Error('webglcontextlost did not arrive')), 15_000);
        gl.canvas.addEventListener('webglcontextlost', () => { clearTimeout(timer); resolve(); }, { once: true });
      });
      ext!.loseContext();
      await lost;
      // Keep the original context lost until the replacement has settled and drawn.
    });
    await page.waitForTimeout(250);
    assert.equal(await page.evaluate(() => window.__rvShader.context === window.__lostShaderContext), true, 'no unrelated draw replaced the context before the delayed fallback');
    await page.waitForFunction(() => {
      const state = window.__rvShader;
      return state.contexts.length >= 2 && state.context !== window.__lostShaderContext && state.pollContexts.includes(state.context!);
    }, null, { timeout: 15_000, polling: 50 });
    const pending = await page.evaluate(() => ({
      firstStillLost: window.__lostShaderContext.isContextLost(),
      newContextLive: !window.__rvShader.context!.isContextLost(),
      pending: window.__richview!.pending(),
      settled: window.__noRestoreSettled,
      modelSettled: window.__richview!.charts()[0]?.settled,
      early: window.__rvShader.events.filter((event: string) => event.startsWith('early-')),
    }));
    assert.equal(pending.firstStillLost, true, 'the original context was never restored');
    assert.equal(pending.newContextLive, true, 'the fallback creates a different live context');
    assert.ok(pending.pending.length, 'replacement compilation remains pending');
    assert.equal(pending.settled, false);
    assert.equal(pending.modelSettled, false);
    assert.deepEqual(pending.early, [], 'neither context reads status or draws while pending');
    assert.equal(await noticeVisible(page), false);
    await page.evaluate(() => { window.__rvShader.released = true; });
    await page.waitForFunction(() => window.__noRestoreSettled, null, { timeout: 30_000, polling: 50 });
    assert.equal(await painted(page), true, 'the replacement context draws');
    assert.equal(await noticeVisible(page), false);
    assert.equal(await page.evaluate(() => window.__lostShaderContext.isContextLost()), true);
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    await page.waitForTimeout(200);
    await assertNoWebglError(page, 'replacement context');
    assert.deepEqual(faults, [], 'context replacement produces no console or page errors');
    const before = await page.evaluate(() => {
      window.__replacementShaderContext = window.__rvShader.context!;
      return {
        contexts: window.__rvShader.contexts.length,
        links: window.__rvShader.events.filter((event: string) => event === 'link').length,
      };
    });
    await page.evaluate(async () => {
      const gl = window.__lostShaderContext;
      const restored = new Promise<void>((resolve, reject) => {
        const timer = setTimeout(() => reject(new Error('stale webglcontextrestored did not arrive')), 15_000);
        gl.canvas.addEventListener('webglcontextrestored', () => { clearTimeout(timer); resolve(); }, { once: true });
      });
      window.__lostShaderExtension.restoreContext();
      await restored;
    });
    await settle(page);
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    await page.waitForTimeout(200);
    const after = await page.evaluate(() => ({
      staleRestored: !window.__lostShaderContext.isContextLost(),
      sameActive: window.__rvShader.context === window.__replacementShaderContext,
      contexts: window.__rvShader.contexts.length,
      links: window.__rvShader.events.filter((event: string) => event === 'link').length,
    }));
    assert.equal(after.staleRestored, true, 'the old context delivered a late restore event');
    assert.equal(after.sameActive, true, 'late restore leaves the replacement renderer active');
    assert.equal(after.contexts, before.contexts, 'late restore creates no new context');
    assert.equal(after.links, before.links, 'late restore does not rebuild shader programs');
    assert.equal(await painted(page), true, 'late restore leaves the replacement chart drawn');
    assert.equal(await noticeVisible(page), false);
    await assertNoWebglError(page, 'replacement after stale restore');
    assert.deepEqual(faults, [], 'late restore produces no console or page errors');
    return 'replaced, stale context restored harmlessly';
  } finally { await context.close(); }
}

async function readyContextLossWithoutRestore(browser: import('playwright').Browser, url: string) {
  const faults: string[] = [];
  const { context, page } = await open(browser, url, 'pending', faults);
  try {
    await waitForPoll(page);
    await page.evaluate(() => { window.__rvShader.released = true; });
    await settle(page);
    await page.evaluate(async () => { await document.fonts.ready; });
    assert.equal(await painted(page), true, 'the ready context paints before it is lost');
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    await page.waitForTimeout(200);
    const supported = await page.evaluate(() => Boolean(window.__rvShader.context!.getExtension('WEBGL_lose_context')));
    if (!supported) {
      assert.deepEqual(faults, [], 'no console or page errors before the unsupported ready-loss case');
      return 'unsupported';
    }
    const before = await page.evaluate(() => {
      window.__readyLostContext = window.__rvShader.context!;
      const canvas = document.querySelector('figure[data-rv-chart] canvas.rv-chart__canvas');
      return {
        pixels: (canvas! as HTMLCanvasElement).toDataURL(),
        contexts: window.__rvShader.contexts.length,
        links: window.__rvShader.events.filter((event: string) => event === 'link').length,
      };
    });
    await page.evaluate(async () => {
      const gl = window.__readyLostContext, extension = gl.getExtension('WEBGL_lose_context');
      const lost = new Promise<void>((resolve, reject) => {
        const timer = setTimeout(() => reject(new Error('ready webglcontextlost did not arrive')), 15_000);
        gl.canvas.addEventListener('webglcontextlost', () => { clearTimeout(timer); resolve(); }, { once: true });
      });
      extension!.loseContext();
      await lost;
      // Deliberately leave the previously painted context lost and do not request a redraw.
    });
    await page.waitForTimeout(1_500);
    const after = await page.evaluate(() => ({
      sameContext: window.__rvShader.context === window.__readyLostContext,
      lost: window.__readyLostContext.isContextLost(),
      contexts: window.__rvShader.contexts.length,
      links: window.__rvShader.events.filter((event: string) => event === 'link').length,
      pixels: (document.querySelector('figure[data-rv-chart] canvas.rv-chart__canvas')! as HTMLCanvasElement).toDataURL(),
    }));
    assert.equal(after.lost, true, 'the ready context remains lost');
    assert.equal(after.sameContext, true, 'a ready context is not replaced without a redraw');
    assert.equal(after.contexts, before.contexts, 'the pending-only fallback creates no new context');
    assert.equal(after.links, before.links, 'the pending-only fallback does not rebuild programs');
    assert.equal(after.pixels, before.pixels, 'the already-painted chart stays intact');
    assert.equal(await noticeVisible(page), false, 'ready-context loss does not show a failure notice');
    assert.deepEqual(faults, [], 'ready-context loss produces no console or page errors');
    await page.evaluate(() => {
      window.__readyLossSettled = false;
      window.__richview!.settled().then(() => { window.__readyLossSettled = true; });
    });
    await page.waitForFunction(() => window.__readyLossSettled, null, { timeout: 10_000, polling: 50 });
    const afterSettle = await page.evaluate(() => ({
      sameContext: window.__rvShader.context === window.__readyLostContext,
      contexts: window.__rvShader.contexts.length,
      links: window.__rvShader.events.filter((event: string) => event === 'link').length,
      pixels: (document.querySelector('figure[data-rv-chart] canvas.rv-chart__canvas')! as HTMLCanvasElement).toDataURL(),
    }));
    assert.equal(afterSettle.sameContext, true, 'settled() does not replace the ready lost context');
    assert.equal(afterSettle.contexts, before.contexts);
    assert.equal(afterSettle.links, before.links);
    assert.equal(afterSettle.pixels, before.pixels, 'settled() preserves the painted chart');
    const dialogs: string[] = [], downloads: Download[] = [];
    page.on('dialog', dialog => { dialogs.push(dialog.message()); void dialog.dismiss(); });
    page.on('download', download => downloads.push(download));
    const [png] = await Promise.all([
      page.waitForEvent('download', { timeout: 30_000 }),
      page.locator('figure[data-rv-chart] .rv-chart__action--png').first().click(),
    ]);
    await assertPng(png);
    await settle(page);
    await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    await page.waitForTimeout(200);
    const afterPng = await page.evaluate(() => ({
      originalStillLost: window.__readyLostContext.isContextLost(),
      newContext: window.__rvShader.context !== window.__readyLostContext,
      contexts: window.__rvShader.contexts.length,
      links: window.__rvShader.events.filter((event: string) => event === 'link').length,
    }));
    assert.equal(afterPng.originalStillLost, true);
    assert.equal(afterPng.newContext, true, 'PNG creates a replacement for the lost context');
    assert.equal(afterPng.contexts, before.contexts + 1, 'PNG creates exactly one replacement context');
    assert.equal(afterPng.links, before.links + 2, 'the replacement links both programs once');
    assert.equal(downloads.length, 1, 'one PNG request produces one download');
    assert.deepEqual(dialogs, [], 'PNG after ready-context loss does not report a failure');
    assert.equal(await noticeVisible(page), false);
    assert.equal(await painted(page), true);
    await assertNoWebglError(page, 'PNG replacement after ready-context loss');
    assert.deepEqual(faults, [], 'settling and PNG after ready-context loss produce no console or page errors');
    return 'paint retained, PNG replaced context';
  } finally { await context.close(); }
}

const engine = process.env.RICHVIEW_VERIFY_BROWSER || 'chromium';
const engines = { chromium, firefox, webkit };
assert.ok(engine === 'chromium' || engine === 'firefox' || engine === 'webkit', `Unknown browser ${engine}`);
await waitForBenchmark();
mkdirSync(join(root, 'tmp'), { recursive: true });
const dir = mkdtempSync(join(root, 'tmp', 'shader-readiness-'));
let input: Awaited<ReturnType<typeof reports>> | undefined, browser: Browser | undefined;
try {
  input = await reports(dir, { only: [], extra: { shader: source, hatch: hatchSource, viewport: viewportSource }, files: { viewport: { 'report.rstyle.yaml': viewportStyle } } });
  const launched: Browser = await launchBrowser(engines[engine], {
    headless: true,
    ...(engine === 'chromium' ? { executablePath: chromiumPath(), args: ['--no-sandbox', ...(process.platform === 'darwin' && process.arch === 'arm64' ? ['--enable-gpu'] : [])] } : {}),
  }, { TimeoutError: errors.TimeoutError });
  browser = launched;
  const url = `${input.base}/shader/report.html`;
  await hatchAfterBufferGrowth(browser, `${input.base}/hatch/report.html`);
  await lineAfterBufferGrowth(browser, `${input.base}/viewport/report.html`);
  const delayed = await pending(browser, url);
  await withoutAnimationFrames(browser, url);
  const synchronous = await fallback(browser, url);
  assertSamePixels(delayed, synchronous);
  await failure(browser, url);
  const recovery = await contextLoss(browser, url);
  const noRestore = await contextLossWithoutRestore(browser, url);
  const readyNoRestore = await readyContextLossWithoutRestore(browser, url);
  console.log(JSON.stringify({ engine, hatchAfterBufferGrowth: 'pass', lineAfterBufferGrowth: 'pass', delayed: 'pass', withoutAnimationFrames: 'pass', fallback: 'pass', failure: 'pass', contextLoss: recovery, contextLossWithoutRestore: noRestore, readyContextLossWithoutRestore: readyNoRestore }));
} finally {
  await browser?.close();
  await input?.close();
  rmSync(dir, { recursive: true, force: true });
}
