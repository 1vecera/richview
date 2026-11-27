// The real research snapshot must survive native/browser render and offline export.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { cpSync, readFileSync, writeFileSync } from 'node:fs';
import { join, basename } from 'node:path';
import { pathToFileURL } from 'node:url';
import { harness, root, watchErrors, settledCharts, type Scope } from './browser-harness.ts';
import { chartTextAppearance } from './chart-appearance.ts';
import { captureFlagshipImages } from './flagship-images.ts';

const h = await harness('flagship');
const route = '/showcase/openai-vs-anthropic/';
const source = readFileSync(join(root, 'examples/openai-vs-anthropic/report.rmd'), 'utf8');
// The research pipeline's committed chart inventory: an independent list, so a chart dropped from the source fails.
const charts = JSON.parse(readFileSync(join(root, 'examples/openai-vs-anthropic/data/charts/index.json'), 'utf8')).length;
const digest = (value: string | object) => createHash('sha256').update(typeof value === 'string' ? value : JSON.stringify(value)).digest('hex');
type Snapshot = ReturnType<typeof snapshot>;
type Export = Snapshot['appearance'] & { name: string; output: string; content_sha256: string; loaded_fonts: string[]; chart_text: Awaited<ReturnType<typeof chartTextAppearance>> };
type Diagnostic = { level: string; [field: string]: unknown };
type Evidence = {
  source_sha256: string;
  checks: string[];
  exports: Export[];
  images: Awaited<ReturnType<typeof captureFlagshipImages>>[];
  errors: string[];
  network_errors: { url: string; status: number }[];
  offline_requests: string[];
  native_diagnostics?: Diagnostic[];
  browser_export?: string;
  status?: string;
  failure?: string;
};
const evidence: Evidence = { source_sha256: digest(source), checks: [], exports: [], images: [], errors: [], network_errors: [], offline_requests: [] };
const folder = join(h.dir, 'openai-vs-anthropic');
cpSync(join(root, 'examples/openai-vs-anthropic'), folder, { recursive: true, filter: path => !['research', 'Rendered'].includes(basename(path)) });
const cli = join(root, 'target/release/richview');
const native = <Result>(command: string, ...args: string[]) => {
  const result = spawnSync(cli, [command, join(folder, 'report.rmd'), '--format', 'json', ...args], { cwd: root, encoding: 'utf8', timeout: 60_000 });
  assert.equal(result.status, 0, result.stderr || result.stdout);
  return JSON.parse(result.stdout) as Result;
};
// Charts mount as they near the screen; the runtime's settled state mounts and paints them all.
const waitCharts = async (scope: Scope) => {
  // The editor replaces srcdoc after a worker reply. Retry only that navigation race;
  // a real runtime/readiness failure must still fail the check.
  for (let attempt = 0; ; attempt++) {
    try { await settledCharts(scope, charts); break; }
    catch (error) {
      if (attempt >= 3 || !/Execution context was destroyed|Frame was detached/i.test((error as Error).message)) throw error;
    }
  }
  assert.equal(await scope.locator('figure[data-rv-chart] canvas.rv-chart__canvas').count(), charts);
};
const snapshot = (body: Element) => {
  const payloads = [...body.querySelectorAll('script[data-rv-payload]')].map(node => JSON.parse(node.textContent));
  const models = window.__richview!.charts();
  // The drawn colour of a form's first series, read from the geometry model (sRGB floats).
  const drawn = (type: string) => {
    const index = payloads.findIndex(payload => payload.block.type === type);
    const [r, g, b] = models[index].geometry.series[0].color;
    return `rgb(${[r, g, b].map(value => Math.round(value * 255)).join(', ')})`;
  };
  const document = body.ownerDocument;
  return {
    text: [...body.querySelectorAll('h1,h2,h3,p,.rv-big__value,.rv-big__label,td,th')].filter(node => !node.closest('.rv-chart')).map(node => node.textContent.trim()),
    citations: [...body.querySelectorAll('.rv-report a[href]')].map(node => ({ text: node.textContent.trim(), href: node.getAttribute('href') })),
    charts: payloads.map(payload => ({ title: payload.block.title, data: { columns: payload.columns, rows: payload.rows }, encoding: payload.block.encoding, axes: payload.block.axes, annotations: payload.block.annotations })),
    // The page colour, read from the style: a page background (STY-74) leaves the body transparent.
    appearance: { background: getComputedStyle(document.documentElement).getPropertyValue('--rv-bg').trim(), titleFont: getComputedStyle(body.querySelector('h1')!).fontFamily, bar: drawn('bar'), temporal: drawn('line') },
    loaded_fonts: [...document.fonts].filter(font => font.status === 'loaded').map(font => font.family),
  };
};
const stable = (value: Snapshot) => ({ text: value.text, citations: value.citations, charts: value.charts });

