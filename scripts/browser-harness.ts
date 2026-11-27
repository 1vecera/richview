// Host reason (ARC-03): drive real browsers and isolated loopback verification surfaces.
import { chromium, firefox, webkit, errors } from 'playwright';
import type { Browser, Frame, FrameLocator, Locator, Page } from 'playwright';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, statSync, symlinkSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import type { ServerResponse } from 'node:http';
import type { AddressInfo } from 'node:net';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, extname, join, resolve, sep } from 'node:path';
import { waitForBenchmark } from '../tools/rv-gate/lib/quiet.mts';
import { launchBrowser } from '../tools/rv-gate/lib/browser-launch.mts';
import type { ReportHook } from './report-model.ts';
export { waitForBenchmark };
export { RUNTIME_WIDTHS, verificationShard } from '../tools/rv-gate/lib/shards.mts';

export type Scope = Page | Frame | FrameLocator;
type Header = [name: string, value: string | null];
type HeaderRule = { pattern: string; headers: Header[] };
type StaticOptions = {
  port?: number;
  route?: (path: string, url: URL, response: ServerResponse) => unknown;
};

type WorkerRequest = { id: number; source?: string; options?: string };
type WorkerReply = { id: number; error?: string };
type RequestEvidence = Pick<WorkerRequest, 'id' | 'source'> & {
  source_bytes: number;
  at: number;
  source_sha256?: string;
};

type SourceRendering = {
  input: number;
  rendered: number;
  loaded: number;
  generation?: number;
  loadedGeneration?: number;
  changedAt?: number;
  loadedAt?: number | null;
  styleClickedAt?: number | null;
  restoreMutations?: number[];
};

interface BrowserEvidence {
  longTasks: { start_ms: number; duration_ms: number }[];
  requests: RequestEvidence[];
  replies: (WorkerReply & { at: number })[];
  gateReply: boolean;
  heldReply: (() => void) | null;
  failNextRequest?: boolean;
  releaseReply: () => void;
  queuedEdits?: number;
  sourceRendering?: SourceRendering;
}

declare global {
  interface Window {
    __richview?: ReportHook;
    __rvEvidence: BrowserEvidence;
    __rvFrameClock: number[];
  }
}

export const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
// The release CLI the suites generate reports with; RICHVIEW_CLI points at a kept build instead of this checkout's target/.
export const cli = process.env.RICHVIEW_CLI || join(root, 'target/release/richview');
export const fixture = (name: string) => readFileSync(join(root, 'tmp/performance/fixtures', name, 'report.rmd'), 'utf8');
export const quantiles = (values: number[]) => {
  const sorted = [...values].sort((a, b) => a - b);
  return { samples: sorted.length, p50_ms: sorted[Math.floor(sorted.length / 2)], p95_ms: sorted[Math.max(0, Math.ceil(sorted.length * .95) - 1)], min_ms: sorted[0], max_ms: sorted.at(-1) };
};
export const chromiumPath = () => process.env.RICHVIEW_CHROMIUM || process.env.CHROMIUM_PATH || (existsSync('/usr/bin/chromium') ? '/usr/bin/chromium' : undefined);

/** Install a clock in a loaded report, mapping its large timer handles to web-sys's signed 32-bit handles. */
export async function installWasmClock(page: Page) {
  await page.clock.install();
  const bridge = () => {
    const browserWindow: Window = window;
    const timeout = browserWindow.setTimeout.bind(browserWindow), interval = browserWindow.setInterval.bind(browserWindow);
    const clearTimeout = browserWindow.clearTimeout.bind(browserWindow), clearInterval = browserWindow.clearInterval.bind(browserWindow);
    const timers = new Map<number, { handle: ReturnType<typeof timeout>; repeat: boolean }>();
    let next = 1;
    const schedule = (handler: TimerHandler, delay: number | undefined, repeat: boolean, args: unknown[]) => {
      const id = next++;
      // Wasm timer callbacks are functions. The browser's wider signature also accepts strings.
      const run = () => { if (!repeat) timers.delete(id); (handler as (...args: unknown[]) => void).apply(window, args); };
      timers.set(id, { handle: (repeat ? interval : timeout)(run, delay), repeat });
      return id;
    };
    const clear = (id?: number) => {
      const timer = timers.get(id!);
      if (timer) {
        (timer.repeat ? clearInterval : clearTimeout)(timer.handle);
        timers.delete(id!);
      }
    };
    browserWindow.setTimeout = (handler: TimerHandler, delay?: number, ...args: unknown[]) => schedule(handler, delay, false, args);
    browserWindow.setInterval = (handler: TimerHandler, delay?: number, ...args: unknown[]) => schedule(handler, delay, true, args);
    browserWindow.clearTimeout = browserWindow.clearInterval = clear;
  };
  await page.evaluate(bridge);
}

