// The geometry model as the chart test oracle (DEL-72). Every example and chart form is read
// through `window.__richview.charts()` at desktop and phone widths on a 2x screen. The model must
// hold together (marks, labels and observations inside the chart, no two labels on top of each
// other), and the canvas must draw that model: each solid bar and point shows its fill at its
// centre, each line its stroke, each label has ink inside its box, and the plot's empty places
// show only the background. A shader or upload bug that every model-based check would miss
// fails here.
//
// `--out <dir>` writes `geometry.json` there: one entry per chart view with its report, chart index,
// chart type, style, width, verdict and failures, which the type-evidence tool reads per chart type.
// The every-block fixture of the captures step is read in every builtin style at 320, 390 and 1440 px
// besides the examples, so each type has a view in every style at the three widths FEA-CH-75 names.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { cpSync, mkdirSync, writeFileSync } from 'node:fs';
import { basename, join } from 'node:path';
import { cli, harness, root, serveStatic, watchErrors } from './browser-harness.ts';
import { reports, settle } from './report-runtime.ts';
import { WIDTHS, builtinStyles, everyBlockFixture } from '../tools/rv-gate/lib/captures.mts';
import type { ChartModel, Item, Point, Rect } from './report-model.ts';

const outIndex = process.argv.indexOf('--out');
const outDir = outIndex >= 0 ? process.argv[outIndex + 1] : undefined;
if (outIndex >= 0 && !outDir) throw new Error('--out needs a folder');

const h = await harness('geometry');
const inputs = await reports(h.dir);
const cases: { label: string; probes: number; failed: number; }[] = [];
const errors: string[] = [];
const failures: string[] = [];
const views = [];
const failuresOf = new Map<string, string[]>();
const fail = (label: string, message: string) => {
  failures.push(`${label}: ${message}`);
  failuresOf.set(label, [...(failuresOf.get(label) ?? []), message]);
};

const rgb = (c: number[]) => c.slice(0, 3).map((v: number) => Math.round(v * 255));
const distance = (a: number[], b: number[]) => Math.max(...a.map((v: number, i: number) => Math.abs(v - b[i])));
const inside = (r: Rect, [x, y]: Point, slack = 0) => x >= r.x - slack && x <= r.x + r.w + slack && y >= r.y - slack && y <= r.y + r.h + slack;
const overlap = (a: Rect, b: Rect, slack: number) => a.x < b.x + b.w - slack && b.x < a.x + a.w - slack && a.y < b.y + b.h - slack && b.y < a.y + a.h - slack;

/** The painted box of an item, from the model alone. */
function bounds(item: Item) {
  const grow = (r: { x: number; y: number; w: number; h: number; }, by: number) => ({ x: r.x - by, y: r.y - by, w: r.w + 2 * by, h: r.h + 2 * by });
  switch (item.kind) {
    case 'rect': return grow(item.rect, (item.stroke?.width ?? 0) / 2);
    case 'point': { const r = item.radius + (item.stroke?.width ?? 0) / 2; return { x: item.center[0] - r, y: item.center[1] - r, w: 2 * r, h: 2 * r }; }
    case 'text': return item.bounds;
    default: {
      const pts = item.path.cmds.filter(c => c.p).map(c => c.p!);
      if (!pts.length) return null;
      const xs = pts.map(p => p[0]), ys = pts.map(p => p[1]);
      const by = (item.stroke?.width ?? 0) / 2;
      return { x: Math.min(...xs) - by, y: Math.min(...ys) - by, w: Math.max(...xs) - Math.min(...xs) + 2 * by, h: Math.max(...ys) - Math.min(...ys) + 2 * by };
    }
  }
}

