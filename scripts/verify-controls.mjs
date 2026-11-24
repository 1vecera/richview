// Reader controls in the WebGL report runtime (FEA-BR-01 to FEA-BR-06, FEA-BR-31, FEA-BR-32,
// FEA-BR-37). Every input with controls (the Gapminder scatter, the indicator line and a variant
// fixture of bars and a share pie) is driven at 1440 and 390 px: every option of every selector
// by pointer, keyboard and touch, the scrubber, and a full playback with motion on and off.
// After each change the model, the axis title, the readout and the Table follow the state, one
// drawing exists, the CSV keeps every row and the PNG names the state; Expand shares the state
// and print names it. RICHVIEW_VERIFY_BROWSER picks the engine.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { harness, watchErrors } from './browser-harness.mjs';
import { figureDownload } from './chart-interaction-checks.mjs';
import { reports } from './report-runtime.mjs';

const rows = ['North', 'South', 'East', 'West'].flatMap((region, r) =>
  ['Q1', 'Q2'].map((quarter, q) => ({ region, quarter, sales: 40 + r * 15 + q * 9 })));
/** FEA-BR-05's fixture: bars and a share-of-total pie of the same rows, and a dropdown filter. */
const VARIANTS = '---\ntitle: Sales views\nversion: "1"\n---\n\nOne chart, two views.\n\n```chart\n'
  + JSON.stringify({
    type: 'bar', title: 'Sales by region', encoding: { x: 'region', y: 'sales' }, data: { inline: rows },
    controls: [
      { kind: 'variant', id: 'view', label: 'View', options: [
        { value: 'bars', label: 'Bars', chart: {} },
        { value: 'share', label: 'Share', chart: { type: 'pie', encoding: { x: 'region', y: 'sales' } } },
      ] },
      { kind: 'filter', id: 'quarter', field: 'quarter', label: 'Quarter', options: ['Q1', 'Q2'] },
    ],
  }) + '\n```\n';

const h = await harness('controls');
const inputs = await reports(h.dir, { only: ['playable-gapminder', 'playable-line-indicator'], extra: { variants: VARIANTS } });
const cases = [];
const errors = [];
const failures = [];
const check = async (label, fn) => {
  try {
    await fn();
  } catch (error) {
    failures.push(`${label}: ${error.message}`);
  }
};

const model = page => page.evaluate(async () => {
  await window.__richview.settled();
  return window.__richview.charts()[0];
});
const payloadOf = page => page.evaluate(() => JSON.parse(document.querySelector('figure[data-rv-chart] script[data-rv-payload]').textContent));
const axisTitles = m => m.geometry.panels.flatMap(p => p.items).concat(m.geometry.figure_items ?? []).filter(i => i.kind === 'text' && i.role === 'axis-title').map(i => i.text);
const valueTicks = m => m.geometry.panels[0].items.filter(i => i.kind === 'text' && i.role === 'axis-label').map(i => i.text);
const observations = m => m.geometry.panels.reduce((n, p) => n + p.observations.length, 0);

/** What the state should show, from the payload alone: the rows it keeps (FEA-BR-02, FEA-BR-03). */
function expected(payload, state) {
  const objects = payload.rows.map(r => Object.fromEntries(payload.columns.map((c, i) => [c, r[i]])));
  const text = v => (v === null || v === undefined || v === '' ? null : String(v).trim());
  let keep = objects;
  let view = { ...payload.block };
  for (const [k, c] of payload.block.controls.entries()) if (c.kind === 'variant') view = { ...view, ...c.options[state[k]].chart };
  for (const [k, c] of payload.block.controls.entries()) {
    if (c.kind === 'filter') {
      const options = c.all ? [null, ...c.options] : c.options;
      if (options[state[k]] !== null) keep = keep.filter(r => text(r[c.field]) === String(options[state[k]]));
    } else if (c.kind === 'playback') {
      const field = c.field ?? view.encoding.x;
      const frames = frameList(objects, field);
      const reveal = ['line', 'area'].includes(view.type) && field === view.encoding.x;
      keep = keep.filter(r => {
        const i = frames.indexOf(text(r[field]));
        return i >= 0 && (reveal ? i <= state[k] : i === state[k]);
      });
    }
  }
  return { view, keep };
}

function frameList(objects, field) {
  const seen = [];
  for (const r of objects) {
    const v = r[field] === null || r[field] === undefined ? null : String(r[field]).trim();
    if (v && !seen.includes(v)) seen.push(v);
  }
  const numbers = seen.map(Number);
  if (numbers.every(Number.isFinite)) return seen.map((v, i) => [numbers[i], v]).sort((a, b) => a[0] - b[0]).map(p => p[1]);
  const dates = seen.map(v => Date.parse(v));
  if (dates.every(Number.isFinite)) return seen.map((v, i) => [dates[i], v]).sort((a, b) => a[0] - b[0]).map(p => p[1]);
  return seen;
}

