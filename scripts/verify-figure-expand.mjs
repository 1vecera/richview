// Real report controls: subtitle layout, width changes, exports and the Expand modal.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { cli, harness, root, serveStatic, settled, watchErrors } from './browser-harness.mjs';
import { reports, settle } from './report-runtime.mjs';
import { FIGURE_THEMES, FIGURE_WIDTHS, figureBuildThemes, figureCases } from '../tools/rv-gate/lib/figure-cases.mjs';

const selection = figureCases();
const selectedCases = new Set(selection.cases);
const completedCases = [];
const wants = key => selectedCases.has(key);
const h = await harness('figure-expand');
let inputs;
const directory = join(h.dir, 'reports');
mkdirSync(directory);
const themes = FIGURE_THEMES;
const buildThemes = figureBuildThemes(selection);
const rows = [0, .03, .06, .12, .18, .21].map((value, index) => ({ category: 'ABCDEF'[index], value }));
const chart = {
  type: 'bar', title: 'Six small bars start at plain zero',
  subtitle: 'Fractional values retain one consistent tick precision',
  data: { inline: rows }, encoding: { x: 'category', y: 'value' },
  style: { orientation: 'vertical' },
};
const fence = value => `\n\`\`\`chart\n${JSON.stringify(value)}\n\`\`\`\n`;
const source = '---\ntitle: Figure expansion\n---\n\n## A section heading\n\nThe chart and its caption share one report column.\n'
  + fence(chart)
  + fence({ ...chart, title: 'Independent facet panels', encoding: { ...chart.encoding, facet: 'panel' }, data: { inline: rows.map((row, index) => ({ ...row, panel: index ? 'Later' : 'Earlier' })) } })
  + fence({ ...chart, title: "Chart with no subtitle", subtitle: undefined });
