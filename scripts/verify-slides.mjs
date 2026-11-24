// Verify slides (FEA-RP-65) in a generated report: the title and each top-level
// heading or thematic break are a slide; on a landscape window a slide is a 16:9 stage whose
// composition is the same at every size, with charts laid out for the room they get; on a phone
// the slides stack; keys, the counter and the address move through them; without scripts the
// sequence still reads and snaps; and paper is one slide a page. Each builtin style sets its
// slides on its own stage and every stage keeps those fits; a slide below the first screen
// builds as it arrives and keeps nothing of the build, and reduced motion keeps it at rest.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { cpSync, mkdirSync, mkdtempSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { chromium, firefox, webkit, errors } from 'playwright';
import { chromiumPath, root, serveStatic, waitForBenchmark } from './browser-harness.mjs';
import { launchBrowser } from '../tools/rv-gate/lib/browser-launch.mjs';

const cli = process.env.RICHVIEW_CLI || join(root, 'target/release/richview');
const parent = join(root, 'tmp/verification');
mkdirSync(parent, { recursive: true });
const dir = mkdtempSync(join(parent, 'slides-'));
const folder = join(dir, 'monthly-review');
cpSync(join(root, 'docs/public/gallery/monthly-review'), folder, { recursive: true });

// The example is clean: slides are no longer experimental, so no run carries a slides note.
const result = JSON.parse(execFileSync(cli, ['generate', join(folder, 'report.rmd'), '--format', 'json'], { stdio: 'pipe' }).toString());
const codes = result.diagnostics.map(d => `${d.level} ${d.code}`);
assert.ok(!codes.some(code => code.includes(' slides.')), `generate says nothing about a clean deck: ${codes}`);
assert.deepEqual(codes.filter(code => !code.startsWith('info ')), [], 'the gallery example has no warning');
const html = readFileSync(join(folder, 'Rendered/report.html'), 'utf8');
const SLIDES = 8;
assert.equal((html.match(/<section class="rv-slide[ "]/g) ?? []).length, SLIDES, 'the title and each heading are a slide');
assert.match(html, /<html[^>]* data-rv-slides/, 'the document is marked as slides');
assert.doesNotMatch(html, /class="rv-toc/, 'slides list no contents');
// Each builtin style sets the deck on its own stage.
const STAGES = { report: 'keynote', air: 'gallery', brutalist: 'poster', 'hand-drawn': 'sketchbook', scientific: 'paper' };
for (const style of Object.keys(STAGES).filter(name => name !== 'report')) {
  execFileSync(cli, ['generate', join(folder, 'report.rmd'), '--style', style, '--output', `${style}.html`], { stdio: 'pipe' });
}
for (const [file, stage] of Object.entries(STAGES)) {
  assert.match(readFileSync(join(folder, `Rendered/${file}.html`), 'utf8'), new RegExp(`<html[^>]* data-rv-stage="${stage}"`), `${file} stands on the ${stage} stage`);
}

const engines = { chromium, firefox, webkit };
const engine = process.env.RICHVIEW_VERIFY_BROWSER || 'chromium';
await waitForBenchmark();
const server = await serveStatic({ '/': join(folder, 'Rendered') }, { port: Number(process.env.RICHVIEW_VERIFY_PORT ?? 0) });
const browser = await launchBrowser(engines[engine], {
  headless: true,
  ...(engine === 'chromium' ? { executablePath: chromiumPath(), args: ['--no-sandbox', ...(process.platform === 'darwin' && process.arch === 'arm64' ? ['--enable-gpu'] : [])] } : {}),
}, { TimeoutError: errors.TimeoutError });
const url = `${server.base}/report.html`;
const problems = [];
const watch = (page, label) => {
  page.on('pageerror', error => problems.push(`${label}: ${error.message}`));
  page.on('console', message => { if (message.type() === 'error') problems.push(`${label}: console ${message.text()}`); });
};
// The runtime starts after load; wait for it, as the harness's settledCharts does, then for its charts.
const settle = page => page.evaluate(async () => {
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
const show = async (page, n) => {
  await page.locator(`.rv-slide[data-rv-slide="${n}"]`).evaluate(slide => slide.scrollIntoView({ behavior: 'instant', block: 'start' }));
  await settle(page);
  await page.waitForTimeout(150);
  await settle(page);
};
// Every box the composition is made of, as shares of the stage, so two window sizes compare.
const measure = (page, n) => page.locator(`.rv-slide[data-rv-slide="${n}"]`).evaluate(slide => {
  const stage = slide.querySelector('.rv-slide__body').getBoundingClientRect();
  const share = element => {
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
    text: parseFloat(getComputedStyle(slide.querySelector('.rv-slide__body')).fontSize) / stage.width,
    heading: slide.querySelector('.rv-slide__body > h2') ? share(slide.querySelector('.rv-slide__body > h2')) : null,
    numbers: [...slide.querySelectorAll('.rv-slide__number')].map(share),
    // A number's figures stay inside its column (a stage's marks, such as a pencil underline,
    // may run past the figures themselves).
    values: [...slide.querySelectorAll('.rv-metric__value')].map(value => {
      const range = document.createRange();
      range.selectNodeContents(value);
      const text = range.getBoundingClientRect();
      const column = value.closest('.rv-slide__number, .rv-metric').getBoundingClientRect();
      return { size: parseFloat(getComputedStyle(value).fontSize) / stage.width, fits: text.left >= column.left - 1 && text.right <= column.right + 1 };
    }),
    charts: [...slide.querySelectorAll('figure[data-rv-chart]')].map(figure => {
      const frame = figure.querySelector('.rv-chart__frame').getBoundingClientRect();
      const slot = figure.closest('.rv-slide__figure').getBoundingClientRect();
      // Its title, legend, plot and source note all stay in the slot the slide gave it.
      const content = figure.closest('.rv-slide__figure').lastElementChild.getBoundingClientRect();
      return { frame: share(figure.querySelector('.rv-chart__frame')), slot: share(figure.closest('.rv-slide__figure')), inside: frame.bottom <= slot.bottom + 1 && frame.right <= slot.right + 1 && content.bottom <= slot.bottom + 1, fills: frame.height / slot.height };
    }),
    outside,
    pageWidth: document.documentElement.scrollWidth,
  };
});
const state = page => page.evaluate(() => ({
  hash: location.hash,
  count: document.querySelector('.rv-slides__count')?.textContent ?? null,
  top: [...document.querySelectorAll('.rv-slide')].findIndex(slide => Math.abs(slide.getBoundingClientRect().top) < 2) + 1,
}));
// The keys glide; wait until the slide the key asked for rests at the top.
const arrive = async (page, n) => {
  await page.waitForFunction(n => {
    const slide = document.querySelector(`.rv-slide[data-rv-slide="${n}"]`);
    return Math.abs(slide.getBoundingClientRect().top) < 2 && document.querySelector('.rv-slides__count')?.textContent === `${n} / ${document.querySelectorAll('.rv-slide').length}`;
  }, n, { timeout: 10_000 });
};

// A stage: 16:9, as large as the window allows, the same composition at every size, in every
// style. The default style is also checked for what its charts draw.
const stageChecks = async file => {
  const stages = {};
  const url = `${server.base}/${file}.html`;
  for (const [width, height] of [[1440, 900], [1920, 1080], [1280, 720]]) {
    const context = await browser.newContext({ viewport: { width, height }, reducedMotion: file === 'report' ? 'no-preference' : 'reduce' });
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
      // A table is sized to its column: on a stage it never scrolls sideways inside it.
      const scrolled = await page.locator(`.rv-slide[data-rv-slide="${n}"] .rv-slide__figure--table .rv-table-scroll`).evaluateAll(all => all.map(box => box.scrollWidth - box.clientWidth));
      for (const over of scrolled) assert.ok(over <= 1, `${file} ${width}: the table on slide ${n} fits its column, ${over}px over`);
      for (const chart of m.charts) {
        assert.ok(chart.inside, `${file} ${width}: a chart stays in its slot on slide ${n}`);
        assert.ok(chart.fills > 0.6, `${file} ${width}: a chart takes the room its slot leaves on slide ${n}, got ${chart.fills}`);
      }
    }
    const slideAt = n => page.locator(`.rv-slide[data-rv-slide="${n}"]`);
    // Every number of a row reads the same way: its label, then its change on a line of its own.
    const changes = await slideAt(3).locator('.rv-slide__number').evaluateAll(all => all.map(number => number.querySelector('.rv-metric__delta').getBoundingClientRect().top - number.querySelector('.rv-metric__label').getBoundingClientRect().bottom));
    assert.ok(changes.every(gap => gap >= -1), `${file} ${width}: each number's change stands under its label, gaps ${changes}`);
    // Two charts side by side share one card height, so their source notes line up.
    const cards = await slideAt(5).locator('.rv-slide__figure').evaluateAll(all => all.map(slot => [slot.querySelector('.rv-chart').getBoundingClientRect().height, slot.lastElementChild.getBoundingClientRect().top]));
    assert.ok(Math.abs(cards[0][0] - cards[1][0]) <= 1 && Math.abs(cards[0][1] - cards[1][1]) <= 1, `${file} ${width}: the two cards and notes line up: ${JSON.stringify(cards)}`);
    // A table alone on its slide is set larger than the text, with the text under it.
    const lone = await slideAt(6).evaluate(slide => {
      const table = slide.querySelector('table').getBoundingClientRect();
      const text = slide.querySelector('.rv-slide__text').getBoundingClientRect();
      return { size: parseFloat(getComputedStyle(slide.querySelector('table')).fontSize) / slide.querySelector('.rv-slide__body').getBoundingClientRect().width, under: text.top >= table.bottom - 1 };
    });
    assert.ok(lone.size >= 0.02 && lone.under, `${file} ${width}: a table alone is set large with its text under it: ${JSON.stringify(lone)}`);
    // A slide shows its charts without the report's tools. On a stage the number is the message:
    // every bar carries its value, so none lacks one its neighbour has, and a line names its
    // values. A group of bars stands centred on its category's name, and on a stage a few
    // columns take their band's share of a wide plot.
    if (width === 1440 && file === 'report') {
      assert.equal(await page.locator('.rv-slide .rv-chart__actions').evaluateAll(all => all.filter(a => a.getClientRects().length).length), 0, 'no chart actions on a slide');
      const charts = await page.evaluate(() => window.__richview.charts().map(model => {
        const g = model.geometry;
        const panel = g.panels[0];
        // The numbers the chart printed by itself, not its series names.
        const values = panel.items.filter(i => i.kind === 'text' && i.role === 'label' && /\d/.test(i.text)).map(i => i.text);
        const sizes = { value: panel.items.find(i => i.kind === 'text' && i.role === 'label' && /\d/.test(i.text))?.size, tick: panel.items.find(i => i.kind === 'text' && i.role === 'tick')?.size };
        const bars = panel.items.filter(i => i.kind === 'rect' && i.role === 'mark').map(i => [i.rect.x, i.rect.x + i.rect.w]);
        const names = panel.items.filter(i => i.kind === 'text' && i.role === 'axis-label' && /^[A-Z][a-z]{2}$/.test(i.text)).map(i => [i.text, i.bounds.x + i.bounds.w / 2]);
        return { kind: g.kind, values, sizes, bars, names, stack: model.block?.stack ?? false };
      }));
      for (const chart of charts) {
        if (chart.kind === 'bar') assert.equal(chart.values.length, chart.bars.length, `every bar on a slide carries its value: ${chart.values.length} values for ${chart.bars.length} bars (${chart.values})`);
        else assert.ok(chart.values.length >= 2, `a ${chart.kind} chart on a slide names its values: ${chart.values}`);
        // In the axis's compact form, as the headlines write them, and larger than the axis.
        assert.ok(chart.values.every(value => !/thousand|million|billion/.test(value)), `a slide's values are compact: ${chart.values}`);
        assert.ok(!chart.sizes.tick || chart.sizes.value >= chart.sizes.tick * 1.4, `a slide's values are set larger than its axis: ${JSON.stringify(chart.sizes)}`);
      }
      for (const chart of charts.filter(c => c.kind === 'bar' && c.names.length)) {
        for (const [name, centre] of chart.names) {
          const mine = chart.bars.filter(([a, b]) => Math.abs((a + b) / 2 - centre) < 200);
          const from = Math.min(...mine.map(b => b[0])), to = Math.max(...mine.map(b => b[1]));
          assert.ok(mine.length && Math.abs((from + to) / 2 - centre) <= 2, `bars of ${name} stand centred on its name: ${from}..${to} against ${centre}`);
        }
        const widest = Math.max(...chart.bars.map(([a, b]) => b - a));
        assert.ok(widest > 64, `a bar chart's columns take their band's share on a slide, not a report's 64 px: ${widest}px`);
      }
    }
    // The hero is the largest number the format sets; beside a chart numbers are a rail.
    assert.ok(stages[width][1].values[0].size > 0.12, `${file} ${width}: the hero number is large`);
    assert.equal(stages[width][2].values.length, 4);
    assert.ok(stages[width][2].values.every(value => value.size < stages[width][1].values[0].size / 2), `${file} ${width}: numbers beside a chart are a rail`);
    // Four numbers over one chart stand in one column of one width on the left, top to bottom,
    // and the chart takes the slide's height beside them, not a strip under a row.
    const rail = stages[width][2].numbers;
    const [beside] = stages[width][2].charts;
    assert.ok(rail.every(box => Math.abs(box[0] - rail[0][0]) < 0.002 && Math.abs(box[2] - rail[0][2]) < 0.002) && rail.every((box, i) => i === 0 || box[1] >= rail[i - 1][1] + rail[i - 1][3] - 0.003), `${file} ${width}: four numbers stand in one column: ${JSON.stringify(rail)}`);
    assert.ok(beside.slot[0] >= rail[0][0] + rail[0][2] && beside.slot[3] >= 0.3, `${file} ${width}: the chart stands beside the numbers at the slide's height: ${JSON.stringify(beside.slot)}`);
    if (width === 1440) {
      // Every slide a stage moment: a one-chart slide's claim is its headline; the first of a rail
      // of numbers is larger; a table's changes are chips, the column they compare leads and the
      // sum of the rows above is ruled off as the total; a callout beside the points is a pull quote.
      const moment = await page.evaluate(() => {
        const at = n => document.querySelector(`.rv-slide[data-rv-slide="${n}"]`);
        const size = el => parseFloat(getComputedStyle(el).fontSize);
        const values = [...at(3).querySelectorAll('.rv-metric__value')].map(size);
        const callout = at(8).querySelector('.rv-callout').getBoundingClientRect();
        const list = at(8).querySelector('.rv-slide__text ul').getBoundingClientRect();
        return {
          claim: at(4).classList.contains('rv-slide--claim') && !at(3).classList.contains('rv-slide--claim') && !at(5).classList.contains('rv-slide--claim'),
          headline: size(at(4).querySelector('.rv-chart__title')) / size(at(4).querySelector('h2')),
          lead: values[0] / values[1],
          chips: [...at(6).querySelectorAll('.rv-slide__chip')].map(chip => [chip.textContent, chip.classList.contains('rv-slide__chip--up')]),
          latest: [...at(6).querySelectorAll('.rv-slide__latest')].map(cell => cell.textContent),
          total: [...at(6).querySelectorAll('.rv-slide__total td')].map(cell => cell.textContent).join(' '),
          beside: callout.left >= list.right - 1 && callout.top <= list.bottom,
        };
      });
      assert.ok(moment.claim, `${file}: only the one-chart slide states a claim`);
      assert.ok(moment.headline >= 1.5, `${file}: the claim is the headline over its topic: ${moment.headline}`);
      assert.ok(moment.lead >= 1.2, `${file}: the first number of a rail leads: ${moment.lead}`);
      assert.deepEqual(moment.chips, [['+50%', true], ['+90.9%', true], ['+61.5%', true]], `${file}: a table's changes are chips`);
      assert.deepEqual(moment.latest, ['September', '$420K', '$210K', '$630K'], `${file}: the column the changes compare leads`);
      assert.equal(moment.total, 'Total $390K $500K $630K +61.5%', `${file}: the sum of the rows above is the total`);
      assert.ok(moment.beside, `${file}: a callout stands beside the points as a pull quote`);
    }
    // Two figures stand side by side.
    const pair = stages[width][4].charts;
    assert.equal(pair.length, 2);
    // Their slots share a row; one chart may carry a legend row the other has not.
    assert.ok(Math.abs(pair[0].slot[1] - pair[1].slot[1]) < 0.002 && pair[1].slot[0] > pair[0].slot[0] + pair[0].slot[2] - 0.002, `${file} ${width}: two charts stand side by side, slots ${JSON.stringify(pair.map(c => c.slot))}`);
    await context.close();
  }
  const close = (a, b) => a.length === b.length && a.every((value, i) => Math.abs(value - b[i]) <= 0.012);
  for (let n = 1; n <= SLIDES; n++) {
    const [a, b] = [stages[1440][n - 1], stages[1920][n - 1]];
    if (a.heading) assert.ok(close(a.heading, b.heading), `${file} slide ${n}: the title sits alike at 1440 and 1920: ${a.heading} ${b.heading}`);
    a.numbers.forEach((box, i) => assert.ok(close(box, b.numbers[i]), `${file} slide ${n}: number ${i + 1} sits alike at 1440 and 1920: ${box} ${b.numbers[i]}`));
    // A chart's slot is the composition's; inside it the engine lays the chart out, and may name
    // series in a legend at one size and inside the plot at another, so its plot is near alike.
    a.charts.forEach((chart, i) => {
      assert.ok(close(chart.slot, b.charts[i].slot), `${file} slide ${n}: chart ${i + 1}'s slot sits alike at 1440 and 1920: ${chart.slot} ${b.charts[i].slot}`);
      assert.ok(chart.frame.every((value, j) => Math.abs(value - b.charts[i].frame[j]) <= 0.03), `${file} slide ${n}: chart ${i + 1} sits near alike at 1440 and 1920: ${chart.frame} ${b.charts[i].frame}`);
    });
  }
};

// A phone: a stack at the style's own sizes, numbers two to a row, nothing wider than the screen.
const phoneChecks = async file => {
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
    // Nothing is clipped: the example's five-column table fits the screen whole.
    const over = await page.locator(`.rv-slide[data-rv-slide="${n}"] .rv-table-scroll`).evaluateAll(all => all.map(box => box.scrollWidth - box.clientWidth));
    for (const px of over) assert.ok(px <= 1, `${file} phone: the table on slide ${n} fits the screen, ${px}px over`);
  }
  const row = (await measure(page, 3)).numbers;
  assert.ok(Math.abs(row[0][1] - row[1][1]) < 0.002 && row[2][1] > row[0][1] + 0.05, `${file} phone: numbers stand two to a row`);
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
    for (const [key, n] of [['ArrowRight', 2], ['Space', 3], ['PageDown', 4], ['ArrowDown', 5], ['Shift+Space', 4], ['ArrowLeft', 3], ['PageUp', 2], ['ArrowUp', 1], ['End', SLIDES], ['Home', 1]]) {
      await page.keyboard.press(key);
      await arrive(page, n);
      assert.equal((await state(page)).hash, `#slide=${n}`, `${key} goes to slide ${n} and the address follows`);
    }
    assert.equal(await page.evaluate(() => history.length), entries, 'moving adds no history entries');
    // A chart that has focus keeps the arrows; paging still moves the slides.
    await page.keyboard.press('PageDown');
    await arrive(page, 2);
    await page.keyboard.press('PageDown');
    await arrive(page, 3);
    await settle(page);
    await page.locator('.rv-slide[data-rv-slide="3"] .rv-chart__frame').focus();
    await page.keyboard.press('ArrowRight');
    await page.waitForTimeout(400);
    assert.equal((await state(page)).top, 3, 'an arrow on a focused chart stays with the chart');
    await page.keyboard.press('PageDown');
    await arrive(page, 4);
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
  for (const [reducedMotion, behaviour] of [['reduce', 'auto'], ['no-preference', 'smooth']]) {
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
    assert.ok(built.parts.every(part => part === '1 none none none 0'), `a built slide keeps nothing of its build: ${built.parts}`);
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
    const numbers = await page.locator('.rv-slide__figure').evaluateAll(all => all.map(figure => `${figure.classList.contains('rv-slide__figure--table') ? 'table' : 'figure'} ${getComputedStyle(figure).getPropertyValue('--rv-n').trim()}`));
    assert.deepEqual(numbers, ['figure 1', 'figure 2', 'figure 3', 'figure 4', 'table 1'], `figures and tables count apart, in order: ${numbers}`);
    await context.close();
  }

  // Without scripts the sequence reads and snaps; the counter is the runtime's.
  {
    const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, javaScriptEnabled: false });
    const page = await context.newPage();
    await page.goto(url);
    assert.equal(await page.locator('.rv-slide').count(), SLIDES);
    assert.equal(await page.locator('.rv-slides__count').count(), 0, 'no counter without scripts');
    assert.equal(await page.locator('html').evaluate(root => getComputedStyle(root).scrollSnapType), 'y mandatory');
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
    const printed = JSON.parse(execFileSync(cli, ['generate', join(folder, 'report.rmd'), '--to', 'pdf', '--page', 'letter', ...(named ? ['--browser', named] : []), '--format', 'json'], { stdio: 'pipe', timeout: 180_000 }).toString());
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
