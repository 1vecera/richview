// Verify frontmatter `tabs` in generated interactive reports: rendered tab markup, deep links,
// roving-tabindex keyboard, charts in revealed panels, panel-visibility events for playback-aware
// charts, narrow-screen overflow, no-script and print fallbacks, and the `export: active` print
// selection. The core contracts cover the static (no-runtime) and PDF renderings.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync, cpSync } from 'node:fs';
import { join } from 'node:path';
import { chromium, firefox, webkit, errors } from 'playwright';
import type { Browser } from 'playwright';
import { chromiumPath, root, serveStatic } from './browser-harness.ts';
import { launchBrowser } from '../tools/rv-gate/lib/browser-launch.mts';

declare global {
  interface Window {
    __rvTabEvents: string[];
    __rvChartBox?: Record<string, string>;
  }
}


const cli = process.env.RICHVIEW_CLI || join(root, 'target/release/richview');
const parent = join(root, 'tmp/verification');
mkdirSync(parent, { recursive: true });
const dir = mkdtempSync(join(parent, 'tabs-'));

// Copy the example into a scratch folder so `richview generate` writes next to it.
const example = join(root, 'examples/sales-report-tabs');
const folder = join(dir, 'sales-report-tabs');
cpSync(example, folder, { recursive: true });

const generate = (name: string, style?: string) => {
  execFileSync(cli, ['generate', join(folder, 'report.rmd'), '--output', `${name}.html`, '--format', 'json', ...(style ? ['--style', style] : [])], { stdio: 'pipe' });
  return join(folder, 'Rendered', `${name}.html`);
};

const originalSource = readFileSync(join(folder, 'report.rmd'), 'utf8');
const linkedSource = originalSource
  .replace('## Overview {#overview}', '[Open the Sales heading](#sales)\n\n## Overview {#overview}\n\nOverview reference.[^overview-proof]')
  .replace('## Sales {#sales}', '## Sales {#sales}\n\nSales reference.[^sales-proof]')
  + '\n[^overview-proof]: Overview note fixture.\n[^sales-proof]: Sales note fixture.\n';
writeFileSync(join(folder, 'report.rmd'), linkedSource);
const defaultHtml = generate('default');
// Build an `export: active` variant by rewriting the source once.
assert.match(originalSource, /export: all/, 'example states its export choice');
writeFileSync(join(folder, 'report.rmd'), linkedSource.replace('export: all', 'export: active'));
const activeHtml = generate('active');
writeFileSync(join(folder, 'report.rmd'), linkedSource);
const styles = ['sleek', 'air', 'scientific', 'brutalist', 'hand-drawn'];
for (const style of styles) generate(`style-${style}`, style);
writeFileSync(join(folder, 'report.rmd'), originalSource);

const ids = ['overview', 'sales', 'customers'];
const defaultText = readFileSync(defaultHtml, 'utf8');
// Without a runtime every panel is plain stacked content under its label: no nav, no hidden
// panels, no style block (FEA-BR-09). The runtime builds the tab row.
assert.doesNotMatch(defaultText, /<nav class="rv-tabs"/, 'the static page carries no tab row');
for (const id of ids) assert.match(defaultText, new RegExp(`<section class="rv-tabpanel" id="panel-${id}" data-rv-tab="${id}" data-rv-tab-label="[^"]+"`), `${id} panel emitted`);
assert.match(defaultText, /id="panel-overview" data-rv-tab="overview" data-rv-tab-label="[^"]+" data-rv-default/, 'overview is the default panel');
assert.doesNotMatch(defaultText, /<section class="rv-tabpanel"[^>]* hidden/, 'no panel is hidden in the static page');
assert.doesNotMatch(defaultText, /<p class="rv-tabs__export-note"/, 'export: all emits no notice');

// The interactive report always carries every panel; `export: active` only changes paper output.
const activeText = readFileSync(activeHtml, 'utf8');
for (const id of ids) assert.match(activeText, new RegExp(`id="panel-${id}"`), `${id} panel kept in the interactive active-export report`);
assert.match(activeText, /data-rv-export="active"/, 'panels record the active export choice');
assert.match(activeText, /<p class="rv-tabs__export-note" role="note"><\/p>/, 'active export reserves its notice for the runtime to fill');

