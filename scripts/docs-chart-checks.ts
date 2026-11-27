// Host reason (ARC-03): Playwright drives chart controls inside the published docs wrapper,
// where its header, source panel and page policy can obstruct an otherwise working report.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import type { Page } from 'playwright';
import { figureDownload, judgeChart } from './chart-interaction-checks.ts';
import { settle } from './report-runtime.ts';

type Model = Parameters<typeof judgeChart>[2];

/**
 * Exercise every live example on one type's docs page, including its downloads and modal. The
 * first example is the type's own and must draw the named type; a page may carry more (a feature's
 * demo, such as the scatter page's zoom), and each of those is judged as fully as the first, so a
 * second chart on a page is checked, never skipped and never refused.
 */
export async function checkChartPage(page: Page, type: string, { touch = false } = {}) {
  const models = await settle(page);
  assert.ok(models.length >= 1, `${type}: the page must mount its live example`);
  assert.equal(models[0].geometry.kind, type, `${type}: the page must render the named chart type`);
  const examples: Awaited<ReturnType<typeof checkExample>>[] = [];
  for (const [index, model] of models.entries()) examples.push(await checkExample(page, type, index, model, { touch }));
  const [first, ...more] = examples;
  return more.length ? { ...first, extra_examples: more } : first;
}

/** One live example: its interaction judgement, Table, CSV, PNG and Expand. */
async function checkExample(page: Page, type: string, index: number, model: Model, { touch = false } = {}) {
  const label = index === 0 ? type : `${type} example ${index + 1}`;
  const result = await judgeChart(page, index, model, { touch });
  assert.ok(result.ok, `${label}: ${JSON.stringify(result)}`);
  let modelChecks: { unreported_drop: boolean; drawn_missing_hit: boolean } | undefined;
  if (model.geometry.kind === 'wordcloud') {
    const unplaced = result.pointer!.unplaced!;
    if (unplaced.length) {
      const unreported = structuredClone(model);
      unreported.geometry.dropped = unreported.geometry.dropped.filter(drop => drop.kind !== 'word');
      const refused = await judgeChart(page, index, unreported, { touch });
      assert.equal(refused.pointer!.ok, false, `${label}: unreported missing marks must fail`);
      assert.ok(refused.pointer!.problems!.some(problem => problem.includes('provenance')), JSON.stringify(refused));
    }
    const missingHit = structuredClone(model);
    const drawn = missingHit.geometry.panels.flatMap(panel => panel.observations).find(obs => (obs.area?.w ?? 0) > 0 && (obs.area?.h ?? 0) > 0);
    assert.ok(drawn, `${label}: the example needs a drawn word`);
    delete drawn.area;
    const refused = await judgeChart(page, index, missingHit, { touch });
    assert.equal(refused.pointer!.ok, false, `${label}: a drawn word without hit geometry must fail`);
    assert.ok(refused.pointer!.problems!.some(problem => problem.includes('drawn word') && problem.includes('no hit area')), JSON.stringify(refused));
    modelChecks = { unreported_drop: unplaced.length > 0, drawn_missing_hit: true };
  }
  const figure = page.locator(`#${model.figure}`);
  const payload = await figure.locator('script[data-rv-payload]').evaluate((node): { rows: unknown[]; columns: string[] } => JSON.parse(node.textContent!));

  await figure.locator('.rv-chart__action--table').click();
  const table = figure.locator('.rv-chart__table table');
  assert.equal(await table.isVisible(), true, `${label}: Table opens`);
  assert.equal(await table.locator('tbody tr').count(), Math.min(payload.rows.length, 1000), `${label}: Table includes the example rows`);
  await figure.locator('.rv-chart__action--table').click();
  assert.equal(await table.isVisible(), false, `${label}: Table returns to the live chart`);

  const download = async (format: 'csv' | 'png') => {
    const started = Date.now();
    const timeout = format === 'png' ? 30_000 : 5000;
    let clicked = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    try {
      const completed = (async () => {
        // PNG encoding is asynchronous; FEA-CH-47 sets correctness, not a five-second budget. A phone's
        // figure has no download icons, so there the action is the expanded view's.
        const file = await figureDownload(page, figure, format, { timeout, onClick: async () => {
          clicked = true;
          await page.evaluate(() => window.__richview!.settled());
        } });
        // path() waits for the file, so the recorded time includes delivery, not just its start.
        const path = await file.path();
        assert.ok(path, `${label}: ${format.toUpperCase()} did not finish downloading`);
        return { file, path, elapsedMs: Date.now() - started };
      })();
      return await Promise.race([
        completed,
        new Promise<never>((_, reject) => {
          timer = setTimeout(() => reject(new Error(`download did not complete within ${timeout}ms`)), timeout);
        }),
      ]);
    } catch (error) {
      throw new Error(`${label}: ${format.toUpperCase()} download failed after ${Date.now() - started}ms (click ${clicked ? 'completed' : 'pending'}): ${error instanceof Error ? error.message : String(error)}`, { cause: error });
    } finally {
      clearTimeout(timer);
    }
  };
  const csv = await download('csv');
  const text = readFileSync(csv.path, 'utf8').trim().split(/\r?\n/);
  assert.match(csv.file.suggestedFilename(), /\.csv$/);
  assert.equal(text[0].replaceAll('"', ''), payload.columns.join(','), `${label}: CSV names the example's columns`);
  assert.equal(text.length - 1, payload.rows.length, `${label}: CSV contains the example's rows`);
  const png = await download('png');
  assert.match(png.file.suggestedFilename(), /\.png$/);
  assert.deepEqual([...readFileSync(png.path).subarray(0, 8)], [137, 80, 78, 71, 13, 10, 26, 10], `${label}: PNG is an image`);

  await figure.locator('.rv-chart__action--expand').click();
  const dialog = page.locator('dialog.rv-chart-dialog[open]');
  await dialog.waitFor({ state: 'visible' });
  await page.evaluate(() => window.__richview!.settled());
  assert.equal(await dialog.locator('canvas.rv-chart__canvas').count(), 1, `${label}: expanded chart draws`);
  await page.mouse.move(1, 1);
  await dialog.locator('.rv-chart__frame').focus();
  // Escape clears any pointer selection before it closes the modal.
  await page.keyboard.press('Escape');
  if (await dialog.count()) await page.keyboard.press('Escape');
  assert.equal(await dialog.count(), 0, `${label}: Escape closes Expand`);
  assert.equal(await figure.locator('.rv-chart__action--expand').evaluate(node => node === document.activeElement), true, `${label}: focus returns to Expand`);
  return { ...result, ...(modelChecks && { model_checks: modelChecks }), actions: { table: true, csv: true, png: true, expand: true }, downloads: { csvMs: csv.elapsedMs, pngMs: png.elapsedMs } };
}