/** Model invariants that need no pixels. */
function model(label: string, chart: ChartModel) {
  const g = chart.geometry;
  const box = { x: 0, y: 0, w: g.width, h: g.height };
  const items = [...g.panels.flatMap(p => p.items), ...(g.figure_items ?? [])];
  for (const item of items) {
    const b = bounds(item);
    if (b && !(inside(box, [b.x, b.y], 1) && inside(box, [b.x + b.w, b.y + b.h], 1))) fail(label, `${item.kind} ${item.role} ${item.text ?? ''} leaves the chart: ${JSON.stringify(b)}`);
  }
  for (const panel of g.panels) {
    for (const o of panel.observations) if (!inside(box, o.at, 1)) fail(label, `observation "${o.readout.heading}" lies outside the chart`);
  }
  const labels = items.filter((i): i is Extract<typeof i, { kind: 'text' }> => i.kind === 'text' && !i.rotate && ['label', 'axis-label', 'annotation', 'panel-title', 'key', 'axis-title'].includes(i.role));
  for (let a = 0; a < labels.length; a++) {
    for (let b = a + 1; b < labels.length; b++) {
      if (overlap(labels[a].bounds, labels[b].bounds, 1)) fail(label, `"${labels[a].text}" and "${labels[b].text}" overlap`);
    }
  }
  if (g.width !== chart.width) fail(label, `model width ${g.width} is not the view's ${chart.width}`);
}

/** Pixel probes for one chart, computed from the model; read in the page. */
type Probe = { what: string } & ({ kind: 'fill' | 'stroke' | 'empty'; p: Point; color: number[]; box?: never } | { kind: 'ink'; box: Rect; p?: never; color?: never });
function probes(chart: ChartModel) {
  const g = chart.geometry;
  const items = [...g.panels.flatMap(p => p.items), ...(g.figure_items ?? [])];
  const boxes = items.map(bounds);
  // The last item whose box holds the point paints over the others.
  const top = (p: Point) => { for (let i = items.length - 1; i >= 0; i--) if (boxes[i] && inside(boxes[i]!, p, 1.5)) return i; return -1; };
  const out: Probe[] = [];
  items.forEach((item, i) => {
    if (item.kind === 'rect' && item.role === 'mark' && !item.fill.hatch && item.fill.color[3] > 0.95 && item.rect.w >= 4 && item.rect.h >= 4) {
      const p: Point = [item.rect.x + item.rect.w / 2, item.rect.y + item.rect.h / 2];
      if (top(p) === i) out.push({ kind: 'fill', what: `bar ${i}`, p, color: rgb(item.fill.color) });
    } else if (item.kind === 'point' && item.fill && !item.fill.hatch && item.fill.color[3] > 0.95 && item.radius >= 3) {
      if (top(item.center) === i) out.push({ kind: 'fill', what: `point ${i}`, p: item.center, color: rgb(item.fill.color) });
    } else if (item.kind === 'line' && item.role === 'mark' && item.stroke.width >= 1.5 && !item.stroke.dash?.length && item.stroke.color[3] > 0.95) {
      // Straight segments only: a curve leaves the chord between its ends.
      const cmds = item.path.cmds;
      for (let k = 1; k < cmds.length; k++) {
        if (cmds[k].c !== 'l' || !cmds[k - 1].p) continue;
        const [a, b] = [cmds[k - 1].p!, cmds[k].p!];
        if (Math.hypot(b[0] - a[0], b[1] - a[1]) < 8) continue;
        const p: Point = [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2];
        if (top(p) === i) { out.push({ kind: 'stroke', what: `line ${i}`, p, color: rgb(item.stroke.color) }); break; }
      }
    } else if (item.kind === 'text' && !item.rotate && item.bounds.w >= 10) {
      out.push({ kind: 'ink', what: `text "${item.text}"`, box: item.bounds });
    }
  });
  // Empty places in each plot: nothing in the model within 4 px.
  if (g.background[3] > 0.99) {
    for (const panel of g.panels) {
      const plot = panel.plot;
      let found = 0;
      for (let gy = 1; gy < 8 && found < 6; gy++) {
        for (let gx = 1; gx < 8 && found < 6; gx++) {
          const p: Point = [plot.x + (plot.w * gx) / 8, plot.y + (plot.h * gy) / 8];
          if (boxes.every(b => !b || !inside(b, p, 4))) { out.push({ kind: 'empty', what: `empty ${p.map(Math.round)}`, p, color: rgb(g.background) }); found++; }
        }
      }
    }
  }
  return out;
}

