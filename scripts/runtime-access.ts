// Host reason (ARC-03): verify sandboxed report previews behind a cookie-gated HTTPS host.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createServer } from 'node:https';
import { existsSync, mkdtempSync, mkdirSync, readFileSync, rmSync, statSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { extname, join, resolve, sep } from 'node:path';
import { headersFrom, parseHeaderRules, root } from './browser-harness.ts';
import { PREVIEW_RELOAD, recordPreviewReload, reloadRefusal } from '../tools/rv-gate/lib/preview-reload.mts';
import type { Browser, Frame } from 'playwright';
import type { AddressInfo } from 'node:net';
import type { Server } from 'node:https';

type Probe<T> = { value: T; timeout?: never } | { error: string; value?: never; timeout?: never } | { timeout: true; value?: never };
type AccessFailure = {
  expected: string;
  error: string;
  identity?: { same: boolean; currentDetached?: boolean; retainedDetached?: boolean };
  parent?: { hasExpected: boolean; [detail: string]: unknown };
  parentProbe?: Probe<unknown>;
  snapshotTimeout?: boolean;
  currentFrame?: unknown;
  retainedFrame?: unknown;
};
export type PreviewEvidence = {
  case: string;
  recovery: number;
  screenshot: string;
  failure: AccessFailure | Record<string, unknown>;
  restore?: { status: string } | { mutationCount: number; mutationTimesMs: number[] } | { error: string };
};
export type RuntimeAccessEvidence = {
  preview_reloads?: PreviewEvidence[];
  runtime_access_failures?: { case: string; failure: AccessFailure; denied: string[]; refusal?: string }[];
};

// Like Access, this host returns a login redirect when a request omits the Secure cookie. A
// sandboxed frame's anonymous CORS request would omit it, while the site's own fetch carries it.
export async function verifyRuntimeAccess(browser: Browser, { engine, dir, evidence }: { engine: string; dir: string; evidence: RuntimeAccessEvidence }) {
  evidence.preview_reloads ||= [];
  const publicDir = resolve(process.env.RICHVIEW_VERIFY_PUBLIC_DIR || join(root, 'dist/web'));
  const policies = parseHeaderRules(readFileSync(join(publicDir, '_headers'), 'utf8'));
  const parent = join(root, 'tmp');
  mkdirSync(parent, { recursive: true });
  const scratch = mkdtempSync(join(parent, 'runtime-access-'));
  const key = join(scratch, 'key.pem'), cert = join(scratch, 'cert.pem');
  let server: Server | undefined;
  try {
    execFileSync('openssl', ['req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', key, '-out', cert, '-days', '1', '-subj', '/CN=localhost'], { stdio: 'ignore' });
    const denied: string[] = [], authenticatedRuntime: string[] = [];
    const types: Record<string, string> = { '.html': 'text/html', '.js': 'text/javascript', '.mjs': 'text/javascript', '.wasm': 'application/wasm', '.css': 'text/css', '.json': 'application/json', '.csv': 'text/csv', '.woff2': 'font/woff2', '.svg': 'image/svg+xml', '.webp': 'image/webp', '.png': 'image/png', '.jpg': 'image/jpeg', '.webm': 'video/webm', '.mp4': 'video/mp4' };
    server = createServer({ key: readFileSync(key), cert: readFileSync(cert) }, (req, res) => {
      const path = decodeURIComponent(new URL(req.url!, 'https://127.0.0.1').pathname);
      if (path === '/login') {
        res.writeHead(200, { 'set-cookie': 'CF_Authorization=ok; Path=/; SameSite=None; Secure; HttpOnly' });
        return res.end('signed in');
      }
      const runtime = /^\/(?:richview_runtime(?:_bg\.wasm|\.js)|(?:landing|docs)\/runtime\/[^/]+\/richview_runtime(?:_bg\.wasm|\.js))$/.test(path);
      if (!/CF_Authorization=ok/.test(req.headers.cookie ?? '')) {
        if (runtime) denied.push(path);
        res.writeHead(302, { location: '/login' });
        return res.end();
      }
      if (runtime) authenticatedRuntime.push(path);
      const file = resolve(publicDir, `.${path}`);
      if (file !== publicDir && !file.startsWith(publicDir + sep)) { res.writeHead(403); return res.end(); }
      const found = [file, join(file, 'index.html'), `${file}.html`].find(f => existsSync(f) && statSync(f).isFile());
      if (!found) { res.writeHead(404); return res.end(); }
      res.writeHead(200, { 'content-type': types[extname(found)] || 'application/octet-stream', 'cache-control': 'no-store', ...headersFrom(policies, path) });
      res.end(readFileSync(found));
    });
    await new Promise<void>((ok, reject) => server!.once('error', reject).listen(0, '127.0.0.1', ok));
    const base = `https://127.0.0.1:${(server.address() as AddressInfo).port}`;
    const context = await browser.newContext({ ignoreHTTPSErrors: true, reducedMotion: 'reduce' });
    try {
      const page = await context.newPage();
      page.setDefaultTimeout(60_000);
      // A failed probe must not hang with the frame it is diagnosing.
      const bounded = async <T>(operation: () => T | Promise<T>): Promise<Probe<T>> => {
        let timer;
        try {
          return await Promise.race([
            Promise.resolve().then(operation).then<Probe<T>, Probe<T>>(value => ({ value }), error => ({ error: String(error) })),
            new Promise<Probe<T>>(resolve => { timer = setTimeout(() => resolve({ timeout: true }), 2_000); }),
          ]);
        } finally { clearTimeout(timer); }
      };
      const waitForInlineRuntime = async (selector: string, title: string, beforeSeq: number | null = null) => {
        const element = await page.locator(selector).elementHandle();
        const retained = await element.contentFrame();
        await element.dispose();
        try {
          await page.frameLocator(selector).locator('html[data-rv-runtime="started"]').waitFor();
          return true;
        } catch (error) {
          const failure: AccessFailure = { expected: 'html[data-rv-runtime="started"]', error: String(error) };
          const peek = (frame: Frame|null) => frame!.evaluate(() => ({
            url: location.href, ready: document.readyState, runtime: document.documentElement.dataset.rvRuntime,
            title: document.title, body: document.body?.textContent.slice(0, 1000),
          }));
          const [parent, current, previous] = await Promise.all([
            bounded(() => page.evaluate(({ selector, beforeSeq }) => {
              const frame = document.querySelector(selector);
              const srcdoc = frame?.getAttribute('srcdoc') ?? '';
              const seq = Number(frame?.getAttribute('data-seq') ?? 0);
              const buffers = [...document.querySelectorAll('#demo-result, #demo-result-next')].map(item => ({
                id: item.id, seq: Number(item.getAttribute('data-seq') ?? 0), buffer: item.getAttribute('data-buffer'),
              }));
              const hasInline = srcdoc.includes('<script type="module">');
              const hasExternal = srcdoc.includes('<script src=');
              const latest = beforeSeq === null || (seq > beforeSeq && buffers.every(item => item.seq <= seq));
              return { srcdoc, seq, beforeSeq, buffers, hasInline, hasExternal, latest,
                status: document.querySelector('#demo-status')?.textContent,
                hasExpected: hasInline && !hasExternal && latest };
            }, { selector, beforeSeq })),
            bounded(async () => {
              const handle = await page.locator(selector).elementHandle();
              let frame;
              try { frame = await handle.contentFrame(); } finally { await handle.dispose(); }
              failure.identity = { same: frame === retained, currentDetached: frame?.isDetached(), retainedDetached: retained?.isDetached() };
              return peek(frame);
            }),
            bounded(() => peek(retained)),
          ]);
          if (parent.value) {
            const { srcdoc, ...details } = parent.value;
            failure.parent = { ...details, srcdocBytes: Buffer.byteLength(srcdoc), srcdocSha256: createHash('sha256').update(srcdoc).digest('hex') };
          } else failure.parentProbe = parent;
          if (current.timeout && previous.timeout) failure.snapshotTimeout = true;
          else {
            if (!current.timeout) failure.currentFrame = current.value ?? current;
            if (!previous.timeout) failure.retainedFrame = previous.value ?? previous;
          }
          (evidence.runtime_access_failures ||= []).push({ case: title, failure, denied: [...denied] });
          assert.deepEqual(denied, [], 'No opaque preview requests a cookie-gated runtime');
          let refusal = reloadRefusal(failure, { engine, reloads: evidence.preview_reloads!, title });
          if (!refusal && failure.identity?.same !== true) refusal = 'the current and retained frames differ';
          const recovery = evidence.preview_reloads!.length + 1;
          const screenshot = join(process.env.RV_GATE_PREVIEW_RELOAD_DIR || dir, `preview-reload-${recovery}.png`);
          if (!refusal) await page.locator(selector).screenshot({ path: screenshot, timeout: 10_000 }).catch(shot => { refusal = `no parent-side screenshot of the preview: ${shot.message}`; });
          if (refusal) { evidence.runtime_access_failures.at(-1)!.refusal = refusal; throw error; }
          evidence.preview_reloads!.push({ case: title, recovery, screenshot, failure });
          recordPreviewReload({ title, recovery, screenshot });
          return false;
        }
      };
      // Repeat the complete failed page check; denied requests persist across every attempt.
      const runStage = async (title: string, check: () => Promise<boolean>) => {
        for (let attempt = 0; attempt <= PREVIEW_RELOAD.maxPerCase; attempt++) {
          if (await check()) return;
        }
        throw new Error(`${title} exceeded its recovery attempts`);
      };
      await page.goto(`${base}/login`);
      await runStage('runtime access: playground', async () => {
        await page.goto(`${base}/playground`, { waitUntil: 'domcontentloaded' });
        if (!await waitForInlineRuntime('#preview', 'runtime access: playground')) return false;
        const html = await page.locator('#preview').getAttribute('srcdoc');
        assert.ok(html?.includes('<script type="module">') && !html.includes('<script src='), 'Playground runtime is inline');
        return true;
      });
      await runStage('runtime access: landing demo', async () => {
        // The site's report pages link the one hashed runtime with SRI. The styles scene frames them lazily, so the
        // first, /landing/anchor.html, is checked as its own page (the first screen frames no report).
        await page.goto(`${base}/landing/anchor.html`, { waitUntil: 'domcontentloaded' });
        await page.locator('html[data-rv-runtime="started"]').waitFor();
        assert.ok(await page.locator('script[src][integrity]').count(), 'The site report runtime is integrity-pinned');
        await page.goto(`${base}/`, { waitUntil: 'domcontentloaded' });
        await page.keyboard.press('Shift');
        await page.locator('html[data-richview-started="rust"]').waitFor();
        const source = page.locator('#demo-source');
        const beforeSeq = await page.evaluate(() => Math.max(0, ...[...document.querySelectorAll('#demo-result, #demo-result-next')].map(frame => Number(frame.getAttribute('data-seq') ?? 0))));
        await source.fill((await source.inputValue()) + '\n');
        await page.waitForFunction(() => document.querySelector<HTMLIFrameElement>('#demo-result')?.srcdoc.includes('<script type="module">'), null, { timeout: 60_000 });
        return waitForInlineRuntime('#demo-result', 'runtime access: landing demo', beforeSeq);
      });
      await runStage('runtime access: documentation', async () => {
        await page.goto(`${base}/docs/charts/line`, { waitUntil: 'domcontentloaded' });
        await page.locator('html[data-richview-started="rust"]').waitFor();
        await page.locator('.docs-styles__links a').first().click();
        if (!await waitForInlineRuntime('.docs-styles__frame', 'runtime access: documentation')) return false;
        const frame = page.locator('.docs-styles__frame');
        assert.match((await frame.getAttribute('src'))!, /^\/docs\/previews\/line\//, 'Docs use a staged preview');
        const script = page.frameLocator('.docs-styles__frame').locator('script[src][integrity]');
        assert.match((await script.getAttribute('src'))!, /^\/docs\/runtime\/[a-f0-9]+\/richview_runtime\.js$/, 'Docs use the pinned shared runtime');
        assert.equal(await script.getAttribute('crossorigin'), 'use-credentials');
        await page.frameLocator('.docs-styles__frame').locator('canvas').first().waitFor();
        return true;
      });
      await runStage('runtime access: gallery', async () => {
        await page.goto(`${base}/docs/`, { waitUntil: 'domcontentloaded' });
        // A gallery card is a picture the build draws (16 Nov 2026), so it needs no runtime and no
        // frame: the card after the flagship's cover loads its picture behind the same gate.
        const picture = page.locator('.docs-report-grid .docs-chart-card--flagship + article .docs-thumbnail > img');
        await picture.scrollIntoViewIfNeeded();
        assert.match((await picture.getAttribute('src'))!, /^\/docs\/previews\/report-[\w-]+\/[\w-]+-card\.png$/, 'A gallery card shows its built picture');
        await page.waitForFunction((selector: string) => document.querySelector<HTMLImageElement>(selector)?.naturalWidth! > 0, '.docs-report-grid .docs-chart-card--flagship + article .docs-thumbnail > img');
        assert.equal(await page.locator('.docs-report-grid iframe').count(), 0, 'No gallery card is a live frame');
        return true;
      });
      assert.ok(authenticatedRuntime.some(path => /^\/docs\/runtime\/.+\/richview_runtime\.js$/.test(path)), 'Docs runtime script carried the cookie');
      assert.deepEqual(denied, [], 'No opaque preview requests a cookie-gated runtime');
      const anonymous = await browser.newContext({ ignoreHTTPSErrors: true });
      try {
        const scripts = authenticatedRuntime.filter(path => path.startsWith('/docs/runtime/'));
        for (const path of new Set(scripts.flatMap(path => [path, path.replace(/\.js$/, '_bg.wasm')]))) {
          const response = await anonymous.request.get(`${base}${path}`, { maxRedirects: 0 });
          assert.equal(response.status(), 302, `The cookie gate still protects ${path}`);
        }
      } finally { await anonymous.close(); }
    } finally { await context.close(); }
  } finally {
    if (server) await new Promise(ok => server!.close(ok));
    rmSync(scratch, { recursive: true, force: true });
  }
}