/** The state in words, "Year: 2000 · Region: Europe", from the payload alone. */
function describe(payload, state) {
  const objects = payload.rows.map(r => Object.fromEntries(payload.columns.map((c, i) => [c, r[i]])));
  return payload.block.controls.map((c, k) => {
    const field = c.field ?? payload.block.encoding.x;
    const labels = c.kind === 'playback' ? frameList(objects, field)
      : c.kind === 'filter' ? [...(c.all ? [c.all] : []), ...c.options.map(String)]
        : c.options.map(o => o.label ?? o.value);
    const name = c.label ?? (c.kind === 'metric' ? 'Measure' : c.kind === 'variant' ? 'View' : field);
    return `${name}: ${labels[state[k]]}`;
  }).join(' · ');
}

/** The control state as option indices, read from the row. */
const stateOf = figure => figure.evaluate(f => [...f.querySelectorAll('.rv-chart__controls [data-rv-control]')].map(g => {
  const s = g.querySelector('.rv-chart__scrubber, select');
  if (s) return Number(s.value);
  return [...g.querySelectorAll('.rv-chart__option')].findIndex(b => b.getAttribute('aria-pressed') === 'true');
}));

/** The model, axis title, readout and Table agree with the state on screen. */
async function agrees(page, figure, label) {
  const [m, payload, state] = [await model(page), await payloadOf(page), await stateOf(figure)];
  const { view, keep } = expected(payload, state);
  await check(`${label}: the accessible name follows the state`, async () => {
    const name = await figure.locator('.rv-chart__frame').getAttribute('aria-label');
    assert.ok(name?.includes(m.state), `${name} lacks ${m.state}`);
    assert.ok(name?.includes(m.geometry.summary.trim().replace(/\.$/, '')), `${name} lacks the current summary`);
  });
  await check(`${label}: one drawing`, async () => {
    assert.equal(await figure.locator('canvas.rv-chart__canvas').count(), 1);
    assert.equal(await figure.locator('.rv-chart__controls').count(), 1);
    assert.equal(m.geometry.kind, view.type, 'the chosen view draws');
  });
  await check(`${label}: plotted rows follow the state`, () => {
    const lineLike = ['line', 'area'].includes(view.type);
    const want = lineLike ? new Set(keep.map(r => String(r[view.encoding.x]))).size : keep.length;
    assert.equal(observations(m), want, `${observations(m)} observations for ${keep.length} rows`);
  });
  const metric = payload.block.controls.findIndex(c => c.kind === 'metric');
  if (metric >= 0) {
    const option = payload.block.controls[metric].options[state[metric]];
    await check(`${label}: the axis title follows the metric`, () => assert.ok(axisTitles(m).includes(option.axis.label), `${axisTitles(m)} lacks ${option.axis.label}`));
  }
  // Readout: the first observation, then the state's own values.
  await figure.locator('.rv-chart__frame').focus();
  await page.keyboard.press('Home');
  const readout = await figure.locator('.rv-chart__readout').evaluate(n => (n.hidden ? null : n.textContent));
  const target = (await model(page)).target;
  await check(`${label}: the readout reads the state`, () => {
    assert.ok(target && readout, 'no readout');
    const o = m.geometry.panels[target.panel].observations[target.observation];
    assert.ok(readout.includes(o.readout.heading), `${readout} lacks ${o.readout.heading}`);
  });
  await page.keyboard.press('Escape');
  // Table: the state's rows, named (FEA-BR-32).
  if (!(await figure.locator('.rv-chart__table').count())) await figure.locator('.rv-chart__action--table').click();
  const status = await figure.locator('.rv-chart__table-status').textContent();
  const listed = await figure.locator('.rv-chart__table tbody tr').count();
  const name = (await model(page)).state;
  await check(`${label}: the Table lists and names the state`, () => {
    assert.equal(listed, Math.min(keep.length, 1000));
    assert.ok(status.startsWith(`${name}: ${keep.length} of ${payload.rows.length} observations`), status);
  });
  return { m, state, payload };
}

