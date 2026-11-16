// Host reason (ARC-03): browser behavior and filmstrips for report entrances, the whole-page
// reduced-motion floor and scrollytelling (FEA-RP-35/36/48). Curves and visual quality are judged
// separately. Default scope is all five styles at desktop and 390 px, one browser at a time.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { parseArgs } from 'node:util';
import { chromium, errors } from 'playwright';
import { chromiumPath, cli, root } from './browser-harness.mjs';
import { launchBrowser } from '../tools/rv-gate/lib/browser-launch.mjs';
import { reports } from './report-runtime.mjs';

const { values: options } = parseArgs({ options: { only: { type: 'string' } } });
const styles = ['sleek', 'air', 'scientific', 'brutalist', 'hand-drawn'].filter(s => !options.only || options.only.split(',').includes(s));
assert.ok(styles.length, 'at least one known style must run');
const widths = [1440, 390], HEIGHT = 844;
const PRIMARY = '.rv-report__body > :not(.rv-keep):not(.rv-print-only), .rv-keep > :not(.rv-print-only)';
const fence = (tag, value) => `\n\`\`\`${tag}\n${JSON.stringify(value)}\n\`\`\`\n`;
const rows = ['2023', '2024'].flatMap((year, y) => ['North', 'South'].flatMap((region, r) =>
  [0, 1, 2, 3].map(t => ({ year, region, t, v: 3 + t * 7 + y * 11 + r * 30 }))));
const chart = {
  type: 'line', title: 'The changing figure', data: { inline: rows }, encoding: { x: 't', y: 'v', color: 'region' },
  controls: [
    { kind: 'playback', id: 'year', field: 'year', initial: '2023', speed: 5000, loop: true },
    { kind: 'filter', id: 'region', field: 'region', options: ['North', 'South'], initial: 'North', as: 'buttons' },
  ],
};
const steps = [
  { title: 'First view', text: 'First narrative step. [First keyboard stop](#after-stories).', state: { year: '2024', region: 'South' } },
  { title: 'Second view', text: 'Second narrative step. [Second keyboard stop](#after-stories).', state: { year: '2023', region: 'North' } },
  { title: 'Third view', text: 'Third narrative step. [Third keyboard stop](#after-stories).', state: { year: '2024', region: 'South' } },
];
const source = style => `---\ntitle: Report motion behavior\nstyles: [${style}]\n---\n\nFirst-screen text reads at rest.\n\n`
  + Array.from({ length: 6 }, (_, i) => `Opening paragraph ${i + 1}. This ordinary prose makes the later blocks begin below the first screen.\n\n`).join('')
  + '## Blocks below the first screen\n\nA paragraph with **readable emphasis** and a [link](#after-stories).\n\n'
  + '- A list entry\n- A second entry\n\n> A quotation remains in the document flow.\n\n```text\nA code block stays in its box.\n```\n'
  + fence('metric', { label: 'A metric', value: 42, delta: 0.1 })
  + fence('callout', { kind: 'neutral', title: 'A callout', body: 'The callout enters once.' })
  + fence('table', { data: { inline: rows.slice(0, 4) }, columns: [{ key: 't' }, { key: 'v' }], max_rows: 2 })
  + fence('image', { src: 'media/fixture-image.png', alt: 'A test picture', width: 1600, height: 900 })
  + fence('audio', { src: 'media/tone.mp3', title: 'A test tone', transcript: 'transcript.md' })
  + fence('video', { src: 'media/clip.mp4', poster: 'media/poster.png', title: 'A test clip', width: 640, height: 360 })
  + fence('chart', chart)
  + fence('scrolly', { title: 'Image story', image: { src: 'media/fixture-image.png', alt: 'Story picture', width: 1600, height: 900 }, steps: [
    { ...steps[0], state: undefined },
    { ...steps[1], state: undefined, view: { x: 0.25, y: 0.25, zoom: 2 } },
    { ...steps[2], state: undefined, highlight: { x: 0.3, y: 0.3, w: 0.3, h: 0.3 } },
  ] })
  + fence('scrolly', { title: 'Chart story', chart, steps })
  + '## After stories {#after-stories}\n\nThe report continues after both stories.\n\n'
  + Array.from({ length: 4 }, () => 'Trailing prose gives the final narrative step room to reach the middle of the viewport.\n\n').join('');

