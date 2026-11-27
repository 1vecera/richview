// No-JS reading and the runtime drill (DEL-57, ARC-05, FEA-CH-64, FEA-RP-39). Every example and
// chart form is read three ways at desktop and phone widths: with the runtime, without scripts,
// and with scripts but no WebGL. Without the runtime every text, table row and tab panel is
// readable and every chart's figure keeps the box the runtime fills, with its title and the
// notice in place of the chart; no static chart, image or table stands in. The drill then shows
// the interactive check failing with one named error when the runtime is missing and passing
// straight after with it present.
import assert from 'node:assert/strict';
import { copyFileSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { harness, watchErrors, verificationShard } from './browser-harness.ts';
import { figures, reports, settle } from './report-runtime.ts';

const NOTICE = 'This chart cannot be displayed here: it needs JavaScript and WebGL.';
const MISSING = 'The report runtime did not start';
const WIDTHS = [1280, 390];
const MODES = ['live', 'no-js', 'no-webgl'];
const RECOVERY_REPORT = 'sales-report';
const admitted = process.env.RICHVIEW_VERIFY_PARENT === 'no-js';
const outAt = process.argv.indexOf('--out');
const out = outAt < 0 ? undefined : process.argv[outAt + 1];
if (outAt >= 0 && (!out || out.startsWith('--'))) throw new Error('--out requires a result directory');
const h = await harness('no-js');
let inputs;
let selection;
let recovery: { missing: string; present: string } | undefined;
const cases = [];
const views: { name: string; width: number; mode: string }[] = [];
const failures: string[] = [];
const check = async (label: string, fn: () => unknown) => {
  try {
    await fn();
  } catch (error) {
    failures.push(`${label}: ${(error as Error).message}`);
  }
};
const near = (a: number, b: number) => Math.abs(a - b) <= 1.01;

/** Text, tables and tab panels as a reader without scripts sees them. */
const reading = (page: import('playwright').Page) => page.evaluate(() => {
  const shown = (node: Element) => node.getClientRects().length > 0 && getComputedStyle(node).visibility !== 'hidden';
  const tables = [...document.querySelectorAll('table')].filter(t => !t.closest('figure[data-rv-chart]'));
  return {
    text: (document.querySelector('main, article, body')! as HTMLElement).innerText.trim().length,
    panels: [...document.querySelectorAll('.rv-tabpanel')].map(p => shown(p)),
    tables: tables.map(t => ({ rows: t.querySelectorAll('tbody tr').length, shownRows: [...t.querySelectorAll('tbody tr')].filter(shown).length })),
  };
});

/** The interactive check every chart suite starts with: the runtime must answer. */
async function interactive(page: import('playwright').Page) {
  const started = await page.waitForFunction(() => window.__richview, null, { timeout: 5_000 }).then(() => true, () => false);
  if (!started) throw new Error(MISSING);
  await page.evaluate(() => window.__richview!.settled());
}

try {
  inputs = await reports(h.dir);
  // A bare selector is never admission to run only part of the normal full check.
  selection = admitted ? verificationShard(inputs.names, { key: name => name }) : {
    cases: inputs.names,
    shard: { index: 0, count: 1, cohort: [...inputs.names], selected: [...inputs.names] },
  };
  if (admitted) assert.ok(selection.shard.cohort.includes(RECOVERY_REPORT), 'The complete no-JS cohort must include its recovery report');
  for (const width of WIDTHS) {
    const viewport = { width, height: 900 };
    const live = await h.browser.newContext({ viewport, reducedMotion: 'reduce' });
    const off = await h.browser.newContext({ viewport, javaScriptEnabled: false });
    const blocked = await h.browser.newContext({ viewport, reducedMotion: 'reduce' });
    // No WebGL: context creation fails as on a host without a GPU.
    await blocked.addInitScript(() => {
      const get = HTMLCanvasElement.prototype.getContext;
      HTMLCanvasElement.prototype.getContext = function (this: HTMLCanvasElement, kind: string, ...rest: [options?: unknown]) {
        return /webgl/i.test(kind) ? null : get.call(this, kind, ...rest);
      } as typeof get;
    });
    for (const name of selection.cases) {
      const url = `${inputs.base}/${name}/report.html`;
      const label = `${name} @${width}`;
      const item: { name: string; width: number; figures?: number } = { name, width };
      cases.push(item);
      // With the runtime: the boxes the reader finally sees.
      const page = await live.newPage();
      await page.goto(url);
      await settle(page);
      const drawn = await figures(page);
      await page.close();
      views.push({ name, width, mode: 'live' });
      // Without scripts.
      const plain = await off.newPage();
      await plain.goto(url);
      const still = await figures(plain);
      const read = await reading(plain);
      await plain.close();
      item.figures = still.length;
      await check(`${label}: text reads`, () => assert.ok(read.text > 0));
      await check(`${label}: every tab panel shows`, () => assert.ok(read.panels.every(Boolean), JSON.stringify(read.panels)));
      await check(`${label}: every table row shows`, () => assert.ok(read.tables.every(t => t.rows === t.shownRows), JSON.stringify(read.tables)));
      await check(`${label}: every figure is present`, () => assert.equal(still.length, drawn.length));
      for (const [i, fig] of still.entries()) {
        const at = `${label} #${i}`;
        await check(`${at}: notice`, () => assert.equal(fig.notice?.trim(), NOTICE));
        await check(`${at}: title`, () => assert.equal(fig.title, drawn[i].title));
        await check(`${at}: no stand-in chart, image or table`, () => assert.deepEqual([fig.canvas, fig.svg, fig.img], [0, 0, 0]));
        // A chart in a closed tab panel has no box with the runtime until its tab opens.
        if (drawn[i].frame!.width === 0) continue;
        await check(`${at}: the frame keeps its final box`, () => {
          assert.ok(near(fig.frame!.width, drawn[i].frame!.width), `${fig.frame!.width} vs ${drawn[i].frame!.width}`);
          assert.ok(near(fig.frame!.height, drawn[i].frame!.height), `${fig.frame!.height} vs ${drawn[i].frame!.height}`);
        });
        await check(`${at}: nothing above the frame moves`, () => assert.ok(near(fig.frame!.y - fig.figure!.y, drawn[i].frame!.y - drawn[i].figure!.y), `${fig.frame!.y - fig.figure!.y} vs ${drawn[i].frame!.y - drawn[i].figure!.y}`));
      }
      views.push({ name, width, mode: 'no-js' });
      // Scripts on, WebGL blocked: the notice appears with no script error and no shift.
      const errors: string[] = [];
      const noGl = await blocked.newPage();
      watchErrors(noGl, errors);
      await noGl.goto(url);
      await interactive(noGl).catch(error => errors.push(error.message));
      const fallback = await figures(noGl);
      await noGl.close();
      await check(`${label}: no script error without WebGL`, () => assert.deepEqual(errors, []));
      for (const [i, fig] of fallback.entries()) {
        const at = `${label} #${i} without WebGL`;
        // A closed tab panel shows nothing while scripts run the tabs.
        if (fig.frame!.width === 0) continue;
        await check(`${at}: notice`, () => assert.equal(fig.notice?.trim(), NOTICE));
        await check(`${at}: no canvas`, () => assert.equal(fig.canvas, 0));
        await check(`${at}: the frame keeps the no-script box`, () => {
          assert.ok(near(fig.frame!.height, still[i].frame!.height), `${fig.frame!.height} vs ${still[i].frame!.height}`);
          assert.ok(near(fig.frame!.y - fig.figure!.y, still[i].frame!.y - still[i].figure!.y));
        });
      }
      views.push({ name, width, mode: 'no-webgl' });
    }
    await Promise.all([live.close(), off.close(), blocked.close()]);
  }
  // The drill: without the runtime the interactive check fails with its one named error, and
  // passes straight after with the runtime present.
  if (!admitted || selection.cases.includes(RECOVERY_REPORT)) {
    const drill = await h.browser.newContext({ viewport: { width: 1280, height: 900 } });
    const page = await drill.newPage();
    const url = `${inputs.base}/${RECOVERY_REPORT}/report.html`;
    await page.route(url, async route => {
      const response = await route.fetch();
      const body = (await response.text()).replace(/<script\b[^>]*>(?:(?!<\/script>)[\s\S])*richview_runtime(?:(?!<\/script>)[\s\S])*<\/script>/g, '');
      await route.fulfill({ response, body });
    });
    await page.goto(url);
    const missing = await interactive(page).then(() => 'passed', error => error.message);
    await page.unroute(url);
    await page.goto(url);
    const present = await interactive(page).then(() => 'passed', error => error.message);
    await drill.close();
    await check('drill: a missing runtime fails with its named error', () => assert.equal(missing, MISSING));
    await check('drill: the runtime present passes', () => assert.equal(present, 'passed'));
    recovery = { missing, present };
  }
  assert.deepEqual(failures, []);
  console.log(`PASS ${cases.length} report views read without scripts and without WebGL with every box kept; the drill fails on a missing runtime and recovers`);
} finally {
  try {
    const completed = selection?.cases.filter((name: string) =>
      WIDTHS.every(width => MODES.every(mode => views.some(view => view.name === name && view.width === width && view.mode === mode)))
      && (name !== RECOVERY_REPORT || recovery)) ?? [];
    const record = {
      engine: h.engine, cases, failures,
      admission: admitted ? 'no-js' : null,
      ...(selection ? { shard: selection.shard } : {}),
      completed, views,
      ...(recovery ? { recovery } : {}),
    };
    const filename = admitted && selection
      ? `no-js-${selection.shard.index}-of-${selection.shard.count}.json`
      : admitted ? 'no-js-invalid-shard.json' : 'no-js.json';
    const artifact = h.save(filename, record);
    if (out) {
      mkdirSync(out, { recursive: true });
      copyFileSync(artifact, join(out, filename));
    }
  } finally {
    try { await inputs?.close(); }
    finally { await h.close(); }
  }
}
