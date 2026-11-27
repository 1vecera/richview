import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import http from 'node:http';
import https from 'node:https';
import { gzipSync } from 'node:zlib';
import type { Browser, CDPSession, Page } from 'playwright';
import { frameClock, harness, fixture, instrument, quantiles, root, settled, settledCharts, watchErrors } from './browser-harness.ts';

interface SourceSample {
  input_to_settled_charts_ms: number;
  input_to_two_animation_frames_ms: number;
  canvases: number;
}

type Quantiles = ReturnType<typeof quantiles>;
interface BodyBytes { status: number | undefined; encoding: string; bytes: Buffer }
interface ProcessIdentity { pid: number; type: string }
interface ProcessMemory extends ProcessIdentity { rss_bytes: number; hwm_bytes: number }
interface MemorySample {
  at_ms: number;
  sum_current_rss_bytes: number;
  sum_process_hwm_bytes: number;
  processes: ProcessMemory[];
  unavailable: ProcessIdentity[];
}
interface DenseEdit {
  input_to_interactive_preview_ms: number;
  input_to_settled_charts_ms?: number;
  worker_requests: Window['__rvEvidence']['requests'];
  main_thread_long_tasks: Window['__rvEvidence']['longTasks'];
  main_thread_max_16ms_heartbeat_gap_ms: number;
  synchronous_input_ms?: number;
  source_bytes?: number;
  source_sha256?: string;
}
interface BrowserBenchmark {
  browser: string;
  base: string;
  platform: string;
  architecture: string;
  scope: string;
  errors: string[];
  wasm?: {
    sha256: string;
    delivered_identity_bytes: number;
    delivered_identity_encoding: string;
    delivered_gzip_bytes: number;
    delivered_gzip_encoding: string;
    computed_gzip_bytes: number;
  };
  warm_20_flagship_theme_changes?: {
    workload: string;
    event_to_first_frame: Quantiles;
    event_to_settled_charts: Quantiles;
    event_to_two_animation_frames: Quantiles;
    samples?: object[];
  };
  warm_20_small_source_edits?: {
    workload: string;
    input_to_settled_charts: Quantiles;
    input_to_two_animation_frames: Quantiles;
    samples?: object[];
  };
  dense_source_live_edit?: DenseEdit;
  heap_after_dense_source_edit?: object;
  fixed_budgets?: Record<string, { actual: number; maximum: number; passed: boolean }>;
  status?: 'passed' | 'failed';
  failure?: string;
  browser_process_memory?: { scope: string; supported: boolean; samples?: MemorySample[] };
}

declare global {
  interface Window { __rvHeartbeat: { last: number; max_gap_ms: number; timer?: number } }
}

// The research pipeline's committed chart inventory: an independent list, so a chart dropped from the source fails.
const flagshipCharts = JSON.parse(readFileSync(join(root, 'examples/openai-vs-anthropic/data/charts/index.json'), 'utf8')).length;