mkdirSync(join(root, 'tmp/verification'), { recursive: true });
const dir = mkdtempSync(join(root, 'tmp/verification/report-motion-'));
const media = Object.fromEntries(['fixture-image.png', 'tone.mp3', 'clip.mp4', 'poster.png'].map(name => [`media/${name}`, readFileSync(join(root, 'examples/every-block/media', name))]));
media['transcript.md'] = 'An audio transcript.';
const inputs = await reports(dir, { only: [], extra: Object.fromEntries(styles.map(s => [s, source(s)])), files: Object.fromEntries(styles.map(s => [s, media])) });
for (const [style, made] of Object.entries(inputs.generated)) assert.ok(made.ok && !made.diagnostics?.some(d => d.level === 'error'), `${style}: fixture renders cleanly`);
let browser;
const evidence = { cli: execFileSync(cli, ['--version'], { encoding: 'utf8' }).trim(), cases: [], captures: [], failures: [] };
const check = async (record, name, run) => {
  try { record[name] = await run() ?? true; }
  catch (error) { evidence.failures.push(`${record.style} @${record.width} ${record.mode}/${name}: ${error.message}`); record[name] = { error: error.message }; }
};
const frames = page => page.evaluate(() => new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done))));
const noAnimations = page => page.evaluate(() => document.getAnimations().filter(a => a.playState !== 'finished' && a.playState !== 'idle').map(a => ({ target: a.effect?.target?.className, name: a.animationName ?? a.transitionProperty })));
const cssFloor = page => page.evaluate(() => [...document.querySelectorAll('*')].flatMap(node =>
  [null, '::before', '::after'].flatMap(pseudo => {
    const css = getComputedStyle(node, pseudo);
    const moving = css.animationName.split(',').some(n => n.trim() !== 'none')
      || (css.transitionProperty !== 'none' && css.transitionDuration.split(',').some(n => parseFloat(n) > 0))
      || css.scrollBehavior === 'smooth';
    return moving ? [{ tag: node.tagName, class: node.className, pseudo, animation: css.animationName, transition: css.transitionProperty, duration: css.transitionDuration }] : [];
  })));
const visibleBlocks = page => page.evaluate(selector => [...document.querySelectorAll(selector)].map(node => {
  const css = getComputedStyle(node), box = node.getBoundingClientRect();
  return { tag: node.tagName, class: node.className, opacity: css.opacity, transform: css.transform, width: box.width, height: box.height, visible: css.display !== 'none' && css.visibility === 'visible' };
}), PRIMARY);

// A later component's real CSS motion must also obey the report's floor. This fixture exercises
// normal descendants and pseudo-elements without adding a production flag or runtime hook.
const laterComponentMotion = page => page.evaluate(() => {
  const sheet = [...document.styleSheets].find(s => !s.href);
  for (const rule of [
    '@keyframes report-proof {from {opacity:1} to {opacity:.5}}',
    '.rv-callout {animation: report-proof 2s infinite; transition: transform 2s}',
    '.rv-callout::before {content:""; animation: report-proof 2s infinite; transition: opacity 2s}',
  ]) sheet.insertRule(rule, sheet.cssRules.length);
  return document.getAnimations().filter(a => a.animationName === 'report-proof').length;
});

