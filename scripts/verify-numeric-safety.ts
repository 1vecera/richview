// Numbers at the edge of the representable range, through the release CLI and the report runtime
// (DEL-16, FEA-CH-70, FEA-CH-18, FEA-CH-45, FEA-CH-48). Checked at desktop and phone widths:
// - Underflow: numeric text too small to represent, in every numeric channel, is a
//   `chart.source-error` naming its row and field. It never becomes zero or a category, and
//   Table and CSV keep the row with its text as written.
// - Grouped calendar rows: the error names the source row, not a position after grouping.
// - Overflow: a stack or pie total beyond the finite range is `chart.value-overflow`, and the
//   chart shows why it cannot draw.
// - Extremes: the largest doubles, a subnormal and a true zero draw with finite geometry, and only
//   the zero reads as zero.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { harness, watchErrors } from './browser-harness.ts';
import { figureDownload } from './chart-interaction-checks.ts';
import { reports, settle } from './report-runtime.ts';
import type { ChartModel } from './report-model.ts';

const fence = (value: object) => `\n\`\`\`chart\n${JSON.stringify(value)}\n\`\`\`\n`;
const TINY = '1e-400';
const base = [
  { x: 1, c: 'A', value: 2, measure: '2', hi: 10 },
  { x: 2, c: 'B', value: 3, measure: TINY, hi: 10 },
  { x: 3, c: 'C', value: 4, measure: '4', hi: 10 },
];
// [name, type, encoding, fails]: `measure` is the channel under test. A coordinate or statistic
// fails its chart; a pie value, heatmap colour or bubble size leaves its row out, as text that is
// not a number does in those channels.
const CHANNELS: [string, string, Record<string, string>, boolean][] = [
  ['line x', 'line', { x: 'measure', y: 'value' }, true],
  ['line y', 'line', { x: 'x', y: 'measure' }, true],
  ['area x', 'area', { x: 'measure', y: 'value' }, true],
  ['area y', 'area', { x: 'x', y: 'measure' }, true],
  ['scatter x', 'scatter', { x: 'measure', y: 'value' }, true],
  ['scatter y', 'scatter', { x: 'x', y: 'measure' }, true],
  ['scatter size', 'scatter', { x: 'x', y: 'value', size: 'measure' }, false],
  ['bar y', 'bar', { x: 'c', y: 'measure' }, true],
  ['pie y', 'pie', { x: 'c', y: 'measure' }, false],
  ['heatmap color', 'heatmap', { x: 'c', y: 'x', color: 'measure' }, false],
  ['histogram x', 'histogram', { x: 'measure' }, true],
  ['boxplot y', 'boxplot', { x: 'c', y: 'measure' }, true],
  ['interval lower', 'interval', { x: 'c', y: 'value', lower: 'measure', upper: 'hi' }, true],
  ['interval estimate', 'interval', { x: 'c', y: 'measure', lower: 'x', upper: 'hi' }, true],
];
const grouped = [
  { date: '2024-03', group: 'A', value: 3 },
  { date: '2024-01', group: 'B', value: 1 },
  { date: '2024-02', group: 'A', value: 2 },
  { date: '2024-01', group: 'A', value: ` ${TINY} ` },
];
const stack = [
  { date: '2024-01', s: 'a', v: 1.5e308 },
  { date: '2024-01', s: 'b', v: 1.5e308 },
  { date: '2024-02', s: 'a', v: 1 },
  { date: '2024-02', s: 'b', v: 2 },
];
type Case = { name: string; chart: { type: string; title: string; encoding: Record<string, string>; data: { inline?: object[]; csv?: string }; stack?: boolean } }
  & ({ path: string; row: string; fails: boolean; overflow?: never; extremes?: never }
    | { overflow: true; failure?: string; path?: never; extremes?: never }
    | { extremes: true; path?: never; overflow?: never });
