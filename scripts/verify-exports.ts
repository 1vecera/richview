// PDF export verification. The release CLI prints real report folders to PDF through the local
// Chromium-family browser, twice (the second time without a network when `unshare` allows it),
// and Playwright prints the interactive report as a reader's browser would. Poppler and qpdf
// measure every PDF: page size, tags, outline, links, fonts, page luminance and bottom gaps,
// headings, figures and their source notes kept together, table rows, chart controls, and
// identical text and rasters across the two exports.
//
// The every-block report's 56 chart types are held to one more check, per chart (FEA-CH-75): types.json.
// RICHVIEW_BIN and RICHVIEW_DIST point at another build than target/release and dist/web.
//
//   node scripts/verify-exports.ts [--reports a,b] [--themes a,b] [--out DIR] [--browser PATH]
//                                   [--evidence tmp/verification/report.pdf]
import assert from 'node:assert/strict';
import { execFileSync, spawnSync, type SpawnSyncReturns } from 'node:child_process';
import { createHash } from 'node:crypto';
import { copyFileSync, cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { basename, join, resolve } from 'node:path';
import { parseArgs } from 'node:util';
import { chromium, errors, type Browser, type Route } from 'playwright';
import { chromiumPath, root, verificationShard, waitForBenchmark } from './browser-harness.ts';
import { launchBrowser } from '../tools/rv-gate/lib/browser-launch.mts';
import { imageFonts } from './export-image-ledger.ts';
import { figureFrames, gapAccepted, openingOf } from './export-gaps.ts';
import { drawnLabels, labelInStream, labelOnPage, locateTitles, MIN_VECTOR_PATHS, overlapShare, pageStreams, paintedPaths, parseBlocks, parseImages, vectorPaths } from './export-types.ts';

const { values: options } = parseArgs({ options: {
  reports: { type: 'string', default: 'openai-vs-anthropic,first-report,sales-report,mixed-media' },
  themes: { type: 'string', default: 'sleek,air,scientific,brutalist,hand-drawn' },
  out: { type: 'string' }, browser: { type: 'string' }, evidence: { type: 'string' },
} });
const cli = process.env.RICHVIEW_BIN ?? join(root, 'target/release/richview');
const has = (tool: string) => !spawnSync(tool, ['-v']).error;
for (const tool of ['pdfinfo', 'pdftotext', 'pdftoppm', 'pdffonts', 'qpdf']) if (!has(tool)) throw new Error(`${tool} is required (Poppler utilities and qpdf)`);
if (!existsSync(cli) || !existsSync(join(process.env.RICHVIEW_DIST ?? join(root, 'dist/web'), 'richview_site_bg.wasm'))) throw new Error('Build the current Rust release first: cargo run --release --locked -p richview-build');
mkdirSync(join(root, 'tmp/verification'), { recursive: true });
const out = resolve(options.out ?? mkdtempSync(join(root, 'tmp/verification/exports-')));
const reportNames = options.reports.split(',').filter(Boolean);
const themes = options.themes.split(',');
type ExportCase = { key: string; kind: string; report?: string };
const cases: ExportCase[] = [
  ...reportNames.map(name => ({ key: 'report:' + name, kind: 'report', report: name })),
  { key: 'catalogue:every-block', kind: 'catalogue' },
  { key: 'mcp:letter', kind: 'mcp' },
  { key: 'invalid-image:preserve', kind: 'invalid-image' },
];
const admission = process.env.RICHVIEW_VERIFY_PARENT === 'exports' ? 'exports' : null;
const cohort = cases.map(item => item.key);
if (new Set(cohort).size !== cohort.length) throw new Error('Export cases must have unique names');
const selection: { cases: ExportCase[]; shard: { index: number; count: number; cohort: string[]; selected: string[] } } = admission ? verificationShard(cases, { key: (item: ExportCase) => item.key }) : { cases, shard: { index: 0, count: 1, cohort, selected: [...cohort] } };
if (selection.shard.count > 1 && reportNames.includes('every-block')) throw new Error('The explicit every-block report and its catalogue share output paths; run this selection without sharding');
const workRoot = selection.shard.count === 1 ? join(out, 'work') : join(out, 'work', 'shard-' + selection.shard.index + '-of-' + selection.shard.count);
mkdirSync(workRoot, { recursive: true });
const run = (file: string, args: string[]) => execFileSync(file, args, { encoding: 'utf8', maxBuffer: 256 << 20 });
const env = { ...process.env, ...(options.browser ? { RICHVIEW_BROWSER: options.browser } : {}) };
// Native CLI/MCP calls buffer stderr; keep their relaunch events visible in the enclosing gate log.
function forwardBrowserEvents(result: SpawnSyncReturns<string>) {
  for (const line of String(result.stderr ?? '').split('\n')) if (line.startsWith('RICHVIEW_BROWSER_RELAUNCH ')) console.error(line);
}
// An empty network namespace proves the export needs no network. Chrome runs as the namespace's
// root there, so the CLI turns Chrome's sandbox off and says so.
const isolated = spawnSync('unshare', ['-rn', 'true']).status === 0;
// ImageMagick 7's `magick`; montage is avoided because it needs a configured default font.
const magick = has('magick');
const norm = (text: string) => text.toLowerCase().replace(/[\s­]+/g, '');
const skipped = new Set(['research', 'Rendered', 'tmp', 'node_modules', '__pycache__']);
const shippedFontFaces: { catalog: string; postscript: string }[] = JSON.parse(readFileSync(join(root, 'crates/richview-core/assets/font-manifest.json'), 'utf8')).faces;
const shippedPostscript = new Set(shippedFontFaces.map(face => face.postscript));
const mathPostscript = new Set(shippedFontFaces.filter(face => face.catalog === 'fonts-math.css').map(face => face.postscript));
const mathFamilies = [...new Set([...mathPostscript].map(name => name.split('-')[0]))];

// FEA-RP-17: real code whose long lines must remain complete on paper, including a long identifier.
const codeText = (text: string) => text.replace(/[\s­]+/g, '');
const paperCode = `def fraction_by_customer_segment_and_product_category(observed_revenue_by_customer_segment_and_product_category):
    total_observed_revenue_czk = sum(observed_revenue_by_customer_segment_and_product_category.values())
    return {segment_and_product: observed_revenue_czk / total_observed_revenue_czk for segment_and_product, observed_revenue_czk in observed_revenue_by_customer_segment_and_product_category.items()}`;
const paperMath = String.raw`## Code, notes and formulas use bundled fonts

An inline formula $E=mc^2$ and a note[^font-proof] exercise the same report's math and return link.

$$\bar{x} = \frac{1}{n}\sum_{i=1}^{n}x_i$$

[^font-proof]: This note's return link must not introduce a host font.`;

function prepareReport(name: string, folder: string) {
  rmSync(folder, { recursive: true, force: true });
  if (name !== 'mixed-media') {
    cpSync(join(root, 'examples', name), folder, { recursive: true, filter: path => !skipped.has(basename(path)) && !basename(path).startsWith('.') });
    return;
  }
  mkdirSync(folder, { recursive: true });
  copyFileSync(join(root, 'examples/every-block/media/poster.png'), join(folder, 'poster.png'));
  // Players are never instantiated in PDF mode; these are deliberately inert local files.
  writeFileSync(join(folder, 'clip.mp4'), 'PDF media fixture');
  writeFileSync(join(folder, 'audio.mp3'), 'PDF media fixture');
  const rows = Array.from({ length: 45 }, (_, index) => ({ id: `R${String(index + 1).padStart(3, '0')}`, value: index + 1 }));
  writeFileSync(join(folder, 'report.rmd'), `---\ntitle: PDF media and complete tables\n---\n\n## Chart with source\n\n\`\`\`chart\ntype: bar\ntitle: The second observation is twice the first\ndata: {inline: [{category: A, value: 10}, {category: B, value: 20}]}\nencoding: {x: category, y: value}\n\`\`\`\n\n*Source: Synthetic PDF verification fixture.*\n\n## Media renditions\n\n\`\`\`video\ntitle: Video with poster\nsrc: clip.mp4\nposter: poster.png\ncaption: Local poster\n\`\`\`\n\n\`\`\`video\ntitle: Video without poster\nsrc: clip.mp4\ncaption: Video without poster\n\`\`\`\n\n\`\`\`audio\ntitle: Local recording\nsrc: audio.mp3\ncaption: Local recording\n\`\`\`\n\n\`\`\`embed\nurl: https://www.youtube.com/embed/example\ncaption: External page is a link\n\`\`\`\n\n## All observations\n\n\`\`\`table\ndata: {inline: ${JSON.stringify(rows)}}\ncolumns: [{key: id}, {key: value, format: number}]\ntotal: true\n\`\`\`\n\n## Complete code on paper\n\n\`\`\`python\n${paperCode}\n\`\`\`\n\n${paperMath}\n`);
}

// The page foot's running title, cut as the core cuts it.
function runningTitle(title: string) {
  if ([...title].length <= 70) return title;
  const cut = [...title].slice(0, 69).join('');
  return cut.slice(0, cut.lastIndexOf(' ') > 0 ? cut.lastIndexOf(' ') : cut.length).replace(/[ ,.;:]+$/, '') + '…';
}

type Diagnostic = { level: string; code: string; message: string };
type PrintSettled = { charts: number; report: { output: string } | null };
type NativePrint = { ok: boolean; diagnostics: Diagnostic[]; browser: { sandbox: boolean }; settled: PrintSettled; charts: unknown[] };
async function generate(folder: string, theme: string, name: string, isolate = false): Promise<NativePrint> {
  if (name.endsWith('.pdf')) { await closeBrowser(); await waitForBenchmark(); }
  const args = ['generate', join(folder, 'report.rmd'), '--style', theme, '-o', name, '--format', 'json'];
  const [file, argv]: [string, string[]] = isolate ? ['unshare', ['-rn', cli, ...args]] : [cli, args];
  const result = spawnSync(file, argv, { encoding: 'utf8', env, timeout: 180_000, maxBuffer: 64 << 20 });
  forwardBrowserEvents(result);
  let value: NativePrint | { ok: false; error: string };
  try { value = JSON.parse(result.stdout); } catch { value = { ok: false, error: result.stderr || result.stdout }; }
  if (result.status !== 0 || value.ok === false) throw new Error(`${folder} ${name}: ${JSON.stringify(value).slice(0, 2000)}`);
  return value;
}

// Grey 40 dpi pages: mean luminance, and the distance of the last content from the bottom of
// the page area as a share of its height. The margins (the foot holds the page numbers) are
// left out.
function rasters(pdf: string, dir: string, margins: number[]) {
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  run('pdftoppm', ['-r', '40', '-gray', pdf, join(dir, 'p')]);
  return readdirSync(dir).sort().map(file => {
    const data = readFileSync(join(dir, file));
    const header = data.toString('latin1', 0, 64).match(/^P5\s+(\d+)\s+(\d+)\s+255\s/);
    const [width, height] = [Number(header![1]), Number(header![2])];
    const pixels = data.subarray(header![0].length);
    const background = pixels[width * 2 + 2];
    let sum = 0;
    for (const value of pixels) sum += value;
    const top = Math.ceil(height * margins[0]), bottom = Math.floor(height * (1 - margins[1]));
    let last = -1;
    for (let y = top; y < bottom; y++) {
      for (let x = 0; x < width; x++) if (Math.abs(pixels[y * width + x] - background) > 24) { last = y; break; }
    }
    return { luminance: +(sum / pixels.length / 255).toFixed(3), empty: last < 0, gap: +(last < 0 ? 1 : (bottom - 1 - last) / (bottom - top)).toFixed(3) };
  });
}

// Per-page hashes of 110 dpi colour rasters.
function rasterHashes(pdf: string, dir: string) {
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  run('pdftoppm', ['-r', '110', pdf, join(dir, 'p')]);
  const hashes = readdirSync(dir).sort().map(file => createHash('sha256').update(readFileSync(join(dir, file))).digest('hex'));
  rmSync(dir, { recursive: true, force: true });
  return hashes;
}

// Every page at 40 dpi on one sheet, for review.
function sheet(pdf: string, file: string) {
  if (!magick) return null;
  const dir = `${file}.pages`;
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  run('pdftoppm', ['-r', '40', '-png', pdf, join(dir, 'p')]);
  const pages = readdirSync(dir).sort().map(f => join(dir, f));
  const rows = [];
  for (let i = 0; i < pages.length; i += 9) rows.push(['(', ...pages.slice(i, i + 9), '-bordercolor', '#8a8a8a', '-border', '4', '-background', '#8a8a8a', '+append', ')']);
  run('magick', [...rows.flat(), '-background', '#8a8a8a', '-gravity', 'northwest', '-append', file]);
  rmSync(dir, { recursive: true, force: true });
  return file;
}

// What the interactive report says about itself: headings, figure titles with their source
// notes, every table's rows (a link cell by its address) and its web links.
async function facts(browser: Browser, html: string) {
  const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, reducedMotion: 'reduce' });
  await context.route('**/*', (route: Route) => /^(file:|data:|blob:)/.test(route.request().url()) ? route.continue() : route.abort('blockedbyclient'));
  const page = await context.newPage();
  await page.goto('file://' + html, { waitUntil: 'load' });
  const value = await page.evaluate(() => {
    const text = (node: Element|null) => node?.textContent.replace(/\s+/g, ' ').trim() ?? '';
    const rows = (scope: Element) => [...scope.querySelectorAll<HTMLTableRowElement>('tbody tr')].map(row => [...row.cells].map(cell => cell.querySelector('a[href^="http"]')?.getAttribute('href') ?? text(cell)));
    return {
      title: text(document.querySelector('.rv-report__title')),
      code: [...document.querySelectorAll('.rv-report__body pre')].map(text),
      math: document.querySelectorAll('.rv-report__body math').length,
      footnote_backrefs: document.querySelectorAll('.rv-report__body .rv-fn-back').length,
      headings: [...document.querySelectorAll('.rv-report__body :is(h1,h2,h3,h4)')].map(text),
      // What opens a section: a heading h1 to h3, or the first words of a section element.
      section_starts: [...document.querySelectorAll('.rv-report__body :is(h1,h2,h3), .rv-report__body section')].map(node => text(node.matches('section') ? node.firstElementChild : node)).filter(Boolean),
      figures: [...document.querySelectorAll('figure.rv-chart')].map(figure => ({
        title: text(figure.querySelector('.rv-chart__title')),
        subtitle: text(figure.querySelector('.rv-chart__subtitle')),
        method: text(figure.querySelector('.rv-chart__method')),
        state: text(figure.querySelector('.rv-chart__state')),
        // The series names a printed legend carries (FEA-CH-67), part of the figure's unit.
        legend: [...(figure.querySelector('.rv-chart__legend')?.querySelectorAll('.rv-chart__legend-name') ?? [])].map(text),
        source: figure.nextElementSibling?.classList.contains('rv-source') ? text(figure.nextElementSibling) : null,
      })),
      tables: [...document.querySelectorAll('.rv-table'), ...[...document.querySelectorAll('.rv-report__body table')].filter(table => !table.closest('.rv-table'))].map(rows),
      links: document.querySelectorAll('.rv-report__body a[href^="http"]').length,
    };
  });
  await context.close();
  return value;
}