async function entrances(page, record) {
  const inventory = await page.evaluate(selector => {
    const nodes = [...document.querySelectorAll(selector)];
    window.__rpNodes = nodes;
    return nodes.map((node, i) => ({ i, tag: node.tagName, class: node.className, below: node.getBoundingClientRect().top >= innerHeight, marked: node.classList.contains('rv-enter') }));
  }, PRIMARY);
  const enabled = record.style !== 'brutalist';
  assert.ok(inventory.some(n => n.below), 'the fixture must exercise below-fold blocks');
  assert.ok(inventory.filter(n => !n.below).every(n => !n.marked), 'first-screen text starts at rest');
  if (enabled) assert.ok(inventory.filter(n => n.below).every(n => n.marked), 'every below-fold block has its entrance');
  else assert.ok(inventory.every(n => !n.marked), 'Brutalist deliberately leaves blocks at rest');
  await page.evaluate(() => { window.__rp.shifts.length = 0; });
  let captured = false;
  const results = [];
  for (const item of inventory) {
    const { before, during } = await page.evaluate(async i => {
      const n = window.__rpNodes[i];
      const layout = () => ({ top: n.offsetTop, width: n.offsetWidth, height: n.offsetHeight, page: document.documentElement.scrollHeight });
      const before = layout(); n.scrollIntoView({ block: 'center', behavior: 'instant' });
      await new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done)));
      const css = getComputedStyle(n);
      return { before, during: { opacity: Number(css.opacity), rise: new DOMMatrixReadOnly(css.transform).m42, runs: window.__rp.runs.get(n) ?? 0 } };
    }, item.i);
    if (!captured && enabled && item.marked && during.runs) {
      const animations = await page.evaluate(i => {
        window.__rpCapture = document.getAnimations().filter(a => a.effect?.target === window.__rpNodes[i]);
        for (const a of window.__rpCapture) a.pause();
        return window.__rpCapture.length;
      }, item.i);
      if (animations) {
        for (const fraction of [0, 0.25, 0.5, 0.75, 1]) {
          await page.evaluate(f => { for (const a of window.__rpCapture) { const t = a.effect.getTiming(); a.currentTime = Number(t.delay) + Number(t.duration) * f; } }, fraction);
          const file = `entrance-${record.style}-${record.width}-${fraction}.png`;
          await page.screenshot({ path: join(dir, file), animations: 'allow' });
          evidence.captures.push(file);
        }
        await page.evaluate(() => { for (const a of window.__rpCapture) a.finish(); });
        captured = true;
      }
    }
    const after = await page.evaluate(async i => {
      await Promise.all(document.getAnimations().filter(a => a.effect?.target === window.__rpNodes[i]).map(a => a.finished.catch(() => {})));
      const n = window.__rpNodes[i], css = getComputedStyle(n);
      return { top: n.offsetTop, width: n.offsetWidth, height: n.offsetHeight, page: document.documentElement.scrollHeight, opacity: Number(css.opacity), rise: new DOMMatrixReadOnly(css.transform).m42, runs: window.__rp.runs.get(n) ?? 0 };
    }, item.i);
    for (const field of ['top', 'width', 'height', 'page']) assert.ok(Math.abs(after[field] - before[field]) <= 1, `${item.tag}.${item.class}: ${field} changed during entrance: ${before[field]} -> ${after[field]}`);
    assert.equal(after.opacity, 1, `${item.tag} is readable when entered`);
    assert.equal(after.rise, 0, `${item.tag} rests at its own position`);
    if (enabled && item.marked) assert.equal(after.runs, 1, `${item.tag} enters exactly once`);
    if (record.style === 'scientific') assert.equal(during.rise, 0, 'Scientific fades without rising');
    results.push({ ...item, ...after });
  }
  await page.evaluate(() => scrollTo({ top: 0, behavior: 'instant' }));
  await frames(page);
  for (const item of inventory) {
    await page.evaluate(async i => {
      window.__rpNodes[i].scrollIntoView({ block: 'center', behavior: 'instant' });
      await new Promise(done => requestAnimationFrame(() => requestAnimationFrame(done)));
    }, item.i);
  }
  assert.deepEqual(await page.evaluate(() => window.__rpNodes.map(n => window.__rp.runs.get(n) ?? 0)), results.map(n => n.runs), 'returning to blocks never replays an entrance');
  const shifts = await page.evaluate(() => window.__rp.shifts);
  assert.deepEqual(shifts, [], 'entrances never shift layout');
  return { blocks: results, filmstrip: captured };
}