for (const theme of buildThemes) {
  const report = join(directory, theme);
  mkdirSync(report);
  const path = join(report, 'report.rmd');
  writeFileSync(path, source);
  execFileSync(cli, ['generate', path, '--style', theme], { cwd: root, stdio: 'pipe' });
}
const server = await serveStatic(Object.fromEntries(buildThemes.map(theme => [`/${theme}/`, join(directory, theme, 'Rendered')])));
// Genuine runtime gaps stay failing assertions, collected so one gap does not hide the rest.
const failures = [];
const evidence = { engine: h.engine, browser: h.browser.version(), cases: [], errors: [] };
try {
  const page = await h.browser.newPage({ viewport: { width: 1440, height: 1000 }, hasTouch: true, reducedMotion: 'no-preference', acceptDownloads: true });
  watchErrors(page, evidence.errors);
  const geometry = figure => {
    const box = node => { const r = node.getBoundingClientRect(); return { x: r.x, y: r.y, width: r.width, height: r.height, right: r.right, bottom: r.bottom }; };
    const model = window.__richview.charts().find(chart => chart.id === figure.id);
    const panel = model.geometry.panels[0], frame = figure.querySelector('.rv-chart__frame').getBoundingClientRect();
    // The first category's label must sit under its own band, even for a zero-height bar.
    const first = panel.observations[0], label = panel.items.find(item => item.kind === 'text' && item.role === 'axis-label' && item.text === 'A');
    return {
      figure: box(figure), header: box(figure.querySelector(':scope > figcaption')),
      title: box(figure.querySelector('.rv-chart__title')),
      titleLine: parseFloat(getComputedStyle(figure.querySelector('.rv-chart__title')).lineHeight),
      titleLines: (() => { const range = document.createRange(); range.selectNodeContents(figure.querySelector('.rv-chart__title')); return new Set([...range.getClientRects()].map(r => Math.round(r.bottom))).size; })(),
      subtitle: box(figure.querySelector('.rv-chart__subtitle')),
      subtitleLine: parseFloat(getComputedStyle(figure.querySelector('.rv-chart__subtitle')).lineHeight),
      actions: box(figure.querySelector('.rv-chart__actions')),
      firstLabel: { x: frame.x + label.bounds.x, right: frame.x + label.bounds.x + label.bounds.w },
      zeroCategoryX: frame.x + first.area.x + first.area.w / 2,
      model: JSON.stringify(model.geometry),
      documentWidth: document.documentElement.scrollWidth, viewport: innerWidth,
    };
  };
  const names = [['Table', 'Table'], ['Download PNG', 'Download PNG'], ['CSV', 'CSV'], ['Expand', 'Expand']];
  const dialogFrame = page.locator('dialog.rv-chart-dialog[open] .rv-chart__frame');
  /** The expanded plot's own size, and whether the dialog is turned a quarter clockwise (FEA-CH-68). */
  const plotBox = () => dialogFrame.evaluate(node => {
    const r = node.getBoundingClientRect();
    const m = new DOMMatrixReadOnly(getComputedStyle(node.closest('dialog')).transform);
    const turned = Math.abs(m.a) < 1e-6 && Math.abs(m.b - 1) < 1e-6 && Math.abs(m.c + 1) < 1e-6 && Math.abs(m.d) < 1e-6;
    return { left: r.left, top: r.top, right: r.right, width: node.offsetWidth, height: node.offsetHeight, turned, upright: m.isIdentity };
  });
  /** Client positions along the expanded plot's middle where a readout shows, keyed by its text.
   * The sweep runs in the plot's own coordinates; a turned plot's x axis runs down the screen. */
  const sweep = async () => {
    const box = await plotBox();
    const screen = (x, y) => (box.turned ? { x: box.right - y, y: box.top + x } : { x: box.left + x, y: box.top + y });
    const seen = new Map();
    for (const fraction of [.45, .6, .7, .78, .85]) {
      for (let x = 8; x < box.width - 8; x += 6) {
        const at = screen(x, box.height * fraction);
        await page.mouse.move(at.x, at.y);
        const text = await page.locator('dialog.rv-chart-dialog[open] .rv-chart__readout').evaluate(node => (node.hidden ? null : node.textContent));
        if (text && !seen.has(text)) seen.set(text, at);
      }
    }
    await page.mouse.move(2, 2);
    return seen;
  };
  for (const theme of themes) for (const width of FIGURE_WIDTHS) {
    if (!wants(`theme/${theme}/${width}`)) continue;
    await page.setViewportSize({ width, height: 1000 });
    await page.goto(`${server.base}/${theme}/report.html`);
    await page.waitForFunction(() => window.__richview);
    await page.evaluate(() => window.__richview.settled());
    const figures = page.locator('figure[data-rv-chart]');
    // Header, section heading, prose and footer share one left edge (STY-30): a centred measure
    // must not shrink them to their content, and a measure in ch must not widen a heading.
    const edges = await page.evaluate(() => {
      const left = selector => { const node = document.querySelector(selector); return node ? Math.round(node.getBoundingClientRect().left) : null; };
      return { header: left('.rv-report__header'), heading: left('.rv-report__body h2'), prose: left('.rv-report__body > p'), footer: left('.rv-report__footer') };
    });
    for (const [name, edge] of Object.entries(edges)) if (edge !== null) assert.ok(Math.abs(edge - edges.prose) <= 1, `${theme}/${width}: the ${name} starts at ${edge}, the prose at ${edges.prose}`);
    const figure = figures.first();
    assert.equal(await figures.count(), 3);
    assert.equal(await figure.locator('.rv-chart__action--expand').count(), 1);
    assert.equal(await figures.nth(1).locator('.rv-chart__action--expand').count(), 1, 'One outer control expands all facet panels together');
    assert.equal(await figures.nth(2).locator('.rv-chart__action--expand').count(), 1, 'Charts without a subtitle remain expandable');
    // Only the visible action buttons form the row (Reset zoom stays hidden until zoomed).
    // The actions are icons in one row at the right end of the title row, or under the title where the row cannot hold both (FEA-CH-43; Daniel, 9 Nov).
    const untitled = await figures.nth(2).locator('.rv-chart__actions button:visible').evaluateAll(buttons => buttons.map(button => { const r = button.getBoundingClientRect(); return { y: r.y, left: r.left, right: r.right }; }));
    assert.ok(Math.max(...untitled.map(b => b.y)) - Math.min(...untitled.map(b => b.y)) < 1, `${theme}/${width}: the actions sit in one row`);
    const untitledFigure = await figures.nth(2).evaluate(node => { const r = node.getBoundingClientRect(); return { left: r.left, right: r.right }; });
    assert.ok(untitled.every(b => b.left >= untitledFigure.left - 1 && b.right <= untitledFigure.right + 1), `${theme}/${width}: wrapped actions stay inside the figure`);
    const before = await figure.evaluate(geometry);
    assert.ok(before.firstLabel.x <= before.zeroCategoryX + 1 && before.firstLabel.right >= before.zeroCategoryX - 1, 'The zero-value category label remains under its own band');
    assert.ok(Math.abs(before.title.x - before.subtitle.x) <= 3, `${theme}/${width}: title starts at the caption's left edge, including when the first bar has zero height`);
    // The title box is as high as its lines, wherever the style sets it (a flex basis once made it 240 px in Hand-drawn).
    assert.ok(before.title.height <= before.titleLines * before.titleLine + 2, `${theme}/${width}: the title is ${before.title.height.toFixed(1)} px high for ${before.titleLines} line(s) of ${before.titleLine} px`);
    for (const [name, label] of names) {
      const button = figure.getByRole('button', { name, exact: true });
      assert.equal(await button.locator('svg[aria-hidden="true"]').count(), 1, `${name}: the action keeps its icon`);
      // At every width the actions are icons whose label is their accessible name and shows only
      // on hover and focus, on 32 px targets (FEA-CH-43, STY-42; Daniel, 9 Nov).
      const look = await button.evaluate(b => { const r = b.getBoundingClientRect(), span = getComputedStyle(b.querySelector('span')); return { aria: b.getAttribute('aria-label'), text: b.textContent.trim(), w: r.width, h: r.height, shown: span.opacity !== '0' }; });
      assert.equal(look.aria, label, `${theme}/${width}: ${name} is named by its aria-label`);
      assert.equal(look.text, label, `${theme}/${width}: ${name} keeps its name in the DOM`);
      assert.ok(!look.shown, `${theme}/${width}: ${name} shows its icon alone at rest`);
      assert.ok(look.w >= 32 && look.h >= 32, `${theme}/${width}: ${name} keeps a 32 px target (${look.w}x${look.h})`);
    }
    // The actions sit at the right of the title, in its row, or under it; never over it.
    const besideTitle = before.actions.x >= before.title.right - 1 && before.actions.y < before.title.bottom - 1;
    assert.ok(besideTitle || before.actions.y >= before.title.bottom - 1 || before.actions.bottom <= before.title.y + 1, `${theme}/${width}: actions never overlap the title`);
    // A title short enough to share its row keeps the icons beside it at every width; only a longer
    // one sends them under (S27-996; Daniel, 9 Nov: in the title row if they fit, otherwise under it).
    for (let index = 0; index < 3; index++) {
      const fit = await figures.nth(index).evaluate(node => {
        const row = node.querySelector('.rv-chart__titlerow'), title = row.querySelector('.rv-chart__title'), actions = row.querySelector('.rv-chart__actions');
        if (!actions || getComputedStyle(row).display === 'contents') return null;
        const probe = title.cloneNode(true);
        probe.style.cssText = 'position:absolute;visibility:hidden;white-space:nowrap;width:auto;flex:none';
        row.append(probe);
        const natural = probe.getBoundingClientRect().width;
        probe.remove();
        const t = title.getBoundingClientRect(), a = actions.getBoundingClientRect();
        return { row: row.getBoundingClientRect().width, gap: parseFloat(getComputedStyle(row).columnGap), natural, actions: a.width, beside: a.top < t.bottom - 1 && a.left >= t.right - 1 };
      });
      if (fit && fit.natural + fit.gap + fit.actions <= fit.row - 2) assert.ok(fit.beside, `${theme}/${width}: figure ${index + 1}'s title fits beside its icons, so they stay in its row: ${JSON.stringify(fit)}`);
    }
    // Where the title row holds them, the icons' centre is the title's first line's. A style that sets the title
    // below the chart (Hand-drawn) keeps them at the figure's top right instead.
    if (width === 1440 && before.title.y < before.actions.bottom) {
      assert.ok(besideTitle, `${theme}: at 1440 the actions sit in the title row (${JSON.stringify([before.actions, before.title])})`);
      const off = (before.actions.y + before.actions.height / 2) - (before.title.y + before.titleLine / 2);
      assert.ok(Math.abs(off) <= 2, `${theme}: the icons' centre is ${off.toFixed(1)} px off the title's first line's`);
    }
    // Actions share the subtitle's first row only when they fit beside a readable subtitle;
    // otherwise they wrap below it. Either way the two never overlap.
    // A style may put the description below the chart, away from the actions; only a shared row
    // (the boxes overlap vertically) is held to the alignment and width rules.
    const sharedRow = before.actions.y < before.subtitle.bottom - 1 && before.actions.bottom > before.subtitle.y + 1;
    if (sharedRow) {
      assert.ok(before.actions.y < before.subtitle.y + before.subtitleLine, `${theme} ${width}: shared-row actions align with the subtitle's first line`);
      assert.ok(before.subtitle.right <= before.actions.x + 1, 'Subtitle and actions never overlap');
      assert.ok(before.subtitle.width >= before.header.width * .4, `${theme}/${width}: actions leave a readable subtitle column`);
    } else {
      assert.ok(before.actions.y >= before.subtitle.bottom - 1 || before.actions.bottom <= before.subtitle.y + 1, `${theme}/${width}: the actions and the subtitle never share a row without fitting beside each other`);
    }
    assert.ok(before.documentWidth <= width + 1, 'The controls do not overflow the phone viewport');
    // Expanding must neither remount the report's chart nor animate it.
    await figure.evaluate(figure => {
      window.__expansionCanvas = figure.querySelector('canvas.rv-chart__canvas');
      window.__expansionCharts = window.__richview.charts().length;
    });
    await figure.getByRole('button', { name: 'Table', exact: true }).click();
    const table = await figure.locator('.rv-chart__table tbody').innerText();
    const expand = figure.getByRole('button', { name: 'Expand', exact: true });
    // The modal never locks the screen's orientation (FEA-CH-68).
    await page.evaluate(() => {
      window.__orientationLocks = 0;
      if (screen.orientation) screen.orientation.lock = () => { window.__orientationLocks += 1; return Promise.resolve(); };
    });
    await expand.focus(); await page.keyboard.press('Enter'); await settled(page);
    await page.evaluate(() => window.__richview.settled());
    const dialog = page.locator('dialog.rv-chart-dialog[open]');
    assert.equal(await dialog.count(), 1, 'Expansion opens one modal');
    assert.equal(await dialog.locator('canvas').count(), 1, 'The modal draws the chart');
    assert.equal((await page.evaluate(() => window.__richview.charts()))[0].expanded, true);
    const expandedBox = await plotBox();
    assert.ok(expandedBox.width >= before.figure.width * .7, `${theme}/${width}: the modal offers a plotting area`);
    if (width === 1440) assert.ok(expandedBox.width > before.figure.width + 20, `${theme}: expansion must provide real space`);
    assert.ok(expandedBox.width > expandedBox.height, 'Expansion offers a landscape plotting area');
    // A phone held upright lays the modal out in landscape and turns it a quarter (FEA-CH-68):
    // the screen's height becomes the plot's width, and title, actions and readout turn with it.
    // Touch makes every engine's pointer coarse. Below 360 px the turned dialog could not hold the plot, so the narrowest phones stay upright.
    assert.equal(expandedBox.turned, width === 420, `${theme}/${width}: the modal turns on a phone held upright, and only there`);
    assert.ok(expandedBox.turned || expandedBox.upright, `${theme}/${width}: the modal is upright or turned a quarter`);
    if (expandedBox.turned) {
      // The screen's height becomes the plot's width.
      assert.ok(expandedBox.width > 1000 * 0.8, `${theme}/${width}: the turned plot takes the screen's height as its width (${expandedBox.width})`);
      const fit = await dialog.evaluate(node => {
        const r = node.getBoundingClientRect(), head = node.querySelector('.rv-chart-dialog__head').getBoundingClientRect();
        return { inside: r.left >= -1 && r.top >= -1 && r.right <= innerWidth + 1 && r.bottom <= innerHeight + 1, headSideways: head.height > head.width, scroll: node.scrollHeight - node.clientHeight };
      });
      assert.ok(fit.inside, `${theme}/${width}: the turned modal fits the screen`);
      assert.ok(fit.headSideways, `${theme}/${width}: the title and Close turn with the chart`);
      // The turned dialog's short side is the plot's height: its actions sit in the head row,
      // beside the title and Close, overlapping neither (S27-866). Boxes are on the screen, so
      // the test holds through the turn.
      const row = await dialog.evaluate(node => {
        const box = el => { const r = el.getBoundingClientRect(); return { left: r.left, top: r.top, right: r.right, bottom: r.bottom }; };
        const text = document.createRange();
        text.selectNodeContents(node.querySelector('.rv-chart-dialog__title'));
        return {
          head: box(node.querySelector('.rv-chart-dialog__head')), title: box(text), close: box(node.querySelector('.rv-chart-dialog__close')),
          actions: [...node.querySelectorAll('.rv-chart--expanded > .rv-chart__actions .rv-chart__action')].filter(b => !b.hidden).map(box),
        };
      });
      const inside = (a, b) => a.left >= b.left - 1 && a.right <= b.right + 1 && a.top >= b.top - 1 && a.bottom <= b.bottom + 1;
      const apart = (a, b) => a.right <= b.left + 1 || b.right <= a.left + 1 || a.bottom <= b.top + 1 || b.bottom <= a.top + 1;
      assert.ok(row.actions.length && row.actions.every(a => inside(a, row.head) && apart(a, row.title) && apart(a, row.close)), `${theme}/${width}: the turned dialog's actions sit in its head row beside the title and Close: ${JSON.stringify(row)}`);
      // The actions keep clear of Close, by the head row's 1rem gap: a row placed by its right
      // edge once overhung it, CSV 3 px from Close (S27-870).
      const gap = Math.min(...row.actions.map(a => Math.max(row.close.left - a.right, a.left - row.close.right, row.close.top - a.bottom, a.top - row.close.bottom)));
      assert.ok(gap >= 12, `${theme}/${width}: the turned dialog's actions keep clear of Close (${Math.round(gap)} px): ${JSON.stringify(row)}`);
      assert.ok(fit.scroll <= 1, `${theme}/${width}: the turned modal does not scroll`);
      // The chart fills the height the dialog leaves it: a fill that fell back to a narrower, shorter plot would miss the room.
      const room = await dialog.evaluate(node => {
        const wide = selector => node.querySelector(selector).getBoundingClientRect().width;
        return innerWidth * 0.92 - 2 - wide('.rv-chart-dialog__head') - (wide('figure') - wide('.rv-chart__frame'));
      });
      assert.ok(Math.abs(expandedBox.height - room) <= 24, `${theme}/${width}: the turned plot (${expandedBox.height} px) fills the room the dialog leaves (${room} px)`);
    }
    assert.equal(await figure.locator('.rv-chart__table tbody').innerText(), table, 'The report keeps its Table view while expanded');
    // The expanded view looks like the report in every style: its Table like the inline one, with
    // numbers on the right in tabular figures in both, as in a table block, and its title like the
    // chart's (S27-872, FEA-CH-44, FEA-CH-45). The bar's table holds a category column and a
    // numeric value column.
    await dialog.getByRole('button', { name: 'Table', exact: true }).click(); await settled(page);
    await page.evaluate(() => window.__richview.settled());
    const looks = await page.evaluate(() => {
      const properties = ['text-align', 'font-family', 'font-weight', 'font-size', 'line-height', 'text-transform', 'letter-spacing', 'color', 'padding-top', 'padding-left', 'border-bottom-width', 'border-bottom-style', 'border-bottom-color', 'font-variant-numeric', 'background-image'];
      const read = node => node && Object.fromEntries(properties.map(name => [name, getComputedStyle(node).getPropertyValue(name)]));
      const look = root => {
        const table = root.querySelector('.rv-chart__table table');
        return {
          head: read(table?.querySelector('thead')), label: read(table?.querySelector('th:first-child')), number: read(table?.querySelector('th:last-child')),
          cell: read(table?.querySelector('tbody tr td:first-child')), value: read(table?.querySelector('tbody tr td:last-child')),
        };
      };
      const face = node => node && Object.fromEntries(['text-transform', 'font-family', 'font-weight', 'font-style', 'letter-spacing', 'color'].map(name => [name, getComputedStyle(node).getPropertyValue(name)]));
      return {
        inline: look(document.querySelector('.rv-report__body figure[data-rv-chart]')), expanded: look(document.querySelector('dialog.rv-chart-dialog[open]')),
        titles: { inline: face(document.querySelector('.rv-report__body figure[data-rv-chart] .rv-chart__title')), expanded: face(document.querySelector('dialog.rv-chart-dialog[open] .rv-chart-dialog__title')) },
      };
    });
    // The expanded view's title is the chart's title, set as the style sets chart titles.
    const titleDiffers = Object.keys(looks.titles.inline ?? {}).filter(name => looks.titles.inline[name] !== looks.titles.expanded?.[name]);
    if (!looks.titles.expanded || titleDiffers.length) failures.push(`${theme}/${width}: the expanded view's title is not set like the chart's title: ${JSON.stringify(titleDiffers.map(name => [name, looks.titles.inline[name], looks.titles.expanded?.[name]]))}`);
    for (const part of ['head', 'label', 'number', 'cell', 'value']) {
      const differ = Object.keys(looks.inline[part] ?? {}).filter(name => looks.inline[part][name] !== looks.expanded[part]?.[name]);
      if (!looks.expanded[part] || differ.length) failures.push(`${theme}/${width}: the expanded Table's ${part} does not look like the inline one: ${JSON.stringify(differ.map(name => [name, looks.inline[part][name], looks.expanded[part]?.[name]]))}`);
    }
    for (const view of ['inline', 'expanded']) for (const part of ['number', 'value']) {
      const look = looks[view];
      const cell = look[part] ?? {};
      if (!['right', 'end'].includes(cell['text-align']) || !cell['font-variant-numeric']?.includes('tabular-nums')) failures.push(`${theme}/${width}: the ${view} Table's numeric ${part === 'number' ? 'header' : 'cell'} is not right-aligned in tabular figures: ${cell['text-align']}, ${cell['font-variant-numeric']}`);
    }
    await dialog.getByRole('button', { name: 'Table', exact: true }).click(); await settled(page);
    await page.evaluate(() => window.__richview.settled());
    assert.equal(await dialog.evaluate(node => node.contains(document.activeElement)), true, 'Expansion moves focus into the modal');
    if (!(await dialogFrame.getAttribute('aria-label')).startsWith(chart.title)) failures.push(`${theme}/${width}: the expanded frame's aria-label lacks the chart title: ${await dialogFrame.getAttribute('aria-label')}`);
    if (width < 650) {
      const seen = await sweep();
      const at = text => [...seen.entries()].find(([key]) => key.includes(text))?.[1];
      assert.ok(at('0.03'), `Pointer inspection reads the second bar: ${[...seen.keys()]}`);
      assert.ok(at('0.12'), `Pointer inspection reads the fourth bar: ${[...seen.keys()]}`);
      const contained = await dialog.locator('.rv-chart__readout').evaluate(readout => {
        const a = readout.getBoundingClientRect(), b = readout.parentElement.getBoundingClientRect();
        return readout.hidden || (a.left >= b.left - 1 && a.right <= b.right + 1 && a.top >= b.top - 1 && a.bottom <= b.bottom + 1);
      });
      assert.ok(contained, 'Readouts stay inside the plot');
      await page.touchscreen.tap(at('0.12').x, at('0.12').y); await settled(page);
      assert.match(await dialog.locator('.rv-chart__readout:not([hidden])').innerText(), /0\.12/, 'Touch inspection reads the tapped bar');
      if (expandedBox.turned) {
        // Without the sweep's mapping: turned clockwise, the categories run down the screen and the
        // bars grow rightwards from a baseline near the screen's left, so a tap a quarter in from
        // the left lands on D near the middle and on F near the bottom. Zero-height A has no mark.
        const screenBox = await dialogFrame.boundingBox();
        for (const [share, value] of [[0.92, /^F\b[\s\S]*0\.21/], [0.59, /^D\b[\s\S]*0\.12/]]) {
          await page.touchscreen.tap(screenBox.x + screenBox.width / 4, screenBox.y + screenBox.height * share); await settled(page);
          assert.match(await dialog.locator('.rv-chart__readout:not([hidden])').innerText(), value, `${theme}/${width}: a tap at ${share} of the turned chart's height reads the bar there`);
        }
      }
      await page.screenshot({ path: join(h.dir, `${theme}-${width}-expanded.png`) });
      await page.setViewportSize({ width: 1000, height: width }); await settled(page);
      const rotated = await plotBox();
      assert.ok(rotated.width > rotated.height, 'Physical rotation retains landscape layout');
      assert.ok(rotated.upright, `${theme}/${width}: turning the phone sideways shows the layout upright`);
      const back = await dialogFrame.boundingBox();
      assert.ok(back.width > back.height, 'Upright on the screen once the phone is turned');
      await page.screenshot({ path: join(h.dir, `${theme}-${width}-landscape.png`) });
      await page.setViewportSize({ width, height: 1000 }); await settled(page);
    }
    const [pngDownload] = await Promise.all([page.waitForEvent('download'), dialog.getByRole('button', { name: 'Download PNG', exact: true }).click()]);
    assert.deepEqual([...readFileSync(await pngDownload.path()).subarray(0, 4)], [0x89, 0x50, 0x4e, 0x47]);
    const [csvDownload] = await Promise.all([page.waitForEvent('download'), dialog.getByRole('button', { name: 'CSV', exact: true }).click()]);
    assert.equal(readFileSync(await csvDownload.path(), 'utf8'), 'category,value\r\nA,0\r\nB,0.03\r\nC,0.06\r\nD,0.12\r\nE,0.18\r\nF,0.21\r\n');
    assert.equal(await page.evaluate(() => window.__orientationLocks), 0, 'The modal never locks the screen orientation');
    const contract = dialog.getByRole('button', { name: 'Close', exact: true });
    // Close shows itself at every width: an icon, or its word where actions are labelled. On a phone
    // the actions are icons alone, and a phone has no Escape key.
    const shown = await contract.evaluate(button => {
      const icon = button.querySelector('svg')?.getBoundingClientRect();
      const style = getComputedStyle(button);
      return { icon: icon ? [icon.width, icon.height] : null, text: parseFloat(style.fontSize) >= 10 && button.textContent.trim() !== '', color: style.color, visible: style.visibility !== 'hidden' && style.opacity !== '0' };
    });
    assert.ok(shown.visible && ((shown.icon && shown.icon[0] >= 12 && shown.icon[1] >= 12) || shown.text), `${theme}/${width}: Close shows an icon or its word: ${JSON.stringify(shown)}`);
    await contract.focus(); await page.keyboard.press(width < 650 ? 'Escape' : 'Space'); await settled(page);
    assert.equal(await page.getByRole('dialog').count(), 0, 'Close removes the modal and restores report access');
    const after = await figure.evaluate(geometry);
    assert.equal((await page.evaluate(() => window.__richview.charts()))[0].expanded, false);
    assert.equal(after.figure.width, before.figure.width);
    assert.equal(after.model, before.model, 'Close restores the exact settled geometry');
    assert.equal(await figure.locator('.rv-chart__table tbody').innerText(), table);
    assert.equal(await expand.evaluate(button => document.activeElement === button), true, 'Close returns focus to Expand');
    const lifecycle = await figure.evaluate(figure => ({ sameCanvas: window.__expansionCanvas === figure.querySelector('canvas.rv-chart__canvas'), charts: window.__richview.charts().length - window.__expansionCharts }));
    assert.deepEqual(lifecycle, { sameCanvas: true, charts: 0 });
    await figure.getByRole('button', { name: 'Table', exact: true }).click();
    await figure.screenshot({ path: join(h.dir, `${theme}-${width}.png`) });
    const facets = figures.nth(1);
    assert.equal((await page.evaluate(() => window.__richview.charts()))[1].geometry.panels.reduce((count, panel) => count + panel.observations.length, 0), rows.length, 'Every facet panel draws its bars');
    await facets.getByRole('button', { name: 'Expand', exact: true }).click(); await settled(page);
    assert.equal(await dialog.locator('canvas').count(), 1, 'One modal canvas holds all facet panels');
    await dialog.getByRole('button', { name: 'Close', exact: true }).click(); await settled(page);
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await figure.getByRole('button', { name: 'Expand', exact: true }).click(); await settled(page);
    assert.equal(await page.evaluate(() => document.getAnimations().length), 0, 'Reduced motion has no width or entrance animation');
    await dialog.getByRole('button', { name: 'Close', exact: true }).click(); await settled(page);
    await page.emulateMedia({ reducedMotion: 'no-preference' });
    evidence.cases.push({ theme, width, contracted: before.figure.width, expanded: expandedBox.width, turned: expandedBox.turned, lifecycle });
    if (selection.scope) completedCases.push(`theme/${theme}/${width}`);
  }
  if (wants('expanded-regression')) {
  // The turn's bounds (FEA-CH-68): portrait, touch, from 360 to 600 px wide, where the turned
  // chart is drawn larger, as these bars are; no dialog scrolls.
  for (const [width, height, turned] of [[359, 780, false], [360, 780, true], [360, 1000, true], [600, 1000, true], [601, 1000, false]]) {
    await page.setViewportSize({ width, height });
    await page.goto(`${server.base}/${themes[0]}/report.html`);
    await page.waitForFunction(() => window.__richview);
    await page.evaluate(() => window.__richview.settled());
    await page.locator('figure[data-rv-chart]').first().getByRole('button', { name: 'Expand', exact: true }).click(); await settled(page);
    await page.evaluate(() => window.__richview.settled());
    const box = await plotBox();
    if (turned) {
      assert.ok(box.turned, `${width}x${height}: the modal turns at the edge of the range`);
    } else {
      assert.ok(box.upright, `${width}x${height}: the modal stays upright`);
    }
    const scroll = await page.locator('dialog.rv-chart-dialog[open]').evaluate(node => node.scrollHeight - node.clientHeight);
    assert.ok(scroll <= 1, `${width}x${height}: the modal does not scroll (${scroll} px)`);
    await page.keyboard.press('Escape'); await settled(page);
  }
  // Every chart type on a phone held upright (FEA-CH-68): Expand never draws a chart narrower
  // than inline or more than a tenth shorter, and the dialog never scrolls. The dialog turns
  // only for a type whose x scale shows more with width, and only where the turned chart is
  // drawn larger, as a time series is. The drawn size is the box of the canvas's inked pixels,
  // in CSS pixels.
  inputs = await reports(h.dir, { only: ['every-block', 'playable-line-indicator', 'playable-gapminder'], extra: {} });
  const phone = await h.browser.newPage({ viewport: { width: 390, height: 844 }, hasTouch: true, reducedMotion: 'reduce' });
  watchErrors(phone, evidence.errors);
  await phone.goto(`${inputs.base}/every-block/report.html`);
  await settle(phone, 45_000, { models: false });
  const inked = root => {
    const canvas = root.querySelector('.rv-chart__canvas');
    const { data, width, height } = canvas.getContext('2d').getImageData(0, 0, canvas.width, canvas.height);
    const [r0, g0, b0] = data;
    let [x0, y0, x1, y1] = [width, height, -1, -1];
    for (let y = 0; y < height; y += 2) for (let x = 0; x < width; x += 2) {
      const p = (y * width + x) * 4;
      if (Math.abs(data[p] - r0) + Math.abs(data[p + 1] - g0) + Math.abs(data[p + 2] - b0) <= 30) continue;
      x0 = Math.min(x0, x); x1 = Math.max(x1, x); y0 = Math.min(y0, y); y1 = Math.max(y1, y);
    }
    const scale = canvas.width / parseFloat(canvas.style.width);
    return x1 < 0 ? [0, 0] : [(x1 - x0) / scale, (y1 - y0) / scale];
  };
  const tapExpand = async figure => {
    const box = await figure.getByRole('button', { name: 'Expand', exact: true }).boundingBox();
    await phone.touchscreen.tap(box.x + box.width / 2, box.y + box.height / 2);
    await phone.evaluate(() => window.__richview.settled()); await settled(phone);
    return phone.locator('dialog.rv-chart-dialog[open]');
  };
  const shownOf = dialog => dialog.evaluate(node => {
    const m = new DOMMatrixReadOnly(getComputedStyle(node).transform);
    return { turned: Math.abs(m.a) < 1e-6 && Math.abs(m.b - 1) < 1e-6, scroll: Math.max(node.scrollHeight - node.clientHeight, node.scrollWidth - node.clientWidth) };
  });
  // Targets: on a touch screen every one in the expanded view takes a tap anywhere in the 44 px
  // square about its centre, turned or upright (actions and Close, playback buttons, options,
  // select, scrubber, legend entries, the overview strip), so a phone reader meets one target
  // size there; the inline figure's actions keep 32 px (S27-870). The square is sampled where the
  // taps land on the screen, so a target may reach past its drawn box, a neighbour that covers
  // it counts against it, and a turned dialog reads the same.
  const targetsOf = root => [...root.querySelectorAll('.rv-chart__action, .rv-chart__control-button, .rv-chart__option, .rv-chart__select, .rv-chart__scrubber, .rv-chart__legend-entry, .rv-chart__overview')]
    .filter(b => b.offsetParent).map(b => {
      const r = b.getBoundingClientRect();
      const [x, y] = [r.left + r.width / 2, r.top + r.height / 2];
      const missed = [[-21, -21], [0, -21], [21, -21], [-21, 0], [21, 0], [-21, 21], [0, 21], [21, 21]]
        .filter(([dx, dy]) => !b.contains(document.elementFromPoint(x + dx, y + dy))).length;
      return [b.className.split(' ')[0], Math.round(r.width), Math.round(r.height), missed];
    });
  const actionsOf = root => [...root.querySelectorAll('.rv-chart__action')].filter(b => b.offsetParent).map(b => [b.offsetWidth, b.offsetHeight]);
  const phoneFigures = phone.locator('.rv-report__body figure[data-rv-chart]');
  const types = [];
  for (let i = 0; i < await phoneFigures.count(); i++) {
    const figure = phoneFigures.nth(i);
    const type = await figure.evaluate(f => JSON.parse(f.querySelector('script[data-rv-payload]').textContent).block.type);
    const expandButton = figure.getByRole('button', { name: 'Expand', exact: true });
    await expandButton.scrollIntoViewIfNeeded();
    const inline = await figure.evaluate(inked);
    const inlineTargets = await figure.evaluate(actionsOf);
    const dialog = await tapExpand(figure);
    const shown = await shownOf(dialog);
    const expanded = await dialog.evaluate(inked);
    const dialogTargets = await dialog.evaluate(targetsOf);
    if (!dialogTargets.length || dialogTargets.some(([, , , missed]) => missed)) failures.push(`phone ${type}${shown.turned ? ' (turned)' : ''}: these expanded-view targets miss taps in their 44 px square (class, width, height, missed of 8): ${JSON.stringify(dialogTargets.filter(([, , , missed]) => missed))}`);
    if (!inlineTargets.length || inlineTargets.some(([w, h]) => w !== 32 || h !== 32)) failures.push(`phone ${type}: the inline figure's action targets are ${JSON.stringify(inlineTargets)}, not 32 px`);
    types.push({ type, turned: shown.turned });
    const size = ([w, h]) => `${Math.round(w)}x${Math.round(h)}`;
    if (expanded[0] < inline[0] - 2 || expanded[1] < 0.9 * inline[1] - 2) failures.push(`phone ${type}${shown.turned ? ' (turned)' : ''}: Expand draws ${size(expanded)}, inline ${size(inline)}`);
    if (shown.scroll > 1) failures.push(`phone ${type}${shown.turned ? ' (turned)' : ''}: the expanded dialog scrolls ${shown.scroll} px`);
    await phone.keyboard.press('Escape'); await settled(phone);
  }
  // Reader controls: the playable examples' charts (a line and a scatter with a legend) on the
  // same phone. Every target in the expanded view takes the taps of its 44 px square, and the
  // dialog does not scroll (S27-870).
  evidence.controls = [];
  const player = await h.browser.newPage({ viewport: { width: 390, height: 844 }, hasTouch: true, reducedMotion: 'reduce' });
  watchErrors(player, evidence.errors);
  for (const name of ['playable-line-indicator', 'playable-gapminder']) {
    await player.goto(`${inputs.base}/${name}/report.html`);
    await settle(player, 45_000, { models: false });
    const figure = player.locator('.rv-report__body figure[data-rv-chart]').first();
    const expandButton = figure.getByRole('button', { name: 'Expand', exact: true });
    await expandButton.scrollIntoViewIfNeeded();
    const inline = await figure.evaluate(inked);
    const box = await expandButton.boundingBox();
    await player.touchscreen.tap(box.x + box.width / 2, box.y + box.height / 2);
    await player.evaluate(() => window.__richview.settled()); await settled(player);
    const dialog = player.locator('dialog.rv-chart-dialog[open]');
    const shown = await shownOf(dialog);
    const expanded = await dialog.evaluate(inked);
    const targets = await dialog.evaluate(targetsOf);
    const kinds = new Set(targets.map(([kind]) => kind));
    const label = `phone ${name}${shown.turned ? ' (turned)' : ''}`;
    if (!['rv-chart__control-button', 'rv-chart__option', 'rv-chart__scrubber'].every(kind => kinds.has(kind))) failures.push(`${label}: the expanded view lacks its reader controls: ${[...kinds]}`);
    if (targets.some(([, , , missed]) => missed)) failures.push(`${label}: these expanded-view targets miss taps in their 44 px square (class, width, height, missed of 8): ${JSON.stringify(targets.filter(([, , , missed]) => missed))}`);
    if (shown.scroll > 1) failures.push(`${label}: the expanded dialog scrolls ${shown.scroll} px`);
    evidence.controls.push({ name, turned: shown.turned, inline, expanded, targets });
    await player.keyboard.press('Escape'); await settled(player);
  }
  await player.close();
  // A chart in a style without a card around it (Scientific) fills the report's column on a
  // phone, so an upright expanded view gives it at least that width: the dialog's inset and the
  // figure's padding together keep the column's 1rem margin (S27-873).
  for (const [width, height] of [[360, 780], [390, 844], [430, 932]]) {
    const narrow = await h.browser.newPage({ viewport: { width, height }, hasTouch: true, reducedMotion: 'reduce' });
    watchErrors(narrow, evidence.errors);
    for (const name of ['playable-line-indicator', 'playable-gapminder']) {
      await narrow.goto(`${inputs.base}/${name}/report.html`);
      await settle(narrow, 45_000, { models: false });
      const figure = narrow.locator('.rv-report__body figure[data-rv-chart]').first();
      const expandButton = figure.getByRole('button', { name: 'Expand', exact: true });
      await expandButton.scrollIntoViewIfNeeded();
      const inline = await figure.evaluate(inked);
      const box = await expandButton.boundingBox();
      await narrow.touchscreen.tap(box.x + box.width / 2, box.y + box.height / 2);
      await narrow.evaluate(() => window.__richview.settled()); await settled(narrow);
      const dialog = narrow.locator('dialog.rv-chart-dialog[open]');
      const shown = await shownOf(dialog);
      const expanded = await dialog.evaluate(inked);
      if (!shown.turned && expanded[0] < inline[0] - 2) failures.push(`upright ${width}x${height} ${name}: Expand draws the chart ${Math.round(expanded[0])} px wide, inline ${Math.round(inline[0])}`);
      await narrow.keyboard.press('Escape'); await settled(narrow);
    }
    await narrow.close();
  }
  // In a room wider than tall, an expanded chart narrowed to fit the dialog's height keeps its
  // legend and note in a column beside its plot, when the chart fits so with its plot no smaller
  // (S27-875). The column stands right of the plot, top-aligned with it, the two centred in the
  // figure and inside it. A pie's key, which the chart draws itself, stands beside its disk: the
  // pie is drawn wider than tall. No other window puts anything beside the plot.
  const asideOf = node => {
    const figure = node.querySelector('.rv-chart--expanded'), column = figure.querySelector(':scope > .rv-chart__aside');
    if (!column) return null;
    const box = el => { const r = el.getBoundingClientRect(); return { left: r.left, top: r.top, right: r.right, bottom: r.bottom }; };
    const style = getComputedStyle(figure), outer = box(figure);
    const content = { left: outer.left + parseFloat(style.paddingLeft), right: outer.right - parseFloat(style.paddingRight) };
    // The column's first line: the column box starts at the plot's edge and pads its gap.
    const first = box(column.firstElementChild);
    return { column: { ...box(column), left: first.left }, frame: box(figure.querySelector(':scope > .rv-chart__frame')), content, holds: [...column.children].map(c => c.className) };
  };
  const besideFailures = (label, aside) => {
    if (!aside) return [];
    const { column: a, frame: f, content: c } = aside, found = [];
    if (a.left < f.right + 16 || Math.abs(a.top - f.top) > 1) found.push(`${label}: the column beside the plot does not stand right of it, top-aligned: ${JSON.stringify(aside)}`);
    if (f.left < c.left - 1 || a.right > c.right + 1 || Math.abs((f.left - c.left) - (c.right - a.right)) > 2) found.push(`${label}: the plot and the column beside it are not centred inside the figure: ${JSON.stringify(aside)}`);
    return found;
  };
  // A phone held sideways (S27-871): its dialog is already wide and short, the turned one's
  // shape, so it takes that layout without the turn. The actions sit in the title row beside
  // Close, so no action row takes the plot's height, and the dialog keeps its full width, so the
  // title keeps its line. Every chart opens at the dialog's top and the dialog does not scroll;
  // each chart is drawn at least as wide as inline, or fills the height the dialog leaves it, or
  // with its legend and note beside it takes the width they leave (two fifths at most, S27-875).
  // A chart that fits the room at no width scrolls: its height does not follow its width (a
  // gantt's rows, a flow). It is drawn at the engine's least width, 240 px, or at its own where
  // narrowing would not lower it, never squeezed below. The pie, splom, waffle and sunburst, whose
  // plot, legend and note were taller than the room at 240 px, fit with them beside the plot.
  // The smallest phone held sideways, 568 by 320, leaves the chart about 230 px: there a
  // fourth of the charts are taller than that at any width, the pie fits with its key beside its
  // disk, and the contour and pareto are drawn across the dialog at their flattest (S27-875).
  const tooTall = {
    '568x320': ['gauge', 'ribbon', 'gantt', 'splom', 'sankey', 'alluvial', 'contour', 'flow', 'surface', 'sunburst', 'pareto', 'rose', 'waffle'],
    '844x390': ['gantt', 'flow'], '932x430': ['gantt'],
  };
  // The types that more width only enlarges: the engine type table's `Wider::Enlarges`. Every
  // other type shows more with width, and flattens in a short, wide window.
  const enlarges = ['sankey', 'alluvial', 'chord', 'flow', 'network', 'treemap', 'sunburst', 'icicle', 'tree', 'packed-bubble', 'pie', 'rose', 'radar', 'gauge', 'waffle', 'venn', 'map', 'wordcloud', 'funnel', 'surface', 'scatter3d', 'bar3d'];
  // A short, wide dialog takes 96 % of the window's height, as of its width. A chart that
  // flattens and fits the room at no width, with or without its legend beside the plot, is drawn
  // across the dialog at its flattest: both scroll, and the wide one reads better. A splom keeps
  // its narrowed layout, since its square cells grow with the width and across it would be
  // taller still (S27-875).
  const keepsNarrowed = ['splom'];
  const shortWideFailures = (label, s, type) => {
    const found = [];
    if (Math.abs(s.tallest - s.screenTall * 0.96) > 1) found.push(`${label}: the short, wide dialog may be ${s.tallest} px high, not 96 % of the window's ${s.screenTall}`);
    if (s.scroll > 1 && !enlarges.includes(type) && !keepsNarrowed.includes(type) && s.plotWidth < s.full - 1) found.push(`${label}: fitting at no width, the chart is drawn ${s.plotWidth} px wide, not across the dialog (${Math.round(s.full)}) at its flattest`);
    return found;
  };
  evidence.sideways = [];
  for (const [width, height] of [[568, 320], [844, 390], [932, 430]]) {
    const side = await h.browser.newPage({ viewport: { width, height }, hasTouch: true, reducedMotion: 'reduce' });
    watchErrors(side, evidence.errors);
    await side.goto(`${inputs.base}/every-block/report.html`);
    await settle(side, 45_000, { models: false });
    const figures = side.locator('.rv-report__body figure[data-rv-chart]');
    for (let i = 0; i < await figures.count(); i++) {
      const figure = figures.nth(i);
      const type = await figure.evaluate(f => JSON.parse(f.querySelector('script[data-rv-payload]').textContent).block.type);
      const expandButton = figure.getByRole('button', { name: 'Expand', exact: true });
      await expandButton.scrollIntoViewIfNeeded();
      const inline = await figure.evaluate(inked);
      const box = await expandButton.boundingBox();
      await side.touchscreen.tap(box.x + box.width / 2, box.y + box.height / 2);
      await side.evaluate(() => window.__richview.settled()); await settled(side);
      const dialog = side.locator('dialog.rv-chart-dialog[open]');
      const expanded = await dialog.evaluate(inked);
      const state = await dialog.evaluate(node => {
        const box = el => { const r = el.getBoundingClientRect(); return { left: r.left, top: r.top, right: r.right, bottom: r.bottom }; };
        const text = document.createRange();
        text.selectNodeContents(node.querySelector('.rv-chart-dialog__title'));
        const head = node.querySelector('.rv-chart-dialog__head');
        const figure = node.querySelector('.rv-chart--expanded');
        const frame = node.querySelector('.rv-chart__frame');
        return {
          scroll: node.scrollHeight - node.clientHeight, top: node.scrollTop, wide: node.offsetWidth, screen: innerWidth,
          head: box(head), title: box(text), close: box(node.querySelector('.rv-chart-dialog__close')),
          actions: [...node.querySelectorAll('.rv-chart--expanded > .rv-chart__actions .rv-chart__action')].filter(b => b.offsetParent).map(box),
          room: innerHeight * 0.96 - 2 - head.offsetHeight - (figure.offsetHeight - frame.offsetHeight),
          tallest: parseFloat(getComputedStyle(node).maxHeight), screenTall: innerHeight,
          plot: frame.offsetHeight, plotWidth: frame.offsetWidth,
          full: figure.clientWidth - parseFloat(getComputedStyle(figure).paddingLeft) - parseFloat(getComputedStyle(figure).paddingRight),
        };
      });
      const aside = await dialog.evaluate(asideOf);
      const label = `sideways ${width}x${height} ${type}`;
      const targets = await dialog.evaluate(targetsOf);
      if (targets.some(([, , , missed]) => missed)) failures.push(`${label}: these expanded-view targets miss taps in their 44 px square (class, width, height, missed of 8): ${JSON.stringify(targets.filter(([, , , missed]) => missed))}`);
      const inside = (a, b) => a.left >= b.left - 1 && a.right <= b.right + 1 && a.top >= b.top - 1 && a.bottom <= b.bottom + 1;
      const apart = (a, b) => a.right <= b.left + 1 || b.right <= a.left + 1 || a.bottom <= b.top + 1 || b.bottom <= a.top + 1;
      const fitsNowhere = tooTall[`${width}x${height}`].includes(type);
      if (state.scroll > 1 && !fitsNowhere) failures.push(`${label}: the dialog scrolls ${state.scroll} px`);
      if (state.scroll > 1 && fitsNowhere && state.plotWidth > 241 && state.plotWidth < state.full - 1) failures.push(`${label}: the dialog scrolls ${state.scroll} px with the chart drawn ${state.plotWidth} px wide, neither the least width (240) nor its own (${Math.round(state.full)})`);
      if (state.plotWidth < 239) failures.push(`${label}: the chart is drawn ${state.plotWidth} px wide, below the engine's least width (240)`);
      if (state.top > 0) failures.push(`${label}: the dialog opens scrolled by ${state.top} px`);
      if (state.wide < state.screen * 0.96 - 2) failures.push(`${label}: the dialog is ${state.wide} px wide, not the screen's 96 %`);
      if (!state.actions.length || !state.actions.every(a => inside(a, state.head) && apart(a, state.title) && apart(a, state.close))) failures.push(`${label}: the actions are not in the title row beside the title and Close`);
      // Beside its legend and note, a plot as wide as they leave it is as large as the dialog allows.
      const spans = !!aside && aside.column.right - aside.frame.left >= aside.content.right - aside.content.left - 2;
      if (expanded[0] < inline[0] - 2 && state.plot < state.room - 24 && !spans) failures.push(`${label}: Expand draws ${Math.round(expanded[0])}x${Math.round(expanded[1])}, inline ${Math.round(inline[0])}x${Math.round(inline[1])}, and the plot leaves ${Math.round(state.room - state.plot)} px of the dialog's height`);
      failures.push(...besideFailures(label, aside), ...shortWideFailures(label, state, type));
      if (type === 'pie' && expanded[0] < expanded[1] + 60) failures.push(`${label}: the pie's key does not stand beside its disk: it draws ${Math.round(expanded[0])}x${Math.round(expanded[1])}`);
      evidence.sideways.push({ width, height, type, inline, expanded, scroll: state.scroll, top: state.top, wide: state.wide, room: state.room, plot: [state.plotWidth, state.plot], aside: aside?.holds ?? null });
      await side.keyboard.press('Escape'); await settled(side);
    }
    await side.close();
  }
  // On a laptop or a tablet the expanded dialog does not scroll either: a chart too tall for its
  // room narrows to the widest width at which it fits in the dialog that width gives, its title
  // and legend wrapped to it, never in one step scaled by the room, which leaves a chart with axes
  // or a legend a few pixels too tall (S27-874). A chart that fits at no width is named for its
  // window: the splom, whose panels keep a least size, at 1280 by 720.
  // A short, wide window takes the sideways phone's layout whatever the pointer (S27-876): a
  // laptop zoomed to 200 % gives 640 by 360, or 720 by 450, CSS px. With a mouse its actions sit
  // in the title row beside the title and Close as icons of the mouse's 32 px, so the plot keeps
  // the room an action row took. With the dialog 96 % of the window high and legends and notes
  // beside narrowed plots (S27-875), at 640 by 360 the charts named are those whose height does
  // not follow their width (the gantt's rows, a flow); at 720 by 450, none.
  const tooTallOn = {
    '1440x900': [], '1280x720': ['splom'], '1024x768': [],
    '640x360': ['gantt', 'flow'],
    '720x450': [],
  };
  evidence.windows = [];
  for (const [width, height, touch] of [[1440, 900, false], [1280, 720, false], [1024, 768, true], [640, 360, false], [720, 450, false]]) {
    const page = await h.browser.newPage({ viewport: { width, height }, hasTouch: touch, reducedMotion: 'reduce' });
    watchErrors(page, evidence.errors);
    await page.goto(`${inputs.base}/every-block/report.html`);
    await settle(page, 45_000, { models: false });
    const figures = page.locator('.rv-report__body figure[data-rv-chart]');
    for (let i = 0; i < await figures.count(); i++) {
      const figure = figures.nth(i);
      const type = await figure.evaluate(f => JSON.parse(f.querySelector('script[data-rv-payload]').textContent).block.type);
      const expandButton = figure.getByRole('button', { name: 'Expand', exact: true });
      await expandButton.scrollIntoViewIfNeeded();
      const box = await expandButton.boundingBox();
      if (touch) await page.touchscreen.tap(box.x + box.width / 2, box.y + box.height / 2);
      else { await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2); await page.mouse.move(1, 1); }
      await page.evaluate(() => window.__richview.settled()); await settled(page);
      const dialog = page.locator('dialog.rv-chart-dialog[open]');
      const state = await dialog.evaluate(node => {
        const figure = node.querySelector('.rv-chart--expanded');
        const style = getComputedStyle(figure);
        return {
          scroll: node.scrollHeight - node.clientHeight, top: node.scrollTop, plot: node.querySelector('.rv-chart__frame').offsetWidth,
          full: figure.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight),
          tallest: parseFloat(getComputedStyle(node).maxHeight), screenTall: innerHeight,
        };
      });
      const label = `${width}x${height} ${touch ? 'touch' : 'mouse'} ${type}`;
      const aside = await dialog.evaluate(asideOf);
      if (aside && !(height <= 600 && width > height)) failures.push(`${label}: the legend and note stand beside the plot outside a short, wide window`);
      failures.push(...besideFailures(label, aside));
      state.aside = aside?.holds ?? null;
      if (height <= 600 && width > height) {
        const head = await dialog.evaluate(node => {
          const box = el => { const r = el.getBoundingClientRect(); return { left: r.left, top: r.top, right: r.right, bottom: r.bottom, width: r.width, height: r.height }; };
          const text = document.createRange();
          text.selectNodeContents(node.querySelector('.rv-chart-dialog__title'));
          return {
            head: box(node.querySelector('.rv-chart-dialog__head')), title: box(text), close: box(node.querySelector('.rv-chart-dialog__close')),
            actions: [...node.querySelectorAll('.rv-chart--expanded > .rv-chart__actions .rv-chart__action')].filter(b => b.offsetParent).map(box),
          };
        });
        const inside = (a, b) => a.left >= b.left - 1 && a.right <= b.right + 1 && a.top >= b.top - 1 && a.bottom <= b.bottom + 1;
        const apart = (a, b) => a.right <= b.left + 1 || b.right <= a.left + 1 || a.bottom <= b.top + 1 || b.bottom <= a.top + 1;
        if (!head.actions.length || !head.actions.every(a => inside(a, head.head) && apart(a, head.title) && apart(a, head.close))) failures.push(`${label}: the actions are not in the title row beside the title and Close: ${JSON.stringify(head)}`);
        const sizes = [...head.actions, head.close].map(a => [Math.round(a.width), Math.round(a.height)]);
        if (!touch && sizes.some(([w, hh]) => w !== 32 || hh !== 32)) failures.push(`${label}: the title row's targets are not the mouse's 32 px: ${JSON.stringify(sizes)}`);
        failures.push(...shortWideFailures(label, { ...state, plotWidth: state.plot }, type));
      }
      if (state.scroll > 1 && !tooTallOn[`${width}x${height}`].includes(type)) failures.push(`${label}: the expanded dialog scrolls ${state.scroll} px`);
      if (state.top > 0) failures.push(`${label}: the dialog opens scrolled by ${state.top} px`);
      if (state.plot < 239) failures.push(`${label}: the chart is drawn ${state.plot} px wide, below the engine's least width (240)`);
      evidence.windows.push({ width, height, touch, type, ...state });
      await page.keyboard.press('Escape'); await settled(page);
    }
    await page.close();
  }
  // Where the actions are icons alone, each shows its name on hover and on keyboard focus, in view
  // and in a font a reader can read, and its accessible name stays the word (S27-876): in a short,
  // wide window's title row, and under 520 px inline and in the upright dialog. Moving the pointer
  // away takes the name away; a tap shows none, since a touch screen keeps hover after the tap.
  const nameOf = (button, scope) => button.evaluate((b, scope) => {
    const span = b.querySelector('span'), r = span.getBoundingClientRect(), style = getComputedStyle(span);
    const room = scope === 'dialog' ? b.closest('dialog').getBoundingClientRect() : { left: 0, top: 0, right: document.documentElement.clientWidth, bottom: innerHeight };
    return {
      name: b.textContent.trim(), shown: style.opacity === '1' && style.visibility !== 'hidden' && r.width > 8 && r.height > 8 && parseFloat(style.fontSize) >= 11,
      inside: r.left >= room.left - .5 && r.right <= room.right + .5 && r.top >= room.top - .5 && r.bottom <= room.bottom + .5, box: [r.left, r.top, r.width, r.height].map(Math.round),
    };
  }, scope);
  const namesShow = async (page, root, scope, where) => {
    const buttons = root.locator('.rv-chart__action:visible');
    const seen = [];
    for (let i = 0; i < await buttons.count(); i++) {
      const button = buttons.nth(i);
      const box = await button.boundingBox();
      await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
      const hovered = await nameOf(button, scope);
      await page.mouse.move(box.x + box.width / 2, box.y + box.height + 120);
      const left = await nameOf(button, scope);
      seen.push({ ...hovered, left: left.shown });
      if (!hovered.shown || !hovered.inside) failures.push(`${where}: hovering ${hovered.name} does not show its name in view: ${JSON.stringify(hovered)}`);
      if (left.shown) failures.push(`${where}: ${hovered.name}'s name stays after the pointer leaves`);
      if (await root.getByRole('button', { name: hovered.name, exact: true }).count() !== 1) failures.push(`${where}: no button is named ${hovered.name}`);
    }
    // Keyboard focus, from the chart back through the actions, shows each one's name. WebKit, as
    // Safari, moves the focus to buttons with Option+Tab.
    await page.mouse.move(1, 1);
    await root.locator('.rv-chart__frame').first().focus();
    for (let k = 0; k < 8; k++) {
      await page.keyboard.press(h.engine === 'webkit' ? 'Alt+Shift+Tab' : 'Shift+Tab');
      const focused = await page.evaluate(() => document.activeElement.classList.contains('rv-chart__action'));
      if (!focused) continue;
      const named = await nameOf(await page.evaluateHandle(() => document.activeElement), scope);
      seen.push({ ...named, focus: true });
      if (!named.shown || !named.inside) failures.push(`${where}: keyboard focus on ${named.name} does not show its name in view: ${JSON.stringify(named)}`);
    }
    if (!seen.some(n => n.focus)) failures.push(`${where}: Shift+Tab from the chart reaches no action`);
    return seen;
  };
  evidence.names = {};
  {
    const zoomed = await h.browser.newPage({ viewport: { width: 640, height: 360 }, reducedMotion: 'reduce' });
    watchErrors(zoomed, evidence.errors);
    await zoomed.goto(`${inputs.base}/every-block/report.html`);
    await settle(zoomed, 45_000, { models: false });
    const expand = zoomed.locator('.rv-report__body figure[data-rv-chart]').first().getByRole('button', { name: 'Expand', exact: true });
    await expand.scrollIntoViewIfNeeded();
    await expand.click(); await zoomed.evaluate(() => window.__richview.settled()); await settled(zoomed);
    evidence.names['640x360 dialog'] = await namesShow(zoomed, zoomed.locator('dialog.rv-chart-dialog[open]'), 'dialog', '640x360 mouse dialog');
    await zoomed.close();
    const narrow = await h.browser.newPage({ viewport: { width: 480, height: 800 }, reducedMotion: 'reduce' });
    watchErrors(narrow, evidence.errors);
    await narrow.goto(`${inputs.base}/every-block/report.html`);
    await settle(narrow, 45_000, { models: false });
    const figure = narrow.locator('.rv-report__body figure[data-rv-chart]').first();
    await figure.scrollIntoViewIfNeeded();
    evidence.names['480x800 inline'] = await namesShow(narrow, figure, 'page', '480x800 mouse inline');
    await figure.getByRole('button', { name: 'Expand', exact: true }).click(); await narrow.evaluate(() => window.__richview.settled()); await settled(narrow);
    evidence.names['480x800 dialog'] = await namesShow(narrow, narrow.locator('dialog.rv-chart-dialog[open]'), 'dialog', '480x800 mouse dialog');
    await narrow.close();
    const side = await h.browser.newPage({ viewport: { width: 844, height: 390 }, hasTouch: true, reducedMotion: 'reduce' });
    watchErrors(side, evidence.errors);
    await side.goto(`${inputs.base}/every-block/report.html`);
    await settle(side, 45_000, { models: false });
    const tapped = side.locator('.rv-report__body figure[data-rv-chart]').first().getByRole('button', { name: 'Expand', exact: true });
    await tapped.scrollIntoViewIfNeeded();
    const at = await tapped.boundingBox();
    await side.touchscreen.tap(at.x + at.width / 2, at.y + at.height / 2);
    await side.evaluate(() => window.__richview.settled()); await settled(side);
    const table = side.locator('dialog.rv-chart-dialog[open]').getByRole('button', { name: 'Table', exact: true });
    const t = await table.boundingBox();
    await side.touchscreen.tap(t.x + t.width / 2, t.y + t.height / 2); await settled(side);
    const named = await nameOf(table, 'dialog');
    evidence.names['844x390 tap'] = named;
    if (named.shown) failures.push(`844x390 touch: a tap on Table shows its name, which a touch screen would keep: ${JSON.stringify(named)}`);
    await side.close();
  }
  // The Table's buttons that have words and no icon, "Show all" and "Show 1,000 more
  // observations", keep their words where the actions are icons alone: on a phone inline, and in
  // the upright and short, wide dialogs, whose rule the turned dialog shares (S27-877). The
  // gapminder's Table lists one year and offers "Show all". Its number cells never break inside
  // their value, as "$28 thousand" did on a phone: the Table scrolls in its own box instead, and
  // the page never scrolls sideways (S27-879).
  // A table box whose content is wider than it shows so: on each side that has more, an edge cue
  // fades the content into the surface the box sits on, and it leaves at that end, so a value the
  // box cuts is never read as whole. The box is a focusable region named for its table, so a
  // keyboard scrolls it (WCAG 2.1.1); in the dialog too, whose Table's caption read only "Data
  // for". Read at the box's start, at its end, and after ArrowRight from the start with the box
  // focused. What stands with the table (a status line, Show all, a long table's "Showing 10 of
  // 64 rows") stays in view throughout, while a long table's folded rows scroll with its columns.
  const cuesOf = async (page, box) => {
    const read = () => box.evaluate(node => {
      const cue = side => { const c = getComputedStyle(node, side); return { shown: c.content !== 'none' && parseFloat(c.opacity) > .99 && parseFloat(c.width) >= 16, opacity: c.opacity, width: c.width }; };
      // What stands with the table, in the box's visible width: a chart Table's status line and
      // Show all, a long table's "Showing 10 of 64 rows". A long table's folded rows scroll with
      // its columns: their fold starts where the table does.
      const b = node.getBoundingClientRect();
      const view = [b.left + node.clientLeft, b.left + node.clientLeft + node.clientWidth];
      const kept = [...node.querySelectorAll(':scope > :is(p, button), :scope > details > summary')].filter(c => c.getClientRects().length).map(c => {
        const r = c.getBoundingClientRect();
        return { what: c.textContent.trim().slice(0, 40), inside: r.left >= view[0] - 1 && r.right <= view[1] + 1 };
      });
      const table = node.querySelector(':scope > table')?.getBoundingClientRect();
      const folds = [...node.querySelectorAll(':scope > details')].map(d => ({ fold: Math.round(d.getBoundingClientRect().left), table: Math.round(table?.left ?? 0) }));
      return { wider: node.scrollWidth > node.clientWidth + 1, at: node.scrollLeft, left: cue('::before'), right: cue('::after'), role: node.getAttribute('role'), tabindex: node.getAttribute('tabindex'), name: node.getAttribute('aria-label'), kept, folds };
    });
    const frame = () => page.evaluate(() => new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r))));
    await box.evaluate(node => { node.scrollLeft = 0; }); await frame();
    const start = await read();
    await box.evaluate(node => { node.scrollLeft = node.scrollWidth; }); await frame();
    const end = await read();
    await box.evaluate(node => { node.scrollLeft = 0; node.focus({ preventScroll: true }); }); await frame();
    // Held for a moment, as a finger is: WebKit scrolls from its keydown with an animation that
    // a key released at once cancels before its first step.
    await page.keyboard.down('ArrowRight'); await page.waitForTimeout(300); await page.keyboard.up('ArrowRight'); await page.waitForTimeout(250);
    const keyed = await box.evaluate(node => node.scrollLeft);
    await box.evaluate(node => node.blur());
    return { start, end, keyed };
  };
  const cueFailures = (label, c, name) => {
    if (!c.start.wider) return [];
    const found = [];
    if (!c.start.right.shown || c.start.left.shown) found.push(`${label}: at its start, the box that scrolls shows no cue on the right only: ${JSON.stringify(c.start)}`);
    if (!c.end.left.shown || c.end.right.shown) found.push(`${label}: at its end, the box that scrolls shows no cue on the left only: ${JSON.stringify(c.end)}`);
    if (c.start.role !== 'region' || c.start.tabindex !== '0' || !c.start.name || (name && !c.start.name.includes(name))) found.push(`${label}: the box that scrolls is not a focusable region named for its table: ${JSON.stringify(c.start)}`);
    if (!(c.keyed > 0)) found.push(`${label}: ArrowRight in the focused box does not scroll it`);
    // The status line and Show all stay in place while only the columns scroll: at the end they
    // read ': 5 of 64 observations;' and 'observations' before (S27-879, the lead's 08:59 CET go).
    for (const [end, at] of [['start', c.start], ['end', c.end]]) {
      const out = at.kept.filter(k => !k.inside);
      if (out.length) found.push(`${label}: at its ${end}, what stands with the table leaves the box's view: ${JSON.stringify(out)}`);
      const pinned = at.folds.filter(f => Math.abs(f.fold - f.table) > 1);
      if (pinned.length) found.push(`${label}: at its ${end}, a long table's folded rows do not start where its columns do, so they do not scroll with them: ${JSON.stringify(pinned)}`);
    }
    return found;
  };
  evidence.tableButtons = [];
  for (const [width, height, touch] of [[390, 844, true], [844, 390, true], [640, 360, false]]) {
    for (const report of ['playable-gapminder']) {
      const page = await h.browser.newPage({ viewport: { width, height }, hasTouch: touch, reducedMotion: 'reduce' });
      watchErrors(page, evidence.errors);
      await page.goto(`${inputs.base}/${report}/report.html`);
      await settle(page, 45_000, { models: false });
      const press = async button => {
        await button.scrollIntoViewIfNeeded();
        const b = await button.boundingBox();
        if (touch) await page.touchscreen.tap(b.x + b.width / 2, b.y + b.height / 2);
        else await page.mouse.click(b.x + b.width / 2, b.y + b.height / 2);
        await page.evaluate(() => window.__richview.settled()); await settled(page);
      };
      const words = root => root.evaluate(node => [...node.querySelectorAll('.rv-chart__table button')].map(b => {
        const text = document.createRange();
        text.selectNodeContents(b);
        const r = text.getBoundingClientRect();
        return { text: b.textContent.trim(), font: parseFloat(getComputedStyle(b).fontSize), box: [Math.round(Math.max(r.width, r.height)), Math.round(Math.min(r.width, r.height))] };
      }));
      const broken = root => root.evaluate(node => ({
        cells: [...node.querySelectorAll('.rv-chart__table td.num')].filter(cell => {
          const text = document.createRange();
          text.selectNodeContents(cell);
          return new Set([...text.getClientRects()].filter(r => r.width > 0).map(r => Math.round(r.top))).size > 1;
        }).map(cell => cell.textContent.trim()),
        sideways: document.documentElement.scrollWidth - innerWidth,
      }));
      const figure = page.locator('.rv-report__body figure[data-rv-chart]').first();
      await press(figure.getByRole('button', { name: 'Table', exact: true }));
      const inline = await words(figure);
      const inlineBroken = await broken(figure);
      const title = await figure.evaluate(f => f.querySelector('.rv-chart__title')?.textContent.trim() ?? '');
      const inlineCues = await cuesOf(page, figure.locator('.rv-chart__table'));
      await press(figure.getByRole('button', { name: 'Table', exact: true }));
      await press(figure.getByRole('button', { name: 'Expand', exact: true }));
      const dialog = page.locator('dialog.rv-chart-dialog[open]');
      const layout = await dialog.evaluate(node => (node.hasAttribute('data-rv-turned') ? 'turned' : node.hasAttribute('data-rv-short-wide') ? 'short, wide' : 'upright'));
      await press(dialog.getByRole('button', { name: 'Table', exact: true }));
      const expanded = await words(dialog);
      const expandedBroken = await broken(dialog);
      const expandedCues = await cuesOf(page, dialog.locator('.rv-chart__table'));
      const label = `${width}x${height} ${touch ? 'touch' : 'mouse'} ${report}`;
      failures.push(...cueFailures(`${label} inline Table`, inlineCues, title), ...cueFailures(`${label} ${layout} dialog's Table`, expandedCues, title));
      for (const [where, buttons] of [['inline', inline], [`${layout} dialog`, expanded]]) {
        if (!buttons.length) failures.push(`${label} ${where}: the Table offers no "Show all" to check`);
        for (const b of buttons) if (b.font < 10 || b.box[0] < 40 || b.box[1] < 8) failures.push(`${label} ${where}: the Table's "${b.text}" does not show its words: ${JSON.stringify(b)}`);
      }
      for (const [where, cut] of [['inline', inlineBroken], [`${layout} dialog`, expandedBroken]]) {
        if (cut.cells.length) failures.push(`${label} ${where}: ${cut.cells.length} of the Table's number cells break inside their value: ${JSON.stringify(cut.cells.slice(0, 3))}`);
      }
      if (inlineBroken.sideways > 1) failures.push(`${label}: with the Table open the page scrolls sideways by ${inlineBroken.sideways} px`);
      evidence.tableButtons.push({ width, height, touch, report, layout, inline, expanded, broken: [inlineBroken, expandedBroken], cues: [inlineCues, expandedCues] });
      await page.close();
    }
  }
  // The report's own table boxes on a phone: every-block's table block is wider than a 390 px
  // column, and shows it as the chart's Table does.
  evidence.reportTableCues = [];
  {
    const page = await h.browser.newPage({ viewport: { width: 390, height: 844 }, hasTouch: true, reducedMotion: 'reduce' });
    watchErrors(page, evidence.errors);
    await page.goto(`${inputs.base}/every-block/report.html`);
    await settle(page, 45_000, { models: false });
    const boxes = page.locator('.rv-report__body :is(.rv-table-scroll, div.rv-table)');
    let wider = 0, folded = 0;
    for (let i = 0; i < await boxes.count(); i++) {
      const box = boxes.nth(i);
      await box.scrollIntoViewIfNeeded();
      const c = await cuesOf(page, box);
      if (c.start.wider) wider++;
      if (c.start.wider && c.start.folds.length && c.start.kept.length) folded++;
      failures.push(...cueFailures(`390x844 every-block table box ${i + 1}`, c, ''));
      evidence.reportTableCues.push(c);
    }
    if (!wider) failures.push('390x844 every-block: no table box is wider than its column, so the edge cue goes unchecked');
    if (!folded) failures.push("390x844 every-block: no long table that folds its rows is wider than its column, so its fold's line goes unchecked");
    await page.close();
  }
  // A click or tap that opens the expanded view leaves the chart holding focus without its
  // keyboard ring, in every engine, as a click draws no ring anywhere else; a key pressed in the
  // dialog brings the ring back, and a keyboard open shows it at once (S27-878).
  evidence.focusRing = {};
  for (const [width, height, touch] of [[1440, 900, false], [844, 390, true]]) {
    const page = await h.browser.newPage({ viewport: { width, height }, hasTouch: touch, reducedMotion: 'reduce' });
    watchErrors(page, evidence.errors);
    await page.goto(`${inputs.base}/every-block/report.html`);
    await settle(page, 45_000, { models: false });
    // The frame's ring is its painted ::after (STY-38). Once a key in the dialog selects a mark,
    // the ring round that mark replaces it: data-rv-selected on the frame and a closed ring in the
    // link colour on the canvas. The dialog's view belongs to its inline chart and is not among
    // charts(), so the probe finds the ring in the pixels: a patch of link colour, absent when the
    // dialog opened, whose centre is unlit and which at least 6 of 8 rays from that centre meet.
    const ringOf = () => page.evaluate(async () => {
      // The mark ring arrives over 120 ms: read it once the chart has settled.
      await window.__richview.settled();
      const frame = document.querySelector('dialog.rv-chart-dialog[open] .rv-chart__frame');
      if (!frame) return { focused: false, ring: false };
      const after = getComputedStyle(frame, '::after');
      const framed = (!['none', 'normal'].includes(after.content) && after.borderTopStyle !== 'none' && parseFloat(after.borderTopWidth) > 0)
        || getComputedStyle(frame).outlineStyle !== 'none';
      const selected = frame.hasAttribute('data-rv-selected');
      const swatch = document.createElement('canvas').getContext('2d');
      const style = getComputedStyle(frame);
      swatch.fillStyle = style.getPropertyValue('--rv-link').trim() || style.getPropertyValue('--rv-accent').trim();
      swatch.fillRect(0, 0, 1, 1);
      const link = swatch.getImageData(0, 0, 1, 1).data;
      const canvas = frame.querySelector('canvas.rv-chart__canvas');
      const { width: w, height: h } = canvas;
      const px = canvas.getContext('2d').getImageData(0, 0, w, h).data;
      const lit = new Uint8Array(w * h);
      for (let i = 0; i < lit.length; i++) lit[i] = px[4 * i + 3] > 128 && Math.hypot(px[4 * i] - link[0], px[4 * i + 1] - link[1], px[4 * i + 2] - link[2]) < 40 ? 1 : 0;
      // What is link-coloured before any mark is selected is the chart's own, not a ring.
      const base = window.__rvRingBase;
      if (!selected) window.__rvRingBase = { canvas, lit: lit.slice() };
      else if (base?.canvas === canvas && base.lit.length === lit.length) for (let i = 0; i < lit.length; i++) if (base.lit[i]) lit[i] = 0;
      const label = new Int32Array(w * h);
      const rings = [];
      let share = 0, next = 0;
      for (let s = 0; s < lit.length; s++) {
        if (!lit[s] || label[s]) continue;
        const id = ++next, stack = [s], cells = [];
        label[s] = id;
        while (stack.length) {
          const i = stack.pop(), x = i % w, y = (i - x) / w;
          cells.push(i);
          for (let dy = -1; dy <= 1; dy++) for (let dx = -1; dx <= 1; dx++) {
            const nx = x + dx, ny = y + dy, j = ny * w + nx;
            if (nx >= 0 && ny >= 0 && nx < w && ny < h && lit[j] && !label[j]) { label[j] = id; stack.push(j); }
          }
        }
        if (cells.length < 20) continue;
        let sx = 0, sy = 0, x0 = w, x1 = 0, y0 = h, y1 = 0;
        for (const i of cells) {
          const x = i % w, y = (i - x) / w;
          sx += x; sy += y; x0 = Math.min(x0, x); x1 = Math.max(x1, x); y0 = Math.min(y0, y); y1 = Math.max(y1, y);
        }
        const cx = sx / cells.length, cy = sy / cells.length;
        // A ring, not a blob or a line: its centre is unlit.
        if (label[Math.round(cy) * w + Math.round(cx)] === id) continue;
        let hits = 0;
        for (let k = 0; k < 8; k++) {
          const a = k * Math.PI / 4;
          for (let t = 1; ; t++) {
            const x = Math.round(cx + t * Math.cos(a)), y = Math.round(cy + t * Math.sin(a));
            if (x < x0 || x > x1 || y < y0 || y > y1) break;
            if (label[y * w + x] === id) { hits++; break; }
          }
        }
        rings.push({ box: [x0, y0, x1 - x0, y1 - y0], hits });
        share = Math.max(share, hits / 8);
      }
      return { focused: document.activeElement === frame, ring: framed || (selected && share >= 0.75), framed, selected, share, rings: rings.slice(0, 4) };
    });
    const closeAll = async () => {
      for (let i = 0; i < 4 && (await page.locator('dialog.rv-chart-dialog[open]').count()); i++) { await page.keyboard.press('Escape'); await settled(page); }
    };
    const expand = page.locator('.rv-report__body figure[data-rv-chart]').first().getByRole('button', { name: 'Expand', exact: true });
    await expand.scrollIntoViewIfNeeded();
    const b = await expand.boundingBox();
    if (touch) await page.touchscreen.tap(b.x + b.width / 2, b.y + b.height / 2);
    else { await page.mouse.click(b.x + b.width / 2, b.y + b.height / 2); await page.mouse.move(1, 1); }
    await page.evaluate(() => window.__richview.settled()); await settled(page);
    const opened = await ringOf();
    await page.keyboard.press('ArrowRight'); await settled(page);
    const keyed = await ringOf();
    const label = `${width}x${height} ${touch ? 'touch' : 'mouse'}`;
    if (!opened.focused || opened.ring) failures.push(`${label}: after a ${touch ? 'tap' : 'click'} on Expand the chart ${opened.focused ? 'shows its keyboard focus ring' : 'does not hold focus'}`);
    if (!keyed.focused || !keyed.ring) failures.push(`${label}: after a key in the dialog the chart shows no focus ring: ${JSON.stringify(keyed)}`);
    await closeAll();
    await expand.focus();
    await page.keyboard.press('Enter');
    await page.evaluate(() => window.__richview.settled()); await settled(page);
    const entered = await ringOf();
    if (!entered.focused || !entered.ring) failures.push(`${label}: after Enter on Expand the chart shows no focus ring: ${JSON.stringify(entered)}`);
    evidence.focusRing[label] = { opened, keyed, entered };
    await closeAll();
    await page.close();
  }
  // An open Table sits under the expanded chart and scrolls with the dialog (FEA-CH-45), so the
  // chart keeps its size with it open: hiding and showing a series draws the chart as it does
  // with the Table closed, turned or upright, on a phone and on a desktop window (S27-868).
  const tableKeepsSize = async (page, press, label) => {
    const figures = page.locator('.rv-report__body figure[data-rv-chart]');
    const kinds = await figures.evaluateAll(all => all.map(f => JSON.parse(f.querySelector('script[data-rv-payload]').textContent).block.type));
    const seen = {};
    for (const [type, series] of [['line', 'Solo'], ['bar', '2024']]) {
      const expand = figures.nth(kinds.indexOf(type)).getByRole('button', { name: 'Expand', exact: true });
      await expand.scrollIntoViewIfNeeded();
      await press(expand);
      const dialog = page.locator('dialog.rv-chart-dialog[open]');
      const size = () => dialog.evaluate(node => {
        const f = node.querySelector('.rv-chart__frame'), m = new DOMMatrixReadOnly(getComputedStyle(node).transform);
        return { frame: [f.offsetWidth, f.offsetHeight], turned: Math.abs(m.a) < 1e-6 && Math.abs(m.b - 1) < 1e-6 };
      });
      const toggles = async () => {
        const legend = dialog.getByRole('button', { name: series, exact: true });
        await press(legend);
        const hidden = await size();
        await press(legend);
        return { hidden, shown: await size() };
      };
      const closed = await toggles();
      const table = dialog.getByRole('button', { name: 'Table', exact: true });
      await press(table);
      const open = await toggles();
      await press(table);
      const same = (a, b) => a.turned === b.turned && a.frame.every((v, i) => Math.abs(v - b.frame[i]) <= 1);
      if (!same(open.hidden, closed.hidden) || !same(open.shown, closed.shown)) failures.push(`${label} ${type}: with Table open, hiding and showing ${series} draws the chart ${JSON.stringify(open)}; with it closed ${JSON.stringify(closed)}`);
      seen[type] = { closed, open };
      for (let i = 0; i < 3 && (await page.locator('dialog.rv-chart-dialog[open]').count()); i++) { await page.keyboard.press('Escape'); await settled(page); }
    }
    return seen;
  };
  // Pressing Table in the expanded view brings the table into view: all of it when it fits under
  // the sticky title and action rows, else its top just under them, along the dialog's own axis,
  // so a turned dialog scrolls along the turn. A table whose status line and first row already
  // show leaves the dialog still. The actions stay in sight as the dialog scrolls: under the title
  // row upright, in it turned. Focus stays on Table, and pressing it again brings the chart back
  // in view at its full size. At once when motion is off; else over the style's motion, which
  // settled() waits for and the reader's own scroll ends. The inline figure does not scroll the
  // page (S27-869).
  const tableComesIntoView = async (page, press, label, types, motion = false) => {
    const figures = page.locator('.rv-report__body figure[data-rv-chart]');
    const kinds = await figures.evaluateAll(all => all.map(f => JSON.parse(f.querySelector('script[data-rv-payload]').textContent).block.type));
    const seen = {};
    for (const type of types) {
      const expand = figures.nth(kinds.indexOf(type)).getByRole('button', { name: 'Expand', exact: true });
      await expand.scrollIntoViewIfNeeded();
      await press(expand);
      const dialog = page.locator('dialog.rv-chart-dialog[open]');
      // Layout offsets along the dialog's own axis, so a turned dialog reads the same. What
      // covers the top while it scrolls is the title row and, upright, the action row held under
      // it. The Table button is in sight when the point at its centre hits it.
      const state = () => dialog.evaluate(node => {
        const offset = el => { let y = 0; for (let e = el; e && e !== node; e = e.offsetParent) y += e.offsetTop; return y; };
        const turned = node.hasAttribute('data-rv-turned');
        const actions = node.querySelector('.rv-chart--expanded > .rv-chart__actions');
        const cover = node.querySelector('.rv-chart-dialog__head').offsetHeight + (turned ? 0 : actions.offsetHeight);
        const span = el => (el ? [offset(el), offset(el) + el.offsetHeight] : null);
        const table = node.querySelector('.rv-chart__table'), frame = node.querySelector('.rv-chart__frame');
        const button = [...node.querySelectorAll('.rv-chart__action')].find(b => (b.getAttribute('aria-label') || b.textContent).trim() === 'Table');
        const box = button.getBoundingClientRect();
        const hit = document.elementFromPoint(box.left + box.width / 2, box.top + box.height / 2);
        return {
          turned, scroll: node.scrollTop, cover, height: node.clientHeight,
          status: span(table?.querySelector('.rv-chart__table-status')), row: span(table?.querySelector('tbody tr')), chart: span(frame),
          frame: [frame.offsetWidth, frame.offsetHeight],
          focus: document.activeElement === button, pressed: button.getAttribute('aria-pressed'), sight: !!hit && button.contains(hit),
        };
      });
      // In view at a scroll position: under what covers the top and above the bottom edge.
      const inView = (s, part, at = s.scroll, cover = s.cover) => !!s[part] && s[part][0] >= at + cover - 1 && s[part][1] <= at + s.height + 1;
      const before = await state();
      await dialog.getByRole('button', { name: 'Table', exact: true }).focus();
      // At once when motion is off; with motion on, the press leaves the dialog where it was.
      const first = await dialog.evaluate(node => { document.activeElement.click(); return node.scrollTop; });
      await page.evaluate(() => window.__richview.settled()); await settled(page);
      const opened = await state();
      // A table whose status line and first row show where the dialog stood leaves it still.
      const still = inView(opened, 'status', before.scroll) && inView(opened, 'row', before.scroll);
      // The table scrolled to its end, and the dialog with it: the pressed Table stays in sight.
      const end = await dialog.evaluate(node => {
        const table = node.querySelector('.rv-chart__table');
        table.scrollTop = table.scrollHeight;
        node.scrollTop = node.scrollHeight;
        return { table: table.scrollTop, dialog: node.scrollTop };
      });
      await settled(page);
      const scrolled = await state();
      await page.keyboard.press('Enter');
      await page.evaluate(() => window.__richview.settled()); await settled(page);
      const closed = await state();
      const name = `${label} ${type}${opened.turned ? ' (turned)' : ''}`;
      if (!inView(opened, 'status') || !inView(opened, 'row')) failures.push(`${name}: after pressing Table, the table's status line and first row are not in view under the title and action rows: ${JSON.stringify(opened)}`);
      if (still && Math.abs(opened.scroll - before.scroll) > 1) failures.push(`${name}: the table's status line and first row already showed, yet pressing Table moved the dialog (${before.scroll} -> ${opened.scroll})`);
      if (!opened.focus || opened.pressed !== 'true') failures.push(`${name}: pressing Table moves focus or loses its pressed state: ${JSON.stringify(opened)}`);
      if (!opened.sight || !scrolled.sight || !scrolled.focus || scrolled.pressed !== 'true') failures.push(`${name}: the pressed Table goes out of sight as the dialog scrolls: ${JSON.stringify({ opened: opened.sight, end, scrolled: scrolled.sight, focus: scrolled.focus })}`);
      if (motion ? first !== before.scroll : Math.abs(first - opened.scroll) > 1) failures.push(`${name}: with motion ${motion ? 'on the press jumps' : 'off the press does not scroll at once'} (${before.scroll} -> ${first} -> ${opened.scroll})`);
      if (!inView(closed, 'chart') || !closed.focus || closed.pressed !== 'false' || closed.frame.some((v, i) => Math.abs(v - before.frame[i]) > 1)) failures.push(`${name}: closing Table does not bring the chart back in view at its size: ${JSON.stringify({ before, closed })}`);
      seen[type] = { before, first, opened, still, end, scrolled, closed };
      if (motion) {
        // The reader's own scroll ends the glide where they leave it; the glide never fights them.
        const glide = await dialog.evaluate(async node => {
          document.activeElement.click();
          for (let i = 0; i < 60 && node.scrollTop < 2; i++) await new Promise(done => requestAnimationFrame(done));
          const moving = node.scrollTop;
          node.scrollTop = Math.max(0, moving - 10);
          return { moving, reader: node.scrollTop };
        });
        await page.evaluate(() => window.__richview.settled()); await settled(page);
        const left = await dialog.evaluate(node => node.scrollTop);
        if (glide.moving < 2 || Math.abs(left - glide.reader) > 1) failures.push(`${name}: the reader's own scroll during the glide does not end it: ${JSON.stringify({ ...glide, left })}`);
        seen[type].reader = { ...glide, left };
        await page.keyboard.press('Enter');
        await page.evaluate(() => window.__richview.settled()); await settled(page);
      }
      for (let i = 0; i < 3 && (await page.locator('dialog.rv-chart-dialog[open]').count()); i++) { await page.keyboard.press('Escape'); await settled(page); }
    }
    return seen;
  };
  const tap = async locator => {
    const box = await locator.boundingBox();
    await phone.touchscreen.tap(box.x + box.width / 2, box.y + box.height / 2);
    await phone.evaluate(() => window.__richview.settled()); await settled(phone);
  };
  evidence.table = { phone: await tableKeepsSize(phone, tap, 'phone 390x844') };
  evidence.tableInView = { phone: await tableComesIntoView(phone, tap, 'phone 390x844', ['bar', 'line']) };
  // The upright bar's table already shows (the dialog stays still); the turned line's does not.
  if (!evidence.tableInView.phone.bar.still || evidence.tableInView.phone.line.still) failures.push(`phone 390x844: the Table check needs a bar whose table already shows and a line whose table does not: ${JSON.stringify([evidence.tableInView.phone.bar.still, evidence.tableInView.phone.line.still])}`);
  if (evidence.tableInView.phone.bar.opened.turned || !evidence.tableInView.phone.line.opened.turned) failures.push(`phone 390x844: the Table check needs an upright bar and a turned line: ${JSON.stringify(evidence.tableInView.phone)}`);
  const desk = await h.browser.newPage({ viewport: { width: 1440, height: 900 }, reducedMotion: 'reduce' });
  watchErrors(desk, evidence.errors);
  await desk.goto(`${inputs.base}/every-block/report.html`);
  await settle(desk, 45_000, { models: false });
  const click = async locator => { await locator.click(); await desk.evaluate(() => window.__richview.settled()); await settled(desk); };
  evidence.table.desktop = await tableKeepsSize(desk, click, 'desktop 1440x900');
  evidence.tableInView.desktop = await tableComesIntoView(desk, click, 'desktop 1440x900', ['line', 'heatmap']);
  // The inline figure's Table opens under the chart without scrolling the page.
  const inlineTable = desk.locator('.rv-report__body figure[data-rv-chart]').first().getByRole('button', { name: 'Table', exact: true });
  await inlineTable.scrollIntoViewIfNeeded();
  const pageAt = await desk.evaluate(() => scrollY);
  await click(inlineTable);
  if (await desk.evaluate(() => scrollY) !== pageAt) failures.push('desktop 1440x900: the inline figure\'s Table scrolls the page');
  await click(inlineTable);
  await desk.emulateMedia({ reducedMotion: 'no-preference' });
  evidence.tableInView.motion = await tableComesIntoView(desk, click, 'desktop 1440x900 with motion', ['line'], true);
  await desk.close();
  // Time series gain from the turn; with the actions beside the title (S27-866) the candles and
  // the ribbon fit it too.
  for (const type of ['line', 'area', 'candlestick', 'ohlc', 'ribbon']) assert.ok(types.some(t => t.type === type && t.turned), `On a phone held upright the expanded ${type} chart turns: ${JSON.stringify(types.filter(t => t.type === type))}`);
  // A type that more width only enlarges stays upright at every phone width, where it reads at
  // once (the lead's rule of 4 Nov); `enlarges` lists them.
  const upright = { '390x844': types.filter(t => enlarges.includes(t.type)) };
  for (const [width, height] of [[360, 780], [430, 932]]) {
    await phone.setViewportSize({ width, height });
    await settled(phone);
    const shown = [];
    for (let i = 0; i < types.length; i++) {
      if (!enlarges.includes(types[i].type)) continue;
      const figure = phoneFigures.nth(i);
      await figure.getByRole('button', { name: 'Expand', exact: true }).scrollIntoViewIfNeeded();
      shown.push({ type: types[i].type, turned: (await shownOf(await tapExpand(figure))).turned });
      await phone.keyboard.press('Escape'); await settled(phone);
    }
    upright[`${width}x${height}`] = shown;
  }
  for (const [size, shown] of Object.entries(upright)) {
    assert.deepEqual(shown.map(t => t.type).sort(), [...enlarges].sort(), `${size}: every-block draws each type that stays upright once`);
    for (const t of shown.filter(t => t.turned)) failures.push(`phone ${size} ${t.type}: the expanded dialog turns, though more width only enlarges this type`);
  }
  evidence.phone = types;
  evidence.upright = upright;
  if (selection.scope) completedCases.push('expanded-regression');
  }
  assert.deepEqual(evidence.errors, []);
  assert.deepEqual(failures, []);
  if (selection.scope) { evidence.status = 'passed'; evidence.case_scope = selection.scope; evidence.completed = completedCases; }
  const saved = h.save(selection.scope ? 'figure-expand.partial.json' : 'figure-expand.json', evidence);
  if (selection.scope) {
    const outAt = process.argv.indexOf('--out');
    if (outAt >= 0 && process.argv[outAt + 1]) { mkdirSync(process.argv[outAt + 1], { recursive: true }); copyFileSync(saved, join(process.argv[outAt + 1], 'figure-expand.partial.json')); }
  }
  console.log(selection.scope ? `PASS ${completedCases.length} selected figure-expansion cases` : `PASS ${evidence.cases.length} themed report expansion cases, facet controls, exact round-trip geometry, keyboard, exports and reduced motion`);
} finally {
  await server.close();
  await inputs?.close();
  await h.close();
}
