// Host reason (ARC-03): verify every-point geometry, painted strokes and precision in actual
// browsers, and measure larger line/area/scatter mounts with --perf (FEA-CH-17, PRF-07).
import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { cpus, loadavg } from 'node:os';
import { harness, quantiles, root, watchErrors } from './browser-harness.ts';
import { figureDownload } from './chart-interaction-checks.ts';
import { reports } from './report-runtime.ts';
import { BUDGETS } from '../tools/rv-gate/lib/perf.mts';
import type { CDPSession, Page } from 'playwright';

async function settle(page: Page) {
  await page.waitForFunction(() => window.__richview, null, { timeout: 60_000 });
  await page.evaluate(() => Promise.race([window.__richview!.settled(), new Promise((_, reject) => setTimeout(() => reject(new Error('charts did not settle within 60 seconds')), 60_000))]));
}

async function pointerFrames(page: Page, chartIndex: number) {
  return page.evaluate(chartIndex => {
    // Expand the debug model once, outside the measured frames. Target source observations,
    // including ones the default's representative geometry does not draw separately.
    const prepared = (() => {
      const rows = window.__richview!.charts()[chartIndex].geometry.panels[0].observations;
      return {
        targets: Array.from({ length: 105 }, (_, n) => rows[1 + n * Math.max(1, Math.floor((rows.length - 2) / 105))].at.slice()),
        headings: new Set(rows.map(o => o.readout.heading)),
        values: new Set(rows.flatMap(o => o.readout.lines.map(l => l.value))),
      };
    })();
    const figure = document.querySelectorAll<HTMLElement>('figure[data-rv-chart]')[chartIndex];
    const canvas = figure.querySelector<HTMLCanvasElement>('canvas.rv-chart__canvas');
    const box = canvas!.getBoundingClientRect();
    return new Promise<{ costs: number[]; valid: boolean; readings: number }>(done => {
      const costs: number[] = [], headings: (string | undefined)[] = [], channel = new MessageChannel();
      let n = 0, valid = true;
      channel.port1.onmessage = ({ data }) => {
        if (data.n > 5) {
          costs.push(performance.now() - data.started);
          const readout = figure.querySelector('.rv-chart__readout');
          const heading = readout!.querySelector('.rv-chart__readout-head')?.textContent;
          headings.push(heading);
          valid &&= !(readout! as HTMLElement).hidden && prepared.headings.has(heading!)
            && [...readout!.querySelectorAll('.rv-chart__readout-value')].every(v => prepared.values.has(v.textContent));
        }
        if (data.n === 105) {
          channel.port1.close(); channel.port2.close();
          done({ costs, valid, readings: new Set(headings).size });
        }
      };
      const frame = () => {
        const started = performance.now(), at = prepared.targets[n++];
        canvas!.dispatchEvent(new PointerEvent('pointermove', {
          bubbles: true, pointerType: 'mouse', clientX: box.left + at[0], clientY: box.top + at[1],
        }));
        channel.port2.postMessage({ started, n });
        if (n < 105) requestAnimationFrame(frame);
      };
      requestAnimationFrame(frame);
    });
  }, chartIndex);
}

