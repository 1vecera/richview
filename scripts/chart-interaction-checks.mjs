// Host reason (ARC-03): shared Playwright checks drive the actual report runtime on
// generated reports and docs pages; they do not ship to readers.
// FEA-CH-39's zoom column; the one table in this file that is not read from the model, because the
// model offers no zoom flag and an x scale alone (histograms, 2D bins, hexbin, contour, dot plot)
// does not mean the chart zooms.
const ZOOM_TYPES = new Set(['line', 'area', 'scatter', 'map', 'band', 'density', 'ecdf', 'gantt', 'qq', 'streamgraph', 'vector']);
// Types whose policy says no interaction (FEA-CH-39): they carry no observations. None today.
const NO_INTERACTION = new Set();
const KEY_STEPS = 3;
const HOVERS = 3;

/** The words a readout shows: its heading, then each line's name and value as the DOM prints them. */
const words = readout => JSON.stringify([readout.heading, ...readout.lines.map(line => `${line.name}${line.value}`)]);

/** The model of chart `i` and what its readout shows, once that chart is idle. `settled()` waits for all
 * 61 charts and takes about a second each time; `pending()` names only the charts still moving. */
const probe = (page, i) => page.evaluate(async i => {
  const id = window.__richview.charts()[i].id;
  const end = performance.now() + 4_000;
  while (window.__richview.pending().includes(id) && performance.now() < end) await new Promise(accept => requestAnimationFrame(accept));
  const model = window.__richview.charts()[i];
  const figure = document.getElementById(model.figure);
  const node = figure.querySelector('.rv-chart__readout');
  const shown = node && !node.hidden ? {
    heading: node.querySelector('.rv-chart__readout-head')?.textContent ?? '',
    lines: [...node.querySelectorAll('.rv-chart__readout-line')].map(line => ({ name: '', value: line.textContent })),
  } : null;
  return { target: model.target, hidden: model.hidden, zoom: model.zoom, shown };
}, i);

const observation = (model, target) => model.geometry.panels[target.panel]?.observations[target.observation];
const targetKey = target => `${target.panel}/${target.observation}`;

/** Expect Sankey and alluvial marks to hide multiple derived-only rows (FEA-CH-40). */
export function displayedReadout(readout, targeting, kind) {
  const lines = readout.lines;
  const multipleDerived = ['sankey', 'alluvial'].includes(kind) && targeting !== 'nearest-x' && lines.length > 2
    && lines.slice(1).every(line => line.derived && line.series === undefined);
  return { heading: readout.heading, lines: multipleDerived ? lines.slice(0, 1) : lines };
}

/** Separate observations without drawn marks from the pointer cohort; the keyboard reaches them. */
function pointerCohort(model) {
  const all = model.geometry.panels.flatMap((panel, p) => panel.observations.map((obs, o) => ({ panel: p, observation: o, obs })));
  if (model.geometry.kind !== 'wordcloud') {
    const unplaced = all.filter(({ panel, obs }) => {
      const targeting = model.geometry.panels[panel].targeting;
      return targeting === 'radius' ? !(obs.radius > 0)
        : targeting === 'contain' && !obs.area && !obs.wedge && !(obs.radius > 0);
    }).map(pick => ({ ...pick, word: pick.obs.readout.heading, reason: 'not drawn' }));
    const omitted = new Set(unplaced.map(targetKey));
    return { pointer: all.filter(pick => !omitted.has(targetKey(pick))), unplaced, problems: [] };
  }
  const problems = [], missing = new Map(), dropped = new Map(), unplaced = [];
  for (const drop of model.geometry.dropped ?? []) {
    if (drop.kind === 'word' && drop.reason === 'collision') dropped.set(drop.text, (dropped.get(drop.text) ?? 0) + 1);
  }
  for (const pick of all) {
    const word = pick.obs.readout.heading;
    const painted = model.geometry.panels[pick.panel].items.some(item => item.kind === 'text' && item.role === 'label' && item.text === word && item.bounds?.w > 0 && item.bounds?.h > 0);
    const hittable = pick.obs.area?.w > 0 && pick.obs.area?.h > 0;
    if (painted && !hittable) problems.push(`drawn word ${word} has no hit area`);
    else if (!painted && hittable) problems.push(`word ${word} has a hit area but no drawn mark`);
    else if (!painted && !pick.obs.area && !pick.obs.wedge && !(pick.obs.radius > 0)) {
      const words = missing.get(word) ?? [];
      words.push(pick);
      missing.set(word, words);
    } else if (!painted) problems.push(`word ${word} has invalid hit geometry`);
  }
  for (const word of new Set([...missing.keys(), ...dropped.keys()])) {
    const words = missing.get(word) ?? [], count = dropped.get(word) ?? 0;
    if (words.length !== count) problems.push(`unplaced word ${word} has ${words.length} observations but ${count} collision provenance records`);
    else unplaced.push(...words.map(pick => ({ ...pick, word, reason: 'collision' })));
  }
  const omitted = new Set(unplaced.map(targetKey));
  const pointer = all.filter(pick => !omitted.has(targetKey(pick)));
  return { pointer, unplaced, problems };
}