// The showcase page: one pre-rendered report per style in a frame, compare, keys, citation cards, URL state.
// Removed with the redesigned page and therefore no longer checked: the tour, embed/share/inspector
// drawers and the live source editor (the page is a thin viewer now); editing is covered by the
// playground deep link below.
const themeIds = ['sleek', 'air', 'scientific', 'brutalist', 'hand-drawn'];
const frameSrc = (page: import('playwright').Page, selector: string) => page.locator(selector).getAttribute('src');
const urlParams = (page: import('playwright').Page) => new URL(page.url()).searchParams;
const frameReady = async (page: import('playwright').Page, selector: string, theme: string) => {
  await page.waitForFunction(([selector, theme]) => {
    const frame = document.querySelector(selector);
    return frame?.getAttribute('src') === `Rendered/${theme}.html` && (frame as HTMLIFrameElement).contentDocument?.readyState === 'complete'
      && (frame as HTMLIFrameElement).contentDocument!.querySelector('.rv-report a[data-source-id]');
  }, [selector, theme]);
};
async function verifyShowcase(page: import('playwright').Page, baseline: Snapshot, exports: Export[]) {
  const view = { width: 1440, height: 1000 };
  // Pills: all five, first checked, and each click swaps the frame and the address.
  assert.deepEqual(await page.locator('#style-pills [data-theme]').evaluateAll(nodes => nodes.map(node => node.dataset.theme)), themeIds);
  assert.equal(await page.locator('#theme option').count() - 1, 20, 'twenty extended themes');
  await frameReady(page, '#report-frame', 'sleek');
  assert.equal(await page.locator('#style-pills [data-theme="sleek"]').getAttribute('aria-checked'), 'true');
  assert.equal(urlParams(page).get('theme'), null, 'the default style writes no address');
  for (const theme of ['air', 'scientific', 'brutalist', 'hand-drawn', 'sleek']) {
    await page.locator(`#style-pills [data-theme="${theme}"]`).click();
    await frameReady(page, '#report-frame', theme);
    assert.equal(await page.locator(`#style-pills [data-theme="${theme}"]`).getAttribute('aria-checked'), 'true');
    assert.equal(urlParams(page).get('theme'), theme === 'sleek' ? null : theme);
    assert.equal(await page.locator('#report-frame').count(), 1, 'the old frame is removed after the swap');
    // The frame is the flagship's own render: same words, data and citations as the baseline, and the
    // same appearance as the native export of that style.
    const frame = page.frameLocator('#report-frame');
    await waitCharts(frame);
    const value = await frame.locator('body').evaluate(snapshot);
    assert.deepEqual(stable(value), stable(baseline!), `${theme}: the pill's render carries the baseline content`);
    const exported = exports.find((entry: { name: string; }) => entry.name === theme);
    if (exported) assert.deepEqual(value.appearance, { background: exported.background, titleFont: exported.titleFont, bar: exported.bar, temporal: exported.temporal }, `${theme}: the pill's render looks like its export`);
  }
  // An extended theme from the select.
  await page.locator('#theme').selectOption('v05-editorial');
  await frameReady(page, '#report-frame', 'v05-editorial');
  assert.equal(urlParams(page).get('theme'), 'v05-editorial');
  await page.locator('#style-pills [data-theme="sleek"]').click();
  await frameReady(page, '#report-frame', 'sleek');
  // Compare: the button opens a second frame and writes ?compare=.
  assert.equal(await page.locator('#compare-frame').count(), 0);
  await page.locator('#compare-toggle').click();
  await frameReady(page, '#compare-frame', 'air');
  assert.equal(await page.locator('#compare-toggle').getAttribute('aria-pressed'), 'true');
  assert.equal(await page.locator('iframe:visible').count(), 2);
  assert.equal(urlParams(page).get('compare'), 'air');
  await page.locator('#compare-theme').selectOption('brutalist');
  await frameReady(page, '#compare-frame', 'brutalist');
  assert.equal(urlParams(page).get('compare'), 'brutalist');
  // Keyboard: c closes, c opens, arrows cycle the primary style.
  await page.locator('body').press('c');
  await page.locator('#compare-frame').waitFor({ state: 'detached' });
  assert.equal(urlParams(page).get('compare'), null);
  await page.locator('body').press('c');
  await frameReady(page, '#compare-frame', 'air');
  await page.locator('body').press('c');
  await page.locator('#compare-frame').waitFor({ state: 'detached' });
  await page.locator('body').press('ArrowRight');
  await frameReady(page, '#report-frame', 'air');
  assert.equal(urlParams(page).get('theme'), 'air');
  await page.locator('body').press('ArrowLeft');
  await frameReady(page, '#report-frame', 'sleek');
  await page.locator('body').press('ArrowLeft');
  await frameReady(page, '#report-frame', 'hand-drawn');
  await page.locator('#style-pills [data-theme="sleek"]').click();
  await frameReady(page, '#report-frame', 'sleek');
  // Citation card: scroll a source link into view, wait, then hover/focus it; the card names its source.
  const ledger = await (await fetch(page.url().split('?')[0] + 'ledger/sources.csv')).text();
  const titles = [...ledger.matchAll(/^([A-Z][A-Z0-9-]*),[^,]*,(?:"((?:[^"]|"")*)"|([^,]*)),/gm)].map(match => [match[1], (match[2] ?? match[3]).replaceAll('""', '"')]);
  assert.ok(titles.length > 20, 'ledger parsed');
  const citations = page.frameLocator('#report-frame').locator('.rv-report a[data-source-id]');
  assert.ok(await citations.count() > 20, 'report source links are marked');
  for (const kind of ['hover', 'focus']) {
    const link = citations.nth(kind === 'hover' ? 3 : 7);
    const id = await link.getAttribute('data-source-id');
    await link.scrollIntoViewIfNeeded();
    await page.waitForTimeout(300);
    await link.evaluate((node, kind: string) => node.dispatchEvent(kind === 'hover' ? new PointerEvent('pointerover', { bubbles: true }) : new FocusEvent('focusin', { bubbles: true })), kind);
    await page.locator('#citation-card:not([hidden])').waitFor();
    const card = await page.locator('#citation-card').innerText();
    const title = titles.find(([key]) => key === id)?.[1];
    assert.ok(title, `ledger has ${id}`);
    assert.ok(card.includes(id!) && card.includes(title), `${kind}: card for ${id} carries "${title}" (got ${JSON.stringify(card.slice(0, 160))})`);
    const box = await page.locator('#citation-card').boundingBox();
    assert.ok(box!.x >= 0 && box!.x + box!.width <= view.width && box!.y >= 0 && box!.y + box!.height <= view.height, `${kind}: card stays on screen`);
    await page.keyboard.press('Escape');
    await page.locator('#citation-card[hidden]').waitFor({ state: 'attached' });
  }
  // Every source link opens the entry its own sentence cites, never another entry at the same URL:
  // build_report.py lists each link in reading order with its ids (content/citations.json).
  const listed = JSON.parse(readFileSync(join(root, 'examples/openai-vs-anthropic/content/citations.json'), 'utf8')) as { url: string; text: string; ids: string[] }[];
  const cited = new Set(listed.map(link => link.url));
  const shown = (await page.frameLocator('#report-frame').locator('.rv-report a[href]').evaluateAll(nodes => nodes.map(node => ({
    href: node.getAttribute('href'), id: node.getAttribute('data-source-id'), ids: node.getAttribute('data-source-ids'),
    cell: !!node.closest('td.link'), text: node.textContent.trim(),
  })))).filter(link => cited.has(link.href!));
  assert.equal(shown.length, listed.length, `the frame shows ${shown.length} links to cited sources; citations.json lists ${listed.length}`);
  listed.forEach((link, i) => {
    const at = `source link ${i + 1} ("${shown[i].text}", ${link.url})`;
    assert.equal(shown[i].href, link.url, `${at} is in reading order`);
    assert.equal(shown[i].cell, link.text === '', `${at} is ${link.text === '' ? 'a table cell' : 'in the text'}`);
    assert.equal(shown[i].id, link.ids[0], `${at} opens ${link.ids[0]}, the entry its sentence cites`);
    assert.equal(shown[i].ids ?? shown[i].id, link.ids.join(' '), `${at} keeps every id it cites`);
  });
  // And each card in the text shows that entry.
  const sourceLinks = page.frameLocator('#report-frame').locator('.rv-report a[data-source-id]');
  for (let i = 0; i < await sourceLinks.count(); i++) {
    const link = sourceLinks.nth(i);
    if (await link.evaluate(node => !!node.closest('td.link'))) continue;
    const id = await link.getAttribute('data-source-id');
    await link.evaluate(node => node.dispatchEvent(new FocusEvent('focusin', { bubbles: true })));
    await page.locator('#citation-card:not([hidden])').waitFor();
    const card = await page.locator('#citation-card').innerText();
    const title = titles.find(([key]) => key === id)?.[1];
    assert.ok(title && card.includes(id!) && card.includes(title), `the card of ${id} shows its own entry "${title}" (got ${JSON.stringify(card.slice(0, 120))})`);
    await page.keyboard.press('Escape');
    await page.locator('#citation-card[hidden]').waitFor({ state: 'attached' });
  }
  evidence.checks.push(`Citations: all ${listed.length} source links open the ledger entry their sentence cites, in reading order; every card in the text shows that entry`);
  // Reloading a shared address restores both frames, and the address survives the reload.
  await page.goto(h.base + route + '?theme=air&compare=brutalist');
  await frameReady(page, '#report-frame', 'air');
  await frameReady(page, '#compare-frame', 'brutalist');
  assert.equal(await page.locator('#style-pills [data-theme="air"]').getAttribute('aria-checked'), 'true');
  assert.equal(await page.locator('#compare-theme').inputValue(), 'brutalist');
  assert.equal(await page.locator('#compare-toggle').getAttribute('aria-pressed'), 'true');
  assert.equal(urlParams(page).get('theme'), 'air');
  assert.equal(urlParams(page).get('compare'), 'brutalist');
  // Each frame mounts its own charts.
  await waitCharts(page.frameLocator('#report-frame'));
  await waitCharts(page.frameLocator('#compare-frame'));
  evidence.checks.push(`Showcase: pills swap the frame and ?theme=; compare opens a second frame with ?compare=; c and arrows work; citation card carries the ledger source title on hover and focus; ?theme=air&compare=brutalist restores both; ${charts} charts mount in each frame`);
}
// Collected so one low-contrast label does not hide the rest of the suite; asserted empty at the end.
const contrastFailures = [];
let context, staticContext;
try {
  const check = native<{ charts: unknown[]; diagnostics: Diagnostic[] }>('check');
  assert.equal(check.charts.length, charts);
  assert.deepEqual(check.diagnostics.filter((diagnostic: { level: string; }) => diagnostic.level === 'error'), []);
  evidence.native_diagnostics = check.diagnostics.filter((diagnostic: { level: string; }) => diagnostic.level === 'warning');
  const manifest = await (await fetch(h.base + route + 'report-folder.json')).json();
  assert.equal(manifest.source, 'report.rmd');
  assert.deepEqual(manifest.files.filter((file: string) => file.endsWith('.rmd')), ['report.rmd']);
  assert.equal(await (await fetch(h.base + route + manifest.source)).text(), source);
  for (const file of ['ledger/sources.csv', 'ledger/collection-runs.csv', 'ledger/image-sources.csv', 'assets/CREDITS.md', 'data/charts/index.json']) {
    const response = await fetch(h.base + route + file);
    assert.equal(response.status, 200, file);
    assert.equal(digest(await response.text()), digest(readFileSync(join(folder, file), 'utf8')));
  }
  evidence.checks.push(`Native check accepts the real ${charts}-chart source; public dedicated folder, source and dated evidence files retain exact bytes`);
  context = await h.browser.newContext({ viewport: { width: 1440, height: 1000 }, reducedMotion: 'reduce', acceptDownloads: true });
  staticContext = await h.browser.newContext({ viewport: { width: 1440, height: 1000 }, reducedMotion: 'reduce', javaScriptEnabled: false });
  const staticPage = await staticContext.newPage();watchErrors(staticPage, evidence.errors, 'native static: ');
  await staticPage.route('**/*', route => {
    if (/^(file|data|blob):/.test(route.request().url())) return route.continue();
    evidence.offline_requests.push(route.request().url());return route.abort();
  });
  const offline = await context.newPage();watchErrors(offline, evidence.errors, 'offline: ');
  await offline.route('**/*', route => {
    if (/^(file|data|blob):/.test(route.request().url())) return route.continue();
    evidence.offline_requests.push(route.request().url());return route.abort();
  });
  let baseline: Snapshot | undefined;
  for (const name of ['scientific', 'sleek', 'brutalist']) {
    const result = native<{ output: string; ok: boolean; interactive: boolean }>('generate', '--style', name, '--output', name + '.html');
    assert.equal(result.ok, true, JSON.stringify(result));
    assert.equal(result.interactive, true);
    assert.equal(result.output, join(folder, 'Rendered', name + '.html'));
    await staticPage.goto(pathToFileURL(result.output).href);
    assert.equal(await staticPage.locator('figure[data-rv-chart]').count(), charts);
    // Without scripts every figure keeps its box and shows the notice; no static chart stands in.
    assert.deepEqual(await staticPage.locator('figure[data-rv-chart] .rv-chart__notice').allTextContents(), Array(charts).fill('This chart cannot be displayed here: it needs JavaScript and WebGL.'));
    assert.equal(await staticPage.locator('figure[data-rv-chart] :is(svg:not(.rv-chart__action svg, .rv-chart__legend svg), img, canvas)').count(), 0);
    await staticPage.locator('body').evaluate(async () => { await document.fonts.ready; });
    await offline.goto(pathToFileURL(result.output).href);await waitCharts(offline);
    evidence.images.push(await captureFlagshipImages(offline, h.dir, `offline-${name}-1440`));
    if (name === 'scientific') {
      await offline.setViewportSize({ width: 420, height: 900 });
      evidence.images.push(await captureFlagshipImages(offline, h.dir, 'offline-scientific-420'));
      await offline.setViewportSize({ width: 1440, height: 1000 }); await waitCharts(offline);
    }
    await offline.evaluate(() => scrollTo(0, 0));
    const value = await offline.locator('body').evaluate(snapshot);
    const chartText = await offline.locator('body').evaluate(chartTextAppearance);
    for (const [kind, value] of Object.entries(chartText)) {
      assert.ok(value.count > 0, `${name} ${kind} must be measured`);
      if (!(value.minimum_contrast >= 4.5)) contrastFailures.push(`${name} ${kind}: ${JSON.stringify(value.worst)}`);
    }
    assert.equal(value.charts.length, charts);assert.ok(value.loaded_fonts.length > 0);
    if (baseline) assert.deepEqual(stable(value), stable(baseline!));else baseline = value;
    await offline.screenshot({ path: join(h.dir, `native-${name}.png`) });
    // Review the real sparse and dense series at chart scale, with their actual report fonts.
    const curves = await offline.evaluate(() => window.__richview!.charts().map(chart => ({
      id: chart.id,
      title: document.getElementById(chart.id)!.querySelector('.rv-chart__title')?.textContent,
      commands: [...new Set(chart.geometry.panels.flatMap(panel => panel.items).filter((item): item is Extract<typeof item, { kind: 'line' }> => item.kind === 'line' && item.role === 'mark').flatMap(item => item.path.cmds.map(cmd => cmd.c)))],
    })).filter(chart => chart.commands.length));
    for (const [index, curve] of curves.entries()) await offline.locator(`#${curve.id}`).screenshot({ path: join(h.dir, `curves-${name}-${index + 1}.png`) });
    assert.ok(curves.some(chart => chart.commands.includes('c')), `${name}: flagship includes monotone cubic series`);
    writeFileSync(join(h.dir, `curves-${name}.json`), JSON.stringify(curves, null, 2) + '\n');
    evidence.exports.push({ name, output: result.output, content_sha256: digest(stable(value)), ...value.appearance, loaded_fonts: value.loaded_fonts, chart_text: chartText });
  }
  for (const key of ['background', 'titleFont', 'bar', 'temporal'] as const) assert.equal(new Set(evidence.exports.map(value => value[key])).size, 3, key);
  // Move only generated output. All font/runtime/local evidence dependencies must travel with it.
  const moved = join(h.dir, 'moved-Rendered');cpSync(join(folder, 'Rendered'), moved, { recursive: true });
  await offline.goto(pathToFileURL(join(moved, 'sleek.html')).href);await waitCharts(offline);
  evidence.images.push(await captureFlagshipImages(offline, h.dir, 'moved-offline-sleek'));
  assert.deepEqual(stable(await offline.locator('body').evaluate(snapshot)), stable(baseline!));
  await offline.locator('.rv-chart__frame').first().focus();await offline.keyboard.press('Home');
  await offline.locator('.rv-chart__readout:not([hidden])').first().waitFor();
  assert.ok((await offline.locator('.rv-chart__readout:not([hidden])').first().innerText()).length > 10);
  assert.deepEqual(evidence.offline_requests, []);
  evidence.checks.push('Native scientific/sleek/brutalist exports change actual typography, surfaces, bars and temporal strokes with identical words/data/units/annotations/citations; moved Rendered works offline with keyboard readout');
  const page = await context.newPage();watchErrors(page, evidence.errors);
  page.on('response', response => { if (response.status() >= 400) evidence.network_errors.push({ url: response.url(), status: response.status() }); });
  await page.goto(h.base + route);await page.locator('#theme option').nth(20).waitFor({ state: 'attached' });
  await verifyShowcase(page, baseline!, evidence.exports);
  // The compact showcase no longer has an Edit toolbar. The supported editor deep link
  // still loads the complete dedicated report folder and its paged source buffer.
  await page.goto(h.base + '/playground/?src=' + encodeURIComponent(route + 'report.rmd'));
  await page.waitForFunction(prefix => document.querySelector<HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement>('#source')?.value.startsWith(prefix), source.slice(0, 200));
  const preview = page.frameLocator('#preview');
  await preview.locator('figure[data-rv-chart]').nth(charts - 1).waitFor({ state: 'attached' });
  assert.equal(await preview.locator('figure[data-rv-chart]').count(), charts);
  const sourceEvent = page.waitForEvent('download');await page.locator('#download-source').click();
  const sourceExport = join(h.dir, 'downloaded-report.rmd');await (await sourceEvent).saveAs(sourceExport);
  assert.equal(readFileSync(sourceExport, 'utf8'), source, 'Paged editor downloads the complete original source');
  const visibleSource = await page.locator('#source').inputValue();
  await page.locator('#source').fill(visibleSource.replace('## The verdict', '## The verdict · edit check'));
  await preview.getByRole('heading', { name: 'The verdict · edit check', exact: true }).waitFor();
  await page.locator('#source').fill(visibleSource);
  await preview.getByRole('heading', { name: 'The verdict', exact: true }).waitFor();
  await page.locator('#playground-styles [data-theme="sleek"]').click();
  // The completed Sleek render, not just a changed select value: Scientific output also
  // names Space Grotesk in a font stack, and referenced media lengthen the render. Wait for
  // the actual title face and for the preview document to stop being replaced.
  const settledPreview = async () => {
    let last: string | null = '';
    for (let attempt = 0; attempt < 80; attempt++) {
      const srcdoc = await page.locator('#preview').getAttribute('srcdoc');
      const frame = await (await page.locator('#preview').elementHandle()).contentFrame();
      const face = await frame!.evaluate(() => getComputedStyle(document.querySelector('.rv-report__title')!).fontFamily).catch(() => '');
      if (face.includes('Space Grotesk') && srcdoc === last) return;
      last = srcdoc;
      await page.waitForTimeout(250);
    }
    assert.fail('The Sleek preview did not settle');
  };
  await settledPreview();
  await preview.locator('body').evaluate(async () => { await document.fonts.ready; await new Promise(accept => requestAnimationFrame(() => requestAnimationFrame(accept))); });
  // The preview starts its runtime like any report: its charts mount, and what it shows is the
  // flagship in Sleek, word for word and in the export's own appearance.
  await waitCharts(preview);
  assert.equal(await preview.locator('html').getAttribute('data-rv-runtime'), 'started');
  const authored = await preview.locator('body').evaluate(snapshot);
  assert.deepEqual(stable(authored), stable(baseline!), 'the authored preview carries the baseline content');
  const sleekExport = evidence.exports.find(value => value.name === 'sleek');
  assert.deepEqual(authored.appearance, { background: sleekExport!.background, titleFont: sleekExport!.titleFont, bar: sleekExport!.bar, temporal: sleekExport!.temporal }, 'the authored preview looks like the Sleek export');
  evidence.checks.push(`Playground preview starts its runtime, mounts ${charts} charts and equals the baseline content and the Sleek export appearance after the Sleek pill`);
  if (baseline!.citations.some(link => !/^(?:https?:|mailto:|#)/.test(link.href!))) {
    await page.locator('#download-html').click();
    await page.locator('#diagnostics').filter({ hasText: /This report links local files.*native Generate/ }).waitFor();
    evidence.checks.push('Wasm editor retains and edits the complete paged report; local-linked report correctly directs HTML export to the verified native bundle');
  } else {
    const event = page.waitForEvent('download');await page.locator('#download-html').click();
    const browserExport = join(folder, 'Rendered/browser-sleek.html');await (await event).saveAs(browserExport);
    await offline.goto(pathToFileURL(browserExport).href);await waitCharts(offline);
    const exported = await offline.locator('body').evaluate(snapshot);
    assert.deepEqual(stable(exported), stable(baseline!));const sleek = evidence.exports.find(value => value.name === 'sleek');
    assert.deepEqual(exported.appearance, { background: sleek!.background, titleFont: sleek!.titleFont, bar: sleek!.bar, temporal: sleek!.temporal });
    assert.deepEqual(evidence.offline_requests, []);
    evidence.browser_export = browserExport;
    evidence.checks.push('Editor deep link loads/edits/downloads the complete paged report; Wasm source→render→offline HTML export preserves source/data and the selected dark appearance');
  }
  assert.equal(readFileSync(join(folder, 'report.rmd'), 'utf8'), source);
  assert.deepEqual(evidence.errors, []);assert.deepEqual(evidence.network_errors, []);
  assert.deepEqual(contrastFailures, []);
  evidence.status = 'passed';
} catch (error) { evidence.status = 'failed';evidence.failure = (error as Error).stack;throw error; }
finally { const path = h.save('flagship.json', evidence);if (staticContext) await staticContext.close();if (context) await context.close();await h.close();console.log(JSON.stringify({ evidence: path, ...evidence }, null, 2)); }