async function zoomFrames(page: Page, chartIndex: number) {
  return page.evaluate(async chartIndex => {
    const figure = document.querySelectorAll<HTMLElement>('figure[data-rv-chart]')[chartIndex];
    const frame = figure.querySelector('.rv-chart__frame');
    const press = (key: string) => frame!.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true }));
    // Measure a window around one tenth of the source, alternating in/out rather than
    // repeatedly shrinking to the minimum. Warmup and the full reset are disclosed separately.
    press('Escape');
    for (let n = 0; n < 10; n++) press('+');
    await window.__richview!.settled();
    const panel = window.__richview!.charts()[chartIndex].geometry.panels[0];
    const zoomWindow = { domain: panel.x!.domain, extent: panel.x!.extent, observations: panel.observations.length };
    const canvas = figure.querySelector<HTMLCanvasElement>('canvas.rv-chart__canvas');
    const prototype = HTMLCanvasElement.prototype;
    const dimensions = ['width', 'height'].map(key => [key, Object.getOwnPropertyDescriptor(prototype, key)!] as const);
    let sizeWrites = 0, redundantWrites = 0;
    for (const [key, descriptor] of dimensions) Object.defineProperty(prototype, key!, {
      ...descriptor,
      set(value) {
        if (this === canvas) {
          sizeWrites++;
          if (descriptor.get!.call(this) === Number(value)) redundantWrites++;
        }
        descriptor.set!.call(this, value);
      },
    });
    const costs = [], renders = [], responses = [], channel = new MessageChannel();
    try { for (let n = 0; n < 105; n++) {
      await new Promise(resolve => requestAnimationFrame(resolve));
      const started = performance.now();
      press(n % 2 ? '-' : '+');
      const render = performance.now() - started;
      const committed = new Promise<number>(resolve => {
        channel.port1.onmessage = () => resolve(performance.now() - started);
        channel.port2.postMessage(0);
      });
      const responded = new Promise<number>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve(performance.now() - started))));
      const [cost, response] = await Promise.all([committed, responded]);
      if (n >= 5) { costs.push(cost); renders.push(render); responses.push(response); }
    } } finally {
      for (const [key, descriptor] of dimensions) Object.defineProperty(prototype, key!, descriptor);
    }
    (figure.querySelector('.rv-chart__action--reset')! as HTMLElement).click();
    channel.port1.close(); channel.port2.close();
    return { costs, renders, responses, sizeWrites, redundantWrites, zoomWindow };
  }, chartIndex);
}

const curved = process.argv.includes('--curved');
const combined = process.argv.includes('--100k');
const perf = combined || process.argv.includes('--perf');
// Read the engine's declared limits rather than duplicating product thresholds here.
const limitSource = readFileSync(join(root, 'richview-chart/src/limits.rs'), 'utf8');
const ceilings = Object.fromEntries(['line', 'area', 'scatter'].map(kind => [kind,
  Number(limitSource.match(new RegExp(`limit\\("${kind}", "all-points observations", ([\\d_]+)\\)`))![1].replaceAll('_', ''))]));