async function stories(page, record, reduced) {
  const stories = page.locator('[data-rv-scrolly]');
  assert.equal(await stories.count(), 2, 'image and chart stories both render');
  const states = [];
  for (let s = 0; s < 2; s++) {
    const story = stories.nth(s), step = story.locator('[data-rv-step]');
    for (let i = 0; i < 3; i++) {
      // Sample the step entering the middle band, before its text scrolls behind the sticky
      // figure. Content that has already passed above that figure is legitimately clipped.
      await step.nth(i).evaluate(n => { const r = n.getBoundingClientRect(); scrollTo({ top: scrollY + r.top - innerHeight * 0.52, behavior: 'instant' }); });
      await page.waitForFunction(({ s, i }) => document.querySelectorAll('[data-rv-scrolly]')[s].querySelectorAll('[data-rv-step]')[i].getAttribute('aria-current') === 'step', { s, i }, { timeout: 5000 });
      await frames(page);
      const result = await story.evaluate((n, i) => {
        const chosen = n.querySelectorAll('[data-rv-step]')[i];
        const figure = n.querySelector('figure[data-rv-chart]');
        const model = figure && window.__richview.charts().find(c => c.figure === figure.id);
        const prose = chosen.querySelector(':scope > p:not(.rv-scrolly__step-title)'), rect = prose.getBoundingClientRect();
        const at = document.elementFromPoint(Math.min(innerWidth - 1, Math.max(0, rect.x + rect.width / 2)), Math.min(innerHeight - 1, Math.max(0, rect.y + Math.min(8, rect.height / 2))));
        return { current: [...n.querySelectorAll('[data-rv-step]')].map(x => x.getAttribute('aria-current')), text: chosen.textContent.trim(), view: n.style.getPropertyValue('--rv-vz'), state: model?.state, pending: model && window.__richview.pending().includes(model.id), readable: prose.contains(at), textTop: rect.top, stageBottom: n.querySelector('.rv-scrolly__stage').getBoundingClientRect().bottom };
      }, i);
      assert.equal(result.current.filter(x => x === 'step').length, 1, 'one step is current');
      assert.match(result.text, new RegExp(['First', 'Second', 'Third'][i]), 'steps keep their authored order');
      if (s === 0) assert.equal(Number(result.view), i === 1 ? 2 : 1, 'scroll chooses the image view');
      else {
        assert.match(result.state, new RegExp(`${steps[i].state.year}.*${steps[i].state.region}`), `scroll applies chart playback/filter choices: ${JSON.stringify(result)}`);
        if (reduced) assert.equal(result.pending, false, 'reduced-motion chart steps draw settled');
      }
      const stepMotion = await step.evaluateAll(nodes => nodes.map(n => ({ opacity: getComputedStyle(n).opacity, transform: getComputedStyle(n).transform })));
      assert.ok(stepMotion.every(n => n.opacity === '1' && n.transform === 'none'), 'narrative text stays at rest');
      if (reduced) assert.deepEqual(await noAnimations(page), [], 'reduced-motion story changes have no CSS animation');
      states.push({ story: s, step: i, ...result });
      if (i === 1) {
        const file = `story-entry-${record.style}-${record.width}-${s}-${reduced ? 'reduce' : 'motion'}.png`;
        await page.screenshot({ path: join(dir, file), animations: 'allow' });
        evidence.captures.push(file);
      }
    }
    await step.nth(0).locator('a').focus();
    for (let i = 1; i < 3; i++) {
      await page.keyboard.press('Tab');
      assert.equal(await step.nth(i).locator('a').evaluate(a => a === document.activeElement), true, 'Tab reads the next step');
      assert.equal(await step.nth(i).getAttribute('aria-current'), 'step', 'keyboard focus selects its step');
    }
    const wheels = await page.evaluate(() => window.__rp.wheels ?? 0);
    await page.mouse.wheel(0, -1);
    await page.waitForFunction(wheels => window.__rp.wheels > wheels, wheels);
    await frames(page);
    await step.nth(0).evaluate(n => { const r = n.getBoundingClientRect(); scrollTo({ top: scrollY + r.top - innerHeight * 0.52, behavior: 'instant' }); });
    await page.waitForFunction(s => document.querySelectorAll('[data-rv-scrolly]')[s].querySelector('[data-rv-step]').getAttribute('aria-current') === 'step', s, { timeout: 5000 });
  }
  assert.ok(states.every(s => s.readable), `step text stays visible beside/below its sticky figure: ${JSON.stringify(states.filter(s => !s.readable))}`);
  assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1), 'the report fits its viewport');
  return states;
}