// The interactive report printed by the browser itself: print media, A4, backgrounds on.
async function browserPrint(browser: Browser, html: string, pdf: string) {
  const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, reducedMotion: 'reduce' });
  await context.route('**/*', (route: Route) => /^(file:|data:|blob:)/.test(route.request().url()) ? route.continue() : route.abort('blockedbyclient'));
  const page = await context.newPage();
  await page.goto('file://' + html, { waitUntil: 'load' });
  await page.emulateMedia({ media: 'print' });
  // The runtime installs itself after its module has compiled, which can follow the load event.
  if (await page.locator('figure[data-rv-chart]').count()) await page.waitForFunction(() => window.__richview?.settled, null, { timeout: 30_000 });
  await page.evaluate(async () => {
    await Promise.all([...document.fonts].map(face => face.load()));
    await document.fonts.ready;
    if (document.querySelector('figure[data-rv-chart]')) {
      if (!window.__richview?.settled) throw new Error('Browser print has no chart runtime');
      await window.__richview.settled();
    }
    const frame = () => new Promise<void>(done => requestAnimationFrame(() => done()));
    await Promise.allSettled(document.getAnimations().filter(animation => Number.isFinite(animation.effect?.getComputedTiming().endTime)).map(animation => animation.finished));
    await frame(); await frame();
  });
  await page.pdf({ path: pdf, format: 'A4', printBackground: true, margin: { top: '15mm', bottom: '15mm', left: '15mm', right: '15mm' } });
  await context.close();
}