const counts = combined ? [100_000] : perf ? [1_000, 10_000, 100_000] : [2_000, ...new Set(Object.values(ceilings).map(n => n + 1))];
const errors: string[] = [];
const results = [];
const host = { cpus: cpus().length, loadStart: loadavg() };
const fence = (block: { type: string; title: string; digits: number; readout_digits: number; encoding: { x: string; y: string; }; axes: { x: { digits: number; }; }; style: { curve: string; points?: string|undefined; }; data: { csv: string; }; }) => `\n\`\`\`chart\n${JSON.stringify(block)}\n\`\`\`\n`;
const extra: Record<string, string> = {};
const files: Record<string, Record<string, string>> = {};
const kinds = combined ? ['line', 'scatter'] : ['line', 'area', 'scatter'];
const modes = (kind: string) => combined || kind === 'scatter' ? ['default', 'all'] : ['default', 'all', 'm4', 'band'];
for (const count of counts) {
  const csv = `x,y\n${Array.from({ length: count }, (_, i) => `${i},${12.3456789 + (combined ? Math.sin(i / count * Math.PI * 2) * 100 : i * 37 % 101)}`).join('\n')}\n`;
  for (const kind of kinds) {
    for (const mode of modes(kind)) {
      const name = `${kind}-${mode}-${count}`;
      extra[name] = `---\ntitle: Every point\nversion: '1'\n---\n` + fence({
        type: kind, title: 'Every source observation', digits: 3, readout_digits: 4,
        encoding: { x: 'x', y: 'y' }, axes: { x: { digits: 6 } },
        style: { ...(mode === 'default' ? {} : { points: mode }), curve: curved ? 'monotone' : 'linear' }, data: { csv: 'data/points.csv' },
      });
      files[name] = { 'data/points.csv': csv };
    }
  }
}
if (combined) {
  // Two exports of one generated line+scatter report exercise default and explicit all.
  for (const mode of ['default', 'all']) {
    const name = `combined-${mode}-100k`;
    extra[name] = extra[`line-${mode}-100000`] + extra[`scatter-${mode}-100000`].replace(/^---[\s\S]*?---\n/, '');
    files[name] = files[`line-${mode}-100000`];
    delete extra[`line-${mode}-100000`];
    delete extra[`scatter-${mode}-100000`];
  }
}
let inputs, h, failure;
try {
  const parent = join(root, 'tmp/verification');
  mkdirSync(parent, { recursive: true });
  inputs = await reports(mkdtempSync(join(parent, 'point-inputs-')), { only: [], extra, files });
  // These reports are self-contained CLI output. Their own loopback host is sufficient; the
  // site Wasm and the website distribution are not inputs of this runtime-only regression.
  process.env.RICHVIEW_VERIFY_URL ??= inputs.base;
  h = await harness('point-modes');
  const cases = combined
    ? ['default', 'all'].flatMap(mode => [390, 1440].flatMap(width => kinds.map(kind => ({ count: 100_000, kind, mode, width }))))
    : counts.flatMap(count => kinds.flatMap(kind => modes(kind).flatMap(mode => [390, 1440].map(width => ({ count, kind, mode, width })))));
  let shared: { key: string; page: Page; cdp: CDPSession | undefined; mounts: number[] } | undefined;
  for (const { count, kind, mode, width } of cases) {
    const name = combined ? `combined-${mode}-100k` : `${kind}-${mode}-${count}`;
    const key = `${name}-${width}`;
    const reused: boolean = combined && shared?.key === key;
    const phoneCPU = perf && width === 390;
    const page: Page = reused ? shared!.page : await h.browser.newPage({ viewport: { width, height: width === 390 ? 844 : 1000 }, deviceScaleFactor: phoneCPU ? 2 : 1,
      hasTouch: phoneCPU, reducedMotion: 'reduce', acceptDownloads: true });
    page.setDefaultTimeout(combined ? 120_000 : 30_000);
    const cdp: CDPSession | undefined = reused ? shared!.cdp : perf && h.engine === 'chromium' ? await page.context().newCDPSession(page) : undefined;
    if (!reused) {
      if (phoneCPU && cdp) await cdp.send('Emulation.setCPUThrottlingRate', { rate: 4 });
      watchErrors(page, errors);
    }
    const chartIndex = combined && kind === 'scatter' ? 1 : 0;
    try {
      console.error(`point-modes ${name} ${kind} @${width}: start`);
      const mounts: number[] = reused ? shared!.mounts : [];
      for (let repeat = 0; !reused && repeat < (perf ? 5 : 1); repeat++) {
        const started = performance.now();
        await page.goto(`${inputs.base}/${name}/report.html`);
        console.error(`point-modes ${name} ${kind} @${width}: load${repeat} navigated`);
        if (combined) {
          await page.locator('figure[data-rv-chart]').last().evaluate(el => el.scrollIntoView({ block: 'center', behavior: 'instant' }));
          await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
        }
        await page.locator('figure[data-rv-chart]').nth(chartIndex).evaluate(el => el.scrollIntoView({ block: 'center', behavior: 'instant' }));
        await settle(page);
        await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
        mounts.push(performance.now() - started);
      }
      if (reused) {
        await page.locator('figure[data-rv-chart]').nth(chartIndex).evaluate(el => el.scrollIntoView({ block: 'center', behavior: 'instant' }));
        await settle(page);
      } else if (combined) {
        // One report's five navigation mounts cover both charts; measure each chart's
        // interactions in turn, retaining only this one page between the two cases.
        shared = { key, page, cdp, mounts };
      }
      console.error(`point-modes ${name} ${kind} @${width}: mounts complete`);
      let resizeFrames: number[] = [], resizeWidths: number[] | undefined;
      if (perf) {
        const measured = await page.evaluate((chartIndex: number) => new Promise<{ costs: number[]; widths: number[] }>(done => {
          const figure = document.querySelectorAll<HTMLElement>('figure[data-rv-chart]')[chartIndex];
          const width = figure.getBoundingClientRect().width;
          const costs: number[] = [], widths: number[] = [], channel = new MessageChannel();
          let n = 0;
          channel.port1.onmessage = ({ data }) => {
            if (data.n > 5) costs.push(performance.now() - data.started);
            if (data.n === 105) {
              figure.style.removeProperty('width');
              channel.port1.close(); channel.port2.close();
              done({ costs, widths });
            }
          };
          const frame = () => {
            const started = performance.now();
            n++;
            widths.push(parseFloat(figure.querySelector<HTMLCanvasElement>('canvas.rv-chart__canvas')!.style.width));
            figure.style.width = (width * (n % 2 ? .85 : 1)) + 'px';
            channel.port2.postMessage({ started, n });
            // Queue before ResizeObserver queues the runtime's callback, so the
            // next frame measures its relayout/draw as well as the browser commit.
            if (n < 105) requestAnimationFrame(frame);
          };
          requestAnimationFrame(frame);
        }), chartIndex);
        resizeFrames = measured.costs;
        resizeWidths = [Math.min(...measured.widths), Math.max(...measured.widths)];
        assert.ok(resizeWidths[1] - resizeWidths[0] > 1, 'the engine actually relaid out the figure');
        await settle(page);
      }
      console.error(`point-modes ${name} ${kind} @${width}: resize complete`);
      const pointer = perf ? await pointerFrames(page, chartIndex) : undefined;
      if (pointer) {
        assert.equal(pointer.valid, true, 'pointer readouts retain source precision');
        assert.ok(pointer.readings >= 95, 'at least 95 of 100 source readings change');
      }
      const zoom = combined ? await zoomFrames(page, chartIndex) : undefined;
      if (zoom) {
        assert.equal(zoom.redundantWrites, 0, 'zoom reuses a canvas bitmap of unchanged size');
        const { domain, extent } = zoom.zoomWindow;
        const fraction = (domain[1] - domain[0]) / (extent[1] - extent[0]);
        assert.ok(fraction > .05 && fraction < .2, 'zoom timing covers about one tenth of the full source');
        await settle(page);
      }
      let heap;
      if (cdp) heap = await cdp.send('Runtime.getHeapUsage');
      // Summarize in the page; do not transfer a 100,000-observation model through CDP.
      const model = await page.evaluate(({ kind, mode, chartIndex }) => {
        const chart = window.__richview!.charts()[chartIndex];
        const g = chart.geometry;
        const panel = g.panels[0];
        if (g.failure) return { failure: g.failure, positions: 0, observations: 0, checks: g.checks };
        const items = panel.items.filter(i => i.role === 'mark');
        const lines = items.filter((i): i is Extract<typeof i, { kind: 'line' }> => i.kind === 'line');
        const vertices = lines.flatMap(i => i.path.cmds.filter(c => c.p).map(c => c.p!));
        const expected = panel.observations.flatMap(o => (o.dots ?? []).map(d => d.at));
        const same = kind === 'scatter' || vertices.length === expected.length && vertices.every((p, i) => p.every((v: number, k: number) => Math.abs(v - expected[i][k]) < 1e-7));
        const positions = kind === 'scatter' ? items.filter((i): i is Extract<typeof i, { kind: 'point' }> => i.kind === 'point').length : vertices.length;
        // The displayed canvas is a 2-D copy of the shared WebGL surface. Probe actual
        // stroke ink at every interior full-path vertex, including subpixel x positions.
        // A vertex is painted when a pixel of its 3 x 3 window is the stroke's own colour over
        // the chart's background, covering half of that pixel or more: within 12 of 255 per
        // channel of that mix. It is at full strength when a pixel is within 45 per channel of
        // the stroke colour itself. A dense series draws a 1.25 px line (STY-40), whose
        // outermost peak tips antialias to about three quarters of a pixel at device scale 1,
        // so every vertex must be painted and three in four at full strength: an unpainted,
        // misplaced, differently coloured or washed-out line still fails. Measured shares at
        // full strength: 89% to 99.7% here, and 77% for 1,000 points at 1440 under --perf.
        let painted = null, strong = null;
        if (kind !== 'scatter' && mode === 'all' && lines.length) {
          const canvas = document.querySelectorAll('figure[data-rv-chart]')[chartIndex].querySelector<HTMLCanvasElement>('canvas.rv-chart__canvas');
          const image = canvas!.getContext('2d')!.getImageData(0, 0, canvas!.width, canvas!.height).data;
          const scale = canvas!.width / parseFloat(canvas!.style.width);
          const ink = lines[0].stroke.color.slice(0, 3).map(c => Math.round(c * 255));
          const ground = g.background.slice(0, 3).map(c => Math.round(c * 255));
          const span = ink.map((c, k) => c - ground[k]);
          const length = span.reduce((n, d) => n + d * d, 0);
          const interior = vertices.slice(1, -1).map(p => {
            const x = Math.max(0, Math.min(canvas!.width - 3, Math.floor(p[0] * scale) - 1));
            const y = Math.max(0, Math.min(canvas!.height - 3, Math.floor(p[1] * scale) - 1));
            let half = false, full = false;
            for (let dy = 0; dy < 3; dy++) for (let dx = 0; dx < 3; dx++) {
              const i = ((y + dy) * canvas!.width + x + dx) * 4;
              const cover = Math.min(1, span.reduce((n, d, k) => n + d * (image[i + k] - ground[k]), 0) / length);
              half ||= cover >= .5 && span.every((d, k) => Math.abs(ground[k] + cover * d - image[i + k]) < 12);
              full ||= ink.every((c, k) => Math.abs(c - image[i + k]) < 45);
            }
            return { half, full };
          });
          painted = length > 0 && interior.filter(v => !v.half).length;
          strong = interior.filter(v => v.full).length / interior.length;
        }
        return { failure: g.failure, positions, observations: panel.observations.length,
          values: panel.observations.reduce((n, o) => n + o.readout.lines.filter(l => !l.derived).length, 0), same, painted, strong,
          checks: g.checks.filter(c => ['decimation', 'aggregation'].includes(c.kind)),
          firstReadout: panel.observations[0].readout.lines.map(l => l.value), plotWidth: panel.plot.w };
      }, { kind, mode, chartIndex });
      const label: string = `${name} @${width} ${h.engine}`;
      const over = count > ceilings[kind];
      const figure = page.locator('figure[data-rv-chart]').nth(chartIndex);
      assert.equal(model.failure ?? null, null, label);
      assert.equal(await figure.locator('canvas.rv-chart__canvas').isVisible(), true, `${label}: the renderer displays its canvas`);
      assert.equal(model.values, kind === 'scatter' ? count * 2 : count, `${label}: every value is inspectable`);
      if (mode === 'all' || mode === 'default' && !over) {
        assert.equal(model.positions, count, `${label}: every point is drawn`);
        assert.equal(model.same, true, `${label}: vertices equal source-observation anchors`);
        assert.deepEqual(model.checks, [], `${label}: no reduction`);
        if (model.painted !== null) {
          assert.equal(model.painted, 0, `${label}: stroke ink is painted at every vertex`);
          assert.ok(model.strong! >= .75, `${label}: three in four vertices carry full-strength stroke ink (${model.strong})`);
          console.error(`point-modes ${name} ${kind} @${width}: ink at every vertex, ${(model.strong! * 100).toFixed(1)}% at full strength`);
        }
      } else if (model.positions < count || mode === 'default' && over) {
        assert.ok(model.positions <= count, `${label}: representatives do not add observations`);
        const check = model.checks.find((c: { kind: string; }) => c.kind === 'decimation');
        assert.ok(check, `${label}: reduction is disclosed`);
        const details = check.details as { kept: number; total: number; automatic: boolean; ceiling: number };
        assert.equal(details.kept, model.positions, label);
        assert.equal(details.total, count, label);
        assert.equal(check.level, 'info', label);
        assert.equal(details.automatic, mode === 'default', label);
        assert.equal(details.ceiling, ceilings[kind], label);
        assert.equal(model.checks.some((c: { kind: string; }) => c.kind === 'aggregation'), mode === 'band' && count / model.plotWidth > 6, label);
      } else {
        assert.deepEqual(model.checks, [], `${label}: an unreduced explicit policy needs no reduction check`);
      }
      if (!model.failure) assert.ok(model.firstReadout.includes('12.35'), `${label}: authored readout precision`);
      const download = await figureDownload(page, figure, 'csv');
      const csv = readFileSync(await download.path(), 'utf8');
      assert.equal(csv.trim().split(/\r?\n/).length, count + 1, `${label}: every CSV row`);
      assert.equal(csv.includes('12.3456789'), false, `${label}: source digits stay private`);
      const tableStarted = performance.now();
      await figure.locator('.rv-chart__action--table').click();
      assert.equal(await figure.locator('.rv-chart__table tbody tr').count(), Math.min(1000, count), `${label}: paged Table`);
      const tableMs = performance.now() - tableStarted;
    assert.ok((await figure.locator('.rv-chart__table-status').innerText()).includes(String(count)), `${label}: Table retains the full source count`);
    assert.equal((await figure.locator('.rv-chart__table').innerText()).includes('12.3456789'), false, `${label}: Table respects source precision`);
    if (count > 1000) {
      await figure.locator('.rv-chart__more').click();
      assert.equal(await figure.locator('.rv-chart__table tbody tr').count(), Math.min(2000, count), `${label}: the next source rows remain available`);
    }
      if (combined) await figure.locator('.rv-chart__action--table').click();
      await page.mouse.move(0, 0);
      if (!perf && mode === 'all') await figure.locator('.rv-chart__frame').screenshot({ path: `${h.dir}/${name}-${width}.png` });
      results.push({ name, kind, mode, count, width, engine: h.engine, mount: quantiles(mounts),
        resizeFrame: resizeFrames.length ? quantiles(resizeFrames) : undefined, resizeWidths, heap, tableMs,
        pointerFrame: pointer ? quantiles(pointer.costs) : undefined, pointerReadings: pointer?.readings,
        zoomWindow: zoom?.zoomWindow, zoomBitmapWrites: zoom?.sizeWrites,
        zoomFrame: zoom ? quantiles(zoom.costs) : undefined, zoomRender: zoom ? quantiles(zoom.renders) : undefined, zoomResponse: zoom ? quantiles(zoom.responses) : undefined,
        cpuRate: phoneCPU && cdp ? 4 : 1, dpr: phoneCPU ? 2 : 1, ...model });
      if (combined && mode === 'default' && width === 1440) {
        const frame = quantiles(pointer!.costs);
        assert.ok(frame.p95_ms <= BUDGETS['webkit.frame.p95'].limit, `${label}: default pointer/overlay frame p95 ${frame.p95_ms} ms`);
        assert.ok(frame.max_ms! <= BUDGETS['webkit.frame.max'].limit, `${label}: default pointer/overlay frame max ${frame.max_ms} ms`);
        // The lead moved this new 100k zoom workload out of the gate (5 Nov, 19:07 CET).
        // Keep its measurements visible; PRF-20's existing 35k×4 gate is unchanged.
      }
    } finally {
      if (!combined || kind === kinds.at(-1)) {
        await cdp?.detach();
        await page.close();
        shared = undefined;
      }
    }
  }
  assert.deepEqual(errors, []);
} catch (error) {
  failure = (error as Error).message;
  throw error;
} finally {
  h?.save('point-modes.json', { scope: 'Navigation through two painted frames, including Wasm start; five loads per report in perf mode (the combined report shares its load measurements across its two charts). Resize-frame cost uses the gate proxy: animation callback to next task, including script/layout/paint commit, while changing actual figure width, with reduced motion. Pointer/overlay frames use source observations, validate the displayed precision, and gate the default at 8 ms; full resize/startup are scaling disclosures. Zoom alternates around a tenth of the source with 100 sequential measured keyboard steps after warmup: the synchronous handler contains Rust layout, WebGL updates and DOM writes (PRF-20), while two-frame response and whole-frame commit are recorded separately. Explicit-all timings never fail these feature checks. 390px perf runs emulate touch/2x DPR and Chromium 4x CPU, with unthrottled loopback rather than the network profile. Heap is the main JavaScript isolate and backing storage, not process footprint. Viewport emulation, not physical devices. New sizes carry no inferred fixture-specific budget.', host: { ...host, loadEnd: loadavg() }, engine: h.engine, perf, curved, combined, ceilings, results, errors, failure });
  await inputs?.close();
  await h?.close();
  if (h) console.log(`Point-mode evidence: ${h.dir}/point-modes.json`);
}