type ColorProbe = { got: number[]; color: number[] };
let negative: ColorProbe | null = null;

/** Read every visible chart view of one report page at one width and check its model and its pixels. */
async function readViews(page: import('playwright').Page, { report, style, width, url, label: base }: { report: string; style: string; width: number; url: string; label: string }) {
  await page.setViewportSize({ width, height: 900 });
  await page.goto(url);
  let charts = await settle(page);
  // Open every tab panel's charts in turn so each is read at its real width.
  const tabs = await page.locator('[role=tab]').count();
  for (let t = 0; t <= Math.max(0, tabs - 1); t++) {
    if (tabs) {
      await page.locator('[role=tab]').nth(t).click();
      charts = await page.evaluate(async () => {
        await window.__richview!.settled();
        return window.__richview!.charts();
      });
    }
    const visible = await page.evaluate(() => [...document.querySelectorAll('figure[data-rv-chart]')].map(f => f.querySelector('.rv-chart__frame')!.getBoundingClientRect().width > 0));
    assert.equal(charts.length, visible.length, `${base}: every figure has a geometry model`);
    for (const [i, chart] of charts.entries()) {
      if (!visible[i]) continue;
      const label = `${base} #${i}`;
      if (cases.some(c => c.label === label)) continue;
      model(label, chart);
      const list = probes(chart);
      const seen = await page.evaluate(({ i, list }) => {
        const canvas = document.querySelectorAll('figure[data-rv-chart]')[i].querySelector<HTMLCanvasElement>('canvas.rv-chart__canvas');
        const ctx = canvas!.getContext('2d');
        const dpr = canvas!.width / parseFloat(canvas!.style.width);
        const at = ([x, y]: Point) => [...ctx!.getImageData(Math.min(canvas!.width - 1, Math.floor(x * dpr)), Math.min(canvas!.height - 1, Math.floor(y * dpr)), 1, 1).data].slice(0, 3);
        return list.map(probe => {
          if (probe.kind !== 'ink') return at(probe.p);
          const b = probe.box;
          const x0 = Math.max(0, Math.floor(b.x * dpr)), y0 = Math.max(0, Math.floor(b.y * dpr));
          const w = Math.max(1, Math.min(canvas!.width - x0, Math.ceil(b.w * dpr))), h = Math.max(1, Math.min(canvas!.height - y0, Math.ceil(b.h * dpr)));
          const data = ctx!.getImageData(x0, y0, w, h).data;
          const counts = new Map();
          for (let k = 0; k < data.length; k += 4) { const key = `${data[k]},${data[k + 1]},${data[k + 2]}`; counts.set(key, (counts.get(key) ?? 0) + 1); }
          const ground = [...counts.entries()].sort((a, b) => b[1] - a[1])[0][0].split(',').map(Number);
          let ink = 0;
          for (let k = 0; k < data.length; k += 4) if (Math.max(Math.abs(data[k] - ground[0]), Math.abs(data[k + 1] - ground[1]), Math.abs(data[k + 2] - ground[2])) > 60) ink++;
          return ink / (data.length / 4);
        });
      }, { i, list });
      const item = { label, probes: list.length, failed: 0 };
      cases.push(item);
      list.forEach((probe, k) => {
        const got = seen[k];
        const bad = probe.kind === 'ink' ? (got as number) < 0.02 : distance(got as number[], probe.color) > (probe.kind === 'stroke' ? 40 : 14);
        if (bad) {
          item.failed++;
          fail(label, `${probe.what} at ${probe.p?.map(v => v.toFixed(1)) ?? JSON.stringify(probe.box)} drew ${probe.kind === 'ink' ? `${((got as number) * 100).toFixed(1)}% ink` : got} where the model says ${probe.color ?? 'ink'}`);
        }
        if (!negative && probe.kind === 'fill') negative = { got: got as number[], color: probe.color };
      });
      views.push({ report, chart: i, type: chart.geometry.kind, style, width, ok: !failuresOf.has(label), failures: failuresOf.get(label) ?? [] });
    }
  }
}

