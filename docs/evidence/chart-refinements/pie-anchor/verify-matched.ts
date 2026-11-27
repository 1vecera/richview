import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { createHash } from 'node:crypto';
import { pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
import { chromium } from 'playwright';
const { values } = parseArgs({ options: { before: { type: 'string' }, after: { type: 'string' }, 'facet-before': { type: 'string' }, 'facet-after': { type: 'string' }, baseline: { type: 'string' }, out: { type: 'string' } } });
interface Diagnostics { checks: unknown[] }
interface ChartCapture {
  box: unknown;
  pie_geometry: unknown;
  horizontal_overflow: boolean;
  motion: { active_animations: number };
  svg: string;
  child_svgs?: string[];
  child_diagnostics: Diagnostics[];
  image: string;
}
interface CaptureRecord {
  id: string;
  case: string;
  source_sha256: string;
  charts: ChartCapture[];
  native: Diagnostics[];
  errors: string[];
}
interface CaptureManifest {
  head: string;
  wasm_sha256: string;
  errors: string[];
  records: CaptureRecord[];
}
interface DirectChart {
  svg: string;
}
type ChartRenderer = (chart: unknown, id: string, options: { width: number }) => DirectChart;

const before: CaptureManifest = JSON.parse(readFileSync(join(values.before!, 'manifest.json'), 'utf8')), after: CaptureManifest = JSON.parse(readFileSync(join(values.after!, 'manifest.json'), 'utf8'));
// Older generic captures retained the outer facet's empty SVG. Supplements capture each
// actual child; the same helper now records them directly for a fresh complete reproduction.
for (const [manifest, path] of [[before, values['facet-before']], [after, values['facet-after']]] as const) {
  if (!path) continue;
  const extra: CaptureManifest = JSON.parse(readFileSync(join(path, 'manifest.json'), 'utf8'));
  assert.equal(extra.head, manifest.head); assert.equal(extra.wasm_sha256, manifest.wasm_sha256); assert.deepEqual(extra.errors, []);
  for (const record of extra.records) {
    const index = manifest.records.findIndex(r => r.id === record.id); assert(index >= 0);
    assert.equal(manifest.records[index].source_sha256, record.source_sha256); manifest.records[index] = record;
  }
}
const expected: Record<string, [number[], number[]]> = {
  name: [[0, 1], [1]], field: [[0, 1], [1]], 'explicit-empty-name': [[0], [0]], 'explicit-empty-field': [[0], [0]],
  'conflicting-aliases': [[1, 2], [1, 2]], 'empty-and-named': [[0, 1], [0, 1]], 'no-alias': [[0], []],
  'unmatched-name': [[0], []], 'matching-aliases': [[1], [1]], 'color-field': [[0, 1], [1]],
  'numeric-alias': [[0, 1], [1]], 'field-is-name': [[1], [1]], 'zero-empty': [[0, 1], [1]], facets: [[0, 1], [1]],
};
const browser = await chromium.launch({ executablePath: '/usr/bin/chromium', headless: true });
const page = await browser.newPage();
const inspect = (svg: string) => {
  const doc = new DOMParser().parseFromString(svg, 'image/svg+xml'), marks = [...doc.querySelectorAll('.rv-pie')];
  const outlines = [...doc.querySelectorAll('path[stroke-width="4"]')], notes = [...doc.querySelectorAll('.rv-annotation')];
  const result = { outlineRows: outlines.map(p => marks.findIndex(m => m.getAttribute('d') === p.getAttribute('d'))), notes: notes.map(t => t.textContent), annotations: [...outlines, ...notes].map(n => n.outerHTML) };
  [...outlines, ...notes].forEach(n => n.remove()); return { ...result, withoutAnnotations: doc.documentElement.outerHTML };
};
type Inspection = ReturnType<typeof inspect>;
const summary = { before: { head: before.head, wasm: before.wasm_sha256 }, after: { head: after.head, wasm: after.wasm_sha256 }, reports: 0, child_panels: 0, removed_false_outlines: 0, byte_identical_control_svgs: 0, byte_identical_control_pngs: 0, before_live_findings: 0, after_live_findings: 0, records: [] as { id: string; panel_targets: { before: number[]; after: number[] }[]; before_findings: unknown[]; after_findings: unknown[] }[], direct: [] as { id: string; targets: readonly number[]; before: DirectChart; after: DirectChart }[], files: undefined as Record<string, string> | undefined };
try {
  assert.equal(before.records.length, 210); assert.equal(after.records.length, 210); assert.deepEqual(before.errors, []); assert.deepEqual(after.errors, []);
  for (const a of after.records) {
    const b: CaptureRecord | undefined = before.records.find(r => r.id === a.id); assert(b); assert.deepEqual(a.errors, []); assert.deepEqual(b.errors, []);
    assert.equal(a.source_sha256, b.source_sha256); assert.equal(a.charts.length, b.charts.length);
    const targets = expected[a.case.replace('pie-anchor-', '')];
    for (let i = 0; i < a.charts.length; i++) {
      const ac: ChartCapture = a.charts[i], bc: ChartCapture = b.charts[i]; assert.deepEqual(ac.box, bc.box); assert.deepEqual(ac.pie_geometry, bc.pie_geometry); assert.equal(ac.horizontal_overflow, false); assert.equal(ac.motion.active_animations, 0);
      const aSvgs: string[] = ac.child_svgs || [ac.svg], bSvgs: string[] = bc.child_svgs || [bc.svg];
      assert.equal(aSvgs.length, ac.child_diagnostics.length); assert.equal(aSvgs.length, bSvgs.length);
      const panelTargets = [];
      for (let panel = 0; panel < aSvgs.length; panel++) {
        const pair = panel ? [[0], [0]] : targets;
        const [as, bs] = await page.evaluate(([a, b, fn]) => { const inspect = (0, eval)('(' + fn + ')') as (svg: string) => Inspection; return [inspect(a), inspect(b)]; }, [aSvgs[panel], bSvgs[panel], inspect.toString()]);
        assert.deepEqual(bs.outlineRows, pair[0], a.id + ' before targets'); assert.deepEqual(as.outlineRows, pair[1], a.id + ' after targets'); assert.equal(as.notes.length, pair[1].length); assert.equal(bs.notes.length, pair[0].length);
        const removed = bs.annotations.slice();
        for (const annotation of as.annotations) { const index = removed.indexOf(annotation); assert(index >= 0, a.id + ' surviving annotation unchanged'); removed.splice(index, 1); }
        assert.equal(removed.length, (pair[0].length - pair[1].length) * 2, a.id + ' only false outline/text pair removed');
        assert.equal(as.withoutAnnotations, bs.withoutAnnotations, a.id + ' only annotations may differ'); summary.removed_false_outlines += pair[0].length - pair[1].length;
        if (JSON.stringify(pair[0]) === JSON.stringify(pair[1])) { assert.equal(aSvgs[panel], bSvgs[panel], a.id + ' unchanged control SVG'); summary.byte_identical_control_svgs++; }
        panelTargets.push({ before: bs.outlineRows, after: as.outlineRows });
      }
      const was = bc.child_diagnostics.flatMap(d => d.checks), now = ac.child_diagnostics.flatMap(d => d.checks); summary.before_live_findings += was.length; summary.after_live_findings += now.length; summary.child_panels += ac.child_diagnostics.length;
      for (const finding of now) assert(was.some(x => JSON.stringify(x) === JSON.stringify(finding)), a.id + ' no new live finding');
      const nativeWas: unknown[] = b.native.flatMap(d => d.checks), nativeNow = a.native.flatMap(d => d.checks); for (const finding of nativeNow) assert(nativeWas.some(x => JSON.stringify(x) === JSON.stringify(finding)), a.id + ' no new native finding');
      if (JSON.stringify(targets[0]) === JSON.stringify(targets[1])) {
        assert(readFileSync(ac.image).equals(readFileSync(bc.image)), a.id + ' unchanged control PNG'); summary.byte_identical_control_pngs++;
      }
      summary.records.push({ id: a.id, panel_targets: panelTargets, before_findings: was, after_findings: now });
    }
    summary.reports++;
  }
  const baseline = resolve(values.baseline!);
  const { renderChart: renderBefore } = await import(pathToFileURL(join(baseline, 'packages/charts/dist/index.js')).href) as { renderChart: ChartRenderer };
  const { renderChart: renderAfter } = await import(pathToFileURL(resolve('packages/charts/dist/index.js')).href) as { renderChart: ChartRenderer };
  for (const [id, annotation, targets] of [
    ['null-field', { kind: 'point', at: { category: null }, text: 'Reviewed' }, [0]],
    ['null-name', { kind: 'text', at: { name: null }, text: 'Reviewed' }, [0]],
    ['null-and-name', { kind: 'point', at: { category: null, name: 'North' }, text: 'Reviewed' }, [0, 1]],
    ['missing-at', { kind: 'point', text: 'Reviewed' }, []],
    ['null-at', { kind: 'text', at: null, text: 'Reviewed' }, []],
    ['unsupported-kind', { kind: 'line', at: { name: 'North' }, text: 'Reviewed' }, []],
    ['unannotated', null, []],
  ] as const) {
    const chart = { type: 'pie', encoding: { x: 'category', y: 'value' }, data: { inline: [{ category: '', value: 1 }, { category: 'North', value: 2 }, { category: 'South', value: 3 }] }, ...(annotation ? { annotations: [annotation] } : {}) };
    const b = renderBefore(chart, id, { width: 720 }), a = renderAfter(chart, id, { width: 720 });
    const parsed = await page.evaluate(([svg, fn]) => ((0, eval)('(' + fn + ')') as (svg: string) => Inspection)(svg), [a.svg, inspect.toString()]); assert.deepEqual(parsed.outlineRows, targets, id);
    if (!['missing-at', 'null-at'].includes(id)) assert.deepEqual(a, b, id + ' unchanged explicit/unsupported/unannotated direct contract');
    summary.direct.push({ id, targets, before: b, after: a });
  }
} finally { await browser.close(); }
summary.files = Object.fromEntries(['tmp/pie-anchor/fixtures.json', 'tmp/pie-anchor/verify-matched.ts'].map(f => [f, createHash('sha256').update(readFileSync(f)).digest('hex')]));
writeFileSync(values.out!, JSON.stringify(summary, null, 2));
console.log(JSON.stringify({ ...summary, records: summary.records.length, direct: summary.direct.map(r => r.id) }, null, 2));
