import assert from 'node:assert/strict';
import { readFileSync, mkdirSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { createHash } from 'node:crypto';
import { harness } from '../../../../scripts/browser-harness.ts';
import type { CapturedRender, RecordedRenderer, CapturePair } from '../capture-types.ts';

interface DuplicateBarsReceipt {
  base: { wasm_sha256: string };
  candidate: { wasm_sha256: string };
  capture: { images: (Pick<CapturePair, 'source' | 'source_sha256' | 'theme' | 'motion'> & {
    case: string;
    viewport: number;
    before_png: string;
    before_sha256: string;
    after_png: string;
    after_sha256: string;
  })[] };
}

const wt = resolve(process.argv[2] || '.'), out = resolve(process.argv[3] || 'tmp/duplicate-bars/reproduced');
const side = process.argv[4] || 'after';
assert(side === 'before' || side === 'after');
const evidence: DuplicateBarsReceipt = JSON.parse(readFileSync(new URL('./validation.json', import.meta.url), 'utf8'));
const { default: rust }: { default: RecordedRenderer } = await import(pathToFileURL(join(wt, 'packages/core/dist/index.js')).href);
const hash = (bytes: string | Buffer) => createHash('sha256').update(bytes).digest('hex');
assert.equal(hash(readFileSync(join(wt, 'dist/web/richview_web_bg.wasm'))), evidence[side === 'before' ? 'base' : 'candidate'].wasm_sha256, 'Use the recorded baseline or candidate build');
delete process.env.RICHVIEW_VERIFY_URL;
process.env.RICHVIEW_SERVER_BINARY = join(wt, 'target/release/richview-server');
process.env.RICHVIEW_VERIFY_PUBLIC_DIR = join(wt, 'dist/web');
mkdirSync(out, { recursive: true });
const h = await harness('duplicate-bars-evidence');
try {
  for (const item of evidence.capture.images) {
    const pair = { id: item.case, width: item.viewport, ...item };
    const expected: { file: string; sha256: string } = { file: item[`${side}_png`], sha256: item[`${side}_sha256`] };
    const context = await h.browser.newContext({ viewport: { width: pair.width, height: 1000 }, deviceScaleFactor: 2, reducedMotion: pair.motion });
    const page = await context.newPage(), source = readFileSync(new URL(pair.source, import.meta.url), 'utf8');
    assert.equal(hash(source), pair.source_sha256);
    const output: CapturedRender = JSON.parse(rust.render_source(source, JSON.stringify({ width: pair.width, styles: [pair.theme], runtime_url: h.base + '/richview_web', report_folder: { root: pair.id, source: 'report.rmd', files: ['report.rmd'] } })));
    const url = `${h.base}/evidence-${pair.id}`;
    await page.route(url, route => route.fulfill({ contentType: 'text/html', body: output.html }));
    await page.goto(url, { waitUntil: 'networkidle' });
    await page.evaluate(() => document.fonts.ready);
    const figure = page.locator('figure.rv-chart');
    await figure.scrollIntoViewIfNeeded();
    await page.waitForFunction(() => !!document.querySelector('.rvc[data-rvc-diagnostics]'));
    await page.evaluate(async () => { await window.__richview!.settled(); await new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done))); });
    const png = await figure.screenshot({ path: join(out, expected.file) });
    assert.equal(hash(png), expected.sha256, `${pair.id}: use the recorded browser, fonts and build for identical pixels`);
    await context.close();
  }
} finally { await h.close(); }