try {
  for (const width of [1440, 390]) {
    const context = await h.browser.newContext({ viewport: { width, height: 1000 }, hasTouch: width < 600, acceptDownloads: true });
    const page = await context.newPage();
    watchErrors(page, errors, `${width}: `);
    for (const name of inputs.names) {
      const label = `${name} @${width}`;
      await page.goto(`${inputs.base}/${name}/report.html`);
      await page.waitForFunction(() => window.__richview);
      const figure = page.locator('figure[data-rv-chart]').first();
      const row = figure.locator('.rv-chart__controls');
      // The runtime announces itself before it mounts the first chart, which it does in the next
      // animation frame, and the mount wires the row (ARC-18, docs/MOTION.md). Wait for that one
      // mount, at most 5 s, and not for settled(), which mounts every chart. A mount that fails
      // or never comes leaves the row pending, and the check below says so.
      const mount = `mount ${await figure.getAttribute('id')}`;
      await page.waitForFunction(mount => !window.__richview.pending().includes(mount), mount, { polling: 20, timeout: 5000 }).catch(() => {});
      await check(`${label}: the row is wired`, async () => {
        assert.equal(await row.getAttribute('data-rv-pending'), null);
        assert.equal(await row.getAttribute('inert'), null);
      });
      const first = await agrees(page, figure, `${label} initial`);
      const controls = first.payload.block.controls;
      const item = { name, width, states: 1 };
      cases.push(item);
      // Every option of every selector: buttons by pointer or touch, then keyboard; dropdowns
      // by choosing and by keys (FEA-BR-06).
      for (const [k, c] of controls.entries()) {
        if (c.kind === 'playback') continue;
        const group = row.locator(`[data-rv-control="${k}"]`);
        const count = (c.options?.length ?? 0) + (c.all ? 1 : 0);
        const select = group.locator('select');
        for (let i = 0; i < count; i++) {
          if (await select.count()) await select.selectOption(String(i));
          else if (width < 600) await group.locator('.rv-chart__option').nth(i).tap();
          else await group.locator('.rv-chart__option').nth(i).click();
          await check(`${label}: ${c.label} option ${i} is chosen`, async () => assert.equal((await stateOf(figure))[k], i));
          await agrees(page, figure, `${label} ${c.label}=${i}`);
          item.states++;
        }
        if (await select.count()) {
          await select.focus();
          const before = (await stateOf(figure))[k];
          // Typing an option's name picks it on a closed select in every engine.
          await page.keyboard.type(c.options[before > 0 ? 0 : 1].label ?? String(c.options[before > 0 ? 0 : 1]));
          await check(`${label}: keys change the ${c.label} dropdown`, async () => assert.notEqual((await stateOf(figure))[k], before));
        } else {
          await group.locator('.rv-chart__option').first().focus();
          await page.keyboard.press('Enter');
          await check(`${label}: Enter chooses a ${c.label} option`, async () => assert.equal((await stateOf(figure))[k], 0));
        }
        await agrees(page, figure, `${label} ${c.label} by keyboard`);
      }
      const k = controls.findIndex(c => c.kind === 'playback');
      if (k >= 0) {
        const group = row.locator(`[data-rv-control="${k}"]`);
        const frames = Number(await group.locator('.rv-chart__scrubber').getAttribute('max')) + 1;
        // The scrubber and the step buttons pause and move one frame (FEA-BR-02).
        await group.locator('.rv-chart__scrubber').evaluate(s => { s.value = '0'; s.dispatchEvent(new Event('input', { bubbles: true })); });
        await check(`${label}: the scrubber chooses a frame`, async () => assert.equal((await stateOf(figure))[k], 0));
        const start = await agrees(page, figure, `${label} frame 0`);
        await group.locator('.rv-chart__control-button--next').click();
        await check(`${label}: Next steps a frame`, async () => assert.equal((await stateOf(figure))[k], 1));
        await group.locator('.rv-chart__scrubber').focus();
        await page.keyboard.press('ArrowLeft');
        await check(`${label}: ← steps back`, async () => assert.equal((await stateOf(figure))[k], 0));
        await page.keyboard.press('ArrowRight');
        await check(`${label}: → steps on`, async () => assert.equal((await stateOf(figure))[k], 1));
        await group.locator('.rv-chart__control-button--previous').click();
        await group.locator('.rv-chart__scrubber').focus();
        // A full playback with motion: every frame in order, axes still, then pause (FEA-BR-31).
        const seen = [0];
        const ticks = new Set([JSON.stringify(valueTicks(start.m))]);
        const play = group.locator('.rv-chart__control-button--play');
        await page.keyboard.press(' ');
        await check(`${label}: Space plays`, async () => assert.equal(await play.getAttribute('aria-pressed'), 'true'));
        const deadline = Date.now() + frames * 2000 + 4000;
        while (seen.length < frames && Date.now() < deadline) {
          const f = (await stateOf(figure))[k];
          if (f !== seen.at(-1)) {
            seen.push(f);
            const m = await page.evaluate(() => window.__richview.charts()[0]);
            if (m.settled) ticks.add(JSON.stringify(valueTicks(m)));
          }
          await page.waitForTimeout(50);
        }
        await check(`${label}: playback visits every frame in order`, () => assert.deepEqual(seen, [...Array(frames).keys()]));
        await check(`${label}: frames share their axes`, () => assert.equal(ticks.size, 1, [...ticks].join(' | ')));
        await page.waitForTimeout(2000);
        const end = await group.evaluate(group => ({
          frame: Number(group.querySelector('.rv-chart__scrubber').value),
          label: group.querySelector('.rv-chart__control-button--play').getAttribute('aria-label'),
          playing: window.__richview.charts()[0].playing,
        }));
        assert.deepEqual(end, { frame: frames - 1, label: 'Play', playing: false }, `${label}: playback stops and stays at the last frame`);
        await agrees(page, figure, `${label} last frame`);
        // Read the restarted state in the click's task, before the next frame's timer can fire.
        const restarted = await play.evaluate(button => {
          button.click();
          return { frame: Number(button.closest('[data-rv-control]').querySelector('input').value), label: button.getAttribute('aria-label') };
        });
        await check(`${label}: Play restarts from the first frame`, () => assert.deepEqual(restarted, { frame: 0, label: 'Pause' }));
        await play.click();
        await check(`${label}: play pauses`, async () => assert.equal((await model(page)).playing, false));
        // Reduced motion: each press advances once without gliding, including at the ends.
        await page.emulateMedia({ reducedMotion: 'reduce' });
        await group.locator('.rv-chart__scrubber').evaluate((s, frame) => {
          s.value = String(frame);
          s.dispatchEvent(new Event('input', { bubbles: true }));
        }, frames - 2);
        for (const frame of [frames - 1, 0]) {
          await play.click();
          const after = await page.evaluate(() => ({ settled: window.__richview.charts()[0].settled, playing: window.__richview.charts()[0].playing }));
          await check(`${label}: reduced motion goes to frame ${frame} without a glide`, async () => {
            assert.equal((await stateOf(figure))[k], frame);
            assert.equal(await play.getAttribute('aria-label'), 'Play');
            assert.deepEqual(after, { settled: true, playing: false });
          });
        }
        await page.emulateMedia({ reducedMotion: 'no-preference' });
      }
      // CSV keeps every source row; the PNG names the state (FEA-BR-32).
      const payload = await payloadOf(page);
      const csv = await figureDownload(page, figure, 'csv');
      const lines = readFileSync(await csv.path(), 'utf8').trim().split(/\r?\n/);
      await check(`${label}: CSV keeps every row`, () => assert.equal(lines.length - 1, payload.rows.length));
      const png = await figureDownload(page, figure, 'png');
      const state = (await model(page)).state;
      const slug = state.toLowerCase().replace(/[^\p{L}\p{N}]+/gu, '-').replace(/-+$/, '');
      await check(`${label}: the PNG names the state`, () => assert.ok(png.suggestedFilename().includes(slug), `${png.suggestedFilename()} lacks ${slug}`));
      // Expand shares the state both ways (FEA-BR-32).
      await figure.locator('.rv-chart__action--expand').click();
      const dialog = page.locator('dialog.rv-chart-dialog[open]');
      const selector = controls.findIndex(c => c.kind !== 'playback');
      if (selector >= 0) {
        const group = dialog.locator(`.rv-chart__controls [data-rv-control="${selector}"]`);
        const current = (await stateOf(figure))[selector];
        const next = current === 0 ? 1 : 0;
        if (await group.locator('select').count()) await group.locator('select').selectOption(String(next));
        else await group.locator('.rv-chart__option').nth(next).click();
        await check(`${label}: a change in Expand reaches the inline chart`, async () => assert.equal((await stateOf(figure))[selector], next));
      }
      await page.keyboard.press('Escape');
      await check(`${label}: Expand closes`, async () => assert.equal(await page.locator('dialog.rv-chart-dialog[open]').count(), 0));
      // Print hides the controls and names the state (FEA-BR-02).
      await page.emulateMedia({ media: 'print' });
      const printed = describe(await payloadOf(page), await stateOf(figure));
      await check(`${label}: print names the state`, async () => {
        assert.equal(await row.isVisible(), false);
        const line = figure.locator('.rv-chart__state');
        assert.equal(await line.isVisible(), true);
        assert.equal(await line.textContent(), printed);
      });
      await page.emulateMedia({ media: 'screen' });
    }
    await context.close();
  }
  await check('no script errors', () => assert.deepEqual(errors, []));
  assert.deepEqual(failures, []);
  console.log(`PASS ${cases.length} chart views in ${h.engine}: ${cases.reduce((n, c) => n + c.states, 0)} states by pointer, touch and keys; playback with motion on and off; Table, CSV, PNG, Expand and print follow the state`);
} finally {
  h.save('controls.json', { engine: h.engine, cases, errors, failures });
  await inputs.close();
  await h.close();
}
