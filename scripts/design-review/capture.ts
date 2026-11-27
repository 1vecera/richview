// Design review capture for the build in this worktree: every case in every theme the
// build exposes, at each width. See README.md for outputs and methods.
import { launchBrowser } from "../../tools/rv-gate/lib/browser-launch.mts";
import { chromium, errors as playwrightErrors } from 'playwright';
import { existsSync, mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { loadavg } from 'node:os';
import { join, relative, resolve } from 'node:path';
import { parseArgs } from 'node:util';
import { chromiumPath, serveStatic, watchErrors } from '../browser-harness.ts';
import { buildFacts, contextOptions, discoverThemes, dist, pickCases, pickThemes, produce, requireBuild, viewport } from './outputs.ts';
import type { Capture, CaptureOutput, CapturedCase, PaceEntry, ReviewChart, ReviewManifest } from './manifest.ts';
import type { ReviewViewport } from './outputs.ts';
import { init, probe } from './probes.ts';
import { ENTRANCE_MS, POINTER_MS, SETTLED_MS, chartMotion, compare, reducedMotion } from './filmstrip.ts';
import { ENTRANCE_WINDOW_MS, POINTER_WINDOW_MS, THROTTLE, deviceFacts, pace } from './pacing.ts';
import { focusRelease, mountContinuity, tap } from './runtime.ts';
import { baselineCapture, composer, filmstripHtml, indexHtml, sheetHtml } from './sheets.ts';

const { values: args } = parseArgs({ options: {
  out: { type: 'string' }, cases: { type: 'string' }, themes: { type: 'string' }, widths: { type: 'string' },
  baseline: { type: 'string' }, port: { type: 'string' }, jobs: { type: 'string' }, quick: { type: 'boolean' }, help: { type: 'boolean' },
} });
if (args.help || !args.out) {
  console.log('Usage: node scripts/design-review/capture.ts --out DIR [--cases a,b] [--themes a,b] [--widths 1440,390] [--baseline DIR] [--port N] [--jobs N] [--quick]');
  process.exit(args.help ? 0 : 2);
}
const list = (value?: string) => value?.split(',').map(s => s.trim()).filter(Boolean);
requireBuild();
const started = Date.now();
const out = resolve(args.out);
const baselineDir = args.baseline && resolve(args.baseline);
if (baselineDir === out) throw new Error('--baseline must differ from --out');
const baseline: ReviewManifest | undefined = baselineDir ? JSON.parse(readFileSync(join(baselineDir, 'manifest.json'), 'utf8')) : undefined;
const themeList = await discoverThemes();
const themes = pickThemes(list(args.themes), themeList);
const views = (list(args.widths) ?? ['1440', '390']).map(Number).map(width => {
  if (!Number.isInteger(width) || width < 240 || width > 3840) throw new Error(`Width must be an integer from 240 to 3840: ${width}`);
  return viewport(width);
});
const cases = pickCases(list(args.cases), args.quick).map((item): CapturedCase & { outputs: Record<string, CaptureOutput> } =>
  ({ ...item, captures: [], pacing: [], outputs: {} }));
const jobs = Math.max(1, Number(args.jobs ?? 3));
const rel = (path: string) => relative(out, path);
const errors: string[] = [];
const log = (message: string) => console.log(`${((Date.now() - started) / 1000).toFixed(0).padStart(5)}s  ${message}`);
const writeJson = (file: string, value: unknown) => { writeFileSync(`${file}.tmp`, JSON.stringify(value, null, 2) + '\n'); renameSync(`${file}.tmp`, file); };
// The first chart of each type stands in for the others where a run is bounded.
const representatives = <T extends { type: string }>(charts: T[]) => charts.filter((chart, i) => charts.findIndex(c => c.type === chart.type) === i);
async function pool<T>(items: T[], run: (item: T) => Promise<void>) {
  const queue = [...items];
  await Promise.all(Array.from({ length: Math.min(jobs, queue.length) }, async () => { while (queue.length) await run(queue.shift()!); }));
}

// The output folder keeps only what this run writes for the selected cases.
for (const path of ['reports', 'sheets', '.compose', ...cases.map(item => item.id)]) rmSync(join(out, path), { recursive: true, force: true });
mkdirSync(join(out, '.compose'), { recursive: true });
for (const item of cases) {
  for (const theme of themes) {
    try { item.outputs[theme] = produce(item, theme, out); }
    catch (error) { item.outputs[theme] = { error: (error as Error).message.split('\n')[0] }; errors.push(`${item.id} · ${theme}: ${item.outputs[theme].error}`); }
  }
}
log(`Generated ${cases.length} cases × ${themes.length} themes (${themes.join(', ')})`);

const server = await serveStatic({ '/reports/': join(out, 'reports'), '/': dist }, {
  port: Number(args.port ?? 0),
  route: (path, url, res) => { if (path !== '/favicon.ico') return false; res.writeHead(204); res.end(); return true; },
});
const browser = await launchBrowser(chromium, { executablePath: chromiumPath(), headless: true, args: ['--no-sandbox'] }, { TimeoutError: playwrightErrors.TimeoutError });
let manifest: ReviewManifest;
try {
  const device: ReviewManifest['device'] = await deviceFacts(browser);
  const compose = await composer(browser);

  // Settled page: real clock, everything revealed, animations finished.
  async function still(view: ReviewViewport, url: string, dir: string) {
    const context = await browser.newContext(contextOptions(view));
    await context.addInitScript(init);
    const page = await context.newPage();
    const messages: string[] = [];
    watchErrors(page, messages);
    try {
      await page.goto(url, { waitUntil: 'load', timeout: 60000 });
      const ready = await probe(page, 'settle');
      await probe(page, 'reveal');
      await probe(page, 'settle');
      const charts: ReviewChart[] = await probe(page, 'charts');
      const glyphs = await probe(page, 'glyphs');
      const { page: size, ...layout } = await probe(page, 'layout');
      // Crops come first and in view: capturing beyond the viewport resizes the page,
      // which makes every chart lay out again.
      for (const chart of charts) {
        chart.glyph_checks = glyphs[chart.id] ?? null;
        await probe(page, 'center', chart.id);
        const clip = await probe(page, 'clip', chart.id);
        if (!clip) continue;
        const { page: fullPage, ...rect } = clip;
        chart.box = { width: rect.width, height: rect.height };
        chart.crop = rel(join(dir, `${chart.id}.png`));
        await page.screenshot({ path: join(out, chart.crop), fullPage, clip: rect });
      }
      await page.evaluate(() => scrollTo({ top: 0, behavior: 'instant' }));
      await page.screenshot({ path: join(dir, 'page.png'), fullPage: true });
      return { title: await page.title(), page: rel(join(dir, 'page.png')), page_size: size, charts, diagnostics: { ready, console: messages, ...layout } };
    } finally {
      await context.close();
    }
  }

  async function capture({ item, theme, view }: { item: (typeof cases)[number]; theme: string; view: ReviewViewport }) {
    const begun = Date.now();
    const dir = join(out, item.id, `${theme}-${view.width}`);
    mkdirSync(dir, { recursive: true });
    const output = item.outputs[theme];
    const url = server.base + output.url;
    const capture: Capture = { theme, width: view.width, url: output.url, native: output.native, timing_s: {} };
    const lap = (name: string, since: number) => { capture.timing_s![name] = +((Date.now() - since) / 1000).toFixed(1); return Date.now(); };
    item.captures.push(capture);
    if (output.error) { capture.error = output.error; return; }
    try {
      const record = Object.assign(capture, await still(view, url, dir));
      let since = lap('still', begun);
      // A figure taller than the viewport gets a taller viewport of the same width, so the whole
      // chart is in view for its entrance and pointer target and chart layout is unchanged.
      const tallest = Math.max(0, ...record.charts.map(c => c.box?.height ?? 0));
      const motionView = { ...view, height: Math.max(view.height, tallest + 40) };
      if (motionView.height !== view.height) record.motion_viewport = { width: view.width, height: motionView.height };
      const reduced = await reducedMotion(browser, motionView, url, record.charts);
      record.reduced_motion_page_top = reduced.page_top;
      since = lap('reduced_motion', since);
      const consoleErrors = new Set([...record.diagnostics.console, ...reduced.errors]);
      for (const chart of record.charts) {
        const { settled, ...result } = reduced.charts[chart.id];
        chart.reduced_motion = result;
      }
      for (const chart of args.quick ? representatives(record.charts) : record.charts) {
        let motion;
        try { motion = await chartMotion(browser, motionView, url, chart); }
        catch (error) {
          chart.motion_error = (error as Error).message.split('\n')[0];
          errors.push(`${item.id} · ${theme} · ${view.width}px · ${chart.id}: ${chart.motion_error}`);
          continue;
        }
        motion.errors.forEach(e => consoleErrors.add(e));
        chart.pointer = motion.target;
        chart.entered_while_waiting = motion.entered_while_waiting;
        const settle = compare(motion.settled, reduced.charts[chart.id].settled);
        // Elements that keep a finished animation's fill stay composited, which shifts edge
        // antialiasing slightly; a settled state differing on 0.5% of the crop or less matches.
        chart.settles_to_reduced = { matches: settle.diff_ratio! <= .005, ...settle };
        chart.filmstrip = rel(join(dir, `${chart.id}-filmstrip.png`));
        await compose.render(filmstripHtml({ heading: `${item.id} · ${chart.title || chart.id}`, note: `${chart.id} · ${chart.type} · ${theme} · ${motionView.width}×${motionView.height} @${view.dpr}x · real clock, canvas frames`, view, motion }), join(out, chart.filmstrip));
      }
      since = lap('filmstrips', since);
      // The runtime around a reader: mount continuity on every capture; taps (phones) and legend
      // focus release (desktops, charts with a legend) on one chart of each type, in the first theme only.
      const mount = await mountContinuity(browser, view, url);
      record.mount = mount;
      if (theme === themes[0]) {
        record.interaction = [];
        for (const chart of representatives(record.charts)) {
          if (view.mobile) record.interaction.push({ chart: chart.id, type: chart.type, probe: 'tap', ...await tap(browser, view, url, chart.id) });
          else if (chart.legend_entries >= 2) record.interaction.push({ chart: chart.id, type: chart.type, probe: 'focus-release', ...await focusRelease(browser, view, url, chart.id) });
        }
      }
      lap('runtime', since);
      record.diagnostics.console = [...consoleErrors];
      const filmed = record.charts.filter(c => c.settles_to_reduced);
      record.summary = {
        glyph_findings: record.charts.reduce((n, c) => n + (c.glyph_checks?.length ?? 0), 0),
        unmeasured_charts: record.charts.filter(c => !c.glyph_checks).length,
        text_overlaps: record.diagnostics.text.overlaps, text_clipping: record.diagnostics.text.clipping, scroll_hidden_text: record.diagnostics.text.scroll_hidden,
        overflow_px: record.diagnostics.overflow.px, cls: record.diagnostics.cls.value, console_errors: record.diagnostics.console.length,
        reduced_motion_failures: record.charts.filter(c => !c.reduced_motion!.equal).length + (reduced.page_top.equal ? 0 : 1),
        settle_mismatches: filmed.filter(c => !c.settles_to_reduced!.matches).length,
        mount_dh: record.mount?.first_frame?.dh ?? null, settled_ms: record.diagnostics.ready.settled_ms, runtime_ready: record.diagnostics.ready.ok,
        settled_dh: record.mount?.settled?.summed_abs_dh ?? null,
        interaction_failures: record.interaction ? record.interaction.filter(r => !r.ok && !r.error).length : null,
        native_warnings: output.native?.findings.length ?? 0,
      };
      log(`${item.id} · ${theme} · ${view.width}px: ${record.charts.length} charts, ${filmed.length} filmed (${((Date.now() - begun) / 1000).toFixed(1)} s)`);
    } catch (error) {
      capture.error = (error as Error).message.split('\n')[0];
      errors.push(`${item.id} · ${theme} · ${view.width}px: ${capture.error}`);
      log(`FAILED ${errors.at(-1)}`);
    }
  }
  await pool(cases.flatMap(item => themes.flatMap(theme => views.map(view => ({ item, theme, view })))), capture);

  // Pacing runs alone after the parallel captures, for the first theme.
  const pacingViews = args.quick ? views.slice(0, 1) : views;
  for (const item of cases) for (const view of pacingViews) {
    const record = item.captures.find(c => c.theme === themes[0] && c.width === view.width);
    for (const chart of representatives(record?.charts ?? [])) for (const throttle of THROTTLE) {
      const entry: PaceEntry = { theme: themes[0], width: view.width, chart: chart.id, type: chart.type, throttle };
      try { Object.assign(entry, await pace(browser, view, server.base + record!.url, chart, throttle, device.frame_interval_ms!)); }
      catch (error) { entry.error = (error as Error).message.split('\n')[0]; errors.push(`pacing ${item.id} ${chart.id} ${throttle}x: ${entry.error}`); }
      item.pacing.push(entry);
    }
    log(`Paced ${item.id} at ${view.width}px`);
  }
  device.load_average_after_pacing = loadavg().map(v => +v.toFixed(2));

  // One sheet per case; with a baseline, each theme shows baseline and current pages.
  mkdirSync(join(out, 'sheets'), { recursive: true });
  const pageOf = (root: string, capture?: Capture) => ({ file: capture?.page && existsSync(join(root, capture.page)) ? join(root, capture.page) : null, summary: capture?.summary });
  await pool(cases, async item => {
    const shots = (theme: string, view: ReviewViewport) => {
      const now = item.captures.find(c => c.theme === theme && c.width === view.width) ?? { theme, width: view.width };
      const current = { label: 'Current', ...pageOf(out, now) };
      return baseline ? [{ label: 'Baseline', ...pageOf(baselineDir!, baselineCapture(baseline, item, now)) }, current] : [current];
    };
    try {
      await compose.render(sheetHtml({ item, title: item.captures.find(c => c.title)?.title, views, themes, shots }), join(out, `sheets/${item.id}.png`), join(out, '.compose', `${item.id}.html`));
      item.sheet = `sheets/${item.id}.png`;
    } catch (error) {
      errors.push(`sheet ${item.id}: ${(error as Error).message.split('\n')[0]}`);
    }
  });
  await compose.close();
  log('Composed sheets');

  manifest = {
    schema: 'richview-design-review/1', captured_at: new Date(started).toISOString(), duration_s: Math.round((Date.now() - started) / 1000),
    build: buildFacts(), device,
    options: { quick: !!args.quick, jobs, cases: cases.map(c => c.id), themes, widths: views.map(v => v.width), baseline: baselineDir ? relative(out, baselineDir) : null },
    themes: themeList, viewports: views,
    timeline: { entrance_ms: ENTRANCE_MS, pointer_ms: POINTER_MS, settled_ms: SETTLED_MS, pacing_entrance_window_ms: ENTRANCE_WINDOW_MS, pacing_pointer_window_ms: POINTER_WINDOW_MS, throttle: THROTTLE },
    cases: cases.map(({ outputs, ...item }) => item),
    errors,
  };
  writeJson(join(out, 'manifest.json'), manifest);
  const link = (path: string) => relative(out, join(baselineDir!, path));
  writeFileSync(join(out, 'index.html'), indexHtml({ manifest, baseline, link }));
} finally {
  rmSync(join(out, '.compose'), { recursive: true, force: true });
  await browser.close();
  await server.close();
}
log(`Wrote ${join(out, 'index.html')} and manifest.json${errors.length ? ` with ${errors.length} errors` : ''}`);
if (errors.length) process.exitCode = 1;
