// Retakes the home page's "Your brand" picture, assets/landing/brand/company-report-*.
//
// The picture is the home page's example report (assets/landing/anchor) without its photo, rendered
// by the CLI over Air with assets/landing/brand/company.rstyle.yaml, the placeholder company's logo
// (examples/company-style/logo.png) and its typeface. It is the top of the report at 640 x 868 CSS
// pixels and twice the density, down to the gap under the first paragraph, written as WebP and AVIF
// at 1280 and 720 pixels wide. The page states the picture's size, 1280 x 1736.
//
// It is a committed capture: a change to the report's look does not reach it until this runs again.
// Run it from the checkout's root after a build, on macOS so the text is set as in the other
// pictures of the page:
//   node scripts/capture-brand-report.mjs [path to richview]
// The pictures are encoded by Pillow through uv.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { chromium } from 'playwright';
import { chromiumPath, cli, root } from './browser-harness.mjs';

const binary = process.argv[2] || cli;
const [width, height] = [640, 868];
const folder = mkdtempSync(join(tmpdir(), 'richview-brand-'));
try {
  // The report folder a company would keep: the example, its style file, its logo and its typeface.
  const source = readFileSync(join(root, 'assets/landing/anchor/report.rmd'), 'utf8');
  const report = source
    .replace(/^styles: .*$/m, 'styles: [air, company.rstyle.yaml]')
    .replace(/^title_image:\n(?: {2}.*\n)+/m, '');
  assert.ok(!report.includes('title_image') && report.includes('styles: [air, company.rstyle.yaml]'), 'the example has the frontmatter this script edits');
  writeFileSync(join(folder, 'report.rmd'), report);
  cpSync(join(root, 'assets/landing/anchor/data'), join(folder, 'data'), { recursive: true });
  cpSync(join(root, 'assets/landing/brand/company.rstyle.yaml'), join(folder, 'company.rstyle.yaml'));
  mkdirSync(join(folder, 'brand'));
  cpSync(join(root, 'examples/company-style/logo.png'), join(folder, 'brand/logo.png'));
  cpSync(join(root, 'assets/site-fonts/bricolage-grotesque-600.woff2'), join(folder, 'brand/bricolage-grotesque-600.woff2'));

  const run = spawnSync(binary, ['generate', folder, '--format', 'json'], { encoding: 'utf8', timeout: 120_000 });
  assert.equal(run.status, 0, `richview generate failed: ${run.stderr || run.stdout}`);
  // The file the home page shows must be one today's RichView takes without a word.
  const findings = JSON.parse(run.stdout).diagnostics.filter(finding => finding.level !== 'info');
  assert.deepEqual(findings, [], 'the brand file renders without a warning');

  const browser = await chromium.launch({ headless: true, executablePath: chromiumPath() });
  try {
    // A taller window than the picture, so the report's foot band stays below the clip.
    const page = await browser.newPage({ viewport: { width, height: height + 300 }, deviceScaleFactor: 2, reducedMotion: 'reduce' });
    await page.goto(`file://${join(folder, 'Rendered/report.html')}`, { waitUntil: 'load' });
    await page.evaluate(() => document.fonts.ready);
    await page.waitForFunction(() => document.querySelector('.rv-chart canvas, .rv-chart svg') && [...document.images].every(image => image.complete && image.naturalWidth > 0));
    await page.waitForTimeout(1000);
    // The picture ends between two blocks, never through a line of text.
    const cut = await page.evaluate(edge => [...document.querySelectorAll('.rv-report__body > *, .rv-report__body > .rv-keep > *')].filter(block => { const box = block.getBoundingClientRect(); return box.height > 0 && box.top < edge && box.bottom > edge; }).map(block => block.tagName), height);
    assert.deepEqual(cut, [], `the picture's lower edge at ${height} px cuts through a block; choose another height`);
    await page.screenshot({ path: join(folder, 'capture.png'), clip: { x: 0, y: 0, width, height } });
  } finally {
    await browser.close();
  }

  const out = join(root, 'assets/landing/brand');
  const encode = `
import sys
from PIL import Image
capture = Image.open(sys.argv[1]).convert("RGB")
assert capture.size == (1280, 1736), capture.size
for wide in (1280, 720):
    picture = capture if wide == 1280 else capture.resize((720, 976), Image.LANCZOS)
    picture.save(f"{sys.argv[2]}/company-report-{wide}.webp", "WEBP", quality=82, method=6)
    picture.save(f"{sys.argv[2]}/company-report-{wide}.avif", "AVIF", quality=60, speed=2)
    print(wide, picture.size)
`;
  const encoded = spawnSync('uv', ['run', '--no-project', '--quiet', '--with', 'pillow>=11.3', 'python', '-I', '-c', encode, join(folder, 'capture.png'), out], { encoding: 'utf8', timeout: 300_000 });
  assert.equal(encoded.status, 0, `encoding failed: ${encoded.stderr}`);
  process.stdout.write(encoded.stdout);
} finally {
  rmSync(folder, { recursive: true, force: true });
}
