// Verify Rich Slides (FEA-RP-71) in a generated deck: each slide of the `.rslide` source is a
// screen; on a landscape window a slide is a 16:9 stage cut into 24 columns and 24 rows, its head
// in rows 1 to 4, its foot in rows 23 and 24 and its slots in the tracks of their layout, the same
// composition at every size, with charts laid out for the room their slot gives; on a phone each
// slide is a card; keys, the counter and the address move through them; without scripts the
// sequence still reads and snaps; and paper is one slide a page. Each builtin style sets the deck
// on its own stage and every stage keeps those fits; a slide below the first screen builds as it
// arrives and keeps nothing of the build, and reduced motion keeps it at rest.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { cpSync, mkdirSync, mkdtempSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { chromium, firefox, webkit, errors } from 'playwright';
import type { Browser } from 'playwright';
import type { ChartModel } from './report-model.ts';
import { chromiumPath, root, serveStatic, waitForBenchmark } from './browser-harness.ts';
import { launchBrowser } from '../tools/rv-gate/lib/browser-launch.mts';

const cli = process.env.RICHVIEW_CLI || join(root, 'target/release/richview');
const parent = join(root, 'tmp/verification');
mkdirSync(parent, { recursive: true });
const dir = mkdtempSync(join(parent, 'slides-'));
const folder = join(dir, 'warmest-years');
cpSync(join(root, 'docs/public/gallery/warmest-years'), folder, { recursive: true });

