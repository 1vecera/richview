import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { gunzipSync } from 'node:zlib';
import { copyFileSync, existsSync, mkdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { harness, fixture, instrument, inputValue, root, settled, settledCharts, watchErrors } from './browser-harness.mjs';
import { recordPreviewReload, reloadRefusal } from '../tools/rv-gate/lib/preview-reload.mjs';
import { capturePreviewFailure, settleRestoredPreview } from './preview-recovery.mjs';
import { verifyRuntimeAccess } from './runtime-access.mjs';
import { browserCases } from '../tools/rv-gate/lib/browser-cases.mjs';

// The showcase offers its five primary styles as pills and the rest behind an "Other styles" select.
const chooseStyle = async (page, theme) => {
  const pill = page.locator(`#style-pills [data-theme="${theme}"]`);
  if (await pill.count()) await pill.click(); else await page.locator('#theme').selectOption(theme);
};
// Every style is its own pre-rendered report in `#report-frame` (FEA-SF-36); choosing one swaps the
// frame's document. Wait until the frame shows that style's file with its runtime started.
const showStyle = async (page, theme) => {
  await chooseStyle(page, theme);
  await page.waitForFunction(theme => {
    const doc = document.querySelector('#report-frame').contentDocument;
    return doc?.location.pathname.endsWith(`/Rendered/${theme}.html`) && doc.readyState === 'complete' && doc.documentElement.dataset.rvRuntime === 'started';
  }, theme, { timeout: 60_000 });
};
// Retain the browsing context across srcdoc edits: resolving a FrameLocator reads the whole
// multi-megabyte srcdoc attribute through CDP. A parent-page navigation needs a fresh Frame.
const previewFrame = async page => {
  const element = await page.locator('#preview').elementHandle();
  try {
    const frame = await element.contentFrame();
    assert.ok(frame, 'The preview iframe has a browsing context');
    return frame;
  } finally { await element.dispose(); }
};
const selection = browserCases();
const selectedCases = new Set(selection.cases);
const completedCases = [];
const wants = key => selectedCases.has(key);
const h = await harness('browser');
// The research pipeline's committed chart inventory: an independent list, so a chart dropped from the source fails.
const flagshipCharts = JSON.parse(readFileSync(join(root, 'examples/openai-vs-anthropic/data/charts/index.json'), 'utf8')).length;
const documentStart = process.platform === 'darwin' ? 'Meta+ArrowUp' : 'Control+Home';
const evidence = { engine: h.engine, browser: h.browser.version(), base: h.base, checks: [], timings: {}, errors: [], capabilities: { trustedPinch: h.engine === 'chromium' } };
const check = text => { evidence.checks.push(text); console.error(`PASS ${text}`); };
// Firefox creates its own clipboard store instead of retaining the constructor's
// DataTransfer. Populate and inspect the event that the product actually receives.
const dispatchClipboard = (input, { type, text, missing = false }) => {
  const event = new ClipboardEvent(type, { bubbles: true, cancelable: true, clipboardData: new DataTransfer() });
  if (missing) Object.defineProperty(event, 'clipboardData', { value: null });
  const data = event.clipboardData;
  if (!missing && !data) throw new Error('Synthetic clipboard event has no data store');
  if (text !== undefined) data.setData('text/plain', text);
  input.dispatchEvent(event);
  return { text: data?.getData('text/plain'), prevented: event.defaultPrevented };
};
// Text placement from the engine's geometry model (`window.__richview.charts()`): every text
// item's bounds must sit inside the chart and no two may overlap; scatter bubbles must not cover
// text. `scope` is the page or frame holding the report; `index` picks the chart.
const glyphAudit = (scope, index) => scope.locator('body').evaluate((_, index) => {
  const chart = window.__richview.charts()[index].geometry;
  const items = [...chart.panels.flatMap(panel => panel.items), ...(chart.figure_items ?? [])];
  const texts = items.filter(item => item.kind === 'text' && item.bounds.w > 0 && item.bounds.h > 0);
  const overlap = (a, b) => Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x) > .5 && Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y) > .5;
  const checks = [];
  for (const text of texts) {
    const { x, y, w, h } = text.bounds;
    if (x < -.5 || y < -.5 || x + w > chart.width + .5 || y + h > chart.height + .5) checks.push({ kind: 'clipping', related: [text.text] });
  }
  texts.forEach((a, i) => texts.slice(i + 1).forEach(b => { if (overlap(a.bounds, b.bounds)) checks.push({ kind: 'label-collision', related: [a.text, b.text] }); }));
  const circles = items.filter(item => item.kind === 'point');
  const collisions = circles.flatMap((circle, mark) => texts.filter(({ bounds: box }) => {
    const x = Math.max(box.x, Math.min(circle.center[0], box.x + box.w));
    const y = Math.max(box.y, Math.min(circle.center[1], box.y + box.h));
    return Math.hypot(x - circle.center[0], y - circle.center[1]) < circle.radius - .5;
  }).map(label => ({ mark, label: label.text })));
  return { measured: texts.length > 0, texts: texts.length, checks, circles: circles.map(circle => ({ cx: circle.center[0], cy: circle.center[1], radius: circle.radius })), collisions };
}, index);
// Genuine runtime gaps stay failing assertions, collected so one gap does not hide the rest.
const failures = [];
const expectEqual = (label, actual, expected) => { try { assert.deepEqual(actual, expected); } catch { failures.push(`${label}: expected ${JSON.stringify(expected)?.slice(0, 120)}, got ${JSON.stringify(actual)?.slice(0, 120)}`); } };
let resultPath;
// The five-styles strip's pages and the report runtime they link.
const stripPath = path => path.startsWith('/landing/anchor') || path.startsWith('/landing/runtime/');
// The static landing, which prod serves, makes the five-styles strip's frames only as the reader comes
// near it (F1, S27-927): not with the page, where Chromium would load lazy frames from the first
// screen, and not when the engine starts while the strip is still screens away.
const deferredStrip = async () => {
  const context = await h.browser.newContext({ reducedMotion: 'no-preference' });
  let releaseImage;
  try {
    const page = await context.newPage();
    let imageRequested = false;
    const imageHeld = new Promise(resolve => { releaseImage = resolve; });
    // A cover image can outlive the chart and keep the frame's load event pending.
    if (h.engine === 'webkit') await page.route('https://images.unsplash.com/**', async route => {
      imageRequested = true;
      await imageHeld;
      await route.abort().catch(() => {});
    });
    const requested = [];
    page.on('request', request => requested.push(new URL(request.url()).pathname));
    await page.goto(`${h.base}/`, { waitUntil: 'load' });
    await page.waitForTimeout(500);
    assert.deepEqual(requested.filter(stripPath), [], `${h.engine}: the strip loaded before intent`);
    assert.equal(await page.locator('.voices__page iframe').count(), 0);
    await page.keyboard.press('Shift');
    await page.waitForFunction(() => document.documentElement.dataset.homepageEngine === 'rust', null, { timeout: 60_000 });
    await page.waitForTimeout(500);
    assert.deepEqual(requested.filter(stripPath), [], `${h.engine}: the strip loaded while it was screens away`);
    const named = await page.locator('.voices__page').evaluateAll(holders => holders.map(holder => new URL(holder.dataset.src, location.href).href));
    await page.locator('#styles').scrollIntoViewIfNeeded();
    if (h.engine === 'webkit') {
      await page.waitForFunction(() => {
        const frame = document.querySelector('.voices__sheet[data-voice="air"] iframe');
        return frame?.contentDocument?.documentElement?.dataset.rvRuntime === 'started' && frame.contentDocument.querySelector('figure[data-rv-chart]');
      }, null, { timeout: 60_000 });
      assert.ok(imageRequested, `${h.engine}: the cover image was requested`);
      assert.equal(await page.locator('.voices__sheet[data-voice="air"] iframe').evaluate(frame => frame.contentDocument.readyState), 'interactive');
      await page.waitForFunction(() => document.querySelector('.voices__sheet[data-voice="air"]').hasAttribute('data-ready'), null, { timeout: 10_000 });
    }
    await page.waitForFunction(count => document.querySelectorAll('.voices__sheet[data-ready] .voices__page > iframe').length === count, named.length, { timeout: 60_000 });
    const made = await page.locator('.voices__page > iframe').evaluateAll(frames => frames.map(frame => frame.src));
    expectEqual('strip frames', made, named);
    for (const url of named) {
      const path = new URL(url).pathname; // Fragments target the chart in the frame; HTTP requests omit them.
      expectEqual(`requests of ${path}`, requested.filter(p => p === path).length, 1);
    }
  } finally { releaseImage?.(); await context.close(); }
};
// A first visit leaves the site engine and live example unloaded; edits made while the
// engine downloads must be rendered once its listeners attach.
const deferredLanding = async () => {
  const context = await h.browser.newContext({ reducedMotion: 'no-preference' });
  let release;
  try {
    const page = await context.newPage();
    const requested = [];
    page.on('request', request => requested.push(new URL(request.url()).pathname));
    let arrived;
    const wasmRequested = new Promise(resolve => { arrived = resolve; });
    const gate = new Promise(resolve => { release = resolve; });
    // Exercise the optional story as well: its writing scene must hand over after an early edit.
    await page.route(`${h.base}/`, async route => {
      const response = await route.fetch();
      const body = (await response.text()).replace('<html data-landing="static" ', '<html ');
      await route.fulfill({ response, body });
    });
    await page.route('**/richview_site_bg.wasm', async route => {
      arrived();
      await gate;
      await route.continue().catch(() => {});
    });
    // A desktop reader's cursor rests over the page as it loads, and Firefox sends it pointer events
    // then: hovering is not intent, so nothing may start.
    await page.mouse.move(320, 240);
    await page.goto(`${h.base}/`, { waitUntil: 'load' });
    await page.waitForTimeout(500);
    for (const path of ['/richview_site.js', '/richview_site_bg.wasm', '/landing/demo.html']) {
      assert.ok(!requested.includes(path), `${h.engine}: ${path} loaded before intent`);
    }
    assert.deepEqual(requested.filter(stripPath), [], `${h.engine}: the five-styles strip loaded before intent`);
    assert.equal(await page.locator('#demo-result').getAttribute('src'), null);
    assert.ok(await page.getByText('Editor not ready.').isVisible());
    // No report shows yet, so the line that says it is pre-rendered waits too.
    assert.ok(await page.locator('#demo-status').isHidden());
    assert.equal(await page.locator('#demo').getAttribute('aria-busy'), null);
    assert.ok(await page.locator('#tab-style').isDisabled());
    await page.keyboard.press('Shift');
    await Promise.race([wasmRequested, page.waitForTimeout(15_000).then(() => { throw Error('landing did not request its site Wasm'); })]);
    assert.equal(await page.locator('#demo').getAttribute('aria-busy'), 'true');
    assert.ok(await page.locator('#demo-pending').isVisible());
    assert.ok(await page.locator('#tab-style').isDisabled());
    const source = page.locator('#demo-source');
    await source.fill((await source.inputValue()).replace('title: Longer lives', 'title: First edit while starting'));
    release();
    await page.waitForFunction(() => document.documentElement.dataset.homepageEngine === 'rust', null, { timeout: 60_000 });
    assert.notEqual(await page.locator('#demo').getAttribute('data-handed'), null);
    await page.frameLocator('#demo-result').getByRole('heading', { name: 'First edit while starting', exact: true }).waitFor({ timeout: 60_000 });
    assert.ok(await page.locator('#demo-fallback').isHidden());
    assert.equal(await page.locator('#demo').getAttribute('aria-busy'), null);
    assert.ok(await page.locator('#demo-pending').isHidden());
    assert.ok(await page.locator('#tab-style').isEnabled());
    await page.locator('#tab-style').click();
    assert.equal(await page.locator('#tab-style').getAttribute('aria-selected'), 'true');
    assert.equal(requested.filter(path => path === '/richview_site.js').length, 1);
    assert.equal(requested.filter(path => path === '/richview_site_bg.wasm').length, 1);
  } finally {
    release?.();
    await context.close();
  }
  const noJs = await h.browser.newContext({ javaScriptEnabled: false });
  try {
    const page = await noJs.newPage();
    await page.goto(`${h.base}/`, { waitUntil: 'load' });
    // Playwright's text selector skips noscript contents even when the browser renders them.
    const explanation = page.locator('.sheet--source noscript > p');
    assert.equal(await explanation.textContent(), 'The editor requires JavaScript.');
    assert.ok(await explanation.isVisible());
    // The editor never starts, so its pending line and help give way; the report is pre-rendered.
    assert.ok(await page.locator('#demo-pending').isHidden());
    assert.ok(await page.locator('#demo-help').isHidden());
    assert.ok(await page.locator('#demo-status').isVisible());
    const fallback = page.frameLocator('.report-viewport iframe:not([hidden])');
    await fallback.getByRole('heading', { name: 'Longer lives', exact: true }).waitFor({ timeout: 30_000 });
    // Without JavaScript the strip's frames are the <noscript> ones, which the browser loads lazily.
    const stripFrames = page.locator('.voices__page > noscript > iframe');
    assert.ok(await stripFrames.count() > 0);
    assert.equal(await stripFrames.count(), await page.locator('.voices__page').count());
    await page.locator('#styles').scrollIntoViewIfNeeded();
    await page.frameLocator('.voices__page > noscript > iframe').first().getByRole('heading', { name: 'Longer lives', exact: true }).waitFor({ timeout: 30_000 });
  } finally { await noJs.close(); }
  const hashContext = await h.browser.newContext({ reducedMotion: 'reduce' });
  try {
    const page = await hashContext.newPage();
    await page.goto(`${h.base}/#demo`, { waitUntil: 'load' });
    await page.waitForFunction(() => document.documentElement.dataset.homepageEngine === 'rust', null, { timeout: 60_000 });
    assert.ok(await page.locator('#tab-style').isEnabled());
  } finally { await hashContext.close(); }
  for (const mode of ['script-abort', 'script-throw', 'wasm-abort']) {
    const failureContext = await h.browser.newContext({ reducedMotion: 'reduce' });
    try {
      const page = await failureContext.newPage();
      if (mode === 'script-abort') await page.route('**/richview_site.js', route => route.abort());
      if (mode === 'script-throw') await page.route('**/richview_site.js', route => route.fulfill({ status: 200, contentType: 'text/javascript', body: 'throw new Error("controlled site script failure")' }));
      if (mode === 'wasm-abort') await page.route('**/richview_site_bg.wasm', route => route.abort());
      await page.goto(`${h.base}/`, { waitUntil: 'load' });
      await page.keyboard.press('Shift');
      await page.getByText('Could not start the editor. Reload to try again.').waitFor({ timeout: 15_000 });
      assert.equal(await page.locator('#demo').getAttribute('aria-busy'), null);
      assert.ok(await page.locator('#tab-style').isDisabled());
      assert.ok(await page.locator('#demo-help').isHidden());
      await page.getByRole('link', { name: 'Open the example report' }).click();
      await page.getByRole('heading', { name: 'Longer lives', exact: true }).waitFor({ timeout: 30_000 });
    } finally { await failureContext.close(); }
  }
  const bootstrapFailure = await h.browser.newContext({ reducedMotion: 'reduce' });
  try {
    const page = await bootstrapFailure.newPage();
    await page.route('**/landing/bootstrap.js', route => route.abort());
    await page.goto(`${h.base}/`, { waitUntil: 'load' });
    assert.ok(await page.getByText('Editor not ready.').isVisible());
    assert.ok(await page.locator('#tab-style').isDisabled());
    await page.getByRole('link', { name: 'Open the example report' }).click();
    await page.getByRole('heading', { name: 'Longer lives', exact: true }).waitFor({ timeout: 30_000 });
  } finally { await bootstrapFailure.close(); }
  const historyContext = await h.browser.newContext({ reducedMotion: 'reduce' });
  let resume;
  try {
    await historyContext.addInitScript(() => {
      window.__landingDocumentId = `${performance.timeOrigin}-${Math.random()}`;
      window.__landingPageShows = [];
      addEventListener('pageshow', event => window.__landingPageShows.push(event.persisted));
    });
    const page = await historyContext.newPage();
    let arrived;
    const wasmRequested = new Promise(resolve => { arrived = resolve; });
    const gate = new Promise(resolve => { resume = resolve; });
    await page.route('**/richview_site_bg.wasm', async route => {
      arrived();
      await gate;
      await route.continue().catch(() => {});
    });
    await page.goto(`${h.base}/`, { waitUntil: 'load' });
    const firstDocument = await page.evaluate(() => window.__landingDocumentId);
    await page.keyboard.press('Shift');
    await Promise.race([wasmRequested, page.waitForTimeout(15_000).then(() => { throw Error('landing did not request its site Wasm before navigation'); })]);
    await page.goto(`${h.base}/privacy`, { waitUntil: 'domcontentloaded' });
    resume();
    await page.goBack({ waitUntil: 'load' });
    const restored = await page.evaluate(() => ({
      document: window.__landingDocumentId,
      persisted: window.__landingPageShows.at(-1),
    }));
    assert.equal(restored.document === firstDocument, restored.persisted, 'pageshow must distinguish BFCache from a reload');
    evidence.deferred_history = { restoredFromBFCache: restored.persisted, outcome: 'checking' };
    await page.keyboard.press('Shift');
    await page.waitForFunction(() => document.documentElement.dataset.homepageEngine === 'rust', null, { timeout: 60_000 });
    assert.ok(await page.locator('#tab-style').isEnabled());
    evidence.deferred_history.outcome = 'ready';
  } finally { resume?.(); await historyContext.close(); }
};
const runtimeLoaderControls = async () => {
  const bounded = async (promise, label) => {
    let timer;
    try {
      return await Promise.race([promise, new Promise((_, reject) => {
        timer = setTimeout(() => reject(new Error(label)), 15_000);
      })]);
    } finally { clearTimeout(timer); }
  };
  evidence.runtime_loader_controls = [];
  for (const mode of ['pagehide-abort', 'live-abort', 'live-503', 'pagehide-503']) {
    const context = await h.browser.newContext({ reducedMotion: 'reduce' });
    let release;
    try {
      await context.addInitScript(() => {
        window.__loaderControl = { hidden: false, rejected: null, unhandled: null };
        addEventListener('pagehide', () => { window.__loaderControl.hidden = true; });
        addEventListener('unhandledrejection', event => { window.__loaderControl.unhandled = event.reason?.name; });
        const nativeFetch = fetch.bind(window);
        window.fetch = (input, init) => {
          const url = new URL(typeof input === 'string' ? input : input.url ?? input, location.href);
          if (url.pathname !== '/richview_runtime_bg.wasm') return nativeFetch(input, init);
          const controller = new AbortController();
          window.__loaderControl.controller = controller;
          return nativeFetch(input, { ...init, signal: controller.signal }).catch(error => {
            window.__loaderControl.rejected = error.name;
            throw error;
          });
        };
      });
      const page = await context.newPage(), faults = [];
      let arrived, fault;
      const requested = new Promise(resolve => { arrived = resolve; });
      const firstFault = new Promise(resolve => { fault = resolve; });
      page.on('pageerror', error => { faults.push({ name: error.name, message: error.message }); fault(error); });
      evidence.runtime_loader_controls.push({ mode, synthetic_pagehide: mode.startsWith('pagehide'), faults });
      const gate = new Promise(resolve => { release = resolve; });
      await page.route('**/richview_runtime_bg.wasm', async route => {
        arrived(); await gate;
        if (mode.endsWith('abort')) await route.continue().catch(() => {});
        else await route.fulfill({ status: 503, contentType: 'application/wasm', body: 'Unavailable' });
      });
      await page.goto(`${h.base}/docs/charts.html`, { waitUntil: 'domcontentloaded' });
      await bounded(requested, `${mode}: loader did not fetch its module`);
      if (mode.startsWith('pagehide')) {
        await page.evaluate(() => dispatchEvent(new PageTransitionEvent('pagehide', { persisted: false })));
      }
      if (mode.endsWith('abort')) {
        const canceled = page.waitForEvent('requestfailed', { predicate: request => request.url().endsWith('/richview_runtime_bg.wasm') });
        await page.evaluate(() => window.__loaderControl.controller.abort());
        await bounded(canceled, `${mode}: native fetch did not abort`);
        await settled(page);
        assert.equal(await page.evaluate(() => window.__loaderControl.rejected), 'AbortError');
        if (mode === 'live-abort') await bounded(firstFault, 'A live AbortError was hidden');
        else assert.deepEqual(faults, [], 'Only an AbortError after pagehide is expected cancellation');
      } else {
        release();
        const error = await bounded(firstFault, `${mode}: a real HTTP 503 failure was hidden`);
        assert.notEqual(error.name, 'AbortError');
        await settled(page);
      }
      const unhandled = await page.evaluate(() => window.__loaderControl.unhandled);
      if (mode === 'live-abort') assert.equal(unhandled, 'AbortError');
      else if (mode === 'pagehide-abort') assert.equal(unhandled, null);
      else assert.ok(unhandled && unhandled !== 'AbortError', `${mode}: the non-abort rejection must surface`);
      evidence.runtime_loader_controls.at(-1).unhandled_reason = unhandled;
      if (mode !== 'pagehide-abort') assert.equal(faults.length, 1, `${mode}: only the controlled failure is expected`);
      assert.equal(await page.evaluate(() => document.documentElement.dataset.rvRuntime), undefined);
    } finally { release?.(); await context.close(); }
  }
};
// The first screen (S27-959) at 1440 and 390, each with motion and with reduced motion. With motion the clip is the
// first of its framing's sources whose media and type this engine accepts, so AV1 wherever AV1 plays; it is the
// only clip fetched, and it plays once to its end and holds there, with the still drawn under it. With reduced
// motion no clip is fetched and the still is the clip's last frame.
const heroClip = async () => {
  evidence.hero_clip = [];
  for (const [width, height, framing] of [[1440, 900, 'pip'], [390, 844, 'pip-portrait']]) {
    for (const motion of ['no-preference', 'reduce']) {
      const context = await h.browser.newContext({ viewport: { width, height }, reducedMotion: motion, ...(width < 700 ? { isMobile: true, hasTouch: true } : {}) });
      try {
        const page = await context.newPage();
        watchErrors(page, evidence.errors, `hero ${width} ${motion}: `);
        const fetched = [];
        page.on('request', request => { const path = new URL(request.url()).pathname; if (path.endsWith('.mp4')) fetched.push(path); });
        await page.goto(`${h.base}/`, { waitUntil: 'load' });
        const state = await page.locator('.hero__clip').evaluate(async (video, moving) => {
          if (moving && !video.ended) await new Promise((accept, reject) => {
            video.addEventListener('ended', accept, { once: true });
            video.addEventListener('error', () => reject(new Error(`the hero clip failed with media error ${video.error?.code}`)), { once: true });
            setTimeout(() => reject(new Error(`the hero clip did not end: ${video.currentSrc || 'no source'} at ${video.currentTime} s, readyState ${video.readyState}`)), 20_000);
          });
          const still = document.querySelector('.hero__still');
          await still.decode();
          const first = [...video.querySelectorAll('source')].find(source => matchMedia(source.media).matches && video.canPlayType(source.type) !== '');
          const path = url => (url ? new URL(url).pathname : '');
          return { clip: path(video.currentSrc), first: path(first?.src), ended: video.ended, paused: video.paused, time: video.currentTime, duration: video.duration, still: path(still.currentSrc), drawn: still.naturalWidth > 0 };
        }, motion === 'no-preference');
        await page.screenshot({ path: join(h.dir, `hero-${width}-${motion}.png`) });
        const clips = [...new Set(fetched)];
        evidence.hero_clip.push({ width, motion, ...state, fetched: clips });
        assert.ok(state.drawn, `${width} ${motion}: the hero still is drawn`);
        if (motion === 'reduce') {
          assert.deepEqual([state.clip, clips], ['', []], `${width}: with reduced motion no hero clip loads`);
          assert.match(state.still, new RegExp(`^/landing/hero/${framing}-rest\\.(avif|webp)$`), `${width}: with reduced motion the still is the clip's last frame`);
        } else {
          assert.match(state.clip, new RegExp(`^/landing/hero/${framing}\\.(av1|hevc|h264)\\.mp4$`), `${width}: the clip is this framing's`);
          assert.equal(state.clip, state.first, `${width}: the clip is the first source ${h.engine} accepts`);
          assert.deepEqual(clips, [state.clip], `${width}: only the chosen clip is fetched`);
          assert.ok(state.ended && state.paused && state.duration < 5 && Math.abs(state.time - state.duration) < 0.1, `${width}: the clip plays once, under 5 s, and holds its last frame: ${JSON.stringify(state)}`);
          assert.match(state.still, new RegExp(`^/landing/hero/${framing}\\.(avif|webp)$`), `${width}: the still under the clip is its first frame`);
        }
      } finally {
        await context.close();
      }
    }
  }
};
try {
  if (wants('landing-deferred')) {
  await deferredLanding();
  check('Landing waits for intent, preserves early edits, and keeps a no-JavaScript report');
  if (selection.scope) completedCases.push('landing-deferred');
  }
  if (wants('landing-strip')) {
  await deferredStrip();
  check('The five-styles strip makes its frames only as the reader comes near it');
  if (selection.scope) completedCases.push('landing-strip');
  }
  if (wants('site-preview-access')) {
  await verifyRuntimeAccess(h.browser, { engine: h.engine, dir: h.dir, evidence });
  check('Cookie-gated site previews load their inlined runtime and the site report keeps SRI');
  if (selection.scope) completedCases.push('site-preview-access');
  }
  if (wants('runtime-loader')) {
  await runtimeLoaderControls();
  check('Runtime loader handles abandoned aborts and preserves live and non-abort failures');
  if (selection.scope) completedCases.push('runtime-loader');
  }
  if (wants('landing-hero')) {
  await heroClip();
  check('The first screen plays its framing\'s first playable clip once to its end, and with reduced motion fetches none and shows the last frame');
  if (selection.scope) completedCases.push('landing-hero');
  }
  if (wants('browser-journey')) {
  evidence.artifacts = await Promise.all(['/richview_runtime_bg.wasm', '/richview_runtime.js', '/showcase/showcase.css'].map(async path => {
    const response = await fetch(`${h.base}${path}`, { headers: { 'accept-encoding': 'identity' } });
    assert.equal(response.status, 200);
    const bytes = Buffer.from(await response.arrayBuffer());
    return { path, bytes: bytes.length, sha256: createHash('sha256').update(bytes).digest('hex') };
  }));
  const context = await h.browser.newContext({ viewport: { width: 1440, height: 1000 }, acceptDownloads: true, reducedMotion: 'reduce' });
  await context.addInitScript(instrument);
  // Probe native editing independently of RichView. WebKit loses parent-frame
  // textarea history when any child frame navigates, including a live preview.
  const historyProbe = await context.newPage();
  evidence.native_undo_probe = {};
  for (const navigate of [false, true]) {
    // Initial content is not an edit. WebKit can coalesce Playwright fill() with the next
    // keystroke, making undo erase both and invalidating this independent navigation probe.
    await historyProbe.setContent('<textarea>Retained source</textarea><iframe srcdoc="<p>Before</p>"></iframe><button>Download</button>');
    await historyProbe.frameLocator('iframe').getByText('Before', { exact: true }).waitFor();
    const input = historyProbe.locator('textarea');
    if (navigate) await input.evaluate(input => input.addEventListener('input', () => {
      document.querySelector('iframe').srcdoc = '<p>After</p>';
    }, { once: true }));
    await input.press(documentStart); await input.press('X');
    assert.equal(await input.inputValue(), 'XRetained source');
    if (navigate) await historyProbe.frameLocator('iframe').getByText('After', { exact: true }).waitFor();
    await historyProbe.locator('button').click(); await input.press('ControlOrMeta+Z');
    evidence.native_undo_probe[navigate ? 'after_frame_navigation' : 'plain_textarea'] = await input.inputValue();
  }
  await historyProbe.close();
  assert.equal(evidence.native_undo_probe.plain_textarea, 'Retained source', 'Native textarea undo must work before a child frame navigates');
  evidence.capabilities.nativeUndoAfterFrameNavigation = evidence.native_undo_probe.after_frame_navigation === 'Retained source';
  if (!evidence.capabilities.nativeUndoAfterFrameNavigation) {
    assert.equal(h.engine, 'webkit', 'Only the independently reproduced WebKit history limitation is accepted');
    assert.equal(evidence.native_undo_probe.after_frame_navigation, 'XRetained source');
    evidence.limitations = ['WebKit clears native source undo when the live preview iframe navigates'];
  }
  const gzipRequests = [];
  context.on('request', request => { if (request.url().endsWith('/richview_runtime_bg.wasm.gz')) gzipRequests.push(request.url()); });
  const page = await context.newPage();
  watchErrors(page, evidence.errors);
  const begin = performance.now();
  await page.goto(`${h.base}/playground`, { waitUntil: 'domcontentloaded' });
  let preview = await previewFrame(page);
  await settledCharts(preview, 1);
  assert.match(await page.locator('#diagnostics').innerText(), /Rust render/);
  assert.equal(await preview.locator('figure[data-rv-chart] canvas.rv-chart__canvas').count(), 1);
  assert.match(await preview.locator('body').innerText(), /Revenue rose/);
  evidence.timings.cold_playground_ms = performance.now() - begin;
  check('Rust source rendering mounts a live chart in the sandboxed preview');

  await page.evaluate(() => {
    const preview = document.querySelector('#preview');
    const state = window.__rvEvidence.sourceRendering = { input: 0, rendered: 0, loaded: 0 };
    document.querySelector('#source').addEventListener('input', () => state.input++);
    new MutationObserver(() => { state.rendered = state.input; state.loaded = -1; })
      .observe(preview, { attributes: true, attributeFilter: ['srcdoc'] });
    preview.addEventListener('load', () => { state.loaded = state.rendered; });
  });

  const editBegin = performance.now();
  await page.locator('#source').fill(fixture('six-types'));
  await settledCharts(preview, 6);
  assert.equal(await preview.locator('figure[data-rv-chart] canvas.rv-chart__canvas').count(), 6);
  const radii = await preview.locator('body').evaluate(() => window.__richview.charts()[2].geometry.panels.flatMap(panel => panel.items).filter(item => item.kind === 'point').map(item => item.radius));
  assert.equal(radii.length, 3);
  assert.ok(new Set(radii).size > 1, 'Scatter size encoding must change actual mark radii');
  evidence.timings.six_chart_live_edit_ms = performance.now() - editBegin;
  evidence.six_type_glyph_bounds = [];
  for (const [i, kind] of ['line', 'bar', 'scatter', 'area', 'pie', 'heatmap'].entries()) {
    await preview.locator('figure[data-rv-chart]').nth(i).screenshot({ path: join(h.dir, `six-${kind}-desktop.png`) });
    const bounds = await glyphAudit(preview, i);
    assert.equal(bounds.measured, true);
    assert.ok(!bounds.checks.some(check => check.kind === 'clipping'), `${kind} desktop labels must fit the live viewport`);
    evidence.six_type_glyph_bounds.push({ viewport: 'desktop', kind, ...bounds });
    if (kind === 'scatter') {
      assert.deepEqual(bounds.collisions, [], 'Desktop bubbles must not cover text boxes');
      (evidence.scatter_label_clearance ||= []).push({ viewport: 'desktop', circles: bounds.circles, collisions: bounds.collisions });
    }
  }
  check('All six chart types mount; numeric scatter size encoding changes the geometry');

  const beforeTheme = await preview.locator('html').evaluate(node => getComputedStyle(node).getPropertyValue('--rv-bg'));
  await page.locator('#playground-styles [data-theme="brutalist"]').click();
  // The playground opens in Sleek (dark); Brutalist is a light style with its own background token.
  await page.waitForFunction(() => document.querySelector('#preview').getAttribute('srcdoc').includes('0.985 0.004 70'));
  await settledCharts(preview, 6);
  assert.notEqual(await preview.locator('html').evaluate(node => getComputedStyle(node).getPropertyValue('--rv-bg')), beforeTheme);
  await preview.locator('.rv-chart__frame').first().focus();
  await page.keyboard.press('ArrowRight');
  await preview.locator('.rv-chart__readout:not([hidden])').first().waitFor();
  const readout = await preview.locator('.rv-chart__readout').first().innerText();
  assert.match(readout, /\bValue\b/);
  assert.match(readout, /\b100\b/);
  check('Theme selection remounts live Rust charts and keyboard readout shows source values');

  assert.deepEqual(gzipRequests, [], 'Offline runtime must not be fetched before explicit export');
  const downloadEvent = page.waitForEvent('download');
  await page.locator('#download-html').click();
  const download = await downloadEvent;
  const offlineFile = join(h.dir, 'offline.html');
  await download.saveAs(offlineFile);
  assert.equal(gzipRequests.length, 1);
  const offlineText = readFileSync(offlineFile, 'utf8');
  assert.match(offlineText, /DecompressionStream/, 'Offline export must carry compressed Wasm');
  const embedded = offlineText.match(/atob\("([A-Za-z0-9+/=]+)"\)/);
  assert.ok(embedded, 'Offline HTML contains the complete embedded runtime');
  const compressedRuntime = Buffer.from(embedded[1], 'base64');
  const rawRuntime = gunzipSync(compressedRuntime);
  evidence.offline_runtime = { compressed_bytes: compressedRuntime.length, raw_bytes: rawRuntime.length, sha256: createHash('sha256').update(rawRuntime).digest('hex') };
  assert.equal(evidence.offline_runtime.sha256, evidence.artifacts.find(asset => asset.path === '/richview_runtime_bg.wasm').sha256,
    'One offline decompression must reproduce the current runtime exactly');
  const offline = await context.newPage();
  watchErrors(offline, evidence.errors, 'offline: ');
  const network = [];
  await offline.route('**/*', route => {
    if (/^(file|data|blob):/.test(route.request().url())) return route.continue();
    network.push(route.request().url());
    return route.abort();
  });
  await offline.goto(pathToFileURL(offlineFile).href);
  await settledCharts(offline, 6);
  await offline.locator('.rv-chart__frame').first().focus();
  await offline.keyboard.press('ArrowRight');
  await offline.locator('.rv-chart__readout:not([hidden])').first().waitFor();
  assert.deepEqual(network, []);
  await offline.screenshot({ path: join(h.dir, 'offline.png'), fullPage: true });
  await offline.close();
  check('Downloaded HTML mounts and responds to keyboard with all network requests blocked');

  // Report CSP allows inline styles and data fonts; the trusted parent must resolve math first.
  await inputValue(page.locator('#source'), '---\ntitle: Offline mathematics\n---\n\nInline $x^2 + \\alpha$.\n\n$$\\sum_{i=1}^n i$$\n');
  await preview.locator('math').first().waitFor();
  const mathFace = JSON.parse(readFileSync(join(root, 'crates/richview-core/assets/font-manifest.json'), 'utf8')).faces.find(face => face.family === 'STIX Two Math');
  const mathPayload = html => {
    const encoded = html.match(/@font-face\s*\{\s*font-family:\s*"STIX Two Math";[^}]*?base64,([A-Za-z0-9+/=]+)/);
    assert.ok(encoded, 'Rendered MathML must carry its inline math face');
    const bytes = Buffer.from(encoded[1], 'base64');
    assert.equal(bytes.length, mathFace.woff2_bytes);
    assert.equal(createHash('sha256').update(bytes).digest('hex'), mathFace.sha256);
    assert.doesNotMatch(html, /<link[^>]*href="[^"]*fonts-math\.css"/);
  };
  mathPayload(await preview.content());
  const mathDownload = page.waitForEvent('download');
  await page.locator('#download-html').click();
  const mathFile = join(h.dir, 'offline-math.html');
  await (await mathDownload).saveAs(mathFile);
  mathPayload(readFileSync(mathFile, 'utf8'));
  const mathOffline = await context.newPage();
  const mathNetwork = [];
  try {
    await mathOffline.route('**/*', route => {
      if (/^(file|data|blob):/.test(route.request().url())) return route.continue();
      mathNetwork.push(route.request().url());
      return route.abort();
    });
    await mathOffline.goto(pathToFileURL(mathFile).href);
    await mathOffline.locator('math').first().waitFor();
    await mathOffline.waitForFunction(() => [...document.fonts].some(face => face.family.replaceAll('"', '') === 'STIX Two Math' && face.status === 'loaded'));
    assert.deepEqual(mathNetwork, []);
  } finally { await mathOffline.close(); }
  check('Math previews and offline downloads embed and load the pinned face under the report CSP');

  // A failed math-font fetch is not cached: the next render in the same page asks again and succeeds.
  const retry = await context.newPage();
  try {
    let blocked = true, mathRequests = 0;
    await retry.route('**/fonts-math.css', route => { mathRequests++; return blocked ? route.abort() : route.continue(); });
    await retry.goto(`${h.base}/playground`, { waitUntil: 'domcontentloaded' });
    const mathSource = '---\ntitle: Mathematics\n---\n\nInline $x^2 + \\alpha$.\n';
    await inputValue(retry.locator('#source'), mathSource);
    await retry.waitForFunction(() => document.querySelector('#diagnostics')?.textContent.includes('math font could not be loaded'));
    blocked = false;
    await inputValue(retry.locator('#source'), `${mathSource}\nAgain.\n`);
    await (await previewFrame(retry)).locator('math').first().waitFor();
    assert.equal(mathRequests, 2, 'The second render asks for the math font again');
  } finally { await retry.close(); }
  check('A failed math font request is retried by the next render');

  // The same holds for the font catalog: a failed request leaves system fonts for one render only.
  const catalogRetry = await context.newPage();
  try {
    let blocked = true, catalogRequests = 0;
    await catalogRetry.route('**/richview-fonts.css', route => { catalogRequests++; return blocked ? route.abort() : route.continue(); });
    await catalogRetry.goto(`${h.base}/playground`, { waitUntil: 'domcontentloaded' });
    await catalogRetry.waitForFunction(() => document.querySelector('#diagnostics')?.textContent.includes('falls back to system fonts'));
    blocked = false;
    await inputValue(catalogRetry.locator('#source'), '---\ntitle: Fonts\n---\n\nAgain.\n');
    await catalogRetry.waitForFunction(() => !document.querySelector('#diagnostics')?.textContent.includes('falls back to system fonts'));
    assert.ok(catalogRequests >= 2, 'The next render asks for the font catalog again');
  } finally { await catalogRetry.close(); }
  check('A failed font catalog request is retried by the next render');

  // Hold one real worker response while two further edits enter the single pending slot.
  const dense = fixture('dense');
  const denseVersion = label => dense.replace('title: Dense series\n', `title: ${label}\n`).replace('title: Dense series with exact observations', `title: ${label} exact observations`);
  const queueEdit = async label => {
    const previous = await page.evaluate(() => window.__rvEvidence.queuedEdits);
    await inputValue(page.locator('#source'), denseVersion(label));
    await page.waitForFunction(previous => window.__rvEvidence.queuedEdits > previous, previous);
  };
  // Share the S27-844 recovery budget and receipt with the later chart draws.
  evidence.preview_reloads ||= [];
  const raceTitle = 'Race C exact observations';
  for (;;) {
    await page.evaluate(() => {
      const evidence = window.__rvEvidence;
      evidence.requests = []; evidence.replies = []; evidence.gateReply = true; evidence.queuedEdits = 0;
      const status = document.querySelector('#diagnostics');
      new MutationObserver(() => {
        if (status.textContent.startsWith('Waiting for the current Rust render')) window.__rvEvidence.queuedEdits++;
      }).observe(status, { childList: true, characterData: true, subtree: true });
    });
    await inputValue(page.locator('#source'), denseVersion('Race A'));
    await page.waitForFunction(() => window.__rvEvidence.requests.some(request => request.source.includes('Race A')));
    await page.waitForFunction(() => Boolean(window.__rvEvidence.heldReply), null, { timeout: 60_000 });
    await queueEdit('Race B');
    // Deliberately exceed the old 1200ms timer. Delivery still waits for explicit release.
    await page.waitForTimeout(1_300);
    assert.equal(await page.evaluate(() => Boolean(window.__rvEvidence.heldReply)), true);
    await queueEdit('Race C');
    await page.evaluate(() => window.__rvEvidence.releaseReply());
    try {
      await preview.locator('body').filter({ hasText: raceTitle }).waitFor({ timeout: 60_000 });
      break;
    } catch (error) {
      const failure = await capturePreviewFailure(page, preview, raceTitle, error);
      const recovery = evidence.preview_reloads.length + 1;
      const screenshot = join(process.env.RV_GATE_PREVIEW_RELOAD_DIR || h.dir, `preview-reload-${recovery}.png`);
      let refusal = reloadRefusal(failure, { engine: h.engine, reloads: evidence.preview_reloads, title: raceTitle });
      // A retry must not erase evidence that the superseded pending edit was rendered.
      const requests = failure.parent?.worker?.requests;
      if (!refusal && (!Array.isArray(requests)
        || requests.filter(request => request.source?.includes('Race A')).length !== 1
        || requests.some(request => request.source?.includes('Race B'))
        || requests.filter(request => request.source?.includes('Race C')).length !== 1)) {
        refusal = 'the failed race did not preserve the expected worker request sequence';
      }
      if (!refusal) await page.locator('#preview').screenshot({ path: screenshot, timeout: 10_000 }).catch(shot => { refusal = `no parent-side screenshot of the preview: ${shot.message}`; });
      if (refusal) {
        evidence.worker_race_failure = { ...failure, refusal };
        throw error;
      }
      const recovered = { case: raceTitle, recovery, screenshot, failure, restore: { status: 'not started' } };
      evidence.preview_reloads.push(recovered);
      recordPreviewReload({ title: raceTitle, recovery, screenshot });
      await page.goto(`${h.base}/playground`, { waitUntil: 'domcontentloaded' });
      preview = await previewFrame(page);
      await settledCharts(preview, 1);
      // Navigation resets the original source observer used by the later download checks.
      await page.evaluate(() => {
        const preview = document.querySelector('#preview');
        const state = window.__rvEvidence.sourceRendering = {
          input: 0, rendered: 0, loaded: 0, generation: 0, loadedGeneration: -1,
          changedAt: performance.now(), loadedAt: null, styleClickedAt: null, restoreMutations: [],
        };
        document.querySelector('#source').addEventListener('input', () => state.input++);
        new MutationObserver(records => {
          state.rendered = state.input; state.loaded = -1; state.loadedAt = null;
          state.generation += records.length; state.changedAt = performance.now();
          if (state.styleClickedAt !== null) {
            state.restoreMutations.push(...records.map(() => state.changedAt - state.styleClickedAt));
          }
        })
          .observe(preview, { attributes: true, attributeFilter: ['srcdoc'] });
        preview.addEventListener('load', () => {
          state.loaded = state.rendered; state.loadedGeneration = state.generation; state.loadedAt = performance.now();
        });
      });
      // The earlier theme check selected Brutalist; keep the rest of this suite on that style.
      await page.evaluate(() => { window.__rvEvidence.sourceRendering.styleClickedAt = performance.now(); });
      try {
        await page.locator('#playground-styles [data-theme="brutalist"]').click();
        preview = await settleRestoredPreview(page, preview, previewFrame, settledCharts);
      } finally {
        recovered.restore = await page.evaluate(() => {
          const state = window.__rvEvidence.sourceRendering;
          const mutationTimesMs = state.restoreMutations.slice();
          state.styleClickedAt = null;
          return { mutationCount: mutationTimesMs.length, mutationTimesMs };
        }).catch(error => ({ error: String(error) }));
      }
    }
  }
  await settledCharts(preview, 1);
  await page.waitForTimeout(250);
  const race = await page.evaluate(() => ({ requests: window.__rvEvidence.requests, replies: window.__rvEvidence.replies }));
  assert.equal(race.requests.filter(request => request.source.includes('Race A')).length, 1);
  assert.equal(race.requests.filter(request => request.source.includes('Race B')).length, 0, 'The superseded pending edit must never render');
  assert.equal(race.requests.filter(request => request.source.includes('Race C')).length, 1);
  assert.match(await page.locator('#diagnostics').innerText(), /Rust render/);
  assert.doesNotMatch(await preview.locator('body').innerText(), /Race A|Race B/);
  evidence.worker_race = race;
  check('Dense edits parse/render in the real Rust worker; a stale reply cannot replace the latest pending source');

  await page.evaluate(() => { window.__rvEvidence.requests = []; window.__rvEvidence.replies = []; window.__rvEvidence.gateReply = true; window.__rvEvidence.failNextRequest = true; });
  await inputValue(page.locator('#source'), denseVersion('Failed race'));
  await page.waitForFunction(() => window.__rvEvidence.replies.some(reply => reply.error));
  await queueEdit('Recovered race');
  await page.evaluate(() => window.__rvEvidence.releaseReply());
  await preview.locator('body').filter({ hasText: 'Recovered race exact observations' }).waitFor({ timeout: 60_000 });
  await settledCharts(preview, 1);
  assert.match(await page.locator('#diagnostics').innerText(), /Rust render/);
  evidence.stale_worker_error = await page.evaluate(() => ({ requests: window.__rvEvidence.requests, replies: window.__rvEvidence.replies }));
  check('A delayed Rust worker error cannot replace the newer successful source or status');

  const sourceInput = page.locator('#source');
  const sourcePager = page.locator('.pg__source .rvc-source-window');
  const fullSource = denseVersion('Recovered race');
  assert.equal(await sourceInput.getAttribute('data-source-paged'), 'true');
  const visibleSource = await sourceInput.inputValue();
  assert.ok(Buffer.byteLength(visibleSource) <=64*1024 && visibleSource.split('\n').length <=513);
  assert.equal(evidence.stale_worker_error.requests.at(-1).source_sha256, createHash('sha256').update(fullSource).digest('hex'));
  const saveSource = async name => {
    // A pending status also mentions "Rust render" and the old frame can still be started.
    // Finish the latest input's replacement document before this independent download.
    await page.waitForFunction(() => {
      const state = window.__rvEvidence.sourceRendering;
      return state.input === state.rendered && state.loaded === state.rendered
        && /^Rust render /.test(document.querySelector('#diagnostics').textContent);
    }, null, { timeout: 45_000 });
    await preview.locator('html[data-rv-runtime="started"]').waitFor({ timeout: 45_000 });
    const event = page.waitForEvent('download'); await page.locator('#download-source').click();
    const downloaded = await event; const path = join(h.dir, name); await downloaded.saveAs(path);
    return readFileSync(path, 'utf8');
  };
  assert.equal(await saveSource('dense-retained.rmd'), fullSource, 'Source download must retain every hidden line');
  await sourcePager.getByRole('button', { name: 'Next', exact: true }).click();
  const firstLine = Number(await sourceInput.getAttribute('data-source-start'));
  assert.ok(firstLine >1);
  const windowSource = await sourceInput.inputValue();
  const prefix = fullSource.split('\n').slice(0, firstLine-1).join('\n')+'\n';
  assert.equal(fullSource.slice(prefix.length, prefix.length+windowSource.length), windowSource);
  const changedWindow = windowSource.replace(/value: (\d+(?:\.\d+)?)/, (_whole, value) => `value: ${Number(value)+123}`);
  assert.notEqual(changedWindow, windowSource);
  const editedSource = fullSource.slice(0,prefix.length)+changedWindow+fullSource.slice(prefix.length+windowSource.length);
  await sourceInput.evaluate((input, value) => { input.value=value;input.dispatchEvent(new InputEvent('input',{bubbles:true,inputType:'insertText'})); }, changedWindow);
  await page.waitForFunction(() => window.__rvEvidence.requests.length>=3);
  await settledCharts(preview, 1);
  assert.equal(await saveSource('dense-window-edited.rmd'), editedSource, 'A page edit must preserve the hidden prefix and suffix exactly');
  const jumpLine = editedSource.split('\n').length-5;
  await sourcePager.locator('input').fill(String(jumpLine));await sourcePager.locator('input').dispatchEvent('change');
  assert.equal(Number(await sourceInput.getAttribute('data-source-start')), jumpLine);
  assert.ok((await sourceInput.inputValue()).trimEnd().endsWith('```'));
  await page.evaluate(() => Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText(text){window.__copiedSource=text;return Promise.resolve();}}}));
  await page.locator('#copy-source').click();
  assert.equal(await page.evaluate(() => window.__copiedSource), editedSource, 'Copy source must include every page');
  await sourceInput.focus();await page.keyboard.press('ControlOrMeta+A');
  const copiedByKeyboard = await sourceInput.evaluate(dispatchClipboard, { type: 'copy' });
  assert.equal(copiedByKeyboard.text, editedSource, 'Ctrl+A copy must include the complete retained document');
  assert.equal(copiedByKeyboard.prevented, true);
  await sourcePager.getByRole('button',{name:'Previous',exact:true}).click();
  assert.equal(await saveSource('dense-navigated.rmd'),editedSource);
  await sourceInput.focus();await page.keyboard.press('ControlOrMeta+A');
  const historyWindow = await sourceInput.inputValue();
  await sourceInput.evaluate((input,value)=>{input.value=value;input.dispatchEvent(new InputEvent('input',{bubbles:true,inputType:'historyUndo'}));},historyWindow);
  assert.equal(await saveSource('dense-history.rmd'),editedSource,'Native page history must never delete hidden source after Ctrl+A');
  await sourceInput.focus();await page.keyboard.press('ControlOrMeta+A');
  const cut = await sourceInput.evaluate(dispatchClipboard, { type: 'cut' });
  assert.equal(cut.text,editedSource,'Full-document cut must put every hidden byte on the clipboard');assert.equal(cut.prevented,true);
  assert.equal(await saveSource('dense-cut.rmd'),'');
  await sourceInput.evaluate(dispatchClipboard, { type: 'paste', text: cut.text });
  assert.equal(await saveSource('dense-pasted.rmd'),editedSource,'Full paste must restore the exact retained source');
  await sourceInput.focus();await page.keyboard.press('ControlOrMeta+A');
  assert.equal((await sourceInput.evaluate(dispatchClipboard, { type: 'cut', missing: true })).prevented, true, 'A failed full clipboard cut must prevent a page-only host cut');
  assert.equal(await saveSource('dense-cut-failed.rmd'),editedSource,'Missing clipboard access must leave every source byte intact');
  const pagedDownload=page.waitForEvent('download');await page.locator('#download-html').click();const pagedOfflineFile=join(h.dir,'paged-offline.html');await (await pagedDownload).saveAs(pagedOfflineFile);
  const pagedOffline=await context.newPage();watchErrors(pagedOffline,evidence.errors,'paged offline: ');
  await pagedOffline.route('**/*',route=>/^(file|data|blob):/.test(route.request().url())?route.continue():route.abort());
  await pagedOffline.goto(pathToFileURL(pagedOfflineFile).href);await settledCharts(pagedOffline, 1);
  const offlineRows=await pagedOffline.locator('script[data-rv-payload]').evaluate(script=>JSON.parse(script.textContent).rows.length);
  assert.equal(offlineRows,35040,'Paged-source offline HTML must retain every original observation');await pagedOffline.close();
  evidence.paged_source = { source_bytes:Buffer.byteLength(editedSource), visible_bytes:Buffer.byteLength(await sourceInput.inputValue()), first_line:Number(await sourceInput.getAttribute('data-source-start')), total_lines:Number(await sourceInput.getAttribute('data-source-lines')), complete_sha256:createHash('sha256').update(editedSource).digest('hex') };
  await sourcePager.screenshot({path:join(h.dir,'paged-source-desktop.png')});
  check('Paged source navigation/edit/history/cut/paste preserve every hidden byte for rendering, copy/download and offline HTML');

  // Use the browser's actual editing history, including a redo entry whose source
  // page is text-identical to the next page. Synthetic history events alone do
  // not prove that a page's native undo stack was detached during navigation.
  if (evidence.capabilities.nativeUndoAfterFrameNavigation) {
    const equalPages='Repeated row\n'.repeat(1800);await inputValue(sourceInput,equalPages);
    const equalWindow=await sourceInput.inputValue();await sourceInput.focus();await sourceInput.press(documentStart);await sourceInput.press('X');
    assert.ok((await saveSource('equal-page-edited.rmd')).includes('X'));
    await sourceInput.press('ControlOrMeta+Z');assert.equal(await saveSource('equal-page-undone.rmd'),equalPages);
    await sourcePager.getByRole('button',{name:'Next',exact:true}).click();assert.equal(await sourceInput.inputValue(),equalWindow);
    await sourceInput.focus();await sourceInput.press('ControlOrMeta+Z');await sourceInput.press('ControlOrMeta+Shift+Z');
    assert.equal(await saveSource('equal-page-navigation-history.rmd'),equalPages,'Native undo/redo from an equal-text previous page must never mutate the new page');
    check('Real browser undo/redo cannot cross equal-text pages');
  }
  const mixedEnds='Mixed line endings\r\n'+Array.from({length:700},(_,i)=>`Row ${i}${i%3===0?'\r\n':i%3===1?'\r':'\n'}`).join('');
  await sourceInput.focus();await sourceInput.press('ControlOrMeta+A');
  await sourceInput.evaluate(dispatchClipboard, { type: 'paste', text: mixedEnds });
  assert.equal(await saveSource('mixed-endings-import.rmd'),mixedEnds);
  await sourceInput.focus();await sourceInput.press(documentStart);await sourceInput.press('X');
  assert.equal(await saveSource('mixed-endings-edited.rmd'),'X'+mixedEnds,'A real page edit must retain untouched CRLF, CR and LF endings');
  check('Real edits retain untouched mixed source line endings');

  const shortLines='---\ntitle: Many short lines\nversion: 0.0.1\n---\n'+'\n'.repeat(20000)+'Last retained line.\n';
  assert.ok(Buffer.byteLength(shortLines)<128*1024);await inputValue(sourceInput,shortLines);
  assert.equal(await sourceInput.getAttribute('data-source-paged'),'true');assert.ok((await sourceInput.inputValue()).split('\n').length<=513);
  assert.equal(await saveSource('short-line-source.rmd'),shortLines,'Many short lines must page without truncating source');
  const longLine='---\ntitle: A long Unicode line\nversion: 0.0.1\n---\n\n'+'příliš🧭'.repeat(20000)+'\n';await inputValue(sourceInput,longLine);
  const longPage=await sourceInput.inputValue();assert.ok(Buffer.byteLength(longPage)<=64*1024);assert.ok(!longPage.includes('\uFFFD'));
  await sourcePager.getByRole('button',{name:'Next',exact:true}).click();assert.ok(Buffer.byteLength(await sourceInput.inputValue())<=64*1024);
  assert.equal(await saveSource('long-line-source.rmd'),longLine,'Long UTF-8 lines must preserve complete byte content across segments');

  const tableSource = `---\ntitle: Sort evidence\nversion: 0.0.1\n---\n\n\`\`\`table\ndata:\n  inline:\n  - {name: C, amount: 30}\n  - {name: A, amount: 10}\n  - {name: B, amount: 20}\ncolumns:\n  - {key: name, label: Name}\n  - {key: amount, label: Amount, format: number}\nmax_rows: 2\n\`\`\`\n`;
  await inputValue(page.locator('#source'), tableSource);
  assert.equal(await sourceInput.getAttribute('data-source-paged'),'false');
  assert.equal(await sourcePager.isVisible(),false);
  assert.equal(await saveSource('dense-to-small.rmd'),tableSource,'Replacing a paged document with small source must remove every old hidden line');
  await preview.locator('[data-rv-sort="amount"]').first().waitFor();
  await preview.locator('html[data-rv-runtime="started"]').waitFor({ timeout: 45_000 });
  await preview.locator('[data-rv-sort="amount"]').first().click();
  expectEqual('Sorting ascending with max_rows 2 shows the two smallest of all rows', await preview.locator('.rv-table > table tbody tr td:first-child').allTextContents(), ['A', 'B']);
  assert.equal(await preview.locator('.rv-table > table th').nth(1).getAttribute('aria-sort'), 'ascending');
  await preview.locator('[data-rv-sort="amount"]').first().click();
  expectEqual('Sorting descending with max_rows 2 shows the two largest of all rows', await preview.locator('.rv-table > table tbody tr td:first-child').allTextContents(), ['C', 'B']);
  await preview.locator('.rv-table__overflow summary').click();
  assert.match(await preview.locator('.rv-table__overflow').innerText(), /A/);
  check('Report table sorting toggles ascending/descending with accessible state and overflow rows');

  await page.goto(`${h.base}/showcase/openai-vs-anthropic/`, { waitUntil: 'domcontentloaded' });
  await page.locator('#theme option').nth(20).waitFor({ state: 'attached' });
  const flagship = page.frameLocator('#report-frame');
  await settledCharts(flagship, flagshipCharts);
  const narrative = node => {
    const clone = node.cloneNode(true);
    // Axis labels legitimately reflow with the active font. Report prose and source
    // payloads are invariant; they must not be replaced during presentation changes.
    clone.querySelectorAll('.rv-chart,script,.stats').forEach(node => node.remove());
    return clone.textContent.replace(/\s+/g, ' ').trim();
  };
  const text = await flagship.locator('body').evaluate(narrative);
  // Each render embeds its own style (`theme`, `print`) in the chart payloads; the data, encoding and
  // colors are what stay fixed across styles.
  const sourcePayloads = async () => (await flagship.locator('script[data-rv-payload]').allTextContents()).map(text => { const { theme, print, ...rest } = JSON.parse(text); return rest; });
  const payloads = await sourcePayloads();
  // The style tokens ship inside the page (`#showcase-tokens`); /showcase/themes.json is gone (FEA-SF-46).
  const catalog = await page.locator('#showcase-tokens').evaluate(node => JSON.parse(node.textContent));
  const pills = await page.locator('#style-pills [data-theme]').evaluateAll(buttons => buttons.map(button => button.dataset.theme));
  const others = await page.locator('#theme option:not([value=""])').evaluateAll(options => options.map(option => option.value));
  const themes = [...pills, ...others];
  assert.equal(pills.length, 5);
  assert.equal(themes.length, 25);
  assert.equal(new Set(themes).size, themes.length, 'No style is offered twice');
  const themeTimes = [];
  const actualStyles = [];
  for (const theme of themes) {
    const start = performance.now();
    await showStyle(page, theme);
    await settledCharts(flagship, flagshipCharts);
    assert.equal(await flagship.locator('figure[data-rv-chart] canvas.rv-chart__canvas').count(), flagshipCharts);
    const expected = { id: theme, tokens: catalog[theme] };
    assert.ok(expected.tokens, `${theme} has tokens in the page`);
    const actual = await flagship.locator('html').evaluate(async (node, expected) => {
      await node.ownerDocument.fonts.ready;
      const component = node.querySelector('.rv-big') || node.querySelector('.rv-callout');
      const style = component ? getComputedStyle(component) : null;
      const font = getComputedStyle(node.querySelector('.rv-report__body') || node.querySelector('body')).fontFamily;
      return { id: expected.id, bg: getComputedStyle(node).getPropertyValue('--rv-bg').trim(), font, component: style ? { background: style.backgroundColor, color: style.color, border: style.borderTop, radius: style.borderRadius, font: style.fontFamily } : null };
    }, expected);
    assert.equal(actual.bg, expected.tokens.bg, `${theme} must apply its actual background token`);
    // The reports embed Source Serif 4 under the family name RichView Serif.
    const tokenFont = expected.tokens['font-text'].split(',')[0].replace(/["']/g, '').trim();
    const font = { 'Source Serif 4': 'RichView Serif' }[tokenFont] ?? tokenFont;
    assert.ok(actual.font.includes(font), `${theme} must apply text font ${font}: ${actual.font}`);
    actualStyles.push(actual);
    themeTimes.push(performance.now() - start);
  }
  assert.equal(await flagship.locator('body').evaluate(narrative), text);
  assert.deepEqual(await sourcePayloads(), payloads);
  assert.notDeepEqual(actualStyles.find(item => item.id === 'sleek').component, actualStyles.find(item => item.id === 'brutalist').component, 'Production themes must change actual report components');
  await page.setViewportSize({width:900,height:1000});
  await flagship.locator('body').evaluate(()=>new Promise(accept=>requestAnimationFrame(()=>requestAnimationFrame(()=>requestAnimationFrame(accept)))));
  await flagship.locator('body').evaluate(()=>window.__richview.settled());
  const resizedGeometry=await flagship.locator('body').evaluate(()=>window.__richview.charts().map(chart=>({stage:document.getElementById(chart.id).querySelector('.rv-chart__frame').clientWidth,model:chart.width})));
  assert.ok(resizedGeometry.length===flagshipCharts && resizedGeometry.every(size=>Math.abs(size.model-Math.max(240,size.stage))<.5),'ResizeObserver must update every live chart scale to the new container width');
  evidence.resized_flagship_geometry=resizedGeometry;
  await page.setViewportSize({width:1440,height:1000});
  await flagship.locator('body').evaluate(()=>new Promise(accept=>requestAnimationFrame(()=>requestAnimationFrame(()=>requestAnimationFrame(accept)))));
  // The `chart` treatment parameter is deleted with the showcase's other dormant controls
  // (FEA-SF-37); each style becomes its own render of the flagship (FEA-SF-36).
  for (const theme of ['scientific', 'sleek']) {
    await showStyle(page, theme);
    await settledCharts(flagship, flagshipCharts);
    await flagship.locator('body').evaluate(async () => { window.scrollTo(0, 0); await document.fonts.ready; await new Promise(accept => requestAnimationFrame(() => requestAnimationFrame(accept))); });
    await page.locator('#report-frame').screenshot({ path: join(h.dir, `showcase-${theme}-desktop.png`) });
    await flagship.locator('.rv-chart').first().screenshot({ path: join(h.dir, `showcase-${theme}-chart-desktop.png`) });
    const bounds = await glyphAudit(flagship, 0);
    assert.equal(bounds.measured, true);
    assert.ok(!bounds.checks.some(check => check.kind === 'label-collision' && check.related?.includes('Anthropic share, %')), `${theme} actual glyph bounds must separate the axis title and subtitle`);
    (evidence.flagship_glyph_bounds ||= []).push({ viewport: 'desktop', theme, ...bounds });
  }
  evidence.timings.themes_ms = themeTimes;
  evidence.themes = actualStyles;
  await page.screenshot({ path: join(h.dir, 'showcase.png'), fullPage: true });
  check(`All themes change actual tokens/fonts/components and keep ${flagshipCharts} live charts and the report source`);

  // The /charts/ examples page is gone with the legacy chart view; its route redirects to the docs
  // chart gallery, which renders its datasets as .rmd examples (ARC-40, FEA-SF-44).
  const redirects = readFileSync(join(root, 'dist/web/_redirects'), 'utf8');
  assert.match(redirects, /^\/charts \/docs\/charts 301$/m);
  assert.match(redirects, /^\/charts\/\* \/docs\/charts 301$/m);
  assert.equal(existsSync(join(root, 'dist/web/charts')), false, 'No /charts/ page is staged');
  assert.equal((await page.goto(`${h.base}/docs/charts.html`, { waitUntil: 'domcontentloaded' })).status(), 200);
  // Verify the destination's actual charts before leaving it. Firefox can cancel an in-flight
  // fetch before pagehide; that live-document failure must still reject in the loader controls.
  evidence.gallery_charts = await page.locator('figure[data-rv-chart]').count();
  assert.ok(evidence.gallery_charts > 0, 'The gallery contains live chart examples');
  await settledCharts(page, evidence.gallery_charts);
  check('/charts/ redirects to the docs chart gallery');

  // Source behaviours on report charts, driven through the playground's real Rust render and
  // read from the report runtime's geometry model.
  await page.goto(`${h.base}/playground`, { waitUntil: 'domcontentloaded' });
  preview = await previewFrame(page);
  await settledCharts(preview, 1);
  const reportFor = chart => `---\ntitle: Source behaviour\nversion: 0.0.1\n---\n\n\`\`\`chart\n${JSON.stringify(chart)}\n\`\`\`\n`;
  // Playwright's Firefox can lose the preview frame (microsoft/playwright#43124): Firefox draws the
  // chart, but the frame never answers. Then a parent-side screenshot keeps the preview, the
  // playground reloads and the same draw runs again, so the case's assertions run on the new frame.
  // reloadRefusal() bounds it and the gate's receipt lists each reload (S27-844). The reload goes
  // when DEL-49's pin reaches a stable Playwright with the fix.
  evidence.preview_reloads ||= [];
  const draw = async chart => {
    await inputValue(page.locator('#source'), reportFor(chart));
    for (;;) {
      try {
        await preview.locator('.rv-chart__title', { hasText: chart.title }).waitFor({ timeout: 45_000 });
        break;
      } catch (error) {
        const failure = await capturePreviewFailure(page, preview, chart.title, error);
        const recovery = evidence.preview_reloads.length + 1;
        const screenshot = join(process.env.RV_GATE_PREVIEW_RELOAD_DIR || h.dir, `preview-reload-${recovery}.png`);
        let refusal = reloadRefusal(failure, { engine: h.engine, reloads: evidence.preview_reloads, title: chart.title });
        if (!refusal) await page.locator('#preview').screenshot({ path: screenshot, timeout: 10_000 }).catch(shot => { refusal = `no parent-side screenshot of the preview: ${shot.message}`; });
        if (refusal) {
          evidence.preview_failure = { ...failure, refusal };
          throw error;
        }
        evidence.preview_reloads.push({ case: chart.title, recovery, screenshot, failure });
        recordPreviewReload({ title: chart.title, recovery, screenshot });
        await page.goto(`${h.base}/playground`, { waitUntil: 'domcontentloaded' });
        preview = await previewFrame(page);
        await settledCharts(preview, 1);
        await inputValue(page.locator('#source'), reportFor(chart));
      }
    }
    await settledCharts(preview, 1);
  };
  const modelOf = () => preview.locator('body').evaluate(() => window.__richview.charts()[0]);
  const chartFrame = () => preview.locator('.rv-chart__frame');
  const tableRows = async () => {
    await preview.locator('.rv-chart__action--table').click();
    const rows = await preview.locator('.rv-chart__table tbody tr').evaluateAll(trs => trs.map(tr => [...tr.children].map(cell => cell.textContent)));
    await preview.locator('.rv-chart__action--table').click();
    return rows;
  };
  const observationCount = model => model.geometry.panels.reduce((count, panel) => count + panel.observations.length, 0);

  evidence.numeric_source_zoom = [];
  for (const type of ['scatter', 'line', 'area']) {
    const zooms = {};
    for (const [name, xs] of Object.entries({ numbers: [0, 4, 6, 10], strings: [0, '4.000', '6.000', 10] })) {
      await draw({ type, title: `Zoom ${type} ${name}`, encoding: { x: 'x', y: 'y' }, data: { inline: xs.map((x, i) => ({ x, y: i + 1 })) } });
      await chartFrame().focus(); await page.keyboard.press('+');
      const zoomed = await modelOf();
      // Lines and areas zoom inside their data; a scatter plot inside its padded view, so edge
      // points draw whole (FEA-CH-49).
      const [lo, hi] = type === 'scatter' ? zoomed.geometry.panels[0].x.extent : [0, 10];
      assert.ok(lo <= 0 && hi >= 10, `${type}/${name}: the zoom limit covers the data`);
      assert.ok(zoomed.zoom && zoomed.zoom[0] >= lo && zoomed.zoom[1] <= hi && zoomed.zoom[1] - zoomed.zoom[0] < hi - lo, `${type}/${name}: a numeric x axis zooms: ${JSON.stringify(zoomed.zoom)}`);
      assert.equal(await preview.locator('.rv-chart__action--reset').isVisible(), true);
      assert.deepEqual((await tableRows()).map(row => row.at(-1)), ['1', '2', '3', '4'], `${type}/${name}: the table keeps every source row while zoomed`);
      await chartFrame().focus(); await page.keyboard.press('Escape');
      assert.equal((await modelOf()).zoom, null, `${type}/${name}: Escape resets the zoom`);
      zooms[name] = zoomed.zoom;
    }
    assert.deepEqual(zooms.strings, zooms.numbers, `${type}: numeric strings zoom like numbers`);
    evidence.numeric_source_zoom.push({ type, zoom: zooms.numbers });
  }
  for (const type of ['line', 'area']) {
    await draw({ type, title: `Zoom ${type} years`, encoding: { x: 'x', y: 'y' }, data: { inline: [2020, '2021', '2022', 2023].map((x, i) => ({ x, y: i + 1 })) } });
    await chartFrame().focus(); await page.keyboard.press('+');
    const years = (await modelOf()).zoom;
    assert.ok(years && years[0] >= 2020 && years[1] <= 2023, `${type}: year-like values zoom on a numeric axis: ${JSON.stringify(years)}`);
  }
  await draw({ type: 'line', title: 'Zoom nominal', encoding: { x: 'x', y: 'y' }, data: { inline: [0, 'phase', 10].map((x, i) => ({ x, y: i + 1 })) } });
  await chartFrame().focus(); await page.keyboard.press('+');
  assert.equal((await modelOf()).zoom, null, 'A nominal x axis never zooms');
  assert.equal(await preview.locator('.rv-chart__action--reset').isVisible(), false);
  check('Numeric strings zoom like numbers on scatter, line and area; year-like values zoom numerically; a nominal axis does not zoom');

  evidence.source_series_identity = [];
  for (const type of ['scatter', 'bar']) for (const keys of [[1, 2], [true, false], [1, 'Named'], ['A', 'B']]) {
    const rows = keys.flatMap((group, g) => [0, 10].map(x => ({ x, y: g + x + 1, group })));
    await draw({ type, title: `Identity ${type} ${keys.join('-')}`, encoding: { x: 'x', y: 'y', color: 'group' }, data: { inline: rows } });
    const entries = preview.locator('button.rv-chart__legend-entry:visible');
    assert.equal(await entries.count(), 2, `${type}/${keys}: one legend entry per series key`);
    const initial = await modelOf();
    assert.equal(observationCount(initial), 4);
    await entries.first().click();
    const hidden = await modelOf();
    assert.equal(hidden.hidden.length, 1, `${type}/${keys}: hiding one series`);
    assert.equal(observationCount(hidden), 2);
    assert.deepEqual(await entries.evaluateAll(nodes => nodes.map(node => node.getAttribute('aria-pressed'))), ['false', 'true']);
    assert.equal((await tableRows()).length, 4, `${type}/${keys}: the table keeps hidden series' rows`);
    await entries.last().click();
    expectEqual(`${type}/${keys}: the legend cannot hide the final visible series`, (await modelOf()).hidden.length, 1);
    for (const index of [0, 1]) if (await entries.nth(index).getAttribute('aria-pressed') === 'false') await entries.nth(index).click();
    const restored = await modelOf();
    expectEqual(`${type}/${keys}: restoring the series restores the exact geometry`, JSON.stringify(restored.geometry), JSON.stringify(initial.geometry));
    evidence.source_series_identity.push({ type, keys, hidden: hidden.hidden });
  }
  check('Numeric, boolean and mixed series keys keep distinct legend entries, hide and restore their own marks and keep every table row');

  const conflicting = [{ date: '2026-01-01', value: '10.000' }, { date: '2026-01-01', value: '20.500' }, { date: '2026-01-10', value: 30 }];
  const conflict = { type: 'line', title: 'Conflicting dates', encoding: { x: 'date', y: 'value' }, data: { inline: conflicting } };
  const marks = model => model.geometry.panels.flatMap(panel => panel.items).filter(item => item.kind === 'line' && item.role === 'mark').length;
  await draw(conflict);
  assert.match(await page.locator('#diagnostics').innerText(), /error rows\[1\]\.date: .*repeats a timestamp/);
  assert.match(await page.locator('#diagnostics').innerText(), /Cannot draw this time series/);
  assert.equal(marks(await modelOf()), 0, 'Conflicting timestamps draw no line');
  assert.equal((await tableRows()).length, 3, 'The table still lists every source row');
  await draw({ ...conflict, title: 'Repaired dates', data: { inline: [conflicting[0], conflicting[2]] } });
  assert.doesNotMatch(await page.locator('#diagnostics').innerText(), /error /);
  assert.ok(marks(await modelOf()) > 0, 'Valid dates draw again after the repair');
  check('Conflicting timestamps stay visibly invalid with named source errors and every row; the repaired source draws again');

  await page.screenshot({ path: join(h.dir, 'charts.png'), fullPage: true });

  await page.goto(`${h.base}/`, { waitUntil: 'domcontentloaded' });
  // The landing starts its engine on the reader's first interaction.
  await page.keyboard.press('Shift');
  await page.waitForFunction(() => document.documentElement.dataset.homepageEngine === 'rust');
  const homepageReport = page.frameLocator('#demo-result');
  await settledCharts(homepageReport, 1);
  assert.equal(await homepageReport.locator('figure[data-rv-chart] canvas.rv-chart__canvas').count(), 1);
  // The live example is the hero's report (S27-931): life expectancy in four regions, as lines.
  assert.equal(await homepageReport.locator('body').evaluate(() => new Set(window.__richview.charts()[0].geometry.panels.flatMap(panel => panel.items.filter(item => item.kind === 'line' && item.role === 'mark').map(item => item.series))).size), 4, 'The homepage chart draws its four regions as lines');

  // A plain opening fence remains plain unless the reader asks for a tag (S27-944).
  await page.waitForFunction(() => document.documentElement.dataset.homepageEngine === 'rust');
  const fenceSource = page.locator('#demo-source');
  const fenceList = page.locator('#demo-complete');
  await fenceSource.fill('');
  await fenceSource.pressSequentially('```');
  assert.equal(await fenceList.isHidden(), true, 'A bare fence does not auto-open tags');
  await page.keyboard.press('Enter');
  assert.equal(await fenceSource.inputValue(), '```\n', 'Enter inserts a newline after a bare fence');
  await fenceSource.fill('📈\n  ```');
  assert.equal(await fenceList.isHidden(), true, 'An indented fence after non-BMP text stays plain');
  await page.keyboard.press('Enter');
  assert.equal(await fenceSource.inputValue(), '📈\n  ```\n');
  await fenceSource.fill('```');
  await page.keyboard.press('Control+Space');
  assert.equal(await fenceList.isVisible(), true, 'Ctrl+Space explicitly offers tags');
  await page.keyboard.press('Enter');
  assert.equal(await fenceSource.inputValue(), '```chart', 'Enter accepts an explicit tag choice');
  await page.keyboard.press(process.platform === 'darwin' ? 'Meta+z' : 'Control+z');
  const undoneFence = await fenceSource.inputValue();
  assert.notEqual(undoneFence, '```chart', 'Undo removes the inserted tag');
  // WebKit may undo the preceding typing burst with the insertion.
  if (h.engine !== 'webkit') assert.equal(undoneFence, '```', 'Undo restores the bare fence');
  if (undoneFence !== '```') await fenceSource.fill('```');
  assert.equal(await fenceList.isHidden(), true, 'Undo does not reopen the tag list');
  await page.keyboard.press('Enter');
  assert.equal(await fenceSource.inputValue(), '```\n', 'Enter after undo still inserts a newline');
  await fenceSource.fill('```c');
  assert.equal(await fenceList.isVisible(), true, 'A tag prefix offers matching tags');
  assert.match(await fenceList.innerText(), /chart/);
  await page.keyboard.press('Enter');
  assert.equal(await fenceSource.inputValue(), '```chart', 'Enter accepts a typed tag completion');
  check('A plain fence keeps Enter, while explicit and typed tag completion still work');
  check('The homepage embeds a live Rust chart from its bundled source example');

  // The completion list stays attached to a textbox, not a combobox (S27-943).
  await page.waitForFunction(() => document.documentElement.dataset.homepageEngine === 'rust');
  const demoSource = page.locator('#demo-source');
  const demoList = page.locator('#demo-complete');
  const demoStatus = page.locator('#demo-complete-status');
  assert.equal(await demoSource.getAttribute('aria-expanded'), null);
  assert.equal(await demoStatus.getAttribute('role'), 'status');
  await demoSource.fill('```chart\ntype: l');
  await page.waitForFunction(() => !document.querySelector('#demo-complete').hidden);
  assert.equal(await demoSource.getAttribute('aria-expanded'), null);
  assert.equal(await demoStatus.innerText(), 'Suggestions available.');
  let active = await demoSource.getAttribute('aria-activedescendant');
  assert.ok(active && await demoList.locator(`[id="${active}"][aria-selected="true"]`).count() === 1, 'The active suggestion is named to assistive technology');
  const firstActive = active;
  await page.keyboard.press('ArrowDown');
  active = await demoSource.getAttribute('aria-activedescendant');
  assert.notEqual(active, firstActive, 'ArrowDown moves to the next suggestion');
  assert.ok(active && await demoList.locator(`[id="${active}"][aria-selected="true"]`).count() === 1, 'The active suggestion follows keyboard navigation');
  await page.keyboard.press('Escape');
  assert.equal(await demoStatus.innerText(), 'Suggestions closed.');
  assert.equal(await demoSource.getAttribute('aria-activedescendant'), null);
  assert.equal(await demoList.isHidden(), true);
  await demoSource.fill('```chart\ntype: li');
  await page.waitForFunction(() => !document.querySelector('#demo-complete').hidden);
  assert.equal(await demoStatus.innerText(), 'Suggestions available.');
  check('The live example announces completion state and active choice without aria-expanded on a textbox');

  const mobile = await h.browser.newContext({ viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true, reducedMotion: 'reduce' });
  await mobile.addInitScript(instrument);
  const mobilePage = await mobile.newPage(); watchErrors(mobilePage, evidence.errors, 'mobile: ');
  await mobilePage.goto(`${h.base}/playground`, { waitUntil: 'domcontentloaded' });
  const mobilePreview = await previewFrame(mobilePage);
  await settledCharts(mobilePreview, 1);
  const glyphHeight = await mobilePreview.locator('body').evaluate(() => Math.min(...window.__richview.charts().flatMap(chart => [...chart.geometry.panels.flatMap(panel => panel.items), ...(chart.geometry.figure_items ?? [])]).filter(item => item.kind === 'text' && item.bounds.w > 0 && item.bounds.h > 0).map(item => item.bounds.h)));
  assert.ok(glyphHeight >= 10, `Mobile live chart glyph height ${glyphHeight}px`);
  assert.equal(await mobilePage.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
  evidence.mobile_label_height_px = glyphHeight;
  await mobilePage.locator('#preview').scrollIntoViewIfNeeded();
  await mobilePreview.locator('body').evaluate(() => new Promise(accept => requestAnimationFrame(() => requestAnimationFrame(accept))));
  await mobilePage.screenshot({ path: join(h.dir, 'mobile.png') });
  await mobilePreview.locator('.rv-chart').first().screenshot({ path: join(h.dir, 'mobile-live-chart.png') });
  await inputValue(mobilePage.locator('#source'),dense);
  const mobilePager=mobilePage.locator('.pg__source .rvc-source-window');
  await mobilePager.waitFor();
  const mobilePagerBounds=await mobilePager.evaluate(pager=>({viewport:innerWidth,document:document.documentElement.scrollWidth,boxes:[pager,...pager.querySelectorAll('button,label,input')].map(node=>{const box=node.getBoundingClientRect();return {left:box.left,right:box.right,width:box.width};})}));
  assert.ok(mobilePagerBounds.document<=mobilePagerBounds.viewport+1 && mobilePagerBounds.boxes.every(box=>box.left>=0 && box.right<=mobilePagerBounds.viewport+1),'Paged source controls must wrap inside the mobile viewport');
  evidence.mobile_paged_source_controls=mobilePagerBounds;
  await mobilePager.screenshot({path:join(h.dir,'paged-source-mobile.png')});
  await inputValue(mobilePage.locator('#source'), fixture('six-types'));
  assert.equal(await mobilePager.isVisible(),false,'Small source must hide the pager');
  await settledCharts(mobilePreview, 6);
  for (const [i, kind] of ['line', 'bar', 'scatter', 'area', 'pie', 'heatmap'].entries()) {
    await mobilePreview.locator('figure[data-rv-chart]').nth(i).screenshot({ path: join(h.dir, `six-${kind}-mobile.png`) });
    const bounds = await glyphAudit(mobilePreview, i);
    assert.equal(bounds.measured, true);
    assert.ok(!bounds.checks.some(check => check.kind === 'clipping'), `${kind} mobile labels must fit the live viewport`);
    evidence.six_type_glyph_bounds.push({ viewport: 'mobile', kind, ...bounds });
    if (kind === 'scatter') {
      assert.deepEqual(bounds.collisions, [], 'Mobile bubbles must not cover text boxes');
      (evidence.scatter_label_clearance ||= []).push({ viewport: 'mobile', circles: bounds.circles, collisions: bounds.collisions });
    }
  }
  const headers = Array.from({ length: 6 }, (_, i) => `Column${i}_${'longheader'.repeat(8)}`);
  const cells = Array.from({ length: 2 }, (_, row) => Array.from({ length: 6 }, (_, col) => `row${row}_column${col}_${'retainedvalue'.repeat(3)}`));
  const table = `| ${headers.join(' | ')} |\n| ${headers.map(() => '---').join(' | ')} |\n${cells.map(row => `| ${row.join(' | ')} |`).join('\n')}\n`;
  evidence.mobile_canonical_table_layouts = [];
  for (const theme of ['brutalist', 'scientific', 'sleek']) {
    await mobilePage.locator(`#playground-styles [data-theme="${theme}"]`).click();
    await inputValue(mobilePage.locator('#source'), fixture('six-types').replace('title: Six chart contracts', `title: Wide mobile table ${theme}`) + `\n${table}`);
    await mobilePreview.locator('.rv-report__header').filter({ hasText: `Wide mobile table ${theme}` }).waitFor();
    await settledCharts(mobilePreview, 6);
    await mobilePreview.locator('body').evaluate(async () => { await document.fonts.ready; await new Promise(accept => requestAnimationFrame(() => requestAnimationFrame(accept))); });
    const layout = await mobilePreview.locator('.rv-report__body > .rv-table-scroll').evaluate(scroller => ({
      viewport: innerWidth, document: document.documentElement.scrollWidth,
      boxes: ['.rv-report', '.rv-report__header', '.rv-report__body'].map(selector => {
        const rect = document.querySelector(selector).getBoundingClientRect();
        return { selector, left: rect.left, right: rect.right, width: rect.width };
      }),
      // A Markdown table sits in a `.rv-table-scroll` wrapper, which is what scrolls.
      table: { client: scroller.clientWidth, scroll: scroller.scrollWidth, cells: [...scroller.querySelectorAll('th,td')].map(cell => cell.textContent) },
    }));
    assert.ok(layout.document <= layout.viewport + 1 && layout.boxes.every(box => box.left >= -.5 && box.right <= layout.viewport + 1), `${theme} canonical report, header and body must remain inside the mobile viewport`);
    assert.ok(layout.table.scroll > layout.table.client, `${theme} wide Markdown table must scroll locally`);
    assert.deepEqual(layout.table.cells, [...headers, ...cells.flat()], 'Local overflow must retain every full table cell');
    evidence.mobile_canonical_table_layouts.push({ theme, ...layout });
  }
  await mobilePreview.locator('.rv-report__body > .rv-table-scroll').screenshot({ path: join(h.dir, 'mobile-wide-markdown-table.png') });
  check('Canonical mobile reports contain wide Markdown tables with local scrolling and every cell retained');
  await mobilePage.goto(`${h.base}/showcase/openai-vs-anthropic/`, { waitUntil: 'domcontentloaded' });
  await mobilePage.locator('#theme option').nth(20).waitFor({ state: 'attached' });
  const mobileFlagship = mobilePage.frameLocator('#report-frame');
  await settledCharts(mobileFlagship, flagshipCharts);
  evidence.mobile_theme_layouts = [];
  evidence.mobile_theme_header_bounds = [];
  for (const theme of themes) {
    await showStyle(mobilePage, theme);
    await settledCharts(mobileFlagship, flagshipCharts);
    await mobileFlagship.locator('body').evaluate(async () => { await document.fonts.ready; await new Promise(accept => requestAnimationFrame(() => requestAnimationFrame(accept))); });
    const layout = await mobileFlagship.locator('html').evaluate(node => ({ viewport: innerWidth, document: node.scrollWidth, charts: [...node.querySelectorAll('.rv-chart')].map(chart => chart.getBoundingClientRect().width) }));
    evidence.mobile_theme_layouts.push({ theme, ...layout });
    assert.ok(layout.document <= layout.viewport + 1 && layout.charts.every(width => width <= layout.viewport + 1), `${theme} must contain its report and charts in the mobile iframe`);
    const bounds = await glyphAudit(mobileFlagship, 0);
    assert.equal(bounds.measured, true);
    assert.deepEqual(bounds.checks, [], `${theme} mobile chart titles, subtitles, axes and tick labels must remain separate inside the viewport`);
    await mobileFlagship.locator('body').evaluate(()=>window.__richview.settled());
    const widths=await mobileFlagship.locator('body').evaluate(()=>window.__richview.charts().map(chart=>({stage:document.getElementById(chart.id).querySelector('.rv-chart__frame').clientWidth,model:chart.width})));
    assert.ok(widths.length===flagshipCharts && widths.every(size=>Math.abs(size.model-Math.max(240,size.stage))<.5),`${theme} must use the actual current stage width for every chart scale`);
    evidence.mobile_theme_header_bounds.push({ theme, ...bounds });
  }
  for (const theme of ['scientific', 'sleek']) {
    await showStyle(mobilePage, theme);
    await settledCharts(mobileFlagship, flagshipCharts);
    await mobileFlagship.locator('body').evaluate(async () => { window.scrollTo(0, 0); await document.fonts.ready; await new Promise(accept => requestAnimationFrame(() => requestAnimationFrame(accept))); });
    const layout = await mobileFlagship.locator('html').evaluate(node => ({ viewport: innerWidth, document: node.scrollWidth, chart: node.querySelector('.rv-chart').getBoundingClientRect().width }));
    (evidence.mobile_flagship_layout ||= []).push({ theme, ...layout });
    assert.ok(layout.document <= layout.viewport + 1 && layout.chart <= layout.viewport + 1, `${theme} report and chart must fit the actual mobile iframe: ${JSON.stringify(layout)}`);
    await mobilePage.locator('#report-frame').screenshot({ path: join(h.dir, `showcase-${theme}-mobile.png`) });
    await mobileFlagship.locator('.rv-chart').first().screenshot({ path: join(h.dir, `showcase-${theme}-chart-mobile.png`) });
    const bounds = await glyphAudit(mobileFlagship, 0);
    assert.equal(bounds.measured, true);
    assert.deepEqual(bounds.checks, [], `${theme} mobile glyph bounds must fit and keep every title/subtitle/axis/tick label separate`);
    (evidence.flagship_glyph_bounds ||= []).push({ viewport: 'mobile', theme, ...bounds });
  }
  await mobile.close(); await context.close();
  check('Mobile live SVG labels remain readable without horizontal page overflow');
  if (selection.scope) completedCases.push('browser-journey');
  }
  assert.deepEqual(evidence.errors, []);
  assert.deepEqual(failures, []);
  evidence.status = 'passed';
} catch (error) {
  evidence.status = 'failed'; evidence.failure = error.stack?.slice(0, 4000);
  throw error;
} finally {
  if (selection.scope) { evidence.case_scope = selection.scope; evidence.completed = completedCases; }
  resultPath = h.save(selection.scope ? 'browser.partial.json' : 'browser.json', evidence);
  if (selection.scope) {
    const outAt = process.argv.indexOf('--out');
    if (outAt >= 0 && process.argv[outAt + 1]) { mkdirSync(process.argv[outAt + 1], { recursive: true }); copyFileSync(resultPath, join(process.argv[outAt + 1], 'browser.partial.json')); }
  }
  await h.close();
  console.log(JSON.stringify({ evidence: resultPath, ...evidence }, null, 2));
}