// Each chart as the interactive report holds it: type and drawn text from the geometry model, the
// figure's own HTML text, and its height in points.
async function chartFacts(browser: Browser, html: string) {
  const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, reducedMotion: 'reduce' });
  await context.route('**/*', (route: Route) => /^(file:|data:|blob:)/.test(route.request().url()) ? route.continue() : route.abort('blockedbyclient'));
  const page = await context.newPage();
  await page.goto('file://' + html, { waitUntil: 'load' });
  await page.waitForFunction(() => window.__richview?.settled, null, { timeout: 45_000 });
  const value = await page.evaluate(async () => {
    await window.__richview!.settled();
    return window.__richview!.charts().map(chart => {
      const figure = document.getElementById(chart.id);
      const geometry = chart.geometry;
      return {
        id: chart.id, type: geometry.kind, height: geometry.height * 0.75,
        title: figure!.querySelector('.rv-chart__title')?.textContent.replace(/\s+/g, ' ').trim() ?? '',
        html: figure!.innerText,
        text: [...geometry.panels.flatMap(panel => panel.items), ...(geometry.figure_items ?? [])].filter((item): item is Extract<typeof item, { kind: 'text' }> => item.kind === 'text').map(item => item.text),
      };
    });
  });
  await context.close();
  return value;
}

