// Motion check of every chart type's entrance (FEA-CH-75, DEL-75, DEL-30). The every-block
// report in Sleek runs in Chromium on Playwright's paused clock, which fakes performance.now,
// requestAnimationFrame and timers, so the check steps the runtime's own clock one 16 ms frame
// at a time and needs no hook in the runtime. Per chart it proves, from canvas pixels and the
// model's boxes:
//   - the entrance waits until half the chart is in view, then plays once;
//   - the marks' box differs less and less from the settled frame;
//   - the pixels of axes, titles and legends stay where they are;
//   - the last animated frame equals the settled frame (DEL-30's 0.5 %);
//   - under reduced motion the first frame is the settled frame;
//   - scrolling away and back, and a resize, replay nothing.
// A chart whose model has no entrance, or that starts on the first screen, must draw settled at
// once. The model does not carry the style's timing, so the entrance length is the number of
// frames until the runtime reports the chart settled; the fractions are taken of that.
//
// A report mounts its charts one at a time and a chart far from the screen holds no canvas until
// it comes near (ARC-18, docs/MOTION.md). So every page first waits, at the top where no entrance
// starts, until all charts are mounted, and a frame is read only once the chart it belongs to is
// on screen and drawn; one that is not drawn while it shows fails as `drawn`.
//
// Results go to {out}/motion/types.json, one filmstrip per chart to {out}/motion/filmstrips/
// (frames at 0, 25, 50, 75 and 100 %, at 1x). RICHVIEW_BIN and RICHVIEW_DIST point at another
// build than target/release and dist/web.
//
//   node scripts/verify-motion.mjs [--out DIR] [--only type,type]
import { launchBrowser } from "../tools/rv-gate/lib/browser-launch.mjs";
import { spawnSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { basename, join, resolve } from 'node:path';
import { parseArgs } from 'node:util';
import { chromium, errors as playwrightErrors } from 'playwright';
import sharp from 'sharp';
import { CHROMIUM_ARGS, CLOCK_START, settle } from '../tools/rv-gate/lib/captures.mjs';
import { root } from './browser-harness.mjs';
import { chartVerdict, compareFrames, equalVerdict, frameIndexes, LIMITS, progressVerdict, stillVerdict } from './motion-checks.mjs';

const { values: options } = parseArgs({ options: { out: { type: 'string' }, only: { type: 'string' } } });
const cli = process.env.RICHVIEW_BIN ?? join(root, 'target/release/richview');
const dist = process.env.RICHVIEW_DIST ?? join(root, 'dist/web');
if (!existsSync(cli) || !existsSync(join(dist, 'richview_site_bg.wasm'))) throw new Error('Build the current Rust release first: cargo run --release --locked -p richview-build');
mkdirSync(join(root, 'tmp/verification'), { recursive: true });
const out = resolve(options.out ?? mkdtempSync(join(root, 'tmp/verification/motion-')));
const work = join(out, 'work');
const strips = join(out, 'motion/filmstrips');
mkdirSync(strips, { recursive: true });

// 1440 px wide as the captures; short, so that a chart at the top of the screen leaves its
// neighbours below the half that starts an entrance.
const VIEWPORT = { width: 1440, height: 600 };
const FRAME_MS = 16;
const MAX_FRAMES = 400;
const MOUNT_STEPS = 900;
const WORKERS = 4;
const sleep = ms => new Promise(accept => setTimeout(accept, ms));

// The every-block report, rendered in Sleek.
rmSync(work, { recursive: true, force: true });
cpSync(join(root, 'examples/every-block'), work, { recursive: true, filter: path => !['research', 'Rendered', 'tmp', 'node_modules'].includes(basename(path)) });
const made = spawnSync(cli, ['generate', join(work, 'report.rmd'), '--style', 'sleek', '--format', 'json', '-o', 'motion.html'], { encoding: 'utf8', env: { ...process.env, RICHVIEW_PUBLIC_DIR: dist }, timeout: 180_000, maxBuffer: 64 << 20 });
if (made.status !== 0) throw new Error(`generate failed: ${(made.stderr || made.stdout).slice(0, 2000)}`);
const url = 'file://' + join(work, 'Rendered/motion.html');

// Runs inside the page. Frames are read through a copy, because the visible canvas is a
// presentation target; the masks are drawn from the model's own items, so a box is as tight as
// the item (a line is its stroke, not its bounding box).
function pageKit(compareSource, limits) {
  const compare = new Function(`return ${compareSource}`)();
  const MARK = new Set(['mark', 'band', 'rug']);
  const LATE = new Set(['label', 'annotation', 'leader', 'missing']);
  const kit = (window.__mv = { models: {}, settledFrames: {}, masks: {}, palettes: {}, frames: {} });
  const canvasOf = id => document.getElementById(kit.models[id].figure).querySelector('canvas');
  const scratch = (w, h) => { const c = new OffscreenCanvas(w, h); return [c, c.getContext('2d', { willReadFrequently: true })]; };
  const path2d = path => {
    const p = new Path2D();
    for (const c of path.cmds) {
      if (c.c === 'm') p.moveTo(...c.p); else if (c.c === 'l') p.lineTo(...c.p);
      else if (c.c === 'c') p.bezierCurveTo(...c.c1, ...c.c2, ...c.p); else p.closePath();
    }
    return p;
  };
  const paint = (x, item, pad) => {
    if (item.kind === 'rect') x.fillRect(item.rect.x - pad, item.rect.y - pad, item.rect.w + 2 * pad, item.rect.h + 2 * pad);
    else if (item.kind === 'text') x.fillRect(item.bounds.x - pad, item.bounds.y - pad, item.bounds.w + 2 * pad, item.bounds.h + 2 * pad);
    else if (item.kind === 'line') { x.lineWidth = item.stroke.width + 2 * pad; x.stroke(path2d(item.path)); }
    else if (item.kind === 'area') { const p = path2d(item.path); x.fill(p); x.lineWidth = 2 * pad; if (pad) x.stroke(p); }
    else if (item.kind === 'point') { x.beginPath(); x.arc(item.center[0], item.center[1], item.radius + pad, 0, 2 * Math.PI); x.fill(); }
  };
  // A fill the eye cannot tell from the page (a heat cell at 5 % alpha) shows no entrance of its own.
  const visible = item => item.kind === 'line' || item.kind === 'text' || item.stroke || !item.fill || item.fill.color[3] * 255 > limits.channel;
  const mask = (model, w, h, wanted, pad) => {
    const [, x] = scratch(w, h);
    const scale = w / model.geometry.width;
    x.scale(scale, scale);
    x.fillStyle = x.strokeStyle = '#000';
    x.lineJoin = x.lineCap = 'round';
    for (const item of [...model.geometry.panels.flatMap(panel => panel.items), ...(model.geometry.figure_items ?? [])]) if (wanted(item.role) && visible(item)) paint(x, item, pad);
    const data = x.getImageData(0, 0, w, h).data;
    return Uint8Array.from({ length: w * h }, (_, i) => (data[i * 4 + 3] > 0 ? 1 : 0));
  };
  const png = (data, w, h) => { const [c, x] = scratch(w, h); x.putImageData(new ImageData(data, w, h), 0, 0); return c.convertToBlob().then(blob => blob.arrayBuffer()).then(buffer => Array.from(new Uint8Array(buffer))); };
  const read = id => { const c = canvasOf(id); const [o, x] = scratch(c.width, c.height); x.drawImage(c, 0, 0); return x.getImageData(0, 0, c.width, c.height); };
  const pending = id => window.__richview.pending().includes(id);
  Object.assign(kit, {
    // Models, masks and the way each chart starts: settled at mount or waiting for its entrance.
    // A chart far from the screen has no canvas to measure, so `sizes` gives each chart's canvas
    // size as the reference page drew it; `same` reports a canvas of another size.
    init(sizes = {}) {
      for (const model of window.__richview.charts()) {
        const c = document.getElementById(model.figure).querySelector('canvas');
        const [w, h] = sizes[model.id] ?? [c.width, c.height];
        const late = mask(model, w, h, role => LATE.has(role), 2);
        const marks = mask(model, w, h, role => MARK.has(role), 2);
        const still = mask(model, w, h, role => !MARK.has(role) && !LATE.has(role), 0);
        // What animates is not judged as still; a mark under a label is not judged as a mark.
        const mark = marks.map((v, i) => (v && !late[i] ? 1 : 0));
        // With no visible mark (a pivot of near-transparent cells) the labels are what enters.
        kit.masks[model.id] = { mark: mark.includes(1) ? mark : late, still: still.map((v, i) => (v && !marks[i] && !late[i] ? 1 : 0)) };
        kit.models[model.id] = { id: model.id, figure: model.figure, kind: model.geometry.kind, entrance: model.geometry.entrance ?? null, width: w, height: h, settled: !pending(model.id), title: document.getElementById(model.figure).querySelector('.rv-chart__title')?.textContent.trim() ?? '' };
      }
      return Object.values(kit.models);
    },
    async loadSettled(data) {
      for (const [id, bytes] of Object.entries(data)) {
        const bitmap = await createImageBitmap(new Blob([new Uint8Array(bytes)], { type: 'image/png' }));
        const [, x] = scratch(bitmap.width, bitmap.height);
        x.drawImage(bitmap, 0, 0);
        const frame = (kit.settledFrames[id] = x.getImageData(0, 0, bitmap.width, bitmap.height));
        // The colours the marks are drawn in, to tell a mark passing over a gridline from a gridline moving.
        const counts = new Map(), m = kit.masks[id].mark;
        let total = 0;
        for (let i = 0; i < m.length; i++) if (m[i]) { const o = i * 4, key = (frame.data[o] >> 4) << 8 | (frame.data[o + 1] >> 4) << 4 | frame.data[o + 2] >> 4; counts.set(key, (counts.get(key) ?? 0) + 1); total++; }
        kit.palettes[id] = [...counts].filter(([, n]) => n >= total * 0.01).sort((p, q) => q[1] - p[1]).slice(0, 12).flatMap(([key]) => [(key >> 8 & 15) * 16 + 8, (key >> 4 & 15) * 16 + 8, (key & 15) * 16 + 8]);
      }
    },
    async dumpRefs() { return Object.fromEntries(await Promise.all(Object.keys(kit.models).map(async id => { const f = read(id); return [id, await png(f.data, f.width, f.height)]; }))); },
    // Where the chart is on the page and how much of it shows.
    place(id) { const r = document.getElementById(kit.models[id].figure).getBoundingClientRect(); return { top: r.top + scrollY, height: r.height }; },
    scrollTo(top) { window.scrollTo({ top, behavior: 'instant' }); },
    pending,
    // Whether the chart is drawn: its frame says so and its canvas has pixels.
    drawn(id) { const c = canvasOf(id); return c.width > 0 && c.height > 0 && c.closest('.rv-chart__frame').hasAttribute('data-rv-painted'); },
    // One frame of the entrance, kept; says whether the chart still waits to settle.
    keep(id) { (kit.frames[id] ??= []).push(read(id).data); return pending(id); },
    same(id) {
      const f = read(id);
      if (f.width !== kit.settledFrames[id].width || f.height !== kit.settledFrames[id].height) return { size: [f.width, f.height] };
      return compare(f.data, kit.settledFrames[id].data, kit.masks[id], limits.channel, kit.palettes[id]);
    },
    // The chosen frames against the settled frame, and every frame's still-part share.
    async score(id, picks) {
      const frames = kit.frames[id], ref = kit.settledFrames[id].data, m = kit.masks[id];
      const all = frames.map(f => compare(f, ref, m, limits.channel, kit.palettes[id]));
      const shown = [];
      for (const i of picks) shown.push(await png(new Uint8ClampedArray(frames[i]), kit.models[id].width, kit.models[id].height));
      const result = { at: picks.map(i => all[i]), stills: all.map(s => s.still), frames: frames.length, settledDraw: all.at(-1).all, pngs: shown };
      delete kit.frames[id];
      return result;
    },
  });
  return kit;
}
const install = `(${pageKit})(${JSON.stringify(compareFrames.toString())}, ${JSON.stringify(LIMITS)})`;

async function open(browser, { reduced = false } = {}) {
  const context = await browser.newContext({ viewport: VIEWPORT, deviceScaleFactor: 1, reducedMotion: reduced ? 'reduce' : 'no-preference', colorScheme: 'light' });
  const page = await context.newPage();
  await page.clock.install({ time: CLOCK_START });
  await page.clock.pauseAt(CLOCK_START);
  await page.goto(url, { waitUntil: 'networkidle' });
  await page.evaluate(install);
  // Time for the runtime to start and mount, without touching the charts' entrances.
  for (let i = 0; i < 15; i++) { await page.clock.runFor(100); await sleep(25); }
  // The charts mount one by one on the page's clock, the far ones 40 ms apart. The page stays at
  // its top meanwhile, where a chart is either on the first screen or out of view, so none enters.
  const waiting = () => page.evaluate(() => !window.__richview || window.__richview.pending().some(name => name.startsWith('mount ')));
  for (let i = 0; i < MOUNT_STEPS && (await waiting()); i++) { await page.clock.runFor(100); await sleep(5); }
  if (await waiting()) throw new Error(`charts still wait to mount after ${MOUNT_STEPS / 10} s of page time`);
  // A mounted chart hears of its frame's size on a real frame and lays out on the clock's next one,
  // and counts as pending in between: both pass for the charts mounted last.
  for (let i = 0; i < 10; i++) { await sleep(50); await page.clock.runFor(100); }
  return page;
}

// Waits, on real time and without moving the page's clock, until the chart is drawn; false when
// it is not within 3 s.
async function drawn(page, id) {
  for (let i = 0; i < 60; i++) { if (await page.evaluate(id => window.__mv.drawn(id), id)) return true; await sleep(50); }
  return false;
}
const NOT_DRAWN = { ok: false, detail: 'the chart is on screen and has no drawn canvas after 3 s' };

const started = Date.now();
const browser = await launchBrowser(chromium, { headless: true, args: CHROMIUM_ARGS }, { TimeoutError: playwrightErrors.TimeoutError });
const results = new Map();
const chartOf = (list, id) => list.find(chart => chart.id === id);
try {
  // The settled frames: what every other frame is compared to.
  const reference = await open(browser);
  const failure = await settle(reference);
  if (failure) throw new Error(`reference page: ${failure}`);
  const charts = await reference.evaluate(() => window.__mv.init());
  const refs = await reference.evaluate(() => window.__mv.dumpRefs());
  await reference.context().close();
  const wanted = options.only ? new Set(options.only.split(',')) : null;
  const subject = charts.filter(chart => !wanted || wanted.has(chart.kind));
  const sizes = Object.fromEntries(charts.map(chart => [chart.id, [chart.width, chart.height]]));
  for (const chart of subject) results.set(chart.id, { id: chart.id, type: chart.kind, title: chart.title, entrance: chart.entrance, duration_ms: null, frames: null, checks: {} });

  // Entrances, one chart at a time, each brought to the top of the screen. Pages are independent
  // (each has its own clock), so a few run side by side over every n-th chart.
  const lane = async (mine) => {
  const page = await open(browser);
  const mounted = await page.evaluate(sizes => window.__mv.init(sizes), sizes);
  await page.evaluate(data => window.__mv.loadSettled(data), refs);
  const toTop = async id => { const { top } = await page.evaluate(id => window.__mv.place(id), id); await page.evaluate(top => window.__mv.scrollTo(top), Math.max(0, top - 10)); await sleep(150); return drawn(page, id); };
  for (const chart of mine) {
    const result = results.get(chart.id), at = chartOf(mounted, chart.id), id = chart.id;
    const checks = result.checks;
    if (!at.settled) {
      if (!chart.entrance) { checks.model = { ok: false, detail: 'the model has no entrance, yet the chart waits to play one' }; continue; }
      // The entrance waits until half the chart shows: with 30 % in view, no time passes it by.
      const { top, height } = await page.evaluate(id => window.__mv.place(id), id);
      await page.evaluate(top => window.__mv.scrollTo(top), Math.max(0, top + 0.3 * height - VIEWPORT.height));
      await sleep(150);
      if (!(await drawn(page, id))) { checks.drawn = NOT_DRAWN; continue; }
      const before = await page.evaluate(id => window.__mv.same(id), id);
      await page.clock.runFor(3000);
      const waited = await page.evaluate(id => [window.__mv.pending(id), window.__mv.same(id)], id);
      checks.entersAtHalf = waited[0] && waited[1].all === before.all
        ? { ok: true, detail: 'still waiting 3 s after 30 % came into view' }
        : { ok: false, detail: waited[0] ? 'the chart changed with only 30 % in view' : 'the chart settled with only 30 % in view' };
      if (!waited[0]) continue;
      if (!(await toTop(id))) { checks.drawn = NOT_DRAWN; continue; }
      // Stepping the paused clock; the chart began when it came into view and the clock has not moved since.
      let playing = true, steps = 0;
      while (playing && steps < MAX_FRAMES) {
        playing = await page.evaluate(id => window.__mv.keep(id), id);
        if (playing) { await page.clock.runFor(FRAME_MS); steps++; }
      }
      if (playing) { checks.plays = { ok: false, detail: `the entrance did not finish within ${MAX_FRAMES * FRAME_MS} ms of stepped time` }; await page.evaluate(id => window.__mv.score(id, [0]), id); continue; }
      const last = steps; // frames 0..last; frame `last` is the settled draw
      result.frames = last + 1;
      result.duration_ms = last * FRAME_MS;
      if (last < 2) { checks.plays = { ok: false, detail: `the model has the ${chart.entrance} entrance, but the chart settled after ${last} frames` }; await page.evaluate(id => window.__mv.score(id, [0]), id); continue; }
      const picks = frameIndexes(last);
      const scored = await page.evaluate(([id, picks]) => window.__mv.score(id, picks), [id, picks]);
      result.fractions = picks.map((frame, i) => ({ fraction: [0, 25, 50, 75, 100][i], frame, marks: scored.at[i].mark, still: scored.at[i].still, all: scored.at[i].all }));
      checks.plays = { ok: true, detail: `${chart.entrance}, ${last} frames of ${FRAME_MS} ms` };
      checks.progress = progressVerdict(scored.at.map(s => s.mark));
      checks.still = stillVerdict(scored.stills.slice(0, -1));
      checks.lastFrame = equalVerdict(scored.at[4].all, 'the last animated frame');
      // The settled draw is the settled frame by construction; a difference means the two pages rendered differently.
      checks.determinism = scored.settledDraw > 0 ? { ok: false, detail: `the settled draw differs from the reference page in ${(scored.settledDraw * 100).toFixed(2)} % of pixels (harness)` } : { ok: true, detail: 'exact' };
      await sharp({ create: { width: at.width * 5 + 16, height: at.height, channels: 4, background: '#808080' } })
        .composite(await Promise.all(scored.pngs.map(async (bytes, i) => ({ input: Buffer.from(bytes), left: i * (at.width + 4), top: 0 }))))
        .png().toFile(join(strips, `${id}-${chart.kind}.png`));
    } else {
      // A settled chart further down draws when it comes near: its first frame there is the one judged.
      if (!(await toTop(id))) { checks.drawn = NOT_DRAWN; continue; }
      const same = await page.evaluate(id => window.__mv.same(id), id);
      checks.settledAtOnce = chart.entrance && !at.settled ? undefined : equalVerdict(same.all ?? 1, 'the first frame');
      result.note = chart.entrance ? 'on the first screen at mount: painted settled (FEA-CH-51)' : 'the model has no entrance';
    }
    // Away and back: nothing replays, no time stepped and then a moment of it.
    await page.evaluate(() => window.__mv.scrollTo(0)); await sleep(100);
    if (!(await toTop(id))) { checks.drawn = NOT_DRAWN; continue; }
    const back = await page.evaluate(id => [window.__mv.same(id), window.__mv.pending(id)], id);
    await page.clock.runFor(200);
    const later = await page.evaluate(id => [window.__mv.same(id), window.__mv.pending(id)], id);
    const replay = back[1] || later[1] ? 'the chart is pending again' : (back[0].all ?? 1) > LIMITS.frame ? `the frame on scrolling back differs from rest in ${((back[0].all ?? 1) * 100).toFixed(1)} %` : (later[0].all ?? 1) > LIMITS.frame ? `the frame 200 ms after scrolling back differs from rest in ${((later[0].all ?? 1) * 100).toFixed(1)} %` : null;
    checks.noReplay = replay ? { ok: false, detail: replay } : { ok: true, detail: 'rest after scrolling away and back' };
  }
  // A resize repaints every chart; none plays again.
  await page.setViewportSize({ width: 1200, height: VIEWPORT.height }); await sleep(250); await page.clock.runFor(100);
  await page.setViewportSize(VIEWPORT); await sleep(300); await page.clock.runFor(200);
  for (const { id } of mine) {
    const checks = results.get(id).checks;
    if (!(await toTop(id))) { checks.drawn = NOT_DRAWN; continue; }
    const [same, pending] = await page.evaluate(id => [window.__mv.same(id), window.__mv.pending(id)], id);
    const share = same.all ?? 1;
    checks.noReplayOnResize = pending ? { ok: false, detail: 'the chart is pending again after a resize' } : share > LIMITS.frame ? { ok: false, detail: `after a resize the frame differs from rest in ${(share * 100).toFixed(1)} %` } : { ok: true, detail: 'rest after a resize' };
  }
  await page.context().close();
  };
  await Promise.all(Array.from({ length: WORKERS }, (_, k) => lane(subject.filter((_, i) => i % WORKERS === k))));

  // Reduced motion: the first frame is the settled frame.
  const reduced = await open(browser, { reduced: true });
  await reduced.evaluate(sizes => window.__mv.init(sizes), sizes);
  await reduced.evaluate(data => window.__mv.loadSettled(data), refs);
  for (const { id } of subject) {
    const { top } = await reduced.evaluate(id => window.__mv.place(id), id);
    await reduced.evaluate(top => window.__mv.scrollTo(top), Math.max(0, top - 10));
    if (!(await drawn(reduced, id))) { results.get(id).checks.drawn = NOT_DRAWN; continue; }
    const [same, pending] = await reduced.evaluate(id => [window.__mv.same(id), window.__mv.pending(id)], id);
    const share = same.all ?? 1;
    results.get(id).checks.reducedMotion = pending ? { ok: false, detail: 'under reduced motion the chart still waits to play an entrance' } : equalVerdict(share, 'the first frame under reduced motion');
  }
  await reduced.context().close();
} finally {
  await browser.close();
}

const list = [...results.values()].map(result => {
  for (const name of Object.keys(result.checks)) if (result.checks[name] === undefined) delete result.checks[name];
  return { ...result, hasEntrance: !!result.entrance, ...chartVerdict(result.checks) };
});
const failed = list.filter(chart => !chart.ok);
const summary = { charts: list.length, types: new Set(list.map(chart => chart.type)).size, withEntrance: list.filter(chart => chart.hasEntrance).length, passed: list.length - failed.length, failed: failed.length, seconds: Math.round((Date.now() - started) / 1000) };
mkdirSync(join(out, 'motion'), { recursive: true });
writeFileSync(join(out, 'motion/types.json'), `${JSON.stringify({ summary, charts: list }, null, 2)}\n`);
rmSync(work, { recursive: true, force: true });
console.log(`motion: ${summary.passed}/${summary.charts} charts pass (${summary.types} types, ${summary.withEntrance} with an entrance) in ${summary.seconds} s`);
for (const chart of failed) for (const reason of chart.failures) console.log(`FAIL ${chart.type} (${chart.id}, "${chart.title}"): ${reason}`);
process.exit(failed.length ? 1 : 0);