async function reduceInFlight(page, style) {
  await page.reload();
  await page.waitForFunction(() => window.__richview?.charts().length === 2);
  await page.evaluate(() => document.fonts.ready);
  await page.evaluate(() => window.__richview.settled());
  assert.ok(await laterComponentMotion(page) >= 2, 'the later-component CSS actually animates before reduction');
  if (style !== 'brutalist') {
    await page.evaluate(() => { window.__rp.pauseOnRun = true; });
    await page.locator('.rv-enter:not(.rv-entered)').first().evaluate(n => {
      window.__rpSwitchBlock = n;
      n.scrollIntoView({ block: 'center', behavior: 'instant' });
    });
    await page.waitForFunction(() => document.getAnimations().some(a => a.effect?.target === window.__rpSwitchBlock && a.playState === 'paused'));
  }
  await page.evaluate(() => {
    const now = performance.now.bind(performance), raf = requestAnimationFrame.bind(window);
    window.__rpClock = { now, raf, at: now(), queue: [] };
    performance.now = () => window.__rpClock.at;
    window.requestAnimationFrame = callback => { window.__rpClock.queue.push(callback); return window.__rpClock.queue.length; };
    const figure = document.querySelector('figure[data-rv-chart]');
    figure.querySelector('.rv-chart__option[aria-pressed="false"]').click();
    figure.querySelector('.rv-chart__control-button--play').click();
    window.__rpChanged = false;
    matchMedia('(prefers-reduced-motion: reduce)').addEventListener('change', () => { window.__rpChanged = true; }, { once: true });
  });
  try {
    assert.equal(await page.evaluate(() => window.__richview.charts()[0].playing), true, 'the probe starts actual playback');
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await page.waitForFunction(() => window.__rpChanged, null, { timeout: 5000 });
    const after = await page.evaluate(() => ({ pending: window.__richview.pending(), playing: window.__richview.charts().map(c => c.playing) }));
    assert.ok(after.playing.every(p => !p), 'changing to reduced motion stops ongoing playback');
    assert.deepEqual(after.pending, [], 'changing to reduced motion settles active WebGL glides immediately');
    assert.deepEqual(await cssFloor(page), [], 'the whole-page CSS floor follows the changed preference');
    assert.deepEqual(await noAnimations(page), [], 'the preference change cancels an in-flight block entrance');
    await page.evaluate(() => {
      window.__rpFirstPixels = [...document.querySelectorAll('canvas.rv-chart__canvas')].map(c => c.getContext('2d').getImageData(0, 0, c.width, c.height).data);
    });
    // The public settle operation explicitly draws the target state. A repair that merely
    // freezes the old pixels must fail this comparison even if it cancels every queued frame.
    await page.evaluate(() => window.__richview.settled());
    const changedShare = await page.evaluate(() => {
      const canvases = [...document.querySelectorAll('canvas.rv-chart__canvas')];
      const read = c => c.getContext('2d').getImageData(0, 0, c.width, c.height).data;
      const first = window.__rpFirstPixels, clock = window.__rpClock;
      for (let frame = 0; frame < 10; frame++) {
        clock.at += 1000 / 60;
        for (const callback of clock.queue.splice(0)) callback(clock.at);
      }
      return canvases.map((c, index) => {
        const last = read(c); let changed = 0;
        for (let i = 0; i < last.length; i += 4) if ([0, 1, 2, 3].some(k => Math.abs(last[i + k] - first[index][i + k]) > 3)) changed++;
        return changed / (last.length / 4);
      });
    });
    assert.ok(changedShare.every(s => s <= 0.005), `first reduced WebGL frame equals settled pixels: ${changedShare}`);
    await page.evaluate(() => { window.__rp.pauseOnRun = false; });
    await page.emulateMedia({ reducedMotion: 'no-preference' });
    const resumed = await page.evaluate(() => {
      const figure = document.querySelector('figure[data-rv-chart]');
      figure.querySelector('.rv-chart__option[aria-pressed="false"]').click();
      return window.__richview.pending();
    });
    assert.ok(resumed.length > 0, 'later reader changes animate again when reduced motion is turned off');
    return { ...after, changedShare, resumed };
  } finally {
    await page.evaluate(() => { const c = window.__rpClock; performance.now = c.now; window.requestAnimationFrame = c.raf; c.queue.splice(0).forEach(f => c.raf(f)); delete window.__rpClock; });
  }
}