// FEA-CH-75 and FEA-RP-43: a chart reaches the PDF as vector drawing, with text a reader can copy,
// on the page with its title. Per chart: (a) the title is in the PDF text; (b) a label the chart
// draws is text on that page, within the chart's box (`pdftotext -bbox-layout`, or the content
// stream for text set along a curve, which layout analysis splits); (c) no raster
// covers more than half of the box. Chrome writes vector drawing as paths and text, so an image
// is the one way a chart can turn into pixels, and `pdftohtml -xml` reports every image with its
// place on the page, which `pdfimages -list` does not. The box runs from the title's top down the
// chart's height, a little short of the whole figure, so a raster over the chart still covers
// most of it. A chart with no drawn text (a waffle, a Venn) cannot show a label, so (b) asks for its
// drawing instead: `qpdf --qdf` decompresses the page's content stream, whose path-painting operators
// are read with their transforms (export-types.ts `paintedPaths`), and at least MIN_VECTOR_PATHS painted
// paths must sit inside the box, none of them a backdrop. Never a pass on "no label tried".
type TypeResult = {
  index: number;
  type: string;
  title: string;
  page: number | null;
  a: boolean;
  b: boolean;
  c: boolean;
  ok: boolean;
  label?: string | null;
  via?: string;
  labels_tried?: number;
  vector_paths?: number;
  largest_raster?: number;
};
function typeChecks(pdf: string, charts: { id: string; type: string; height: number; title: string; html: string; text: string[]; }[], dir: string) {
  const pages = parseBlocks(run('pdftotext', ['-bbox-layout', pdf, '-']));
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  run('pdftohtml', ['-xml', '-q', '-nodrm', pdf, join(dir, 'p')]);
  const images = parseImages(readFileSync(join(dir, 'p.xml'), 'utf8'), pages);
  rmSync(dir, { recursive: true, force: true });
  const streams = run('pdftotext', ['-raw', '-enc', 'UTF-8', pdf, '-']).split('\f');
  const qdf = join(dir, 'every.qdf');
  mkdirSync(dir, { recursive: true });
  run('qpdf', ['--qdf', '--object-streams=disable', pdf, qdf]);
  const contents = pageStreams(readFileSync(qdf, 'latin1'));
  rmSync(dir, { recursive: true, force: true });
  const places = locateTitles(pages, charts.map(chart => chart.title));
  return charts.map((chart, index: number) => {
    const place = places[index];
    const entry: TypeResult = { index, type: chart.type, title: chart.title, page: place?.page ?? null, a: Boolean(place), b: false, c: false, ok: false };
    if (!place) return entry;
    const page = pages[place.page - 1];
    const box = { left: 0, top: place.block.top, width: page.width, height: chart.height };
    const labels = drawnLabels(chart.text, chart.html);
    entry.label = labels.length ? labelOnPage(page, { top: box.top, bottom: box.top + box.height + 60 }, labels, chart.html) : null;
    if (labels.length && !entry.label) { entry.label = labelInStream(streams[place.page - 1], chart.title, labels); entry.via = 'content stream'; }
    entry.labels_tried = labels.length;
    entry.vector_paths = vectorPaths(paintedPaths(contents[place.page - 1] ?? '', page.height), box);
    entry.b = labels.length ? entry.label !== null : entry.vector_paths >= MIN_VECTOR_PATHS;
    entry.largest_raster = Math.max(0, ...images.filter(image => image.page === place.page).map(image => +overlapShare(box, image).toFixed(3)));
    entry.c = entry.largest_raster <= 0.5;
    entry.ok = entry.a && entry.b && entry.c;
    return entry;
  });
}

const typeFailure = (where: string, entry: TypeResult) => {
  const reasons = [
    !entry.a && 'the title is not in the PDF text',
    entry.a && !entry.b && (entry.labels_tried ? `no drawn label is copyable text on page ${entry.page}` : `no label to find, and only ${entry.vector_paths} vector paths in the chart's box on page ${entry.page}`),
    entry.a && !entry.c && `a raster image covers ${Math.round(entry.largest_raster! * 100)}% of the chart on page ${entry.page}`,
  ].filter(Boolean);
  return reasons.length ? `${where} chart ${entry.index} (${entry.type}): ${reasons.join('; ')}` : null;
};

function fontsOf(pdf: string) {
  const lines = run('pdffonts', [pdf]).split('\n');
  const columns = [...lines[1].matchAll(/-+/g)].map(match => match.index);
  return lines.slice(2).filter(Boolean).map(line => {
    const field = (i: number) => line.slice(columns[i], columns[i + 1]).trim();
    const postscript = field(0).replace(/^[A-Z]{6}\+/, '');
    return { name: field(0), postscript, family: postscript.split('-')[0], embedded: field(3) === 'yes' };
  });
}

// The first block on a page, as `pdftotext -bbox-layout` reports it (export-gaps.ts says what it means).
function openingUnit(pdf: string, page: number, margins: number[], sectionStarts: string[], figures: Parameters<typeof openingOf>[1]['figures']) {
  const xml = run('pdftotext', ['-bbox-layout', '-f', String(page), '-l', String(page), pdf, '-']);
  const [{ height: pageHeight, blocks: pageBlocks }] = parseBlocks(xml);
  // Keep the existing body bounds, including the face ascent/descent allowance. Retain x too
  // so a nearby paragraph cannot be mistaken for text inside a tagged chart frame.
  const blocks = pageBlocks
    .filter(block => block.text && block.top >= pageHeight * margins[0] - 4 && block.bottom <= pageHeight * (1 - margins[1]) + 1)
    .sort((x, y) => x.top - y.top);
  return openingOf(blocks, { pageHeight, margins, sectionStarts, figures });
}

// Read code from the body box so a running foot cannot interrupt a block across pages.
// These are the same ascent/descent allowances used by openingUnit, in points at 72 dpi.
function codeBody(pdf: string, width: number, height: number, margins: number[]) {
  const top = Math.max(0, Math.floor(height * margins[0] - 4));
  const bottom = Math.min(height, Math.ceil(height * (1 - margins[1]) + 1));
  return run('pdftotext', ['-raw', '-enc', 'UTF-8', '-r', '72', '-x', '0', '-y', String(top), '-W', String(Math.ceil(width)), '-H', String(bottom - top), pdf, '-']);
}