const h = await harness('benchmark');
const evidence: BrowserBenchmark = {
  browser: h.browser.version(), base: h.base, platform: process.platform, architecture: process.arch,
  scope: 'Real release Wasm, WebGL report runtime. The runtime emits no per-frame event: a chart counts as drawn when window.__richview.settled() resolves, and first frame is the first requestAnimationFrame callback after the input. Warm latency starts at the input event dispatch; two animation frames allow the change to be presented. No TypeScript comparison is inferred from these measurements.',
  errors: [],
};
const responseBytes = (url: string, encoding: string) => new Promise<BodyBytes>((accept, reject) => {
  const transport = new URL(url).protocol === 'https:' ? https : http;
  transport.get(url, { headers: { 'accept-encoding': encoding } }, response => {
    const chunks: Buffer[] = [];
    response.on('data', chunk => chunks.push(chunk));
    response.on('end', () => accept({ status: response.statusCode, encoding: response.headers['content-encoding'] || 'identity', bytes: Buffer.concat(chunks) }));
    response.on('error', reject);
  }).on('error', reject);
});
async function ownedProcessMemory(browser: Browser) {
  if (process.platform !== 'linux') return async () => ({
    supported: false,
    scope: 'Owned Chromium RSS/HWM sampling requires Linux /proc; memory is not measured on this host.',
    samples: [],
  });
  const samples: MemorySample[] = [];
  let session: CDPSession | undefined, pending: Promise<void> | undefined, failure: string | undefined;
  const sample = async () => {
    try {
      const { processInfo }: { processInfo: { id: number; type: string }[] } = await session!.send('SystemInfo.getProcessInfo');
      const processes = [], unavailable = [];
      for (const process of processInfo) {
        try {
          const status = readFileSync(`/proc/${process.id}/status`, 'utf8');
          const rss = status.match(/^VmRSS:\s+(\d+)\s+kB/m);
          const hwm = status.match(/^VmHWM:\s+(\d+)\s+kB/m);
          if (!rss || !hwm) throw new Error('RSS/HWM unavailable');
          processes.push({ pid: process.id, type: process.type, rss_bytes: Number(rss[1]) * 1024, hwm_bytes: Number(hwm[1]) * 1024 });
        } catch { unavailable.push({ pid: process.id, type: process.type }); }
      }
      samples.push({ at_ms: performance.now(), sum_current_rss_bytes: processes.reduce((sum, row) => sum + row.rss_bytes, 0), sum_process_hwm_bytes: processes.reduce((sum, row) => sum + row.hwm_bytes, 0), processes, unavailable });
    } catch (error) { failure = String(error); }
  };
  try { session = await browser.newBrowserCDPSession(); await sample(); }
  catch (error) { failure = String(error); }
  const timer = setInterval(() => {
    if (!failure && !pending) pending = sample().finally(() => { pending = undefined; });
  }, 100);
  return async () => {
    clearInterval(timer); await pending;
    if (session) await session.detach();
    const peak = samples.reduce<MemorySample | undefined>((largest, row) => !largest || row.sum_current_rss_bytes > largest.sum_current_rss_bytes ? row : largest, undefined);
    return {
      scope: 'Owned Chromium processes returned by SystemInfo.getProcessInfo, sampled from Linux /proc every ~100ms. Includes returned browser/renderers/GPU/utilities and iframe/worker processes. Shared pages may be counted more than once; transient processes and peaks between samples can be missed. Summed per-process HWM is not a synchronized browser peak.',
      supported: !failure && samples.some(row => row.processes.length > 0), sampling_interval_ms: 100, failure,
      max_sum_current_rss_bytes: peak?.sum_current_rss_bytes,
      largest_sum_process_hwm_bytes: samples.length ? Math.max(...samples.map(row => row.sum_process_hwm_bytes)) : undefined,
      process_types: [...new Set(samples.flatMap(row => row.processes.map(process => process.type)))],
      peak_sample: peak, samples,
    };
  };
}
let finishProcessMemory: Awaited<ReturnType<typeof ownedProcessMemory>> | undefined;
try {
  const [raw, gzip] = await Promise.all([responseBytes(`${h.base}/richview_site_bg.wasm`, 'identity'), responseBytes(`${h.base}/richview_site_bg.wasm`, 'gzip')]);
  assert.equal(raw.status, 200); assert.equal(gzip.status, 200);
  evidence.wasm = {
    sha256: createHash('sha256').update(raw.bytes).digest('hex'),
    delivered_identity_bytes: raw.bytes.length, delivered_identity_encoding: raw.encoding,
    delivered_gzip_bytes: gzip.bytes.length, delivered_gzip_encoding: gzip.encoding,
    computed_gzip_bytes: gzipSync(raw.bytes).length,
  };
  finishProcessMemory = await ownedProcessMemory(h.browser);
  const context = await h.browser.newContext({ viewport: { width: 1440, height: 1000 }, reducedMotion: 'reduce' });
  await context.addInitScript(instrument);
  await context.addInitScript(frameClock);
  const page = await context.newPage(); watchErrors(page, evidence.errors);
  const cdp = await context.newCDPSession(page);
  await cdp.send('HeapProfiler.enable');
  const memory = async () => {
    const result = await cdp.send('Runtime.getHeapUsage');
    return { scope: 'Main page JavaScript isolate; separate worker and sandboxed preview heaps excluded', js_used_bytes: result.usedSize, js_total_bytes: result.totalSize, embedder_heap_used_bytes: result.embedderHeapUsedSize, backing_storage_bytes: result.backingStorageSize };
  };
  // The /charts/ cold-mount, dense-calendar and direct CSV-edit measurements went with the page
  // (ARC-40); PRF-33 measures cold mount on the docs chart gallery once the perf step exists.
  await page.goto(`${h.base}/showcase/openai-vs-anthropic/`, { waitUntil: 'domcontentloaded' });
  await page.locator('#theme option').nth(20).waitFor({ state: 'attached' });
  const flagship = page.frameLocator('#report-frame');
  await settledCharts(flagship, flagshipCharts);
  await flagship.locator('#richview-showcase-treatments').evaluate(link => (link as HTMLLinkElement).sheet ? Promise.resolve() : new Promise(accept => link.addEventListener('load', accept, { once: true })));
  // Five primary styles are pills; the other 20 sit behind the "Other styles" select.
  const themes = await page.evaluate(() => [...document.querySelectorAll('#style-pills [data-style], #theme option:not([value=""])')].map(node => (node as HTMLElement).dataset.style ?? (node as HTMLOptionElement).value));
  assert.equal(themes.length, 25, 'The showcase offers 25 styles');
  const choose = (page: Page, theme: string) => page.evaluate(theme => {
    const pill = document.querySelector<HTMLElement>(`#style-pills [data-style="${theme}"]`);
    const start = performance.timeOrigin + performance.now();
    if (pill) pill.click();
    else { const select = document.querySelector<HTMLSelectElement>('#theme')!; select.value = theme; select.dispatchEvent(new Event('change', { bubbles: true })); }
    return start;
  }, theme);
  // Warm every self-hosted font before timing theme layout. The measured pass still
  // executes the Rust change handler, actual geometry and every DOM chart frame.
  for (const theme of themes) {
    await choose(page, theme);
    await flagship.locator(`html[data-rv-theme="${theme}"]`).waitFor();
    await flagship.locator('body').evaluate(async () => {
      await document.fonts.ready;
      await new Promise(accept => requestAnimationFrame(() => requestAnimationFrame(accept)));
    });
  }
  const themeSamples = [];
  for (let i = 0; i < 20; i++) {
    const theme = themes[i % themes.length];
    const epoch = await choose(page, theme);
    const sample = await flagship.locator('body').evaluate(async (_body, start) => {
      // The first presented frame after the change comes from the page's own rAF clock; the
      // runtime emits no per-frame event, so settled() marks the charts being drawn.
      const frame = window.__rvFrameClock.find(at => at >= start);
      await window.__richview!.settled();
      const settledAt = performance.timeOrigin + performance.now();
      await new Promise(accept => requestAnimationFrame(() => requestAnimationFrame(accept)));
      return {
        event_to_first_frame_ms: (frame ?? performance.timeOrigin + performance.now()) - start,
        event_to_settled_charts_ms: settledAt - start,
        event_to_two_animation_frames_ms: performance.timeOrigin + performance.now() - start,
        charts: window.__richview!.charts().length,
        canvases: document.querySelectorAll('figure[data-rv-chart] canvas.rv-chart__canvas').length,
        bg_token: document.documentElement.style.getPropertyValue('--rv-bg'),
      };
    }, epoch);
    assert.equal(sample.charts, flagshipCharts); assert.equal(sample.canvases, flagshipCharts); assert.ok(sample.bg_token);
    themeSamples.push({ theme, ...sample });
  }
  evidence.warm_20_flagship_theme_changes = {
    workload: `Invariant ${flagshipCharts}-chart flagship, every style fonts warmed before timing; 20 sequential theme changes. First frame is the first requestAnimationFrame callback in the report after the change event; settled charts is when window.__richview.settled() resolves with all ${flagshipCharts} canvases drawn.`,
    event_to_first_frame: quantiles(themeSamples.map(sample => sample.event_to_first_frame_ms)),
    event_to_settled_charts: quantiles(themeSamples.map(sample => sample.event_to_settled_charts_ms)),
    event_to_two_animation_frames: quantiles(themeSamples.map(sample => sample.event_to_two_animation_frames_ms)),
    samples: themeSamples,
  };

  await page.goto(`${h.base}/playground`, { waitUntil: 'domcontentloaded' });
  const preview = page.frameLocator('#preview');
  await preview.locator('figure[data-rv-chart] canvas.rv-chart__canvas').first().waitFor({ timeout: 45_000 });
  const small = fixture('small');
  const smallFrontmatter = small.match(/^---\r?\n[\s\S]*?\r?\n---(?:\r?\n|$)/)?.[0];
  assert.ok(smallFrontmatter, 'Small benchmark fixture must begin with YAML frontmatter');
  assert.equal((smallFrontmatter.match(/^title:[^\r\n]*/gm) || []).length, 1, 'Small benchmark fixture must contain exactly one frontmatter title');
  const sourceSamples: SourceSample[] = [];
  for (let i = 0; i < 20; i++) {
    const title = `Small edit ${i}`;
    const editedFrontmatter: string = smallFrontmatter.replace(/^title:[^\r\n]*/m, `title: ${JSON.stringify(title)}`);
    const source: string = editedFrontmatter + small.slice(smallFrontmatter.length);
    assert.notEqual(source, small, 'Each small source sample must change exactly its frontmatter title');
    const epoch: number = await page.locator('#source').evaluate((input, source) => {
      const start = performance.timeOrigin + performance.now();
      (input as HTMLTextAreaElement).value = source; input.dispatchEvent(new Event('input', { bubbles: true }));
      return start;
    }, source);
    await preview.locator('body').filter({ hasText: title }).waitFor();
    await preview.locator('figure[data-rv-chart] canvas.rv-chart__canvas').first().waitFor({ timeout: 45_000 });
    const sample: SourceSample = await preview.locator('body').evaluate(async (_body, start) => {
      await window.__richview!.settled();
      const settledAt = performance.timeOrigin + performance.now();
      await new Promise(accept => requestAnimationFrame(() => requestAnimationFrame(accept)));
      return { input_to_settled_charts_ms: settledAt - start, input_to_two_animation_frames_ms: performance.timeOrigin + performance.now() - start, canvases: document.querySelectorAll('figure[data-rv-chart] canvas.rv-chart__canvas').length };
    }, epoch);
    assert.ok(sample.canvases > 0, 'Small source edit must draw a chart');
    sourceSamples.push(sample);
  }
  evidence.warm_20_small_source_edits = {
    workload: 'Same small report source with only title changed, 20 sequential native input events; cached external preview runtime',
    input_to_settled_charts: quantiles(sourceSamples.map(sample => sample.input_to_settled_charts_ms)),
    input_to_two_animation_frames: quantiles(sourceSamples.map(sample => sample.input_to_two_animation_frames_ms)),
    samples: sourceSamples,
  };
  const dense = fixture('dense');
  const denseInput = await page.locator('#source').evaluate((input, source) => {
    window.__rvEvidence.longTasks = []; window.__rvEvidence.requests = [];
    const start = performance.now();
    window.__rvHeartbeat = { last: start, max_gap_ms: 0 };
    window.__rvHeartbeat.timer = window.setInterval(() => {
      const now = performance.now(); window.__rvHeartbeat.max_gap_ms = Math.max(window.__rvHeartbeat.max_gap_ms, now - window.__rvHeartbeat.last); window.__rvHeartbeat.last = now;
    }, 16);
    (input as HTMLTextAreaElement).value = source; input.dispatchEvent(new Event('input', { bubbles: true }));
    return { start, epoch:performance.timeOrigin+start, synchronous_input_ms: performance.now() - start };
  }, dense);
  await preview.locator('body').filter({ hasText: 'Dense series with exact observations' }).waitFor({ timeout: 60_000 });
  await preview.locator('figure[data-rv-chart] canvas.rv-chart__canvas').first().waitFor({ timeout: 45_000 });
  const settledDense=await preview.locator('body').evaluate(async(_body,epoch)=>{
    await window.__richview!.settled();
    return {input_to_settled_charts_ms:performance.timeOrigin+performance.now()-epoch};
  },denseInput.epoch);
  await settled(page); await page.waitForTimeout(60);
  evidence.dense_source_live_edit = await page.evaluate(start => {
    window.clearInterval(window.__rvHeartbeat.timer);
    return {
      input_to_interactive_preview_ms: performance.now() - start,
      worker_requests: window.__rvEvidence.requests,
      main_thread_long_tasks: window.__rvEvidence.longTasks.filter(task => task.start_ms + task.duration_ms >= start),
      main_thread_max_16ms_heartbeat_gap_ms: window.__rvHeartbeat.max_gap_ms,
    };
  }, denseInput.start);
  evidence.dense_source_live_edit.synchronous_input_ms = denseInput.synchronous_input_ms;
  Object.assign(evidence.dense_source_live_edit,settledDense);
  evidence.dense_source_live_edit.source_bytes = Buffer.byteLength(dense);
  evidence.dense_source_live_edit.source_sha256 = createHash('sha256').update(dense).digest('hex');
  assert.ok(evidence.dense_source_live_edit.worker_requests.some(request => request.source_bytes > 128 * 1024), 'Dense source must be parsed/rendered by the Rust worker');
  assert.ok(evidence.dense_source_live_edit.worker_requests.some(request => request.source_sha256 === evidence.dense_source_live_edit!.source_sha256), 'Worker must receive the exact complete2MB fixture rather than the visible page');
  evidence.heap_after_dense_source_edit = await memory();
  assert.deepEqual(evidence.errors, []);
  const limits: [string, number, number][] = [
    ['theme_first_frame_p95_ms',evidence.warm_20_flagship_theme_changes.event_to_first_frame.p95_ms,100],
    ['theme_settled_charts_p95_ms',evidence.warm_20_flagship_theme_changes.event_to_settled_charts.p95_ms,100],
    ['small_edit_settled_charts_p95_ms',evidence.warm_20_small_source_edits.input_to_settled_charts.p95_ms,300],
    ['dense_edit_settled_charts_ms',evidence.dense_source_live_edit.input_to_settled_charts_ms!,1000],
    ['delivered_gzip_wasm_bytes',evidence.wasm.delivered_gzip_bytes,1.5*1024*1024],
  ];
  evidence.fixed_budgets=Object.fromEntries(limits.map(([name,actual,maximum])=>[name,{actual,maximum,passed:Number.isFinite(actual)&&actual<=maximum}]));
  assert.ok(Object.values(evidence.fixed_budgets).every(gate=>gate.passed),`Fixed performance gate failure: ${JSON.stringify(evidence.fixed_budgets)}`);
  evidence.status = 'passed';
  await cdp.detach(); await context.close();
} catch (error) {
  evidence.status = 'failed'; evidence.failure = (error as Error).stack?.slice(0, 4000);
  throw error;
} finally {
  if (finishProcessMemory) evidence.browser_process_memory = await finishProcessMemory();
  const output = h.save('benchmark.json', evidence);
  mkdirSync('tmp/performance', { recursive: true });
  writeFileSync('tmp/performance/browser.json', JSON.stringify(evidence, null, 2) + '\n');
  await h.close();
  const summary = { ...evidence };
  for (const key of ['warm_20_flagship_theme_changes', 'warm_20_small_source_edits', 'browser_process_memory'] as const) {
    if (summary[key]) Object.assign(summary, { [key]: { ...summary[key] } });
    delete summary[key]?.samples;
  }
  console.log(JSON.stringify({ evidence: output, ...summary }, null, 2));
}