const charts: Case[] = [
  ...CHANNELS.map(([name, type, encoding, fails]) => ({ name, chart: { type, title: `Underflow in ${name}`, encoding, data: { inline: base } }, path: 'rows[1].measure', row: 'Row 2', fails })),
  ...['line', 'area'].map(type => ({ name: `grouped ${type}`, chart: { type, title: `Grouped ${type}`, encoding: { x: 'date', y: 'value', color: 'group' }, data: { inline: grouped } }, path: 'rows[3].value', row: 'Row 4', fails: true })),
  { name: 'stacked area', chart: { type: 'area', title: 'Stack beyond the finite range', stack: true, encoding: { x: 'date', y: 'v', color: 's' }, data: { inline: stack } }, overflow: true },
  { name: 'pie total', chart: { type: 'pie', title: 'Pie total beyond the finite range', encoding: { x: 'c', y: 'v' }, data: { inline: [{ c: 'A', v: 1.5e308 }, { c: 'A', v: 1.5e308 }, { c: 'B', v: 1 }] } }, overflow: true, failure: 'Cannot draw this pie · Category total is too large' },
  { name: 'extremes', chart: { type: 'line', title: 'Extremes', encoding: { x: 'date', y: 'v' }, data: { csv: 'data/extremes.csv' } }, extremes: true },
];
const source = '---\ntitle: Numeric safety\nversion: "1"\n---\n\nNumbers at the edge of the range.\n' + charts.map(c => fence(c.chart)).join('');
const files = { 'data/extremes.csv': 'date,v\r\n2024-01,1e308\r\n2024-02,5e-324\r\n2024-03,0\r\n2024-04,-1e308\r\n' };

const h = await harness('numeric-safety');
const inputs = await reports(h.dir, { only: [], extra: { numeric: source }, files: { numeric: files }, failing: ['numeric'] });
const cases = [];
const errors: string[] = [];
const failures: string[] = [];
const check = async (label: string, fn: () => unknown) => {
  try {
    await fn();
    cases.push(label);
  } catch (error) {
    failures.push(`${label}: ${(error as Error).message}`);
  }
};

// The CLI's diagnostics for each chart: every chart is a rich block, so the i-th chart is
// `rv-chart-i`. A chart's `chart.failed` carries its id as the path.
const generated = inputs.generated.numeric;
const diagnostics = generated.diagnostics ?? [];
const of = (i: number) => diagnostics.filter(d => d.details?.chartId === `rv-chart-${i}` || d.path === `rv-chart-${i}`);
await check('the CLI reports the failures and still writes the report', () => assert.equal(generated.ok, false));
for (const [i, c] of charts.entries()) {
  const own = of(i);
  if (c.path) {
    await check(`${c.name}: exactly one chart.source-error, at ${c.path}`, () => {
      const found = own.filter((d: { code: string; }) => d.code === 'chart.source-error');
      assert.equal(found.length, 1, JSON.stringify(own));
      const [d] = found;
      assert.equal(d.level, 'error');
      assert.equal(d.path, c.path);
      assert.ok(d.message.includes(c.row) && d.message.includes(TINY) && d.message.includes('too small to represent'), d.message);
    });
    await check(`${c.name}: ${c.fails ? 'the chart cannot draw and says why' : 'the chart draws without the row'}`, () => {
      const failed = own.filter((d: { code: string; }) => d.code === 'chart.failed');
      if (c.fails) {
        assert.equal(failed.length, 1, JSON.stringify(own));
        assert.match(failed[0].message, /^Cannot draw .*too small to represent/);
      } else assert.deepEqual(failed, []);
    });
  }
  if (c.overflow) {
    await check(`${c.name}: chart.value-overflow`, () => assert.equal(own.filter((d: { code: string; level: string; }) => d.code === 'chart.value-overflow' && d.level === 'error').length, 1, JSON.stringify(own)));
  }
  if (c.extremes) {
    await check(`${c.name}: no error`, () => {
      assert.deepEqual(own.filter((d: { level: string; }) => d.level === 'error'), []);
    });
  }
}

/** Parse RFC 4180 CSV text into rows of fields. */
function parseCsv(text: string) {
  const out = [];
  let row = [];
  let field = '';
  let quoted = false;
  for (let i = 0; i < text.length; i++) {
    const ch = text[i];
    if (quoted) {
      if (ch === '"' && text[i + 1] === '"') { field += '"'; i++; } else if (ch === '"') quoted = false; else field += ch;
    } else if (ch === '"') quoted = true;
    else if (ch === ',') { row.push(field); field = ''; } else if (ch === '\r' && text[i + 1] === '\n') { row.push(field); out.push(row); row = []; field = ''; i++; } else field += ch;
  }
  if (field || row.length) { row.push(field); out.push(row); }
  return out;
}
const values = (model: ChartModel) => model.geometry.panels.flatMap(p => p.observations).flatMap(o => o.readout.lines.map(l => l.value));
const ZERO = /^[−-]?0(?:\.0+)?$/;