function analyze(
  pdf: string,
  fact: Awaited<ReturnType<typeof facts>>,
  { foot, margins, fonts: expected, rasterDir, requireShippedFonts = false }: {
    foot: string | null; margins: number[]; fonts?: string[]; rasterDir: string; requireShippedFonts?: boolean;
  },
) {
  const info = Object.fromEntries(run('pdfinfo', [pdf]).split('\n').map(line => line.match(/^([^:]+):\s+(.*)$/)?.slice(1) as [string, string] | undefined).filter(fields => fields !== undefined));
  const [pageWidth, pageHeight] = info['Page size'].match(/([\d.]+) x ([\d.]+)/)!.slice(1).map(Number);
  // Content-stream order keeps each table cell's lines together; reading order puts the last
  // line of a page last. The page foot is removed from both.
  const pagesOf = (mode: string[]) => run('pdftotext', [...mode, '-enc', 'UTF-8', pdf, '-']).split('\f').slice(0, Number(info.Pages)).map((page, i, all) => {
    let content = norm(page).replace(`${i + 1}/${all.length}`, '');
    if (foot) content = content.replace(norm(foot), '');
    return content;
  });
  const text = run('pdftotext', ['-raw', '-enc', 'UTF-8', pdf, '-']);
  const codeBodyText = fact.code.length ? codeBody(pdf, pageWidth, pageHeight, margins) : '';
  const pages = text.split('\f').slice(0, Number(info.Pages));
  const body = pagesOf(['-raw']);
  const reading = pagesOf([]);
  const all = body.join('');
  const qdf = `${rasterDir}.qdf`;
  let qdfText;
  try {
    run('qpdf', ['--qdf', '--object-streams=disable', pdf, qdf]);
    qdfText = readFileSync(qdf, 'latin1');
  } finally { rmSync(qdf, { force: true }); }
  const links = (qdfText.match(/\/Subtype \/Link/g) ?? []).length;
  const raster = rasters(pdf, rasterDir, margins);
  const fonts = fontsOf(pdf);
  // A heading of several words must read with its spaces in content-stream order (copy and paste).
  const flat = text.replace(/\s+/g, ' ').toLowerCase();
  const runTogether = fact.headings.map((h: string) => h.trim().replace(/\s+/g, ' ')).filter((h: string) => h.includes(' ') && !flat.includes(h.toLowerCase()));
  // Figures in document order: the page with the title must also hold the source note's start.
  let from = 0;
  const figures = fact.figures.filter(figure => figure.source).map(figure => {
    const page = body.findIndex((content, i) => i >= from && content.includes(norm(figure.title.slice(0, 60))));
    if (page >= 0) from = page;
    return { title: figure.title, title_page: page + 1, source_on_page: page >= 0 && body[page].includes(norm(figure.source!.slice(0, 80))) };
  });
  const missingRows = fact.tables.flatMap((table, index: number) => {
    const signatures = table.map(cells => norm(cells.reduce((a, b) => (b.length > a.length ? b : a), '')).slice(0, 40)).filter(Boolean);
    const counts = signatures.reduce((map, s) => map.set(s, (map.get(s) ?? 0) + 1), new Map<string, number>());
    return [...counts].filter(([signature, count]) => all.split(signature).length - 1 < count).map(([signature]) => ({ table: index, row: signature }));
  });
  let ledger = null;
  const ledgerRows = fact.tables.find((table: string[][]) => table[0]?.[0] === 'C001');
  if (ledgerRows) {
    const raw = pages.join('\n');
    const section = raw.slice(raw.search(/sources\s+cited\s*\n/i));
    const once = ledgerRows.filter(([id]) => (section.match(new RegExp(`(?<![\\w-])${id.replace(/-/g, '\\-')}(?![\\w-])`, 'g')) ?? []).length === 1).length;
    ledger = { rows: ledgerRows.length, printed_once: once };
  }
  // FEA-RP-43, Daniel's option 3 on 2026-10-27: a page may end more than a quarter empty when
  // the first block on the next page opens a section (a heading h1 to h3 or a section element)
  // and is taller than the gap, because it does not fit in the space left, or when it is a figure
  // that ends its section (nothing after it on its page, a section opening or the end next) and
  // is taller than the gap. Every other gap over a quarter fails.
  const textHeight = pageHeight * (1 - margins[0] - margins[1]);
  // Captions may follow the plot. Bind its physical frame through the PDF's own Figure tag,
  // rather than treating the first axis label as the complete authored block.
  const frames = raster.some((page, i) => i < raster.length - 1 && page.gap > 0.25)
    ? figureFrames(JSON.parse(run('qpdf', ['--json', '--json-key=pages', '--json-key=qpdf', '--json-stream-data=none', pdf])), pageStreams(qdfText), pageHeight, fact.figures as Parameters<typeof openingOf>[1]['figures'])
    : [];
  const figuresOn = (page: number) => (fact.figures as Parameters<typeof openingOf>[1]['figures']).map(figure => {
    const matches = (frames[page - 1] ?? []).filter(other => other.title === figure.title);
    return matches.length === 1 ? matches[0] : figure;
  });
  const gapsOverQuarter = raster.flatMap((page, i) => {
    if (i >= raster.length - 1 || page.gap <= 0.25) return [];
    const next = openingUnit(pdf, i + 2, margins, fact.section_starts, figuresOn(i + 2));
    const accepted = gapAccepted({
      gap: page.gap, textHeight, next,
      readPage: (ahead: number) => (i + 2 + ahead > raster.length ? null : openingUnit(pdf, i + 2 + ahead, margins, fact.section_starts, figuresOn(i + 2 + ahead))),
    });
    return [{ page: i + 1, gap: page.gap, accepted, next }];
  });
  const result = {
    pages: Number(info.Pages), bytes: statSync(pdf).size, page_size: info['Page size'], tagged: info.Tagged === 'yes',
    outline_entries: JSON.parse(run('qpdf', ['--json', '--json-key=outlines', pdf])).outlines.length,
    link_annotations: links, report_web_links: fact.links,
    luminance_min: Math.min(...raster.map(page => page.luminance)), dark_pages: raster.flatMap((page, i) => (page.luminance < 0.5 ? [i + 1] : [])),
    empty_pages: raster.flatMap((page, i) => (page.empty ? [i + 1] : [])),
    gaps_over_quarter: gapsOverQuarter.filter(gap => !gap.accepted).map(({ page, gap }) => ({ page, gap })),
    gaps_accepted: gapsOverQuarter.filter(gap => gap.accepted).map(({ page, gap, next }) => ({ page, gap, next })),
    max_gap: Math.max(0, ...raster.slice(0, -1).map(page => page.gap)),
    headings_ending_pages: reading.flatMap((page, i) => (i < reading.length - 1 && fact.headings.some((heading: string) => norm(heading) && page.endsWith(norm(heading))) ? [i + 1] : [])),
    figures_with_sources: figures.length, stranded_figures: figures.filter(figure => !figure.source_on_page),
    action_label_pages: body.flatMap((page, i) => (page.includes('tablesvgcsv') ? [i + 1] : [])),
    table_rows: fact.tables.reduce((sum, table) => sum + table.length, 0), rows_missing: missingRows, ledger,
    words_run_together: runTogether,
    code_blocks: fact.code.length,
    code_missing: fact.code.filter((code: string) => !codeText(codeBodyText).includes(codeText(code))),
    fonts: fonts.map(font => font.name),
    unexpected_fonts: expected ? fonts.filter(font => !expected.includes(font.family)).map(font => font.name) : [],
    unembedded_fonts: fonts.filter(font => !font.embedded).map(font => font.name),
    unshipped_fonts: requireShippedFonts ? fonts.filter(font => !shippedPostscript.has(font.postscript)).map(font => font.name) : [],
    missing_math_font: requireShippedFonts && fact.math > 0 && !fonts.some(font => mathPostscript.has(font.postscript)),
  };
  const failures = [
    !/\(A4\)/.test(result.page_size) && `page size ${result.page_size}`, !result.tagged && 'not tagged', result.outline_entries < 1 && 'no outline',
    result.link_annotations < result.report_web_links && `${result.link_annotations} link annotations for ${result.report_web_links} web links`,
    result.dark_pages.length && `pages darker than 0.5: ${result.dark_pages}`, result.empty_pages.length && `empty pages: ${result.empty_pages}`,
    result.gaps_over_quarter.length && `bottom gaps over a quarter page: ${result.gaps_over_quarter.map(g => `${g.page} (${Math.round(g.gap * 100)}%)`).join(', ')}`,
    result.headings_ending_pages.length && `pages ending with a heading: ${result.headings_ending_pages}`,
    result.stranded_figures.length && `figures printed apart from their source notes: ${result.stranded_figures.map(f => f.title).join(' | ')}`,
    result.action_label_pages.length && `chart actions printed on pages ${result.action_label_pages}`,
    result.rows_missing.length && `${result.rows_missing.length} table rows missing`,
    result.code_missing.length && `${result.code_missing.length} code blocks are incomplete in PDF text`,
    ledger && ledger.printed_once !== ledger.rows && `ledger rows printed once: ${ledger.printed_once} of ${ledger.rows}`,
    result.words_run_together.length && `headings read without their spaces in the PDF text: ${result.words_run_together.slice(0, 3).join(' | ')}`,
    result.unexpected_fonts.length && `fonts outside the theme and embedded images: ${result.unexpected_fonts.join(', ')}`,
    result.unembedded_fonts.length && `fonts not embedded: ${result.unembedded_fonts.join(', ')}`,
    requireShippedFonts && !fonts.length && 'PDF has no embedded font inventory',
    result.unshipped_fonts.length && `PDF fonts absent from the shipped PostScript manifest: ${result.unshipped_fonts.join(', ')}`,
    result.missing_math_font && 'MathML has no bundled math face in the PDF font inventory',
  ].filter((failure): failure is string => Boolean(failure));
  return { result: { ...result, failures }, text };
}