const types: Record<string, string | undefined> = { '.html': 'text/html', '.js': 'text/javascript', '.mjs': 'text/javascript', '.json': 'application/json', '.wasm': 'application/wasm', '.css': 'text/css', '.csv': 'text/csv', '.webp': 'image/webp', '.avif': 'image/avif', '.png': 'image/png', '.jpg': 'image/jpeg', '.svg': 'image/svg+xml', '.woff2': 'font/woff2', '.webm': 'video/webm', '.mp4': 'video/mp4' };
// The one byte range a Range header asks of a file of `size` bytes, as the host answers it (RFC 9110 §14):
// undefined when the header asks no single valid byte range, so the whole file goes out as without one; false
// when the range starts past the end (416); otherwise its first and last byte, the last cut to the file's end.
// WebKit plays a video only from a server that answers ranges.
export function byteRange(header: string | undefined, size: number) {
  const match = /^bytes=(\d*)-(\d*)$/.exec(header ?? '');
  if (!match || (!match[1] && !match[2])) return undefined;
  if (!match[1]) {
    const suffix = Number(match[2]);
    return suffix > 0 && size > 0 ? { start: Math.max(0, size - suffix), end: size - 1 } : false;
  }
  const start = Number(match[1]);
  if (match[2] && Number(match[2]) < start) return undefined;
  if (start >= size) return false;
  return { start, end: match[2] ? Math.min(Number(match[2]), size - 1) : size - 1 };
}
// Every response header the staged `_headers` sets for a path, as the deployed site sends them (FEA-SF-60):
// `/*` and `/docs/*` match by prefix, any other pattern exactly. The loopback suites run under the real policy, with Cloudflare's merge order.
export const parseHeaderRules = (text: string): HeaderRule[] => text.split(/\n(?=\/)/).map(block => {
  const [pattern, ...lines] = block.split('\n').filter(line => line.trim() && !line.startsWith('#'));
  return { pattern: pattern?.trim() ?? '', headers: lines.map(line => line.trim().startsWith('! ') ? [line.trim().slice(2), null] : line.trim().split(/:\s(.*)/s).slice(0, 2)).filter((pair): pair is Header => pair.length === 2 || pair[1] === null) };
}).filter(rule => rule.pattern.startsWith('/'));
export const headersFrom = (headerRules: HeaderRule[], path: string) => {
  const out: Record<string, string | undefined> = {};
  for (const { pattern, headers } of headerRules) {
    const hit = pattern.endsWith('/*') ? path.startsWith(pattern.slice(0, -1)) || path === pattern.slice(0, -2) : path === pattern;
    if (!hit) continue;
    // As Cloudflare does: a rule's `!` names go first, then its headers; a name set again is joined.
    for (const [name, value] of headers) if (value === null) delete out[name.toLowerCase()];
    for (const [name, value] of headers) if (value !== null) out[name.toLowerCase()] = out[name.toLowerCase()] ? `${out[name.toLowerCase()]}, ${value}` : value;
  }
  return out;
};
// Loopback static files for local evidence: ordered URL prefixes map to directories,
// and `route` may answer a request first by returning true.
export async function serveStatic(roots: Record<string, string>, { port = 0, route }: StaticOptions = {}) {
  // A site build adds its exact inline-runtime hash; report-only fixture folders use the source policy.
  const stagedHeaders = roots['/'] && join(resolve(roots['/']), '_headers');
  if (stagedHeaders && existsSync(join(resolve(roots['/']), 'richview_site_bg.wasm')) && !existsSync(stagedHeaders)) {
    throw new Error('The staged site has no _headers; rebuild it before browser verification');
  }
  const headerText = readFileSync(stagedHeaders && existsSync(stagedHeaders) ? stagedHeaders : join(root, 'assets/_headers'), 'utf8');
  const headerRules = parseHeaderRules(headerText);
  const corsPaths = new Set([...headerText.matchAll(/^(\/\S*)\n(?:[ \t]+.*\n)*?[ \t]+Access-Control-Allow-Origin: \*/gm)].map(match => match[1]));
  const server = createServer((req, res) => {
    try {
      const url = new URL(req.url!, 'http://localhost');
      const path = decodeURIComponent(url.pathname);
      if (route?.(path, url, res)) return;
      const prefix = Object.keys(roots).find(key => path.startsWith(key));
      if (!prefix) { res.writeHead(404); res.end('Not found'); return; }
      const base = resolve(roots[prefix]);
      const file = resolve(base, path.slice(prefix.length));
      if (file !== base && !file.startsWith(base + sep)) { res.writeHead(403); res.end(); return; }
      // Clean URLs as the host serves them: a folder is its index.html, `/playground` is playground.html.
      const found = [file, join(file, 'index.html'), `${file}.html`].find(path => existsSync(path) && statSync(path).isFile());
      if (!found) { res.writeHead(404); res.end('Not found'); return; }
      const content = readFileSync(found);
      const headers = { 'content-type': types[extname(found)] || 'text/plain', 'cache-control': 'no-store', ...(process.env.RICHVIEW_VERIFY_NO_HEADERS ? (corsPaths.has(path) ? { 'access-control-allow-origin': '*' } : {}) : headersFrom(headerRules, path)) };
      const range = byteRange(req.headers.range, content.length);
      if (range === undefined) { res.writeHead(200, headers); res.end(content); return; }
      if (!range) { res.writeHead(416, { ...headers, 'accept-ranges': 'bytes', 'content-range': `bytes */${content.length}` }); res.end(); return; }
      res.writeHead(206, { ...headers, 'accept-ranges': 'bytes', 'content-range': `bytes ${range.start}-${range.end}/${content.length}`, 'content-length': range.end - range.start + 1 });
      res.end(content.subarray(range.start, range.end + 1));
    } catch { res.writeHead(404); res.end('Not found'); }
  });
  await new Promise<void>((accept, reject) => { server.once('error', reject); server.listen(port, '127.0.0.1', accept); });
  const address = server.address() as AddressInfo;
  return { base: `http://127.0.0.1:${address.port}`, close: () => new Promise<Error | undefined>(accept => server.close(accept)) };
}