/** Whether the readout on screen is the one the model's target gives. */
const readsTarget = (model, state) => {
  const obs = state.target && observation(model, state.target);
  if (!obs) return `no observation at target ${JSON.stringify(state.target)}`;
  if (!state.shown) return 'no readout is shown';
  const expected = displayedReadout(obs.readout, model.geometry.panels[state.target.panel].targeting, model.geometry.kind);
  return words(state.shown) === words(expected) ? null : `readout ${words(state.shown)} is not the observation's ${words(expected)}`;
};

/** Keyboard: focus the chart, walk its observations, leave with Escape. Undrawn observations
 * have no mark to point at, but the keyboard reaches every one of them. */
async function keyboard(page, i, model, figure, unplaced) {
  const problems = [], visited = new Set();
  const needed = new Set(unplaced.map(targetKey));
  const total = model.geometry.panels.reduce((sum, panel) => sum + panel.observations.length, 0);
  const steps = unplaced.length ? Math.max(KEY_STEPS, total) : KEY_STEPS;
  await figure.locator('.rv-chart__frame').focus();
  const press = async key => { await page.keyboard.press(key); return probe(page, i); };
  let state = await press('Home');
  for (let step = 0; step < steps; step++) {
    const problem = readsTarget(model, state);
    if (problem) { problems.push(`${step ? 'ArrowRight' : 'Home'} (step ${step}): ${problem}`); break; }
    visited.add(targetKey(state.target));
    needed.delete(targetKey(state.target));
    if (step + 1 >= KEY_STEPS && !needed.size) break;
    state = await press('ArrowRight');
  }
  if (needed.size) problems.push(`undrawn observations are unreachable by keyboard: ${[...needed].join(', ')}`);
  if (!problems.length && total > 1 && visited.size < 2) problems.push(`${total} observations but the arrow keys visit ${visited.size}`);
  state = await press('Escape');
  if (state.target || state.shown) problems.push(`Escape leaves ${state.target ? 'a target' : 'a readout'} (${JSON.stringify(state.target)})`);
  return { ok: !problems.length, visited: visited.size, unplaced_visited: unplaced.length - needed.size, problems };
}

/** Whether the selected observation can reach the delivered pointer in viewport coordinates. */
export function pointerReachesObservation(obs, point, box, scale) {
  const x = (point[0] - box.x) / scale, y = (point[1] - box.y) / scale;
  if (obs.area) return x >= obs.area.x - 1 && x <= obs.area.x + obs.area.w + 1 && y >= obs.area.y - 1 && y <= obs.area.y + obs.area.h + 1;
  return Math.hypot(x - obs.at[0], y - obs.at[1]) <= obs.radius + 4;
}