const server = await serveStatic({ '/': join(folder, 'Rendered') }, { port: Number(process.env.RICHVIEW_VERIFY_PORT ?? 0) });
const engine = process.env.RICHVIEW_VERIFY_BROWSER || 'chromium';
const engines = { chromium, firefox, webkit };
if (!Object.hasOwn(engines, engine)) throw new Error(`Unknown RICHVIEW_VERIFY_BROWSER '${engine}'; use chromium, firefox or webkit`);
const browser: Browser = await launchBrowser(engines[engine as keyof typeof engines], { headless: true, ...(engine === 'chromium' ? { executablePath: chromiumPath(), args: ['--no-sandbox'] } : {}) }, { TimeoutError: errors.TimeoutError });
const evidence: { engine: string; browser: string; checks: string[]; screenshots: string[] } = { engine, browser: browser.version(), checks: [], screenshots: [] };
const visible = (page: import('playwright').Page) => page.evaluate(() => Array.from(document.querySelectorAll('section.rv-tabpanel')).filter(s => getComputedStyle(s).display !== 'none').map(s => s.id));
const activeTab = (page: import('playwright').Page) => page.evaluate(() => document.querySelector<HTMLElement>('.rv-tabs__tab[aria-selected="true"]')?.dataset.rvTab);
const focusedTab = (page: import('playwright').Page) => page.evaluate(() => (document.activeElement as HTMLElement)?.dataset?.rvTab ?? null);
const shown = (page: import('playwright').Page, id: string) => page.waitForFunction((id: string) => document.querySelector<HTMLElement>(`#panel-${id}`)?.hidden === false, id);
try {
  const context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const page = await context.newPage();
  const errors: string[] = [];
  page.on('pageerror', (err: { message: string; }) => errors.push(err.message));
  await page.goto(`${server.base}/default.html`);
  await page.waitForSelector('html[data-rv-runtime="started"]');

  assert.deepEqual(await visible(page), ['panel-overview'], 'only the default panel is visible on first paint');
  for (const id of ['sales', 'customers']) assert.equal(await page.locator(`#panel-${id}`).getAttribute('hidden'), '', `${id} panel hidden but kept in the DOM`);
  assert.equal(await page.locator('#tab-overview').getAttribute('aria-selected'), 'true');
  const unmounted = await page.evaluate(() => Array.from(document.querySelectorAll('#panel-sales figure[data-rv-chart], #panel-customers figure[data-rv-chart]')).filter(f => f.querySelector('.rv-chart__frame')!.getBoundingClientRect().width === 0).length);
  assert.equal(unmounted, 2, 'charts in hidden panels have no box until their panel is shown');
  await page.screenshot({ path: join(dir, 'overview-1440.png'), fullPage: true });
  evidence.screenshots.push('overview-1440.png');
  evidence.checks.push('Overview is active on first paint; Sales and Customers stay in the DOM with the hidden attribute and their charts have no box yet');

  // A click switches panels, moves aria-selected and the roving tabindex, and records the deep link.
  const historyLength = await page.evaluate(() => history.length);
  await page.locator('#tab-sales').click();
  await shown(page, 'sales');
  assert.deepEqual(await visible(page), ['panel-sales']);
  assert.equal(await page.locator('#tab-sales').getAttribute('tabindex'), '0');
  assert.equal(await page.locator('#tab-overview').getAttribute('tabindex'), '-1');
  assert.equal(await page.locator('#tab-overview').getAttribute('aria-selected'), 'false');
  assert.equal(await page.evaluate(() => location.hash), '#tab=sales', 'a tab switch replaces the URL fragment');
  assert.equal(await page.evaluate(() => history.length), historyLength, 'tab switches replace history instead of pushing entries');
  evidence.checks.push('Click on Sales moves aria-selected and the roving tabindex and replaces the fragment with #tab=sales');

  // Charts mount and lay out when their panel is first revealed.
  // The chart fills its frame once the panel is revealed; wait for a size that holds.
  const chartBox = (id: string) => page.waitForFunction((id: string) => {
    const frame = document.querySelector(`#panel-${id} figure[data-rv-chart] .rv-chart__frame`);
    const box = frame?.getBoundingClientRect();
    if (!box || box.width < 200 || box.height < 100) return false;
    const model = window.__richview!.charts().find(c => c.id === frame!.closest('figure')!.id);
    if (!model?.settled || Math.abs(model.width - box.width) > 1.01) return false;
    const size = [Math.round(box.width), Math.round(box.height)];
    const previous = window.__rvChartBox?.[id];
    window.__rvChartBox = { ...window.__rvChartBox, [id]: size.join('x') };
    return previous === size.join('x') ? size : false;
  }, id, { polling: 600 }).then(handle => handle.jsonValue() as Promise<number[]>);
  const capture = async (name: string) => {
    await page.evaluate(() => window.__richview!.settled());
    await page.screenshot({ path: join(dir, name), fullPage: true });
    evidence.screenshots.push(name);
  };
  const salesChart = await chartBox('sales');
  await capture('sales-1440.png');
  evidence.checks.push(`The Sales bar chart lays out at ${salesChart.join('x')} px after its panel is revealed`);

  // Deep link via hashchange, and on initial load; unknown ids fall back to the authored default.
  await page.evaluate(() => { window.location.hash = '#tab=customers'; });
  await shown(page, 'customers');
  assert.equal(await activeTab(page), 'customers');
  const customersChart = await chartBox('customers');
  await capture('customers-1440.png');
  evidence.checks.push(`Fragment change to #tab=customers activates Customers; its pie lays out at ${customersChart.join('x')} px`);

  // A fragment-only goto would stay in the same document, so load a blank page in between.
  const load = async (url: string) => {
    await page.goto('about:blank');
    await page.goto(url);
    await page.waitForSelector('html[data-rv-runtime="started"]');
  };
  await load(`${server.base}/default.html#tab=sales`);
  await shown(page, 'sales');
  assert.deepEqual(await visible(page), ['panel-sales'], 'initial #tab=sales activates Sales');
  await page.evaluate(() => { window.location.hash = '#tab=missing'; });
  await page.waitForFunction(() => location.hash === '#tab=missing');
  assert.deepEqual(await visible(page), ['panel-sales'], 'a fragment naming no tab leaves the selection unchanged');
  await load(`${server.base}/default.html#tab=missing`);
  assert.deepEqual(await visible(page), ['panel-overview'], 'an unknown deep link on load keeps the default panel');
  evidence.checks.push('Loading #tab=sales activates Sales; a later #tab=missing leaves it selected; loading #tab=missing keeps the default Overview');

  // Ordinary anchors and footnotes reveal their hidden panel, focus their target and push
  // history. Tab buttons themselves continue to replace history (FEA-BR-40).
  await load(`${server.base}/default.html#tab=overview`);
  await page.evaluate(() => document.fonts.ready);
  const anchorHistory = await page.evaluate(() => history.length);
  const anchorScroll = await page.evaluate(() => scrollY);
  await page.locator('a[href="#sales"]').click();
  await shown(page, 'sales');
  assert.equal(await page.evaluate(() => document.activeElement!.id), 'sales', 'a hidden heading receives focus');
  assert.equal(await page.evaluate(() => location.hash), '#sales');
  assert.equal(await page.evaluate(() => history.length), anchorHistory + 1, 'following a hidden anchor adds history');
  await page.goBack();
  await shown(page, 'overview');
  await page.waitForFunction((y: number) => Math.abs(scrollY - y) < 2, anchorScroll);
  assert.equal(await page.evaluate(() => location.hash), '#tab=overview', 'Back restores the original panel fragment');
  assert.equal(await page.locator('#rv-fn-2').evaluate(note => note.closest<HTMLElement>('.rv-tabpanel')!.dataset.rvTab), 'sales', 'the Sales footnote lives in its own panel');
  await page.evaluate(() => { location.hash = '#rv-fn-2'; });
  await shown(page, 'sales');
  assert.equal(await page.evaluate(() => document.activeElement!.id), 'rv-fn-2', 'a hidden footnote receives focus');
  await page.goBack();
  await shown(page, 'overview');
  await load(`${server.base}/default.html#%73ales`);
  await shown(page, 'sales');
  assert.equal(await page.evaluate(() => document.activeElement!.id), 'sales', 'an encoded initial fragment reveals and focuses its heading');
  evidence.checks.push('Hidden heading links and footnote fragments reveal their panel and focus the target; links push history, Back restores the panel and scroll, and encoded initial fragments work');
  await load(`${server.base}/default.html`);

  // Keyboard: arrows wrap and move focus with the selection; Home and End jump to the ends.
  await page.locator('#tab-overview').focus();
  await page.keyboard.press('ArrowLeft');
  await shown(page, 'customers');
  assert.equal(await focusedTab(page), 'customers', 'ArrowLeft from the first tab wraps to the last');
  await page.keyboard.press('ArrowRight');
  await shown(page, 'overview');
  assert.equal(await focusedTab(page), 'overview', 'ArrowRight from the last tab wraps to the first');
  await page.keyboard.press('ArrowRight');
  await shown(page, 'sales');
  assert.equal(await focusedTab(page), 'sales');
  await page.keyboard.press('End');
  await shown(page, 'customers');
  await page.keyboard.press('Home');
  await shown(page, 'overview');
  assert.equal(await focusedTab(page), 'overview');
  assert.deepEqual(await visible(page), ['panel-overview']);
  // Tab leaves the tab list for the panel content instead of visiting every tab.
  await page.keyboard.press('Tab');
  assert.equal(await focusedTab(page), null, 'Tab moves focus out of the tab list');
  evidence.checks.push('ArrowLeft/ArrowRight wrap and move focus with the selection; Home/End jump to the ends; Tab leaves the tab list');

  // Panel-visibility events reach .rv-chart descendants, the contract with chart playback controls.
  await page.evaluate(() => {
    window.__rvTabEvents = [];
    for (const chart of document.querySelectorAll('.rv-chart')) {
      const panel = chart.closest('.rv-tabpanel')!.id;
      chart.addEventListener('rv:panel-hidden', () => window.__rvTabEvents.push(`${panel}:hidden`));
      chart.addEventListener('rv:panel-shown', () => window.__rvTabEvents.push(`${panel}:shown`));
    }
  });
  await page.locator('#tab-sales').click();
  await page.locator('#tab-customers').click();
  const events = await page.evaluate(() => window.__rvTabEvents);
  assert.ok(events.includes('panel-sales:shown'), 'Sales charts receive rv:panel-shown');
  assert.ok(events.includes('panel-sales:hidden'), 'Sales charts receive rv:panel-hidden');
  assert.ok(events.includes('panel-customers:shown'), 'Customers charts receive rv:panel-shown');
  evidence.checks.push(`Chart visibility events in order: ${events.join(', ')}`);

  // Narrow viewport: the tab list scrolls by itself and the page does not overflow sideways.
  await page.setViewportSize({ width: 375, height: 800 });
  await page.locator('#tab-sales').click();
  await chartBox('sales').catch(() => null);
  const narrow = await page.evaluate(() => {
    const nav = document.querySelector('.rv-tabs');
    return { overflowX: getComputedStyle(nav!).overflowX, page: document.documentElement.scrollWidth, viewport: innerWidth };
  });
  assert.equal(narrow.overflowX, 'auto', 'tab list scrolls horizontally on phones');
  assert.ok(narrow.page <= narrow.viewport, `no horizontal page overflow at 375 px (${narrow.page} > ${narrow.viewport})`);
  await capture('sales-375.png');
  evidence.checks.push(`At 375 px the tab list uses overflow-x:auto and the page stays within ${narrow.viewport} px`);

  // Print media reveals every panel of an `export: all` report.
  await page.emulateMedia({ media: 'print' });
  assert.deepEqual(await visible(page), ids.map(id => `panel-${id}`), 'every panel is visible under print media');
  assert.equal(await page.locator('.rv-tabs').evaluate((nav: Element) => getComputedStyle(nav).display), 'none', 'tab buttons are not printed');
  evidence.checks.push('Print media shows all three panels in order and hides the tab buttons');
  assert.deepEqual(errors, [], 'no page errors');
  await context.close();

  // Reports carry one immutable style (FEA-RP-33). Every hidden panel inherits it before
  // reveal, and its freshly shown chart must draw the same surface (FEA-BR-39).
  const styleContext = await browser.newContext({ viewport: { width: 1440, height: 900 }, reducedMotion: 'reduce' });
  const signatures = new Set();
  try {
    const styledPage = await styleContext.newPage();
    const styleErrors: string[] = [];
    styledPage.on('pageerror', error => styleErrors.push(error.message));
    for (const style of styles) {
      await styledPage.goto(`${server.base}/style-${style}.html`);
      await styledPage.waitForSelector('html[data-rv-runtime="started"]');
      await styledPage.evaluate(() => document.fonts.ready);
      const inheritance = await styledPage.evaluate(() => {
        const properties = ['--rv-bg-chart', '--rv-accent', '--rv-font-ui'];
        const read = (node: Element) => Object.fromEntries(properties.map(p => [p, getComputedStyle(node).getPropertyValue(p).trim()]));
        return { report: read(document.querySelector('.rv-report')!), panels: [...document.querySelectorAll('.rv-tabpanel')].map(read) };
      });
      for (const panel of inheritance.panels) assert.deepEqual(panel, inheritance.report, `${style}: hidden panels inherit the report style`);
      signatures.add(JSON.stringify(inheritance.report));
      for (const panel of ['sales', 'customers']) {
        await styledPage.locator(`#tab-${panel}`).click();
        await styledPage.evaluate(() => window.__richview!.settled());
        const paint = await styledPage.evaluate((id: string) => {
          const figure = document.querySelector(`#panel-${id} figure[data-rv-chart]`);
          const frame = figure!.querySelector('.rv-chart__frame');
          const model = window.__richview!.charts().find(c => c.figure === figure!.id);
          const canvas = document.createElement('canvas'); canvas.width = canvas.height = 1;
          const context = canvas.getContext('2d');
          context!.fillStyle = getComputedStyle(frame!).backgroundColor;
          context!.fillRect(0, 0, 1, 1);
          return { expected: [...context!.getImageData(0, 0, 1, 1).data].map(v => v / 255), actual: model!.geometry.background, width: model!.width };
        }, panel);
        assert.ok(paint.width >= 200, `${style}/${panel}: revealed chart has a measured width`);
        assert.ok(paint.actual.every((v: number, i: number) => Math.abs(v - paint.expected[i]) <= 3 / 255), `${style}/${panel}: geometry surface matches current stylesheet: ${JSON.stringify(paint)}`);
        const file = `tabs-${style}-${panel}.png`;
        await styledPage.screenshot({ path: join(dir, file), fullPage: true });
        evidence.screenshots.push(file);
      }
    }
    assert.ok(signatures.size > 1, 'the style variants must actually change the report style');
    assert.deepEqual(styleErrors, [], 'no page errors while revealing styled charts');
    evidence.checks.push('Every builtin style reaches hidden panels and the charts they reveal; chart surfaces match the report stylesheet before and after tab switches');
  } finally { await styleContext.close(); }

  // `export: active` prints only the selected panel and its notice; the screen shows every tab.
  const activeContext = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const activePage = await activeContext.newPage();
  await activePage.goto(`${server.base}/active.html#tab=sales`);
  await activePage.waitForSelector('html[data-rv-runtime="started"]');
  await shown(activePage, 'sales');
  const note = () => activePage.locator('.rv-tabs__export-note').evaluate((n: Element) => getComputedStyle(n).display);
  assert.equal(await note(), 'none', 'the paper notice is not shown on screen');
  await activePage.emulateMedia({ media: 'print' });
  // The runtime fills the notice when the reader prints; emulated media fires no print event.
  await activePage.evaluate(() => window.dispatchEvent(new Event('beforeprint')));
  assert.deepEqual(await visible(activePage), ['panel-sales'], 'export: active prints only the selected panel');
  assert.notEqual(await note(), 'none', 'export: active prints its notice');
  evidence.checks.push('export: active keeps all panels on screen and prints only the selected Sales panel with its notice');
  await activeContext.close();

  // Without scripting the panels stack in order and the inert tab buttons are hidden.
  const staticContext = await browser.newContext({ viewport: { width: 1440, height: 900 }, javaScriptEnabled: false });
  const staticPage = await staticContext.newPage();
  await staticPage.goto(`${server.base}/default.html`);
  assert.deepEqual(await visible(staticPage), ids.map(id => `panel-${id}`), 'no-script readers see every panel');
  assert.equal(await staticPage.locator('.rv-tabs').count(), 0, 'no-script readers get no tab row: the runtime builds it');
  evidence.checks.push('With JavaScript disabled every panel is visible in order and the page has no tab row');
  await staticContext.close();
} finally {
  await browser.close();
  await server.close();
}

writeFileSync(join(dir, 'evidence.json'), JSON.stringify(evidence, null, 2));
console.log(JSON.stringify({ ...evidence, dir }, null, 2));