// The host, report and worker all use the just-built release distribution. No writes
// reach the normal report store, and a random loopback port avoids an existing server.
export async function harness(name: string) {
  const engines = { chromium, firefox, webkit };
  const engine = process.env.RICHVIEW_VERIFY_BROWSER || 'chromium';
  if (!Object.hasOwn(engines, engine)) {
    throw new Error(`Unknown RICHVIEW_VERIFY_BROWSER '${engine}'; use chromium, firefox or webkit`);
  }
  await waitForBenchmark();
  const parent = join(root, 'tmp/verification');
  mkdirSync(parent, { recursive: true });
  const dir = mkdtempSync(join(parent, `${name}-`));
  let server: Pick<Awaited<ReturnType<typeof serveStatic>>, 'close'> | undefined;
  let browser: Browser | undefined;
  let base = process.env.RICHVIEW_VERIFY_URL?.replace(/\/$/, '');
  try {
    if (!base) {
      const publicSource = resolve(process.env.RICHVIEW_VERIFY_PUBLIC_DIR || join(root, 'dist/web'));
      if (!existsSync(join(publicSource, 'richview_site_bg.wasm'))) {
        throw new Error('Build the current Rust release first: npm run build');
      }
      // The site is a separate static build (ARC-24): the suites read the built copy over loopback.
      const site = await serveStatic({ '/': publicSource });
      server = { close: site.close };
      base = site.base;
    }
    const launched: Browser = await launchBrowser(engines[engine as keyof typeof engines], {
      headless: true,
      ...(engine === 'chromium' ? { executablePath: chromiumPath(), args: ['--no-sandbox', ...(process.platform === 'darwin' && process.arch === 'arm64' ? ['--enable-gpu'] : [])] } : {}),
    }, { TimeoutError: errors.TimeoutError });
    browser = launched;
    writeFileSync(join(dir, 'browser-environment.json'), JSON.stringify({ engine, version: browser.version(), platform: process.platform, architecture: process.arch }, null, 2) + '\n');
    return {
      base, browser: launched, dir, engine,
      save(name: string, value: unknown) { const file = join(dir, name); writeFileSync(file, JSON.stringify(value, null, 2) + '\n'); return file; },
      async close() {
        await launched.close();
        await server?.close();
      },
    };
  } catch (error) {
    if (browser) await browser.close();
    await server?.close();
    throw error;
  }
}