/** Pointer: hover observations by their anchors, or tap them on a touch device. */
async function pointer(page, i, model, figure, touch, cohort) {
  const problems = [...cohort.problems], hovered = [];
  const all = cohort.pointer;
  if (!all.length) return { ok: !problems.length, hovered: 0, unplaced: cohort.unplaced.map(({ panel, observation, word, reason }) => ({ panel, observation, word, reason })), problems };
  const picks = [...new Set([0, Math.floor(all.length / 2), all.length - 1])].slice(0, HOVERS).map(k => all[k]);
  const canvas = figure.locator('canvas').first();
  for (const pick of picks) {
    await canvas.evaluate(node => node.scrollIntoView({ block: 'center', behavior: 'instant' }));
    const box = await canvas.boundingBox();
    const scale = box.width / model.geometry.width;
    const at = [box.x + pick.obs.at[0] * scale, box.y + pick.obs.at[1] * scale];
    if (!hovered.length && !problems.length) await page.mouse.move(1, 1);
    if (touch) await figure.locator('.rv-chart__frame').press('Escape');
    await page.evaluate(kind => {
      window.__rvDeliveredPoint = null;
      window.addEventListener(kind, event => { window.__rvDeliveredPoint = [event.clientX, event.clientY]; }, { capture: true, once: true });
    }, touch ? 'pointerdown' : 'pointermove');
    if (touch) await page.touchscreen.tap(...at); else await page.mouse.move(...at);
    const delivered = await page.evaluate(() => window.__rvDeliveredPoint);
    if (!delivered) { problems.push(`hover ${pick.panel}/${pick.observation}: no pointer event was delivered`); continue; }
    const state = await probe(page, i);
    const problem = readsTarget(model, state);
    const label = `${pick.panel}/${pick.observation} at ${at.map(Math.round)}`;
    if (problem) { problems.push(`hover ${label}: ${problem}`); continue; }
    // Marks overlap (a dense scatter, a nested tree), so another observation may lie on top; it must
    // then reach the pointer by its own mark, not be unrelated to it.
    const got = observation(model, state.target);
    const near = Math.hypot(box.x + got.at[0] * scale - delivered[0], box.y + got.at[1] * scale - delivered[1]);
    const same = state.target.panel === pick.panel && state.target.observation === pick.observation;
    const nearestX = model.geometry.panels[pick.panel].targeting === 'nearest-x' && Math.abs(got.x - pick.obs.x) < 1e-9;
    if (!same && !nearestX && !pointerReachesObservation(got, delivered, box, scale)) problems.push(`hover ${label} targets ${state.target.panel}/${state.target.observation}, ${near.toFixed(0)} px away`);
    hovered.push(label);
  }
  if (touch) {
    await figure.locator('.rv-chart__frame').focus();
    await page.keyboard.press('Escape');
  } else await page.mouse.move(1, 1);
  const left = await probe(page, i);
  if (left.target || left.shown) problems.push(`leaving keeps ${left.target ? 'a target' : 'a readout'} (${JSON.stringify(left.target)})`);
  return { ok: !problems.length, hovered: hovered.length, unplaced: cohort.unplaced.map(({ panel, observation, word, reason }) => ({ panel, observation, word, reason })), problems };
}

/** A figure's CSV or PNG download (`kind` is `csv` or `png`), taken where the reader finds the action. Up to
 * 600 px a figure shows Table and Expand alone (Daniel, 17 Nov 2026: no download icons on a phone), so there the
 * action is clicked in the expanded view, which shares the chart's state (FEA-BR-32); the view is closed once the
 * download has started, since a PNG is encoded after the click. `onClick` runs after the click, inside the wait. */
export async function figureDownload(page, figure, kind, { timeout = 30_000, onClick } = {}) {
  const press = button => Promise.all([page.waitForEvent('download', { timeout }), (async () => { await button.click({ timeout }); await onClick?.(); })()]);
  const inline = figure.locator(`.rv-chart__action--${kind}`).first();
  if (await inline.isVisible()) return (await press(inline))[0];
  await figure.locator('.rv-chart__action--expand').first().click({ timeout });
  const dialog = page.locator('dialog.rv-chart-dialog[open]');
  const [file] = await press(dialog.locator(`.rv-chart__action--${kind}`));
  await dialog.locator('.rv-chart-dialog__close').click({ timeout });
  await dialog.waitFor({ state: 'hidden', timeout });
  return file;
}

/**
 * Compare real series-name buttons with the genuine line and area names: every label in the data
 * face whose text is a series' name, inside the plot or beside it (names inside the plot, chart
 * design 16 Nov 2026), and nothing else. A value label is set in figures; one whose number spells a
 * series' name ('18' on a series named 18) is never a button.
 */
export async function nameButtonContract(page, i) {
  const nameContract = await page.evaluate(i => {
    const model = window.__richview.charts()[i], g = model.geometry;
    const labels = g.panels.flatMap(p => p.items.filter(t => t.kind === 'text' && t.role === 'label' && t.series !== undefined && g.series[t.series]?.name === t.text));
    const expected = g.legend || !['line', 'area'].includes(g.kind) ? [] : labels.filter(t => t.face === 'data').map(t => ({ key: g.series[t.series].key, text: t.text }));
    // Value labels whose number spells a series' name: each must stay a value, not a button.
    const twins = labels.filter(t => t.face !== 'data').map(t => t.text);
    const actual = [...document.getElementById(model.figure).querySelectorAll('button.rv-chart__name')].map(b => ({ key: b.dataset.rvSeries, text: b.textContent }));
    return { expected: expected.length >= 2 ? expected : [], actual, twins };
  }, i);
  const problems = [];
  if (JSON.stringify(nameContract.expected) !== JSON.stringify(nameContract.actual)) problems.push(`series-name buttons differ from genuine series names: ${JSON.stringify(nameContract)}`);
  for (const text of new Set(nameContract.twins)) {
    const buttons = nameContract.actual.filter(b => b.text === text).length;
    const names = nameContract.expected.filter(n => n.text === text).length;
    if (buttons > names) problems.push(`a value label '${text}' that spells a series name became a button (${buttons} buttons, ${names} names)`);
  }
  return { ...nameContract, problems };
}

