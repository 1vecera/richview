// Host reason (ARC-03): Playwright drives chart controls inside the published docs wrapper,
// where its header, source panel and page policy can obstruct an otherwise working report.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { figureDownload, judgeChart } from './chart-interaction-checks.mjs';
import { settle } from './report-runtime.mjs';

/** Exercise the live example on one type's docs page, including its downloads and modal. */
export async function checkChartPage(page, type, { touch = false } = {}) {
  const models = await settle(page);
  assert.equal(models.length, 1, `${type}: the page must mount its live example`);
  const [model] = models;
  assert.equal(model.geometry.kind, type, `${type}: the page must render the named chart type`);
  const result = await judgeChart(page, 0, model, { touch });
  assert.ok(result.ok, `${type}: ${JSON.stringify(result)}`);
  if (type === 'wordcloud') {
    if (result.pointer.unplaced.length) {
      const unreported = structuredClone(model);
      unreported.geometry.dropped = unreported.geometry.dropped.filter(drop => drop.kind !== 'word');
      const refused = await judgeChart(page, 0, unreported, { touch });
      assert.equal(refused.pointer.ok, false, `${type}: unreported missing marks must fail`);
      assert.ok(refused.pointer.problems.some(problem => problem.includes('provenance')), JSON.stringify(refused));
    }
    const missingHit = structuredClone(model);
    const drawn = missingHit.geometry.panels.flatMap(panel => panel.observations).find(obs => obs.area?.w > 0 && obs.area?.h > 0);
    assert.ok(drawn, `${type}: the example needs a drawn word`);
    delete drawn.area;
    const refused = await judgeChart(page, 0, missingHit, { touch });
    assert.equal(refused.pointer.ok, false, `${type}: a drawn word without hit geometry must fail`);
    assert.ok(refused.pointer.problems.some(problem => problem.includes('drawn word') && problem.includes('no hit area')), JSON.stringify(refused));
    result.model_checks = { unreported_drop: result.pointer.unplaced.length > 0, drawn_missing_hit: true };
  }
  const figure = page.locator(`#${model.figure}`);
  const payload = await figure.locator('script[data-rv-payload]').evaluate(node => JSON.parse(node.textContent));

  await figure.locator('.rv-chart__action--table').click();
  const table = figure.locator('.rv-chart__table table');
  assert.equal(await table.isVisible(), true, `${type}: Table opens`);
  assert.equal(await table.locator('tbody tr').count(), Math.min(payload.rows.length, 1000), `${type}: Table includes the example rows`);
  await figure.locator('.rv-chart__action--table').click();
  assert.equal(await table.isVisible(), false, `${type}: Table returns to the live chart`);

  const download = async format => {
    const started = Date.now();
    const timeout = format === 'png' ? 30_000 : 5000;
    let clicked = false;
    let timer;
    try {
      const completed = (async () => {
        // PNG encoding is asynchronous; FEA-CH-47 sets correctness, not a five-second budget. A phone's
        // figure has no download icons, so there the action is the expanded view's.
        const file = await figureDownload(page, figure, format, { timeout, onClick: async () => {
          clicked = true;
          await page.evaluate(() => window.__richview.settled());
        } });
        // path() waits for the file, so the recorded time includes delivery, not just its start.
        const path = await file.path();
        assert.ok(path, `${type}: ${format.toUpperCase()} did not finish downloading`);
        return { file, path, elapsedMs: Date.now() - started };
      })();
      return await Promise.race([
        completed,
        new Promise((_, reject) => {
          timer = setTimeout(() => reject(new Error(`download did not complete within ${timeout}ms`)), timeout);
        }),
      ]);
    } catch (error) {
      throw new Error(`${type}: ${format.toUpperCase()} download failed after ${Date.now() - started}ms (click ${clicked ? 'completed' : 'pending'}): ${error.message}`, { cause: error });
    } finally {
      clearTimeout(timer);
    }
  };
  const csv = await download('csv');
  const text = readFileSync(csv.path, 'utf8').trim().split(/\r?\n/);
  assert.match(csv.file.suggestedFilename(), /\.csv$/);
  assert.equal(text[0].replaceAll('"', ''), payload.columns.join(','), `${type}: CSV names the example's columns`);
  assert.equal(text.length - 1, payload.rows.length, `${type}: CSV contains the example's rows`);
  const png = await download('png');
  assert.match(png.file.suggestedFilename(), /\.png$/);
  assert.deepEqual([...readFileSync(png.path).subarray(0, 8)], [137, 80, 78, 71, 13, 10, 26, 10], `${type}: PNG is an image`);

  await figure.locator('.rv-chart__action--expand').click();
  const dialog = page.locator('dialog.rv-chart-dialog[open]');
  await dialog.waitFor({ state: 'visible' });
  await page.evaluate(() => window.__richview.settled());
  assert.equal(await dialog.locator('canvas.rv-chart__canvas').count(), 1, `${type}: expanded chart draws`);
  await page.mouse.move(1, 1);
  await dialog.locator('.rv-chart__frame').focus();
  // Escape clears any pointer selection before it closes the modal.
  await page.keyboard.press('Escape');
  if (await dialog.count()) await page.keyboard.press('Escape');
  assert.equal(await dialog.count(), 0, `${type}: Escape closes Expand`);
  assert.equal(await figure.locator('.rv-chart__action--expand').evaluate(node => node === document.activeElement), true, `${type}: focus returns to Expand`);
  return { ...result, actions: { table: true, csv: true, png: true, expand: true }, downloads: { csvMs: csv.elapsedMs, pngMs: png.elapsedMs } };
}
