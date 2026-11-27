import type { CapturedRender, RecordedRenderer } from '../capture-types.ts';
import type { AddressInfo } from 'node:net';
import { createServer } from 'node:http';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'node:fs';
import { resolve, join, extname, sep } from 'node:path';
import { pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
import { execFileSync } from 'node:child_process';

const { values } = parseArgs({ options: {
  worktree: { type: 'string' }, out: { type: 'string' }, cases: { type: 'string' },
  sources: { type: 'string' }, themes: { type: 'string', default: 'sleek,scientific,brutalist,sleek-light' },
  widths: { type: 'string', default: '320,390,720' }, motion: { type: 'string', default: 'reduce' },
} });
if (!values.worktree || !values.out) throw Error('--worktree and --out are required');
const wt = resolve(values.worktree!), out = resolve(values.out!), dist = join(wt, 'dist/web');
mkdirSync(out, { recursive: true });
const { chromium } = await import(pathToFileURL(join(wt, 'node_modules/playwright/index.mjs')).href) as typeof import('playwright');
const { default: rust } = await import(pathToFileURL(join(wt, 'packages/core/dist/index.js')).href) as { default: RecordedRenderer };
const sourceCases: { id: string; path?: string; source?: string; data?: Record<string, unknown> }[] = values.sources ? JSON.parse(readFileSync(values.sources, 'utf8')) :
  (values.cases?.split(',') || JSON.parse(readFileSync(join(wt, 'benchmarks/chart-craft/catalog.json'), 'utf8')) as string[])
    .map(id => ({ id, path: join(wt, `benchmarks/chart-craft/reports/${id}/report.rmd`) }));
const pages = new Map<string, string>();
const server = createServer((req, res) => {
  const pathname = new URL(req.url!, 'http://localhost').pathname;
  if (pathname === '/favicon.ico') { res.writeHead(204); res.end(); return; }
  if (pages.has(pathname)) { res.writeHead(200, { 'content-type': 'text/html' }); res.end(pages.get(pathname)); return; }
  const file = resolve(dist, '.' + pathname);
  if (!file.startsWith(dist + sep) || !existsSync(file)) { res.writeHead(404); res.end('Not found'); return; }
  const mime = ({ '.html': 'text/html', '.js': 'text/javascript', '.wasm': 'application/wasm', '.css': 'text/css', '.woff2': 'font/woff2' } as Record<string, string>)[extname(file)] || 'application/octet-stream';
  res.writeHead(200, { 'content-type': mime, 'cache-control': 'no-store' }); res.end(readFileSync(file));
});
await new Promise<void>(done => server.listen(0, '127.0.0.1', done));
const base = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
const browser = await chromium.launch({ executablePath: '/usr/bin/chromium', headless: true });
const manifest = { head: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: wt, encoding: 'utf8' }).trim(),
  diff_sha256: createHash('sha256').update(execFileSync('git', ['diff', 'HEAD'], { cwd: wt })).digest('hex'),
  wasm_sha256: createHash('sha256').update(readFileSync(join(dist, 'richview_web_bg.wasm'))).digest('hex'),
  worktree: wt, browser: browser.version(), created_at: new Date().toISOString(), records: [] as object[], errors: [] as string[] };
try {
  for (const theme of values.themes.split(',')) for (const width of values.widths.split(',').map(Number)) {
    const context = await browser.newContext({ viewport: { width, height: 900 }, deviceScaleFactor: 2, reducedMotion: values.motion as 'reduce' | 'no-preference' });
    const page = await context.newPage();
    const errors: string[] = [];
    page.on('pageerror', error => errors.push(error.message));
    page.on('console', message => { if (message.type() === 'error') errors.push(`${message.text()} ${JSON.stringify(message.location())}`); });
    for (const item of sourceCases) {
      const source = item.source ?? readFileSync(item.path!, 'utf8');
      const id = `${item.id}-${theme}-${width}`;
      const sourceName = item.path?.split('/').at(-1) || 'report.rmd';
      const output: CapturedRender = JSON.parse(rust.render_source(source, JSON.stringify({
        width, styles: [theme], data: item.data || {},
        report_folder: { root: item.id, source: sourceName, files: [sourceName, 'reference.json', ...Object.keys(item.data || {})] },
        runtime_url: base + '/richview_web',
      })));
      const dir = join(out, id); mkdirSync(dir, { recursive: true });
      writeFileSync(join(dir, 'render.json'), JSON.stringify(output.metadata, null, 2));
      pages.set('/' + id, output.html);
      const record = { id, case: item.id, theme, width,
        source_sha256: createHash('sha256').update(source).digest('hex'),
        native: output.metadata?._richview?.chart_diagnostics, charts: [] as object[], errors: [] as string[], error: undefined as string | undefined };
      const startError = errors.length;
      try {
        await page.goto(base + '/' + id, { waitUntil: 'networkidle' });
        await page.evaluate(() => document.fonts.ready);
        const figures = await page.locator('figure.rv-chart').all();
        if (!figures.length) throw Error('No rendered chart figure');
        for (let i = 0; i < figures.length; i++) {
          const figure = figures[i];
          await figure.scrollIntoViewIfNeeded();
          await page.waitForFunction(() => [...document.querySelectorAll('.rvc')].some(n => n.hasAttribute('data-rvc-diagnostics')));
          await page.evaluate(async () => {
            if (window.__richview?.settled) await window.__richview!.settled();
            await new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done)));
          });
          const details = await figure.evaluate(node => {
            const root = node.querySelector('.rvc'), svg = node.querySelector('.rvc-svg');
            const rect = node.getBoundingClientRect();
            return { box: { width: rect.width, height: rect.height },
              diagnostics: root?.getAttribute('data-rvc-diagnostics'),
              child_diagnostics: [...node.querySelectorAll('.rvc[data-rvc-diagnostics]')].map(n => JSON.parse(n.getAttribute('data-rvc-diagnostics')!)),
              motion: { mode: matchMedia('(prefers-reduced-motion: reduce)').matches ? 'reduce' : 'no-preference', active_animations: document.getAnimations().filter(a => a.playState === 'running').length },
              pie_geometry: [...node.querySelectorAll('.rv-pie')].map(p => ({ row: p.getAttribute('data-row'), center: p.getAttribute('data-center'), angles: p.getAttribute('data-angles'), radius: p.getAttribute('data-r'), path: p.getAttribute('d') })),
              svg: svg?.outerHTML,
              texts: [...node.querySelectorAll<SVGTextElement>('text')].map(n => ({ text: n.textContent, box: { x: n.getBBox().x, y: n.getBBox().y, width: n.getBBox().width, height: n.getBBox().height } })),
              horizontal_overflow: document.documentElement.scrollWidth > innerWidth + 1,
            };
          });
          const image = join(dir, `chart-${i + 1}.png`);
          await figure.screenshot({ path: image });
          record.charts.push({ index: i + 1, image, ...details });
        }
      } catch (error) { record.error = (error as Error).message; manifest.errors.push(`${id}: ${record.error}`); }
      record.errors = errors.slice(startError);
      manifest.records.push(record);
      writeFileSync(join(out, 'manifest.json'), JSON.stringify(manifest, null, 2));
      console.log(id + ': ' + (record.error || `${record.charts.length} chart(s), ${record.errors.length} page errors`));
    }
    await context.close();
  }
} finally {
  await browser.close(); await new Promise<void>(done => server.close(() => done()));
}
if (manifest.errors.length) process.exitCode = 1;