try {
  const page = await h.browser.newPage({ viewport: { width: 1280, height: 900 }, deviceScaleFactor: 2, reducedMotion: 'reduce' });
  watchErrors(page, errors);
  for (const name of inputs.names) {
    for (const width of [1280, 390]) await readViews(page, { report: name, style: 'default', width, url: `${inputs.base}/${name}/report.html`, label: `${name} @${width}` });
  }
  // The every-block fixture, rendered in each builtin style and read at the capture widths.
  const fixture = everyBlockFixture(root);
  const folders: Record<string, string> = {};
  for (const style of builtinStyles(root)) {
    const folder = join(h.dir, 'fixture-reports', style);
    mkdirSync(folder, { recursive: true });
    cpSync(join(root, fixture), folder, { recursive: true });
    execFileSync(cli, ['generate', join(folder, 'report.rmd'), '--style', style, '--format', 'json'], { cwd: root, stdio: 'pipe', encoding: 'utf8', env: { ...process.env, ...(process.env.RICHVIEW_VERIFY_PUBLIC_DIR && { RICHVIEW_PUBLIC_DIR: process.env.RICHVIEW_VERIFY_PUBLIC_DIR }) } });
    folders[`/${style}/`] = join(folder, 'Rendered');
  }
  const server = await serveStatic(folders);
  try {
    // Each style and width gets its own page, closed after it is read: the 57-chart page at 2x, read
    // fifteen times over in one tab, ended with the tab closed under it (release 1zk). A page that
    // still crashes fails that view by name; it never passes.
    for (const style of builtinStyles(root)) {
      for (const width of WIDTHS.fixture) {
        const label = `${basename(fixture)} [${style}] @${width}`;
        let view;
        try {
          view = await h.browser.newPage({ viewport: { width, height: 900 }, deviceScaleFactor: 2, reducedMotion: 'reduce' });
          watchErrors(view, errors);
          await readViews(view, { report: basename(fixture), style, width, url: `${server.base}/${style}/report.html`, label });
        } catch (error) {
          fail(label, `the page could not be read: ${(error as Error).message.split('\n')[0]}`);
          views.push({ report: basename(fixture), chart: null, type: null, style, width, ok: false, failures: [`the page could not be read: ${(error as Error).message.split('\n')[0]}`] });
        } finally {
          await view?.close().catch(() => {});
        }
      }
    }
  } finally {
    await server.close();
  }
  // The comparison has teeth: a fill checked against a clearly different colour fails.
  const sample = negative as ColorProbe | null;
  assert.ok(sample, 'no solid fill was probed');
  assert.ok(distance(sample.got, sample.color.map((v: number) => 255 - v)) > 14 || distance(sample.color, [128, 128, 128]) < 20, 'the colour comparison cannot fail');
  if (errors.length) failures.push(`script errors: ${errors.join('; ')}`);
  assert.deepEqual(failures, []);
  const probed = cases.reduce((n, c) => n + c.probes, 0);
  console.log(`PASS ${cases.length} chart views hold their geometry model, and ${probed} pixel probes find the canvas drawing it`);
} finally {
  h.save('geometry.json', { engine: h.engine, cases, errors, failures });
  if (outDir) {
    mkdirSync(outDir, { recursive: true });
    writeFileSync(join(outDir, 'geometry.json'), JSON.stringify({ engine: h.engine, widths: WIDTHS.fixture, views, errors, failures }, null, 2) + '\n');
  }
  await inputs.close();
  await h.close();
}