// Chromium drops every eleventh download a page starts in quick succession, so the charts are
// read over two page loads of at most nine downloads each.
try {
  for (const width of [1280, 390]) {
    for (const [from, to] of [[0, 9], [9, charts.length]] as const) {
      const page = await h.browser.newPage({ viewport: { width, height: 900 }, reducedMotion: 'reduce', acceptDownloads: true });
      watchErrors(page, errors);
      await page.goto(`${inputs.base}/numeric/report.html`);
      const models = await settle(page);
      const figures = page.locator('figure[data-rv-chart]');
      await check(`@${width} from ${from}: every chart mounts`, () => assert.equal(models.length, charts.length));
      for (const [i, c] of charts.entries()) {
        if (i < from || i >= to) continue;
        const at = `${c.name} @${width}`;
        const model = models[i];
        const figure = figures.nth(i);
        if (c.path) {
          await check(`${at}: ${c.fails ? 'shows why it cannot draw' : 'draws without the row'}`, () => {
            const reason = model.geometry.failure ?? model.geometry.panels.find(p => p.failure)?.failure ?? null;
            if (c.fails) assert.match(String(reason), /^Cannot draw .*too small to represent/);
            else assert.equal(reason, null);
          });
          await check(`${at}: the unrepresentable value never reads as zero`, () => assert.ok(!values(model).some((v: string) => ZERO.test(v)), values(model).join(' | ')));
          await figure.locator('.rv-chart__action--table').click();
          const cells = await figure.locator('.rv-chart__table tbody td').allTextContents();
          await check(`${at}: Table keeps the row with its text as written`, () => assert.ok(cells.some(t => t.trim() === TINY), cells.join(' | ')));
          await figure.locator('.rv-chart__action--table').click();
          const file = await figureDownload(page, figure, 'csv');
          const parsed = parseCsv(readFileSync(await file.path(), 'utf8'));
          await check(`${at}: CSV keeps every row and the text as written`, () => {
            assert.equal(parsed.length - 1, c.chart.data.inline!.length);
            assert.ok(parsed.flat().some(t => t.trim() === TINY), JSON.stringify(parsed));
          });
        }
        if (c.overflow) {
          await check(`${at}: the chart shows why it cannot draw`, () => {
            const reason = model.geometry.failure ?? model.geometry.panels.find(p => p.failure)?.failure;
            assert.ok(reason?.startsWith('Cannot draw'), String(reason));
            if (c.failure) assert.equal(reason, c.failure);
          });
          await check(`${at}: no readout reads infinity`, () => assert.ok(!values(model).some((v: string) => /Infinity|∞|NaN/.test(v)), values(model).join(' | ')));
        }
        if (c.extremes) {
          const obs = model.geometry.panels[0].observations.slice().sort((a, b) => a.x - b.x);
          const read = obs.map(o => o.readout.lines[0].value);
          await check(`${at}: draws with finite anchors`, () => {
            assert.equal(model.geometry.failure ?? null, null);
            assert.equal(obs.length, 4);
            assert.ok(obs.every(o => o.at.every(Number.isFinite)));
          });
          await check(`${at}: only the true zero reads as zero`, () => {
            assert.ok(ZERO.test(read[2]), read.join(' | '));
            assert.ok([0, 1, 3].every(k => !ZERO.test(read[k]) && !/Infinity|∞|NaN/.test(read[k])), read.join(' | '));
          });
          await figure.locator('.rv-chart__action--table').click();
          const cells = await figure.locator('.rv-chart__table tbody tr td:nth-child(2)').allTextContents();
          await check(`${at}: Table keeps the subnormal nonzero`, () => assert.ok(cells.length === 4 && !ZERO.test(cells[1]) && cells[1] !== '', cells.join(' | ')));
          await figure.locator('.rv-chart__action--table').click();
        }
      }
      await page.close();
    }
  }
  await check('no browser errors', () => assert.deepEqual(errors, []));
} finally {
  await inputs.close();
  await h.close();
}
if (failures.length) {
  console.error(failures.join('\n'));
  process.exit(1);
}
console.log(`PASS ${cases.length} numeric safety cases: underflow as a source error in every channel, rows kept in Table and CSV, total overflow and finite extremes`);