/** Legend: toggle the first entry off and on again. */
async function toggle(page, i, figure) {
  const { problems } = await nameButtonContract(page, i);
  const controls = figure.locator('button.rv-chart__legend-entry:visible, button.rv-chart__name:visible');
  const entries = await controls.count();
  if (entries < 2) return { ok: !problems.length, entries, problems, skipped: entries ? 'a single entry' : 'no legend entries' };
  const hiddenCount = async () => (await probe(page, i)).hidden.length;
  await controls.first().click();
  const hidden = await hiddenCount();
  if (hidden !== 1) problems.push(`clicking an entry hides ${hidden} series, expected 1`);
  await controls.first().click();
  if (await hiddenCount() !== 0) problems.push('clicking the entry again does not restore the series');
  return { ok: !problems.length, entries, problems };
}

/** Zoom: `+` narrows a zooming chart's x window and Escape resets it; other charts ignore `+`. */
async function zoom(page, i, model, figure) {
  const zooms = ZOOM_TYPES.has(model.geometry.kind) && Boolean(model.geometry.panels[0]?.x);
  const hasX = Boolean(model.geometry.panels[0]?.x);
  if (!zooms && !hasX) return { ok: true, offered: false, skipped: 'no x scale' };
  const problems = [];
  await figure.locator('.rv-chart__frame').focus();
  await page.keyboard.press('+');
  const state = await probe(page, i);
  if (zooms && !state.zoom) problems.push('the type zooms (FEA-CH-39) but + sets no window');
  if (!zooms && state.zoom) problems.push(`the type does not zoom (FEA-CH-39) but + set ${JSON.stringify(state.zoom)}`);
  if (state.zoom) {
    // Escape clears a selection before it resets the zoom, so a second press may be needed.
    await page.keyboard.press('Escape');
    let after = await probe(page, i);
    if (after.zoom) { await page.keyboard.press('Escape'); after = await probe(page, i); }
    if (after.zoom) problems.push('Escape does not reset the zoom');
  }
  return { ok: !problems.length, offered: zooms, problems };
}

/** Judge chart `i` on `page`: read its policy from its model, then run the checks it offers. */
export async function judgeChart(page, i, model, { touch = false } = {}) {
  const figure = page.locator(`#${model.figure}`);
  const title = (await figure.locator('.rv-chart__title').first().textContent().catch(() => null))?.trim() ?? null;
  const geometry = model.geometry;
  const observations = geometry.panels.reduce((sum, panel) => sum + panel.observations.length, 0);
  const legend = await figure.locator('button.rv-chart__legend-entry:visible, button.rv-chart__name:visible').count();
  const entry = {
    index: i, type: geometry.kind, title,
    policy: {
      interaction: observations ? 'inspect' : 'none',
      declared_none: NO_INTERACTION.has(geometry.kind),
      targeting: [...new Set(geometry.panels.map(panel => panel.targeting))],
      observations, panels: geometry.panels.length, legend_entries: legend,
      zoom: ZOOM_TYPES.has(geometry.kind) && Boolean(geometry.panels[0]?.x),
    },
  };
  try {
    await figure.scrollIntoViewIfNeeded();
    if (!observations || entry.policy.declared_none) {
      const problems = [];
      if (observations) problems.push(`declared as having no interaction but offers ${observations} observations`);
      else if (!entry.policy.declared_none) problems.push(`offers no observations and is not declared in NO_INTERACTION${geometry.failure ? ` (${geometry.failure})` : ''}`);
      await figure.locator('.rv-chart__frame').focus();
      await page.keyboard.press('Home');
      const state = await probe(page, i);
      if (state.target || state.shown) problems.push('a type with no interaction shows a readout');
      entry.keyboard = { ok: !problems.length, problems };
      for (const key of ['pointer', 'toggle', 'zoom']) entry[key] = { ok: true, skipped: 'no interaction' };
    } else {
      const cohort = pointerCohort(model);
      entry.keyboard = await keyboard(page, i, model, figure, cohort.unplaced);
      entry.pointer = await pointer(page, i, model, figure, touch, cohort);
      entry.toggle = await toggle(page, i, figure);
      entry.zoom = await zoom(page, i, model, figure);
    }
  } catch (error) {
    entry.error = String(error.message ?? error).split('\n')[0];
  }
  entry.ok = !entry.error && ['keyboard', 'pointer', 'toggle', 'zoom'].every(key => entry[key]?.ok);
  return entry;
}
