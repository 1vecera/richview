// Calendar sources keep their dates, values and text through the report runtime (DEL-16, TYP-90,
// TYP-91, FEA-CH-38, FEA-CH-45, FEA-CH-48, FEA-CH-50, FEA-CH-70). Each case is read at desktop
// and phone widths, in time zones ten hours behind and fourteen ahead of UTC:
// - Dates: unsorted source dates read in calendar order, and a month never shifts.
// - Missing values: a blank CRLF cell is a gap reading "No observation", empty in Table and CSV.
// - Text: a detail field with a newline, quotes, Ž and a leading '#' stays literal in Table and CSV.
// - Sources: inline rows and a CSV file of the same rows read the same.
// - Extremes: values near the largest and smallest doubles draw with finite geometry, including
//   the overview strip after a zoom, and never read as zero, infinity or NaN.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { harness, watchErrors } from './browser-harness.ts';
import { figureDownload } from './chart-interaction-checks.ts';
import { reports, settle } from './report-runtime.ts';
import type { ChartModel } from './report-model.ts';

const fence = (value: object) => `\n\`\`\`chart\n${JSON.stringify(value)}\n\`\`\`\n`;
const NOTE = 'line 1\n"line 2", Czech Ž';
// Unsorted, with February missing; the CSV file holds the same rows with CRLF line ends.
const rows = [
  { date: '2024-03', value: 427.55, note: '# preliminary' },
  { date: '2024-01', value: 400, note: NOTE },
  { date: '2024-02', value: null, note: '' },
];
const quote = (text: string) => (/[",\r\n]/.test(text) ? `"${text.replaceAll('"', '""')}"` : text);
const csvFile = `date,value,note\r\n${rows.map(r => [r.date, r.value ?? '', quote(r.note)].join(',')).join('\r\n')}\r\n`;
const EXTREMES = {
  large: ['date,value', '2024-01,-1e308', '2024-02,1e308', '2024-03,1.5e308'],
  tiny: ['date,value', '2024-01,1e-308', '2024-02,2e-308', '2024-03,5e-324'],
};
const chart = (title: string, data: { inline?: ({ date: string; value: number; note: string; }|{ date: string; value: null; note: string; })[]; csv?: string; }, extra = {}) => ({ type: 'line', title, encoding: { x: 'date', y: 'value', detail: ['note'] }, data, ...extra });
const source = '---\ntitle: Temporal fidelity\nversion: "1"\n---\n\nCalendar sources.\n'
  + fence(chart('Inline rows', { inline: rows }))
  + fence(chart('CSV file', { csv: 'data/values.csv' }))
  + fence({ type: 'line', title: 'Large values', encoding: { x: 'date', y: 'value' }, data: { csv: 'data/large.csv' } })
  + fence({ type: 'line', title: 'Tiny values', encoding: { x: 'date', y: 'value' }, data: { csv: 'data/tiny.csv' } });
const files = {
  'data/values.csv': csvFile,
  'data/large.csv': `${EXTREMES.large.join('\r\n')}\r\n`,
  'data/tiny.csv': `${EXTREMES.tiny.join('\r\n')}\r\n`,
};

const h = await harness('temporal-fidelity');
const inputs = await reports(h.dir, { only: [], extra: { fidelity: source }, files: { fidelity: files } });
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
/** Readouts in x order: heading and first value. */
const readouts = (model: ChartModel) => model.geometry.panels[0].observations
  .slice()
  .sort((a: { x: number; }, b: { x: number; }) => a.x - b.x)
  .map(o => [o.readout.heading, o.readout.lines[0].value]);
/** An SVG path whose every coordinate is a finite number, with no NaN or Infinity text. */
const finite = (d: string) => {
  if (/NaN|Infinity/i.test(d)) return false;
  const numbers = [...d.matchAll(/-?(?:\d+\.?\d*|\.\d+)(?:e[+-]?\d+)?/gi)].map(m => Number(m[0]));
  return numbers.length >= 2 && numbers.every(Number.isFinite);
};

try {
  const headings: Record<string, string[]> = {};
  for (const [zone, width] of ['Pacific/Honolulu', 'Pacific/Kiritimati'].flatMap(z => [1280, 390].map(w => [z, w] as const))) {
    const context = await h.browser.newContext({ viewport: { width, height: 900 }, reducedMotion: 'reduce', acceptDownloads: true, timezoneId: zone });
    const page = await context.newPage();
    watchErrors(page, errors);
    await page.goto(`${inputs.base}/fidelity/report.html`);
    const models = await settle(page);
    const figures = page.locator('figure[data-rv-chart]');
    await check(`${zone} @${width}: every chart mounts without a notice`, async () => {
      assert.equal(models.length, 4);
      assert.equal(await page.locator('figure[data-rv-chart] .rv-chart__notice:not([hidden])').count(), 0);
    });

    const [inline, file] = [readouts(models[0]), readouts(models[1])];
    await check(`${zone} @${width}: unsorted dates read in calendar order without shifting`, () => {
      assert.equal(inline.length, 3);
      assert.match(inline[0][0], /Jan[a-z]* 2024/);
      assert.match(inline[1][0], /Feb[a-z]* 2024/);
      assert.match(inline[2][0], /Mar[a-z]* 2024/);
    });
    await check(`${zone} @${width}: values at the author's precision; the blank month reads "No observation"`, () => assert.deepEqual(inline.map(r => r[1]), ['400', 'No observation', '428']));
    await check(`${zone} @${width}: inline rows and the CSV file read the same`, () => assert.deepEqual(file, inline));
    headings[`${zone} ${width}`] = inline.map(r => r[0]);

    for (const [i, name] of [[0, 'inline'], [1, 'csv']] as const) {
      const at = `${name} ${zone} @${width}`;
      const figure = figures.nth(i);
      await figure.locator('.rv-chart__action--table').click();
      const table = await figure.locator('.rv-chart__table').evaluate(wrap => ({
        header: [...wrap.querySelectorAll('thead th')].map(th => th.textContent),
        rows: [...wrap.querySelectorAll('tbody tr')].map(tr => [...tr.querySelectorAll('td')].map(td => td.textContent)),
      }));
      await check(`${at}: Table in source order with literal text and an empty missing cell`, () => {
        assert.deepEqual(table.header, ['date', 'value', 'note']);
        assert.deepEqual(table.rows.map(r => r[0]), ['2024-03', '2024-01', '2024-02']);
        assert.deepEqual(table.rows.map(r => r[1]), ['428', '400', '']);
        assert.deepEqual(table.rows.map(r => r[2]), ['# preliminary', NOTE, '']);
      });
      await figure.locator('.rv-chart__action--table').click();
      const download = await figureDownload(page, figure, 'csv');
      const parsed = parseCsv(readFileSync(await download.path(), 'utf8'));
      await check(`${at}: CSV keeps every row, the blank cell, the quoted note and UTF-8`, () => assert.deepEqual(parsed, [
        ['date', 'value', 'note'],
        ['2024-03', '428', '# preliminary'],
        ['2024-01', '400', NOTE],
        ['2024-02', '', ''],
      ]));
    }

    for (const [i, name] of [[2, 'large'], [3, 'tiny']] as const) {
      const at = `${name} ${zone} @${width}`;
      const values = readouts(models[i]).map(r => r[1]);
      await check(`${at}: three readouts, none zero, infinite or NaN`, () => {
        assert.equal(values.length, 3, values.join(' | '));
        assert.ok(values.every((v: string) => !/^[−-]?0$|Infinity|∞|NaN|No observation/.test(v)), values.join(' | '));
      });
      await check(`${at}: finite observation anchors`, () => assert.ok(models[i].geometry.panels[0].observations.every(o => o.at.every(Number.isFinite) && Number.isFinite(o.x))));
      const figure = figures.nth(i);
      await figure.locator('.rv-chart__frame').focus();
      await page.keyboard.press('+');
      await settle(page);
      const paths = await figure.locator('.rv-chart__overview path').evaluateAll(nodes => nodes.map(n => n.getAttribute('d')));
      await check(`${at}: the overview strip after a zoom draws finite paths`, () => {
        assert.ok(paths.length >= 1, 'no overview strip');
        assert.ok(paths.every(d => d && finite(d)), paths.join(' | '));
      });
      await page.keyboard.press('Home');
      await settle(page);
    }
    await context.close();
  }
  await check('the readout months are the same at UTC−10 and UTC+14 and both widths', () => assert.equal(new Set(Object.values(headings).map(h => JSON.stringify(h))).size, 1, JSON.stringify(headings)));
  await check('no browser errors', () => assert.deepEqual(errors, []));
} finally {
  await inputs.close();
  await h.close();
}
if (failures.length) {
  console.error(failures.join('\n'));
  process.exit(1);
}
console.log(`PASS ${cases.length} temporal fidelity cases: calendar order without shifts, missing months, literal text, matching sources and finite extremes`);