try {
  browser = await launchBrowser(chromium, { headless: true, executablePath: chromiumPath() }, { TimeoutError: errors.TimeoutError });
  evidence.browser = browser.version();
  for (const style of styles) for (const width of widths) {
    for (const mode of ['motion', 'reduce', 'no-js']) {
      const record = { style, width, mode }, errors = [];
      evidence.cases.push(record);
      const context = await browser.newContext({ viewport: { width, height: HEIGHT }, deviceScaleFactor: 1, javaScriptEnabled: mode !== 'no-js', reducedMotion: mode === 'reduce' ? 'reduce' : 'no-preference' });
      try {
        await context.addInitScript(() => {
          window.__rp = { runs: new Map(), shifts: [] };
          window.addEventListener('wheel', () => { window.__rp.wheels = (window.__rp.wheels ?? 0) + 1; });
          document.addEventListener('transitionrun', e => {
            if (e.propertyName === 'opacity' && e.target.classList.contains('rv-enter')) {
              window.__rp.runs.set(e.target, (window.__rp.runs.get(e.target) ?? 0) + 1);
              if (window.__rp.pauseOnRun) for (const a of document.getAnimations().filter(a => a.effect?.target === e.target)) {
                a.pause(); const t = a.effect.getTiming(); a.currentTime = Number(t.delay) + Number(t.duration) * 0.4;
              }
            }
          }, true);
          new PerformanceObserver(list => { for (const e of list.getEntries()) window.__rp.shifts.push({ value: e.value, sources: e.sources.map(s => s.node?.className ?? s.node?.nodeName) }); }).observe({ type: 'layout-shift' });
        });
        const page = await context.newPage();
        page.on('pageerror', error => errors.push(error.message));
        await page.goto(`${inputs.base}/${style}/report.html`);
        await page.evaluate(() => document.fonts.ready);
        if (mode !== 'no-js') await page.waitForFunction(() => window.__richview?.charts().length === 2);
        if (mode === 'motion') {
          await check(record, 'entrances', () => entrances(page, record));
          await check(record, 'stories', () => stories(page, record, false));
          await check(record, 'changedPreference', () => reduceInFlight(page, style));
        } else {
          await check(record, 'visible', async () => {
            const blocks = await visibleBlocks(page);
            assert.ok(blocks.length > 15, 'the fixture covers the report block kinds');
            assert.ok(blocks.every(b => b.visible && b.width > 0 && b.height > 0 && b.opacity === '1' && b.transform === 'none'), JSON.stringify(blocks.filter(b => !b.visible || !b.width || !b.height || b.opacity !== '1' || b.transform !== 'none')));
            return blocks;
          });
          if (mode === 'reduce') {
            await laterComponentMotion(page);
            await check(record, 'cssFloor', async () => assert.deepEqual(await cssFloor(page), []));
            await check(record, 'stories', () => stories(page, record, true));
            await check(record, 'disclosuresAndExpand', async () => {
              const summaries = page.locator('.rv-report details > summary');
              assert.ok(await summaries.count() >= 2, 'table overflow and transcript disclosures are exercised');
              for (let i = 0; i < await summaries.count(); i++) {
                await summaries.nth(i).click();
                assert.equal(await summaries.nth(i).evaluate(n => n.parentElement.open), true);
                assert.deepEqual(await noAnimations(page), [], 'disclosures open without an animation');
              }
              const figure = page.locator('figure[data-rv-chart]').first();
              await figure.getByRole('button', { name: 'Expand', exact: true }).click();
              await page.locator('dialog[open]').waitFor();
              assert.deepEqual(await noAnimations(page), [], 'expanded view has no CSS transition');
              assert.deepEqual(await cssFloor(page), []);
              await page.keyboard.press('Escape');
              await page.locator('dialog[open]').waitFor({ state: 'hidden' });
            });
          } else {
            await check(record, 'storyOrder', async () => {
              const texts = await page.locator('[data-rv-scrolly] .rv-scrolly__step').allTextContents();
              assert.equal(texts.length, 6);
              for (const [i, text] of texts.entries()) assert.match(text, new RegExp(['First', 'Second', 'Third'][i % 3]));
              assert.equal(await page.locator('[data-rv-scrolly] .rv-chart__notice').isVisible(), true, 'the chart story keeps its no-runtime notice');
            });
          }
        }
        await check(record, 'errors', () => assert.deepEqual(errors, []));
        console.log(`${style} @${width} ${mode}: checked`);
      } finally { await context.close(); }
    }
  }
} finally {
  await browser?.close(); await inputs.close();
  writeFileSync(join(dir, 'report-motion.json'), `${JSON.stringify(evidence, null, 2)}\n`);
}
if (evidence.failures.length) throw new Error(`${evidence.failures.join('\n')}\nEvidence: ${dir}`);
console.log(`PASS report motion: ${evidence.cases.length} cases; captures for design review in ${dir}`);