// The example is clean: no run carries a finding of the deck.
const result = JSON.parse(execFileSync(cli, ['generate', join(folder, 'deck.rslide'), '--format', 'json'], { stdio: 'pipe' }).toString()) as { diagnostics: { level: string; code: string }[] };
const codes = result.diagnostics.map(d => `${d.level} ${d.code}`);
assert.ok(!codes.some((code: string|string[]) => code.includes(' slides.')), `generate says nothing about a clean deck: ${codes}`);
assert.deepEqual(codes.filter((code: string) => !code.startsWith('info ')), [], 'the gallery example has no warning');
const html = readFileSync(join(folder, 'Rendered/deck.html'), 'utf8');
const SLIDES = 10;
assert.equal((html.match(/<section class="rv-slide[ "]/g) ?? []).length, SLIDES, 'each slide of the source is a slide');
assert.deepEqual([...html.matchAll(/<section class="rv-slide rv-slide--(\w+)/g)].map(m => m[1]), ['cover', 'hero', 'figures', 'figures', 'figures', 'section', 'figures', 'numbers', 'text', 'text'], 'a slide is a cover, a hero, numbers, figures, a section or text by what it holds');
assert.match(html, /<html[^>]* data-rv-slides/, 'the document is marked as slides');
assert.doesNotMatch(html, /class="rv-toc/, 'slides list no contents');
// Each builtin style sets the deck on its own stage.
const STAGES = { deck: 'keynote', air: 'gallery', brutalist: 'poster', 'hand-drawn': 'sketchbook', scientific: 'paper' };
for (const style of Object.keys(STAGES).filter(name => name !== 'deck')) {
  execFileSync(cli, ['generate', join(folder, 'deck.rslide'), '--style', style, '--output', `${style}.html`], { stdio: 'pipe' });
}
for (const [file, stage] of Object.entries(STAGES)) {
  assert.match(readFileSync(join(folder, `Rendered/${file}.html`), 'utf8'), new RegExp(`<html[^>]* data-rv-stage="${stage}"`), `${file} stands on the ${stage} stage`);
}

const engines = { chromium, firefox, webkit };
const engine = process.env.RICHVIEW_VERIFY_BROWSER || 'chromium';
await waitForBenchmark();
const server = await serveStatic({ '/': join(folder, 'Rendered') }, { port: Number(process.env.RICHVIEW_VERIFY_PORT ?? 0) });
const browser: Browser = await launchBrowser(engines[engine as keyof typeof engines], {
  headless: true,
  ...(engine === 'chromium' ? { executablePath: chromiumPath(), args: ['--no-sandbox', ...(process.platform === 'darwin' && process.arch === 'arm64' ? ['--enable-gpu'] : [])] } : {}),
}, { TimeoutError: errors.TimeoutError });
const url = `${server.base}/deck.html`;
const problems: string[] = [];
const watch = (page: import('playwright').Page, label: string) => {
  page.on('pageerror', error => problems.push(`${label}: ${error.message}`));
  page.on('console', message => { if (message.type() === 'error') problems.push(`${label}: console ${message.text()}`); });
};
// The runtime starts after load; wait for it, as the harness's settledCharts does, then for its charts.
const settle = (page: import('playwright').Page) => page.evaluate(async () => {
  const end = performance.now() + 45_000;
  while ((document.documentElement.dataset.rvRuntime !== 'started' || !window.__richview) && performance.now() < end) await new Promise(accept => setTimeout(accept, 50));
  if (!window.__richview) throw new Error('The report runtime did not start');
  await window.__richview.settled();
  await new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done)));
  // A slide's build has ended before anything is measured.
  const builds = document.getAnimations().filter(a => a.effect?.getTiming().iterations !== Infinity);
  await Promise.race([Promise.all(builds.map(a => a.finished.catch(() => {}))), new Promise(done => setTimeout(done, 5_000))]);
});
// Bring a slide to the top without the keys, then let its charts mount and fit.
const show = async (page: import('playwright').Page, n: number) => {
  await page.locator(`.rv-slide[data-rv-slide="${n}"]`).evaluate(slide => slide.scrollIntoView({ behavior: 'instant', block: 'start' }));
  await settle(page);
  await page.waitForTimeout(150);
  await settle(page);
};
// Every box the composition is made of, as shares of the stage, so two window sizes compare.
const measure = (page: import('playwright').Page, n: number) => page.locator(`.rv-slide[data-rv-slide="${n}"]`).evaluate(slide => {
  const stage = slide.querySelector('.rv-slide__body')!.getBoundingClientRect();
  const share = (element: Element) => {
    const r = element.getBoundingClientRect();
    return [(r.left - stage.left) / stage.width, (r.top - stage.top) / stage.width, r.width / stage.width, r.height / stage.width].map(v => Math.round(v * 1000) / 1000);
  };
  const outside = [...slide.querySelectorAll('.rv-slide__body *')].filter(node => {
    // A wide table scrolls inside its own box, as in a report; what it holds may lie outside.
    if (node.closest('.rv-chart__readout, .rv-chart__actions, .rv-table-scroll, .rv-table, [hidden], details:not([open]) > :not(summary)')) return false;
    const r = node.getBoundingClientRect();
    return r.width > 0 && r.height > 0 && (r.right > stage.right + 1 || r.left < stage.left - 1 || r.bottom > stage.bottom + 1 || r.top < stage.top - 1);
  }).map(node => `${node.tagName.toLowerCase()}.${node.className}`);
  return {
    slide: slide.getBoundingClientRect().height,
    stage: [stage.width, stage.height],
    text: parseFloat(getComputedStyle(slide.querySelector('.rv-slide__body')!).fontSize) / stage.width,
    heading: slide.querySelector('.rv-slide__body > .rv-slide__head > .rv-slide__title') ? share(slide.querySelector('.rv-slide__body > .rv-slide__head > .rv-slide__title')!) : null,
    // Each slot's box as shares of the stage, with the stage's row, so a slot is placed on the grid.
    slots: [...slide.querySelectorAll('.rv-slot')].map(slot => ({ name: (slot as HTMLElement).dataset.rvSlot, box: share(slot) })),
    row: (stage.height - parseFloat(getComputedStyle(slide.querySelector('.rv-slide__body')!).paddingTop) - parseFloat(getComputedStyle(slide.querySelector('.rv-slide__body')!).paddingBottom)) / stage.width / 24,
    padTop: parseFloat(getComputedStyle(slide.querySelector('.rv-slide__body')!).paddingTop) / stage.width,
    padSide: parseFloat(getComputedStyle(slide.querySelector('.rv-slide__body')!).paddingLeft) / stage.width,
    head: !!slide.querySelector('.rv-slide__head'),
    foot: !!slide.querySelector('.rv-slide__foot'),
    numbers: [...slide.querySelectorAll('.rv-slide__number')].map(share),
    // A number's figures stay inside its column (a stage's marks, such as a pencil underline,
    // may run past the figures themselves).
    values: [...slide.querySelectorAll('.rv-metric__value')].map(value => {
      const range = document.createRange();
      range.selectNodeContents(value);
      const text = range.getBoundingClientRect();
      const column = value.closest('.rv-slide__number, .rv-metric')!.getBoundingClientRect();
      return { size: parseFloat(getComputedStyle(value).fontSize) / stage.width, fits: text.left >= column.left - 1 && text.right <= column.right + 1 };
    }),
    charts: [...slide.querySelectorAll('figure[data-rv-chart]')].map(figure => {
      const frame = figure.querySelector('.rv-chart__frame')!.getBoundingClientRect();
      const slot = figure.closest('.rv-slide__figure')!.getBoundingClientRect();
      // Its title, legend, plot and source note all stay in the slot the slide gave it.
      const content = figure.closest('.rv-slide__figure')!.lastElementChild!.getBoundingClientRect();
      return { frame: share(figure.querySelector('.rv-chart__frame')!), slot: share(figure.closest('.rv-slide__figure')!), inside: frame.bottom <= slot.bottom + 1 && frame.right <= slot.right + 1 && content.bottom <= slot.bottom + 1, fills: frame.height / slot.height };
    }),
    outside,
    pageWidth: document.documentElement.scrollWidth,
  };
});
const state = (page: import('playwright').Page) => page.evaluate(() => ({
  hash: location.hash,
  count: document.querySelector('.rv-slides__count')?.textContent ?? null,
  top: [...document.querySelectorAll('.rv-slide')].findIndex(slide => Math.abs(slide.getBoundingClientRect().top) < 2) + 1,
}));
// The keys glide; wait until the slide the key asked for rests at the top.
const arrive = async (page: import('playwright').Page, n: number) => {
  await page.waitForFunction(n => {
    const slide = document.querySelector(`.rv-slide[data-rv-slide="${n}"]`);
    return Math.abs(slide!.getBoundingClientRect().top) < 2 && document.querySelector('.rv-slides__count')?.textContent === `${n} / ${document.querySelectorAll('.rv-slide').length}`;
  }, n, { timeout: 10_000 });
};

// A stage: 16:9, as large as the window allows, the same composition at every size, in every
// style. The default style is also checked for what its charts draw.
const stageChecks = async (file: string) => {
  const stages: Record<number, Awaited<ReturnType<typeof measure>>[]> = {};
  const url = `${server.base}/${file}.html`;
  for (const [width, height] of [[1440, 900], [1920, 1080], [1280, 720]] as const) {
    const context = await browser.newContext({ viewport: { width, height }, reducedMotion: file === 'deck' ? 'no-preference' : 'reduce' });
    const page = await context.newPage();
    watch(page, `${file} ${width}`);
    await page.goto(url);
    await settle(page);
    stages[width] = [];
    for (let n = 1; n <= SLIDES; n++) {
      await show(page, n);
      const m = await measure(page, n);
      stages[width].push(m);
      const stageWidth = Math.min(width, height * 16 / 9);
      assert.ok(Math.abs(m.stage[0] - stageWidth) <= 1 && Math.abs(m.stage[1] - stageWidth * 9 / 16) <= 1, `${file} ${width}: slide ${n} is a 16:9 stage, got ${m.stage}`);
      assert.ok(Math.abs(m.slide - height) <= 1, `${file} ${width}: slide ${n} is one screen high, got ${m.slide}`);
      assert.deepEqual(m.outside, [], `${file} ${width}: slide ${n} keeps everything on its stage`);
      assert.ok(m.pageWidth <= width, `${file} ${width}: no sideways scroll on slide ${n}`);
      assert.ok(Math.abs(m.text - 0.015) < 0.0005, `${file} ${width}: text is 1.5% of the stage's width on slide ${n}, got ${m.text}`);
      for (const value of m.values) assert.ok(value.fits, `${file} ${width}: a big number stays in its column on slide ${n}`);
      // A table is sized to its slot: on a stage it never scrolls sideways inside it.
      const scrolled = await page.locator(`.rv-slide[data-rv-slide="${n}"] .rv-slide__figure--table .rv-table-scroll`).evaluateAll((all: { scrollWidth: number; clientWidth: number; }[]) => all.map((box: { scrollWidth: number; clientWidth: number; }) => box.scrollWidth - box.clientWidth));
      for (const over of scrolled) assert.ok(over <= 1, `${file} ${width}: the table on slide ${n} fits its slot, ${over}px over`);
      for (const chart of m.charts) {
        assert.ok(chart.inside, `${file} ${width}: a chart stays in its slot on slide ${n}`);
        assert.ok(chart.fills > 0.6, `${file} ${width}: a chart takes the room its slot leaves on slide ${n}, got ${chart.fills}`);
      }
      // The grid: a head takes rows 1 to 4 and a foot rows 23 and 24, and the slots share the rest.
      const first = m.padTop + (m.head ? 4 : 0) * m.row;
      const last = m.padTop + (m.foot ? 22 : 24) * m.row;
      if (m.slots.length) {
        const top = Math.min(...m.slots.map(slot => slot.box[1]));
        const bottom = Math.max(...m.slots.map(slot => slot.box[1] + slot.box[3]));
        const left = Math.min(...m.slots.map(slot => slot.box[0]));
        const right = Math.max(...m.slots.map(slot => slot.box[0] + slot.box[2]));
        assert.ok(Math.abs(top - first) <= 0.004 && Math.abs(bottom - last) <= 0.004, `${file} ${width}: the slots of slide ${n} stand in rows ${m.head ? 5 : 1} to ${m.foot ? 22 : 24}: ${top} to ${bottom}, wanted ${first} to ${last}`);
        assert.ok(Math.abs(left - m.padSide) <= 0.004 && Math.abs(right - (1 - m.padSide)) <= 0.004, `${file} ${width}: the slots of slide ${n} fill the stage between its margins: ${left} to ${right}`);
      }
    }
    const at = (n: number) => stages[width][n - 1];
    const slot = (n: number, name: string) => at(n).slots.find(item => item.name === name)!.box;
    // The hero is the largest number the format sets; numbers beside one another are a band.
    assert.deepEqual(at(2).slots.map(item => item.name), ['main']);
    assert.ok(at(2).values[0].size > 0.12, `${file} ${width}: the hero number is large`);
    assert.equal(at(8).values.length, 3);
    assert.ok(at(8).values.every((value: { size: number; }) => value.size < at(2).values[0].size / 1.5), `${file} ${width}: three numbers are a band, not the slide`);
    const band = ['left', 'middle', 'right'].map(name => slot(8, name));
    assert.ok(band.every(box => Math.abs(box[1] - band[0][1]) < 0.002 && Math.abs(box[2] - band[0][2]) < 0.002) && band[1][0] > band[0][0] + band[0][2] && band[2][0] > band[1][0] + band[1][2], `${file} ${width}: three numbers stand side by side in one row: ${JSON.stringify(band)}`);
    // The golden split: 9 parts to 15 of what the gutter leaves.
    const [text, figure] = [slot(4, 'left'), slot(4, 'right')];
    assert.ok(Math.abs(text[2] / figure[2] - 9 / 15) < 0.01 && Math.abs(text[1] - figure[1]) < 0.002, `${file} ${width}: \`cols: 9-15\` cuts the stage at the golden split: ${text} ${figure}`);
    // A table beside its note, the columns cut 14 to 10 and the two on one row.
    const [grid, note] = [slot(5, 'left'), slot(5, 'right')];
    assert.ok(Math.abs(grid[2] / note[2] - 14 / 10) < 0.01 && Math.abs(grid[1] - note[1]) < 0.002 && note[0] > grid[0] + grid[2], `${file} ${width}: \`cols: 14-10\` sets a table beside its note: ${grid} ${note}`);
    assert.equal(at(3).charts.length, 1);
    assert.equal(at(4).charts.length, 1);
    // One across two below it, the rows cut 15 to 9.
    const [across, lower, upper] = [slot(7, 'top'), slot(7, 'bottom-left'), slot(7, 'bottom-right')];
    assert.ok(Math.abs(across[3] / lower[3] - 15 / 9) < 0.02 && Math.abs(lower[1] - upper[1]) < 0.002 && Math.abs(lower[2] - upper[2]) < 0.002 && Math.abs(across[2] - (lower[2] + upper[2] + (upper[0] - lower[0] - lower[2]))) < 0.003, `${file} ${width}: \`rows: 15-9\` puts one slot across two below it: ${JSON.stringify([across, lower, upper])}`);
    // A table is set to read from the back of the room, and a section is its head alone.
    const table = await page.locator('.rv-slide[data-rv-slide="5"] table').evaluate((table: Element) => parseFloat(getComputedStyle(table).fontSize) / document.querySelector('.rv-slide[data-rv-slide="5"] .rv-slide__body')!.getBoundingClientRect().width);
    assert.ok(table >= 0.015, `${file} ${width}: the table is not set smaller than the text: ${table}`);
    assert.equal(at(6).slots.length, 0, `${file} ${width}: a section holds no slot`);
    assert.ok(at(6).head && !at(6).foot);
    // A slide shows its charts without the report's tools. On a stage the number is the message:
    // every bar carries its value, so none lacks one its neighbour has, and a line names its
    // values. A group of bars stands centred on its category's name, and on a stage a few
    // columns take their band's share of a wide plot.
    if (width === 1440 && file === 'deck') {
      assert.equal(await page.locator('.rv-slide .rv-chart__actions').evaluateAll(all => all.filter(a => a.getClientRects().length).length), 0, 'no chart actions on a slide');
      // The slide's head speaks: a chart's own title stays for screen readers on a slide with a head.
      const titles = await page.locator('.rv-slide:not(.rv-slide--nohead) .rv-slot--chart .rv-chart__title').evaluateAll(all => all.map(title => title.getBoundingClientRect().width));
      assert.ok(titles.length === 3 && titles.every((w: number) => w <= 2), `a slide with a head shows no chart title: ${titles}`);
      const charts = await page.evaluate(() => window.__richview!.charts().map(model => {
        const g = model.geometry;
        const panel = g.panels[0];
        // The numbers the chart printed by itself, not its series names.
        const values = panel.items.filter((i): i is Extract<typeof i, { kind: 'text' }> => i.kind === 'text' && i.role === 'label' && /\d/.test(i.text)).map(i => i.text);
        const sizes = { value: panel.items.find((i): i is Extract<typeof i, { kind: 'text' }> => i.kind === 'text' && i.role === 'label' && /\d/.test(i.text))?.size, tick: panel.items.find((i): i is Extract<typeof i, { kind: 'text' }> => i.kind === 'text' && i.role === 'tick')?.size };
        const bars = panel.items.filter((i): i is Extract<typeof i, { kind: 'rect' }> => i.kind === 'rect' && i.role === 'mark').map(i => [i.rect.x, i.rect.x + i.rect.w] as const);
        const names = panel.items.filter((i): i is Extract<typeof i, { kind: 'text' }> => i.kind === 'text' && i.role === 'axis-label' && /^[A-Z][a-z]{2}$/.test(i.text)).map(i => [i.text, i.bounds.x + i.bounds.w / 2] as const);
        const slide = Number((document.getElementById(model.id)?.closest('.rv-slide') as HTMLElement)?.dataset.rvSlide);
        return { kind: g.kind, values, sizes, bars, names, stack: (model as ChartModel & { block?: { stack?: boolean } }).block?.stack ?? false, slide };
      }));
      // An author who sets `auto_annotations: false` on a chart asks for no value it adds itself.
      const quiet = new Set(readFileSync(join(folder, 'deck.rslide'), 'utf8').split(/^---$/m).flatMap((text, number) => number && /^\s*auto_annotations: false$/m.test(text) ? [number] : []));
      assert.ok(quiet.size >= 1, 'the example has a chart whose author turned its values off');
      for (const chart of charts.filter((c: { slide: number; }) => !quiet.has(c.slide))) {
        // A bar of zero draws no rectangle but still has its value, so the values may outnumber the marks.
        if (chart.kind === 'bar') assert.ok(chart.values.length >= chart.bars.length, `every bar on a slide carries its value: ${chart.values.length} values for ${chart.bars.length} bars (${chart.values})`);
        else assert.ok(chart.values.length >= 2, `a ${chart.kind} chart on a slide names its values: ${chart.values}`);
        // In the axis's compact form, as the headlines write them, and larger than the axis.
        assert.ok(chart.values.every((value: string) => !/thousand|million|billion/.test(value)), `a slide's values are compact: ${chart.values}`);
        assert.ok(!chart.sizes.tick || chart.sizes.value! >= chart.sizes.tick * 1.4, `a slide's values are set larger than its axis: ${JSON.stringify(chart.sizes)}`);
      }
      for (const chart of charts.filter(c => c.kind === 'bar' && c.names.length)) {
        for (const [name, centre] of chart.names) {
          const mine = chart.bars.filter(([a, b]) => Math.abs((a + b) / 2 - centre) < 200);
          const from = Math.min(...mine.map(b => b[0])), to = Math.max(...mine.map(b => b[1]));
          assert.ok(mine.length && Math.abs((from + to) / 2 - centre) <= 2, `bars of ${name} stand centred on its name: ${from}..${to} against ${centre}`);
        }
        // Not a report's 64 px cap: a column takes its band's share, which a chart of many bars
        // makes narrower than that.
        const widest = Math.max(...chart.bars.map(([a, b]) => b - a));
        const pitch = Math.min(...chart.bars.slice(1).map(([a], i) => a - chart.bars[i][0]).filter((gap: number) => gap > 1), Infinity);
        assert.ok(widest > 64 || widest >= pitch * 0.5, `a bar chart's columns take their band's share on a slide, not a report's 64 px: ${widest}px of a ${pitch}px band`);
      }
    }
    await context.close();
  }
  const close = (a: number[], b: number[]) => a.length === b.length && a.every((value: number, i: number) => Math.abs(value - b[i]) <= 0.012);
  for (let n = 1; n <= SLIDES; n++) {
    const [a, b] = [stages[1440][n - 1], stages[1920][n - 1]];
    if (a.heading) assert.ok(close(a.heading, b.heading!), `${file} slide ${n}: the title sits alike at 1440 and 1920: ${a.heading} ${b.heading}`);
    a.slots.forEach((slot, i) => assert.ok(close(slot.box, b.slots[i].box), `${file} slide ${n}: slot ${slot.name} sits alike at 1440 and 1920: ${slot.box} ${b.slots[i].box}`));
    // A chart's slot is the composition's; inside it the engine lays the chart out, and may name
    // series in a legend at one size and inside the plot at another, so its plot is near alike.
    a.charts.forEach((chart, i) => {
      assert.ok(chart.frame.every((value: number, j: number) => Math.abs(value - b.charts[i].frame[j]) <= 0.03), `${file} slide ${n}: chart ${i + 1} sits near alike at 1440 and 1920: ${chart.frame} ${b.charts[i].frame}`);
    });
  }
};

// A phone: each slide a card at the style's own sizes, its slots one under another in reading
// order, nothing wider than the screen.
const phoneChecks = async (file: string) => {
  const context = await browser.newContext({ viewport: { width: 390, height: 844 }, deviceScaleFactor: 2, isMobile: true, hasTouch: true, reducedMotion: 'reduce' });
  const page = await context.newPage();
  watch(page, `${file} phone`);
  await page.goto(`${server.base}/${file}.html`);
  await settle(page);
  for (let n = 1; n <= SLIDES; n++) {
    await show(page, n);
    const m = await measure(page, n);
    assert.ok(m.slide >= 844 - 1, `${file} phone: slide ${n} is at least a screen high`);
    assert.ok(m.pageWidth <= 390, `${file} phone: no sideways scroll on slide ${n}, page is ${m.pageWidth}`);
    assert.deepEqual(m.outside, [], `${file} phone: slide ${n} keeps everything inside`);
    for (const value of m.values) assert.ok(value.fits, `${file} phone: a big number stays in its column on slide ${n}`);
    // Nothing is clipped: the example's four-column table fits the screen whole.
    const over = await page.locator(`.rv-slide[data-rv-slide="${n}"] .rv-table-scroll`).evaluateAll((all: { scrollWidth: number; clientWidth: number; }[]) => all.map((box: { scrollWidth: number; clientWidth: number; }) => box.scrollWidth - box.clientWidth));
    for (const px of over) assert.ok(px <= 1, `${file} phone: the table on slide ${n} fits the screen, ${px}px over`);
    // The slots stand one under another in reading order, as wide as the card: no `cols` or `rows`.
    const boxes = m.slots.map(slot => slot.box);
    assert.ok(boxes.every((box, i) => i === 0 || box[1] >= boxes[i - 1][1] + boxes[i - 1][3] - 0.003), `${file} phone: the slots of slide ${n} stand in reading order, one under another: ${JSON.stringify(boxes)}`);
    assert.ok(boxes.every(box => Math.abs(box[2] - boxes[0][2]) < 0.003), `${file} phone: the slots of slide ${n} are one width: ${JSON.stringify(boxes)}`);
  }
  await context.close();
};

try {
  for (const file of Object.keys(STAGES)) {
    await stageChecks(file);
    await phoneChecks(file);
  }

  // Keys, the counter and the address.
  {
    const context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
    const page = await context.newPage();
    watch(page, 'keys');
    await page.goto(url);
    await settle(page);
    assert.deepEqual(await state(page), { hash: '', count: `1 / ${SLIDES}`, top: 1 }, 'the address is untouched until the reader moves');
    const entries = await page.evaluate(() => history.length);
    for (const [key, n] of [['ArrowRight', 2], ['Space', 3], ['PageDown', 4], ['ArrowDown', 5], ['Shift+Space', 4], ['ArrowLeft', 3], ['PageUp', 2], ['ArrowUp', 1], ['End', SLIDES], ['Home', 1]] as const) {
      await page.keyboard.press(key);
      await arrive(page, n);
      assert.equal((await state(page)).hash, `#slide=${n}`, `${key} goes to slide ${n} and the address follows`);
    }
    assert.equal(await page.evaluate(() => history.length), entries, 'moving adds no history entries');
    // A chart that has focus keeps the arrows; paging still moves the slides.
    for (const n of [2, 3, 4]) {
      await page.keyboard.press('PageDown');
      await arrive(page, n);
    }
    await settle(page);
    await page.locator('.rv-slide[data-rv-slide="4"] .rv-chart__frame').focus();
    await page.keyboard.press('ArrowRight');
    await page.waitForTimeout(400);
    assert.equal((await state(page)).top, 4, 'an arrow on a focused chart stays with the chart');
    await page.keyboard.press('PageDown');
    await arrive(page, 5);
    // The address opens a slide, and a changed address moves to it.
    await page.goto(`${url}#slide=6`);
    await page.reload();
    await settle(page);
    assert.deepEqual(await state(page), { hash: '#slide=6', count: `6 / ${SLIDES}`, top: 6 }, 'a deep link opens its slide');
    await page.evaluate(() => { location.hash = '#slide=2'; });
    await arrive(page, 2);
    await context.close();
  }

  // Reduced motion jumps: the stylesheet glides only when the reader allows motion.
  for (const [reducedMotion, behaviour] of [['reduce', 'auto'], ['no-preference', 'smooth']] as const) {
    const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, reducedMotion });
    const page = await context.newPage();
    await page.goto(url);
    assert.equal(await page.evaluate(() => getComputedStyle(document.documentElement).scrollBehavior), behaviour, `${reducedMotion}: scrolling is ${behaviour}`);
    await context.close();
  }

  // A slide below the first screen builds as it arrives and keeps nothing of the build; over a
  // field the counter takes the field's ink.
  {
    const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, reducedMotion: 'no-preference' });
    const page = await context.newPage();
    watch(page, 'arrival');
    await page.goto(url);
    await settle(page);
    const waiting = await page.locator('.rv-slide').evaluateAll(all => all.map(slide => slide.classList.contains('rv-slide--enter')));
    assert.deepEqual(waiting, [false, ...Array(SLIDES - 1).fill(true)], 'every slide but the first screen waits to build');
    // A colour as this browser serializes it: Chromium writes `oklch(1 0 0)` as it was given on one
    // build and as `rgb(255, 255, 255)` on another, so white is what the page computes for it.
    const white = await page.evaluate(() => {
      const probe = document.body.appendChild(document.createElement('i'));
      probe.style.color = 'oklch(1 0 0)';
      const color = getComputedStyle(probe).color;
      probe.remove();
      return color;
    });
    const counter = () => page.locator('.rv-slides__count').evaluate(count => getComputedStyle(count).color);
    assert.notEqual(await counter(), white, 'the counter keeps its own ink off a field');
    await page.keyboard.press('ArrowRight');
    await arrive(page, 2);
    await settle(page);
    const built = await page.locator('.rv-slide[data-rv-slide="2"]').evaluate(slide => ({
      entered: slide.classList.contains('rv-slide--entered'),
      parts: [...slide.querySelectorAll('.rv-slide__body > *, .rv-slide__number, .rv-metric__value')].map(part => {
        const style = getComputedStyle(part);
        return [style.opacity, style.translate, style.scale, style.clipPath, part.getAnimations().length].join(' ');
      }),
    }));
    assert.ok(built.entered, 'the second slide built as it arrived');
    assert.ok(built.parts.every((part: string) => part === '1 none none none 0'), `a built slide keeps nothing of its build: ${built.parts}`);
    assert.equal(await counter(), white, 'over a field the counter takes its ink');
    await page.keyboard.press('ArrowRight');
    await arrive(page, 3);
    assert.notEqual(await counter(), white, 'off the field the counter takes its own ink again');
    await context.close();
    const still = await browser.newContext({ viewport: { width: 1440, height: 900 }, reducedMotion: 'reduce' });
    const quiet = await still.newPage();
    await quiet.goto(url);
    await settle(quiet);
    assert.equal(await quiet.locator('.rv-slide--enter').count(), 0, 'reduced motion: every slide is at rest');
    await still.close();
  }

  // Paper numbers its figures and tables as a paper does.
  {
    const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, reducedMotion: 'reduce' });
    const page = await context.newPage();
    await page.goto(`${server.base}/scientific.html`);
    // Each chart and table slot carries its number, counted apart in the order written, and the
    // stage's caption prints it.
    const numbers = await page.locator('.rv-slide__figure').evaluateAll((all: Element[]) => all.map((figure: Element) => `${figure.classList.contains('rv-slide__figure--table') ? 'table' : 'figure'} ${getComputedStyle(figure).getPropertyValue('--rv-n').trim()}`));
    assert.deepEqual(numbers, ['figure 1', 'figure 2', 'table 1', 'figure 3'], `figures and tables count apart, in order: ${numbers}`);
    await context.close();
  }

  // Without scripts the sequence reads and snaps; the counter is the runtime's.
  {
    const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, javaScriptEnabled: false });
    const page = await context.newPage();
    await page.goto(url);
    assert.equal(await page.locator('.rv-slide').count(), SLIDES);
    assert.equal(await page.locator('.rv-slides__count').count(), 0, 'no counter without scripts');
    assert.equal(await page.locator('html').evaluate((root: Element) => getComputedStyle(root).scrollSnapType), 'y mandatory');
    assert.ok(await page.locator('.rv-slide[data-rv-slide="2"] .rv-metric__value').isVisible(), 'a number reads without scripts');
    await context.close();
  }

  // Paper: one slide a page, 16:9.
  if (engine === 'chromium') {
    const context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
    const page = await context.newPage();
    watch(page, 'print');
    await page.goto(url);
    await settle(page);
    await page.emulateMedia({ media: 'print' });
    const pdf = await page.pdf({ preferCSSPageSize: true, printBackground: true });
    const text = pdf.toString('latin1');
    const pages = (text.match(/\/Type\s*\/Page\b(?!s)/g) ?? []).length;
    assert.equal(pages, SLIDES, `paper is one slide a page: ${pages} pages`);
    const box = text.match(/\/MediaBox\s*\[\s*0\s+0\s+([\d.]+)\s+([\d.]+)\s*\]/);
    assert.ok(box && Math.abs(Number(box[1]) - 960) <= 1 && Math.abs(Number(box[2]) - 540) <= 1, `a page is 13.33 × 7.5 in: ${box?.slice(1)}`);
    await context.close();
    // The command's PDF is the same pages, whatever `--page` says. The command finds its browser
    // as a reader's run does (RICHVIEW_BROWSER, then an installed Chrome), as the other PDF checks
    // do: Playwright's Chrome for Testing, handed to it, stalls on its first page in the full.
    const named = chromiumPath();
    const printed = JSON.parse(execFileSync(cli, ['generate', join(folder, 'deck.rslide'), '--to', 'pdf', '--page', 'letter', ...(named ? ['--browser', named] : []), '--format', 'json'], { stdio: 'pipe', timeout: 180_000 }).toString());
    assert.equal(printed.page, '16:9', `the result names the slides' page: ${printed.page}`);
    assert.equal(printed.pages, SLIDES, `generate --to pdf prints one slide a page: ${printed.pages} pages`);
    const file = readFileSync(printed.output).toString('latin1');
    const boxes = [...file.matchAll(/\/MediaBox\s*\[\s*0\s+0\s+([\d.]+)\s+([\d.]+)\s*\]/g)].map(m => [Number(m[1]), Number(m[2])]);
    assert.ok(boxes.length && boxes.every(([w, h]) => Math.abs(w - 960) <= 1 && Math.abs(h - 540) <= 1), `every page of generate --to pdf is 13.33 × 7.5 in: ${JSON.stringify(boxes.slice(0, 3))}`);
  }
  assert.deepEqual(problems, [], 'no page errors');
  console.log(`Slides verified (${engine}): ${SLIDES} slides on five stages at 1280, 1440, 1920 and on a phone, arrivals, keys, address, no scripts${engine === 'chromium' ? ', paper and generate --to pdf' : ''}`);
} finally {
  await browser.close();
  await server.close();
}