// A theme's four font families as PDF font names spell them. The subset faces of Adobe's and
// IBM's families carry RichView names (decision D15). Every style prints and exports in its own
// faces, Sleek on Air's paper (STY-21 as assumed, 0967230a), so both the PDF export and the browser
// print are held to the theme's own families.
const internalNames: Record<string, string> = { 'Source Serif 4': 'RichViewSerif', 'Source Sans 3': 'RichViewSans', 'Source Code Pro': 'RichViewCode', 'IBM Plex Mono': 'RichViewMono' };
function themeFonts(theme: string) {
  const yaml = readFileSync(join(root, 'crates/richview-core/assets', `${theme}.rstyle.yaml`), 'utf8');
  return [...yaml.matchAll(/^\s+font-(?:text|display|ui|mono):\s*(.+)$/gm)].flatMap(([, value]) => {
    const family = value.trim().replace(/^'(.*)'$|^"(.*)"$/, '$1$2').split(',')[0].trim().replace(/^"|"$/g, '');
    return [family.replace(/\s+/g, ''), internalNames[family]].filter(Boolean);
  });
}

let browser: Browser | undefined;
type ExportSummary = {
  isolated_network: boolean;
  reports: Record<string, Record<string, unknown>>;
  failures: string[];
  reports_expected: string[];
  themes: string[];
  admission: string | null;
  shard: typeof selection.shard;
  completed: string[];
  results: ({ caseKey: string; kind: string; durationMs: number; ok: boolean; failures: string[]; report?: string } &
    Partial<{ typeResults: TypeResult[]; typeCohort: { index: number; type: string; title: string }[] }>)[];
  types?: { total: number; passed: number; file: string | null };
  mcp?: { arguments: object; result: unknown; letter_page_size: boolean };
  invalid_image?: { rejected: boolean; diagnostics: Diagnostic[]; previous_output_preserved: boolean };
  evidence?: { path: string; bytes: number };
};
const summary: ExportSummary = { isolated_network: isolated, reports: {}, failures: [], reports_expected: reportNames, themes, admission, shard: selection.shard, completed: [], results: [] };
async function closeBrowser() { if (browser) { await browser.close(); browser = undefined; } }
async function browserForChecks(): Promise<Browser> {
  if (browser) return browser;
  const launched: Browser = await launchBrowser(chromium, { executablePath: options.browser ?? chromiumPath(), headless: true, args: ['--no-sandbox'] }, { TimeoutError: errors.TimeoutError });
  browser = launched;
  return launched;
}
async function checkReport(name: string) {
  const folder = join(out, 'reports', name);
  prepareReport(name, folder);
  const rendered = join(folder, 'Rendered');
  const imageFontSources = imageFonts(name, folder);
  summary.reports[name] = {};
  for (const theme of themes) {
    const where = `${name}/${theme}`;
    const first = await generate(folder, theme, `${theme}.pdf`);
    const second = await generate(folder, theme, `${theme}-isolated.pdf`, isolated);
    await generate(folder, theme, `${theme}.html`);
    const fact = await facts(await browserForChecks(), join(rendered, `${theme}.html`));
    if (name === 'mixed-media') assert.ok(fact.code.some(code => codeText(code) === codeText(paperCode)), 'The long-code regression fixture must reach the rendered report');
    if (name === 'mixed-media') {
      assert.equal(fact.math, 2, 'Both inline and display MathML must reach the font regression');
      assert.equal(fact.footnote_backrefs, 1, 'The footnote return link must reach the font regression');
    }
    const pdf = join(rendered, `${theme}.pdf`);
    const fonts = [...themeFonts(theme), ...imageFontSources.flatMap(image => image.families), ...(fact.math ? mathFamilies : [])];
    const { result: pdfResult, text } = analyze(pdf, fact, { foot: runningTitle(fact.title), margins: [20 / 297, 22 / 297], fonts, rasterDir: join(workRoot, `${name}-${theme}`), requireShippedFonts: name === 'mixed-media' });
    const result = { ...pdfResult, image_font_sources: imageFontSources };
    const deterministic = {
      text: text === run('pdftotext', ['-raw', '-enc', 'UTF-8', join(rendered, `${theme}-isolated.pdf`), '-']),
      rasters: JSON.stringify(rasterHashes(pdf, join(workRoot, `${name}-${theme}-a`))) === JSON.stringify(rasterHashes(join(rendered, `${theme}-isolated.pdf`), join(workRoot, `${name}-${theme}-b`))),
    };
    const blocked = [first, second].flatMap(value => value.diagnostics.filter((d: { code: string; }) => d.code === 'export.network-blocked')).length;
    const fontDiagnostics = [first, second].flatMap(value => value.diagnostics.filter((d: { code: string; }) => d.code === 'export.font-fallback' || d.code === 'export.font-check'));
    const printedPdf = join(rendered, `${theme}-browser-print.pdf`);
    await browserPrint(await browserForChecks(), join(rendered, `${theme}.html`), printedPdf);
    const printed = analyze(printedPdf, fact, { foot: null, margins: [15 / 297, 15 / 297], fonts, rasterDir: join(workRoot, `${name}-${theme}-browser`), requireShippedFonts: name === 'mixed-media' }).result;
    mkdirSync(join(out, 'sheets'), { recursive: true });
    summary.reports[name][theme] = {
      export: { output: pdf, browser: first.browser, settled: first.settled, isolated: isolated && { settled: second.settled, sandbox: second.browser.sandbox }, network_blocked: blocked, diagnostics: first.diagnostics.filter((d: { code: string; }) => d.code.startsWith('export.')).map(d => `${d.level} ${d.code}: ${d.message}`) },
      pdf: result, deterministic,
      browser_print: { output: printedPdf, pages: printed.pages, luminance_min: printed.luminance_min, dark_pages: printed.dark_pages, stranded_figures: printed.stranded_figures, headings_ending_pages: printed.headings_ending_pages, action_label_pages: printed.action_label_pages, rows_missing: printed.rows_missing, code_blocks: printed.code_blocks, code_missing: printed.code_missing, ledger: printed.ledger, gaps_over_quarter: printed.gaps_over_quarter, fonts: printed.fonts, unembedded_fonts: printed.unembedded_fonts, unshipped_fonts: printed.unshipped_fonts, missing_math_font: printed.missing_math_font },
      sheets: { pdf: sheet(pdf, join(out, 'sheets', `${name}-${theme}.png`)), browser_print: sheet(printedPdf, join(out, 'sheets', `${name}-${theme}-browser-print.png`)) },
    };
    const failures = [
      ...result.failures,
      first.settled.charts > 0 && first.settled.report?.output !== 'pdf' && 'runtime must report PDF output',
      name === 'mixed-media' && (text.match(/\bTotal\b/g) ?? []).length !== 1 && 'mixed-media table total must print exactly once',
      name === 'mixed-media' && first.diagnostics.filter((d: { code: string; }) => d.code === 'export.video-poster').length !== 1 && 'missing video poster must be diagnosed',
      !deterministic.text && 'text differs between two exports', !deterministic.rasters && 'rasters differ between two exports',
      blocked && `${blocked} requests blocked`,
      name === 'mixed-media' && fontDiagnostics.length && `bundled-font source check: ${fontDiagnostics.map(d => `${d.code}: ${d.message}`).join('; ')}`,
      ...printed.failures.filter((f: string) => !/gaps|link annotations|outline|tagged/.test(f)).map(f => `browser print: ${f}`),
    ].filter(Boolean);
    summary.failures.push(...failures.map(f => `${where}: ${f}`));
    console.log(`${where}: ${result.pages} pages, ${result.bytes} bytes, max gap ${Math.round(result.max_gap * 100)}%${result.gaps_accepted.length ? ` (section gaps accepted: ${result.gaps_accepted.map(g => `${g.page} ${Math.round(g.gap * 100)}%`).join(', ')})` : ''}, browser print ${printed.pages} pages; ${failures.length ? failures.join('; ') : 'pass'}`);
  }
  if (name === 'openai-vs-anthropic') writeEvidence();
}
// Every chart type, in the default style, through the CLI's PDF only. The page rules above are
// left off the report: it is a catalogue of blocks with 57 charts, not a laid-out document, and
// its pages carry floated figures and an authored poster that those rules were not written for.
async function checkCatalogue() {
  const folder = join(out, 'reports', 'every-block');
  prepareReport('every-block', folder);
  const pdf = await generate(folder, 'sleek', 'sleek.pdf');
  await generate(folder, 'sleek', 'sleek.html');
  const charts = await chartFacts(await browserForChecks(), join(folder, 'Rendered/sleek.html'));
  const typeCohort = charts.map(({ type, title }, index: number) => ({ index, type, title }));
  const entries = typeChecks(join(folder, 'Rendered/sleek.pdf'), charts, join(workRoot, 'every-block-types'));
  const failures = [
    ...entries.map(entry => typeFailure('every-block/sleek', entry)),
    pdf.charts.length !== charts.length && `every-block/sleek: the PDF holds ${pdf.charts.length} charts, the report ${charts.length}`,
  ].filter((failure): failure is string => Boolean(failure));
  if (selection.shard.count === 1) writeFileSync(join(out, 'types.json'), JSON.stringify(entries, null, 2) + '\n');
  summary.types = { total: entries.length, passed: entries.filter(entry => entry.ok).length, file: selection.shard.count === 1 ? join(out, 'types.json') : null };
  summary.failures.push(...failures);
  console.log(`every-block/sleek: ${entries.length} charts, ${summary.types.passed} vector in the PDF; ${failures.length ? failures.join('; ') : 'pass'}`);
  return { entries, typeCohort };
}
async function checkMcp() {
  await closeBrowser(); await waitForBenchmark();
  const mcpFolder = join(out, 'mcp');
  mkdirSync(mcpFolder, { recursive: true });
  const mcpSource = join(mcpFolder, 'report.rmd');
  writeFileSync(mcpSource, '# Local PDF export\n\nA saved report printed through MCP.\n');
  const mcpArgs = { inputPath: mcpSource, output: 'pdf', outputName: 'letter.pdf', page: 'letter' };
  const mcp = spawnSync(cli, ['mcp'], { cwd: root, env, encoding: 'utf8', timeout: 60_000, input: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'tools/call', params: { name: 'richview_render', arguments: mcpArgs } }) + '\n' });
  forwardBrowserEvents(mcp);
  const reply: { isError?: boolean; structuredContent?: { output: string; settled: PrintSettled } } | undefined = JSON.parse(mcp.stdout.trim()).result;
  const content = reply?.structuredContent;
  const letter = content && /612 x 792 pts \(letter\)/.test(run('pdfinfo', [content.output]));
  summary.mcp = { arguments: mcpArgs, result: reply, letter_page_size: Boolean(letter) };
  if (mcp.status !== 0 || reply?.isError || !String(content?.output ?? '').endsWith('letter.pdf') || !letter) summary.failures.push('MCP PDF/Letter export failed');
  if (content?.settled?.charts !== 0 || content.settled.report !== null) summary.failures.push('Chartless PDF must not wait for the chart runtime');
}
async function checkInvalid() {
  await closeBrowser(); await waitForBenchmark();
  const invalidFolder = join(out, 'invalid-image');
  mkdirSync(join(invalidFolder, 'Rendered'), { recursive: true });
  const invalidSource = join(invalidFolder, 'report.rmd');
  const previousPdf = join(invalidFolder, 'Rendered/report.pdf');
  writeFileSync(invalidSource, '# Invalid image\n\n```image\nsrc: broken.png\nalt: Broken fixture\n```\n');
  writeFileSync(join(invalidFolder, 'broken.png'), 'invalid PNG fixture');
  writeFileSync(previousPdf, 'previous output');
  const rejected = spawnSync(cli, ['generate', invalidSource, '--to', 'pdf', '--format', 'json'], { env, encoding: 'utf8', timeout: 60_000 });
  forwardBrowserEvents(rejected);
  const error: { diagnostics: Diagnostic[] } = JSON.parse(rejected.stdout);
  summary.invalid_image = { rejected: rejected.status !== 0, diagnostics: error.diagnostics, previous_output_preserved: readFileSync(previousPdf, 'utf8') === 'previous output' };
  if (!summary.invalid_image.rejected || !error.diagnostics?.some((d: { code: string; }) => d.code === 'export.image-failed') || !summary.invalid_image.previous_output_preserved) summary.failures.push('Invalid image must fail without replacing the previous PDF');
}
function writeEvidence() {
  const flagship = join(out, 'reports/openai-vs-anthropic/Rendered/sleek.pdf');
  if (options.evidence && existsSync(flagship)) {
    const bytes = statSync(flagship).size;
    if (bytes < 3 * 1024 * 1024) {
      mkdirSync(resolve(options.evidence, '..'), { recursive: true });
      copyFileSync(flagship, resolve(options.evidence));
      summary.evidence = { path: options.evidence, bytes };
    } else summary.failures.push(`flagship PDF has ${bytes} bytes, over the 3 MB evidence limit`);
  }
}
try {
  for (const item of selection.cases) {
    const caseStarted = performance.now();
    const priorFailures = summary.failures.length;
    let catalogue;
    try {
      if (item.kind === 'report') await checkReport(item.report!);
      else if (item.kind === 'catalogue') catalogue = await checkCatalogue();
      else if (item.kind === 'mcp') await checkMcp();
      else if (item.kind === 'invalid-image') await checkInvalid();
      else throw new Error('Unknown export case: ' + item.kind);
    } finally { await closeBrowser(); }
    const failures = summary.failures.slice(priorFailures);
    summary.completed.push(item.key);
    summary.results.push({ caseKey: item.key, kind: item.kind, durationMs: Math.round(performance.now() - caseStarted), ok: failures.length === 0, failures, ...(item.report && { report: item.report }), ...(catalogue && { typeResults: catalogue.entries, typeCohort: catalogue.typeCohort }) });
  }
} finally { await closeBrowser(); }
rmSync(workRoot, { recursive: true, force: true });
const filename = selection.shard.count === 1 ? 'exports.json' : 'exports-' + selection.shard.index + '-of-' + selection.shard.count + '.json';
writeFileSync(join(out, filename), JSON.stringify(summary, null, 2) + '\n');
console.log(JSON.stringify({ evidence_path: join(out, filename), isolated_network: isolated, failures: summary.failures }, null, 2));
process.exit(summary.failures.length ? 1 : 0);
