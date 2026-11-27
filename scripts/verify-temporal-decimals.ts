// Calendar charts show values at the author's precision in every place a reader can see them
// (DEL-16, FEA-CH-37, FEA-CH-38, TYP-23, TYP-37, TYP-38). The same unsorted monthly values come
// from a CSV file, a JSON file and inline rows, and every source form reads the same in each place:
// - Readouts: the runtime's model (DEL-72), with the keyboard readout text matching it.
// - Table cells and CSV values.
// Three significant digits are the default; `readout_digits: 4` gives readouts and tables one
// more. A missing month reads "No observation", and no form ever shows more digits than that.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { harness, installWasmClock, watchErrors } from './browser-harness.ts';
import { figureDownload } from './chart-interaction-checks.ts';
import { reports, settle } from './report-runtime.ts';
import type { ChartModel } from './report-model.ts';

const fence = (value: object) => `\n\`\`\`chart\n${JSON.stringify(value)}\n\`\`\`\n`;
// Source order is unsorted; February is missing.
const VALUES = [
  ['2024-03', '1.234567890123456789'],
  ['2024-01', '427.55'],
  ['2024-02', ''],
  ['2024-04', '12345678.9'],
  ['2024-05', '0.000123456'],
  ['2024-06', '-42.4242'],
];
const csv = `date,value\r\n${VALUES.map(([d, v]) => `${d},${v}`).join('\r\n')}\r\n`;
const json = JSON.stringify(VALUES.map(([date, v]) => ({ date, value: v === '' ? null : Number(v) })));
const inline = VALUES.map(([date, v]) => ({ date, value: v === '' ? null : Number(v) }));
// Expected readout values, chronological, at three and four significant digits.
const EXPECTED = {
  3: ['428', 'No observation', '1.23', '12.3 million', '0.000123', '−42.4'],
  4: ['427.6', 'No observation', '1.235', '12.35 million', '0.0001235', '−42.42'],
};
// CSV cells in source order: plain numbers at the same precision, the missing month empty.
const EXPECTED_CSV = {
  3: ['1.23', '428', '', '12300000', '0.000123', '-42.4'],
  4: ['1.235', '427.6', '', '12350000', '0.0001235', '-42.42'],
};
const FORMS = ['csv', 'json', 'inline'];
const data = (form: string) => (form === 'csv' ? { csv: 'data/values.csv' } : form === 'json' ? { json: 'data/values.json' } : { inline });

const h = await harness('temporal-decimals');
const charts: { digits: 3 | 4; form: string; chart: { type: string; title: string; encoding: { x: string; y: string }; data: ReturnType<typeof data>; readout_digits?: number } }[] = [];
for (const digits of [3, 4] as const) {
  for (const form of FORMS) {
    const chart: (typeof charts)[number]['chart'] = { type: 'line', title: `Values from ${form}, ${digits} digits`, encoding: { x: 'date', y: 'value' }, data: data(form) };
    if (digits !== 3) chart.readout_digits = digits;
    charts.push({ digits, form, chart });
  }
}
const source = '---\ntitle: Temporal decimals\nversion: "1"\n---\n\nOne series from three sources.\n' + charts.map(c => fence(c.chart)).join('');
const inputs = await reports(h.dir, { only: [], extra: { decimals: source }, files: { decimals: { 'data/values.csv': csv, 'data/values.json': json } } });

const cases: string[] = [];
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

/** Readout values per observation in x order, from the runtime's model. */
const modelValues = (model: ChartModel) => model.geometry.panels[0].observations
  .slice()
  .sort((a: { x: number; }, b: { x: number; }) => a.x - b.x)
  .map(o => o.readout.lines[0].value);