export function instrument() {
  window.__rvEvidence = { longTasks: [], requests: [], replies: [], gateReply: false, heldReply: null,
    releaseReply() {
      const reply = this.heldReply;
      if (!reply) throw new Error('No worker reply is held');
      this.heldReply = null;
      reply();
    },
  };
  if (PerformanceObserver.supportedEntryTypes.includes('longtask')) {
    new PerformanceObserver(list => {
      for (const item of list.getEntries()) window.__rvEvidence.longTasks.push({ start_ms: item.startTime, duration_ms: item.duration });
    }).observe({ type: 'longtask', buffered: true });
  }
  const post = Worker.prototype.postMessage;
  Worker.prototype.postMessage = function (this: Worker, data: WorkerRequest, ...args: [transferOrOptions?: Transferable[] | StructuredSerializeOptions]) {
    const bytes = new TextEncoder().encode(data.source || '');
    const record: RequestEvidence = { id: data.id, source: data.source?.slice(0, 120), source_bytes: bytes.length, at: performance.now() };
    window.__rvEvidence.requests.push(record);
    if (bytes.length) crypto.subtle.digest('SHA-256', bytes).then(hash => {
      record.source_sha256 = [...new Uint8Array(hash)].map(byte => byte.toString(16).padStart(2, '0')).join('');
    });
    if (window.__rvEvidence.failNextRequest) {
      window.__rvEvidence.failNextRequest = false;
      // Fault injection still goes through the actual Rust options decoder/error path.
      data = { ...data, options: '{' };
    }
    return Reflect.apply(post, this, [data, ...args]);
  };
  const add = Worker.prototype.addEventListener;
  Worker.prototype.addEventListener = function (this: Worker, name: string, listener: EventListenerOrEventListenerObject, options?: boolean | AddEventListenerOptions) {
    if (name !== 'message' || typeof listener !== 'function') return add.call(this, name, listener, options);
    return add.call(this, name, event => {
      const { data } = event as MessageEvent<WorkerReply>;
      window.__rvEvidence.replies.push({ id: data.id, error: data.error, at: performance.now() });
      if (window.__rvEvidence.gateReply) {
        window.__rvEvidence.gateReply = false;
        // Keep the actual Rust reply until the test observes the intended pending edits.
        window.__rvEvidence.heldReply = () => listener.call(this, event);
      } else listener.call(this, event);
    }, options);
  };
}

// Epoch times (ms) of recent requestAnimationFrame callbacks in this document, for latency from an
// input to the first frame after it. The runtime emits no per-frame event.
export function frameClock() {
  const frames: number[] = window.__rvFrameClock = [];
  const loop = (time: number) => {
    frames.push(performance.timeOrigin + time);
    if (frames.length > 600) frames.splice(0, 300);
    requestAnimationFrame(loop);
  };
  requestAnimationFrame(loop);
}

export function watchErrors(page: Page, errors: string[], label = '') {
  page.on('pageerror', error => errors.push(`${label}${error.message}`));
  page.on('console', message => { if (message.type() === 'error') errors.push(`${label}${message.text()}`); });
}

export async function settled(page: Page) {
  await page.evaluate(() => new Promise<number>(accept => requestAnimationFrame(() => requestAnimationFrame(accept))));
}

// Chromium runs no animation frames in a cross-origin frame that lies wholly outside the viewport,
// and Playwright's scroll first waits for two of the element's own frames, so it can never scroll
// such a frame's content into view: the home page's live example sits just below the fold of the
// browser suite's 1440x1000 page. The frame's element lives in its parent, which keeps
// running: scroll that into view first. A frame already on screen is left where it is.
async function revealFrame(scope: Scope) {
  if ('owner' in scope) {
    await scope.owner().evaluate(reveal);
    return;
  }
  const owner = 'parentFrame' in scope && scope.parentFrame() ? await scope.frameElement() : null;
  if (!owner) return;
  try {
    await owner.evaluate(reveal);
  } finally {
    await owner.dispose();
  }
}

function reveal(element: Element) {
  const box = element.getBoundingClientRect();
  if (box.bottom <= 0 || box.right <= 0 || box.top >= innerHeight || box.left >= innerWidth) element.scrollIntoView({ block: 'nearest', inline: 'nearest' });
}

// Static report payloads exist before lazy chart views. Wait for the expected document, then
// ask its runtime to mount and paint every chart before inspecting below-the-fold output.
export async function settledCharts(scope: Scope, count: number) {
  await scope.locator('figure[data-rv-chart]').nth(count - 1).waitFor({ state: 'attached', timeout: 45_000 });
  // Chromium pauses animation frames in a preview iframe below the viewport.
  await revealFrame(scope);
  await scope.locator('body').scrollIntoViewIfNeeded();
  await scope.locator('body').evaluate(async () => {
    const end = performance.now() + 45_000;
    while ((document.documentElement.dataset.rvRuntime !== 'started' || !window.__richview) && performance.now() < end) await new Promise(accept => setTimeout(accept, 50));
    if (document.documentElement.dataset.rvRuntime !== 'started' || !window.__richview) throw new Error('The report runtime did not start');
    await window.__richview.settled();
  });
}

export async function inputValue(locator: Locator, value: string) {
  // Bulk source/CSV paste setup avoids Playwright's insertText path for 100k lines.
  // It invokes the real browser input handler and still measures its synchronous work.
  await locator.evaluate((input, value) => {
    (input as HTMLInputElement | HTMLTextAreaElement).value = value;
    input.dispatchEvent(new Event('input', { bubbles: true }));
  }, value);
}