try {
  for (const width of [1280, 390]) {
    // A fresh page per width: Chromium drops every eleventh quick download a page starts.
    const page = await h.browser.newPage({ viewport: { width, height: 900 }, reducedMotion: 'reduce', acceptDownloads: true });
    watchErrors(page, errors);
    await page.goto(`${inputs.base}/decimals/report.html`);
    const models = await settle(page);
    await installWasmClock(page);
    const figures = page.locator('figure[data-rv-chart]');
    await check(`@${width}: every chart mounts`, () => assert.equal(models.length, charts.length));
    for (const [i, { digits, form }] of charts.entries()) {
      const at = `${form} ${digits} digits @${width}`;
      const figure = figures.nth(i);
      const expected = EXPECTED[digits];
      // The missing month keeps its place in the readouts and reads "No observation".
      const values = modelValues(models[i]);
      await check(`${at}: readouts at the author's precision`, () => assert.deepEqual(values, expected));
      await check(`${at}: no readout shows a source digit beyond the precision`, () => assert.ok(!values.some((v: string) => /2345678|42424|1\.2346|427\.55/.test(v)), values.join(' | ')));
      // Keyboard: stepping through every observation shows the model's values.
      // A pointer hold already on its way must not clear a newer keyboard selection.
      await figure.locator('canvas.rv-chart__canvas').dispatchEvent('pointerleave', { pointerType: 'mouse' });
      await figure.locator('.rv-chart__frame').focus();
      await page.keyboard.press('Home');
      const selected = await page.evaluate(i => window.__richview!.charts()[i].target, i);
      await page.clock.runFor(350);
      await check(`${at}: keyboard selection outlives an earlier pointer hold`, async () => {
        assert.deepEqual(await page.evaluate(i => window.__richview!.charts()[i].target, i), selected);
      });
      const shown: (string | null)[] = [];
      for (let k = 0; k < expected.length; k++) {
        if (k) await page.keyboard.press('ArrowRight');
        shown.push(await figure.locator('.rv-chart__readout-value').first().textContent());
      }
      await page.keyboard.press('Escape');
      await check(`${at}: keyboard readouts`, () => assert.deepEqual(shown, expected));
      // Table: source rows in source order, each at the same precision; the missing cell stays empty.
      await figure.locator('.rv-chart__action--table').click();
      const rows = await figure.locator('.rv-chart__table tbody tr').evaluateAll(trs => trs.map(tr => [...tr.querySelectorAll('td')].map(td => td.textContent)));
      const bySource = VALUES.map(([d]) => d);
      const chronological = [...bySource].sort();
      const want = bySource.map(d => {
        const v = expected[chronological.indexOf(d)];
        return v === 'No observation' ? '' : v;
      });
      await check(`${at}: Table in source order at the readout precision`, () => assert.deepEqual(rows.map(r => r[1]), want));
      await figure.locator('.rv-chart__action--table').click();
      // CSV: plain numbers rounded to the same precision.
      const file = await figureDownload(page, figure, 'csv');
      const lines = readFileSync(await file.path(), 'utf8').trim().split('\r\n');
      const numbers = lines.slice(1).map(line => line.split(',')[1]);
      await check(`${at}: CSV in source order`, () => assert.deepEqual(lines.slice(1).map(line => line.split(',')[0]), bySource));
      await check(`${at}: CSV missing value is empty`, () => assert.equal(numbers[2], ''));
      await check(`${at}: CSV numbers at the readout precision`, () => assert.deepEqual(numbers, EXPECTED_CSV[digits]));
    }
    // Every source form reads the same at each precision.
    for (const digits of [3, 4] as const) {
      const forms = charts.map((c, i) => [c, i] as const).filter(([c]) => c.digits === digits).map(([, i]) => modelValues(models[i]));
      await check(`@${width}: CSV, JSON and inline read the same at ${digits} digits`, () => {
        assert.deepEqual(forms[1], forms[0]);
        assert.deepEqual(forms[2], forms[0]);
      });
    }
    await page.close();
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
console.log(`PASS ${cases.length} temporal precision cases: readouts, keyboard, Table and CSV at the author's precision from CSV, JSON and inline sources`);
