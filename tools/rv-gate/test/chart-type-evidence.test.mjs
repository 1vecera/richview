import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { announcement, builtinStyles, chartTypes, framePaints, requireTypes, selectableLabels } from '../../../scripts/chart-type-evidence.mjs';

// Captured from `richview learn` in the retained 1bb9397358 release build.
const chartGuide = readFileSync(new URL('./fixtures/learn-chart-contract.txt', import.meta.url), 'utf8');
const styleGuide = readFileSync(new URL('./fixtures/learn-style.txt', import.meta.url), 'utf8');

test('the compiled inventory fails closed instead of treating an empty or malformed guide as complete', () => {
  const expected = 'line bar scatter area pie heatmap histogram boxplot interval waterfall treemap funnel dumbbell band bullet candlestick icicle map ohlc radar sunburst pareto tree combo rose waffle gauge calendar streamgraph ribbon sankey alluvial chord network lollipop violin dotplot bin2d hexbin density ecdf qq strip splom parallel gantt contour packed-bubble wordcloud venn vector flow pivot surface scatter3d bar3d'.split(' ');
  assert.deepEqual(chartTypes(chartGuide), expected);
  assert.deepEqual(chartTypes(chartGuide.replaceAll('\n', '\r\n')), expected);
  for (const broken of [
    '', chartGuide.replace('### Chart types', '### Charts'),
    chartGuide.slice(0, chartGuide.indexOf('### Data')),
    chartGuide.replace('- line (x*, y*, color, facet, detail):', '- new syntax:'),
    chartGuide.replace('- bar (x*, y*, color, facet, detail):', '\n- bar (x*, y*, color, facet, detail):'),
    chartGuide.replace('- bar (x*, y*, color, facet, detail):', '- line (x*, y*, color, facet, detail):'),
    chartGuide.replace('### Chart types', '### Chart types\n\n### Chart types'),
  ]) assert.throws(() => chartTypes(broken));
});

test('builtin coverage follows the compiled guide, including parenthetical descriptions', () => {
  assert.deepEqual(builtinStyles(styleGuide), ['sleek', 'air', 'scientific', 'brutalist', 'hand-drawn']);
  assert.deepEqual(builtinStyles(styleGuide.replaceAll('\n', '\r\n')), ['sleek', 'air', 'scientific', 'brutalist', 'hand-drawn']);
  for (const broken of [
    '', styleGuide.replace('### The builtin styles', '### Styles'),
    styleGuide.slice(0, styleGuide.indexOf('### Choose and compare styles')),
    styleGuide.replace('- **air**,', '\n- **air**,'),
    styleGuide.replace('- **air**,', '- **sleek**,'),
    styleGuide.replace('- **air**,', '- air,'),
  ]) assert.throws(() => builtinStyles(broken));
});

test('PDF labels must independently preserve SVG words, or all characters for arbitrary rotation', () => {
  const runs = [{ text: 'Engineering', angle: 0 }, { text: 'Sales Sales', angle: 90 }];
  assert.equal(selectableLabels(runs, ['Sales', 'Engineering', 'Sales']).ok, true);
  assert.equal(selectableLabels(runs, ['Sales', 'Engineering']).ok, false, 'a repeated label is not optional');
  assert.equal(selectableLabels(runs, ['En', 'gineering', 'Sales Sales']).ok, false, 'ordinary words cannot silently degrade to fragments');
  const rotated = [{ text: 'Engineering Sales', angle: 37 }];
  const fragments = ['En', 'gi', 'ne', 'er', 'in', 'g', 'Sa', 'les'];
  assert.deepEqual(selectableLabels(rotated, fragments), { ok: true, mode: 'non-space-characters', expected: 16, actual: 16, missing: [], extra: [] });
  assert.equal(selectableLabels(rotated, fragments.slice(1)).ok, false, 'the same omission in both PDFs fails the SVG oracle');
  assert.equal(selectableLabels(rotated, [...fragments, 'x']).ok, false);
});

test('a frame accounts for unclipped marks under the page CTM and shared container paints once', async () => {
  const { paintedShapes } = await import('../../../scripts/export-types.mjs');
  const shapes = paintedShapes('0 0 100 100 re f q 1 0 0 1 10 10 cm 0 0 20 20 re f Q 15 15 m 18 18 l S 90 90 m 95 95 l S', 100);
  const charts = [{ page: 1, frame: { left: 10, right: 30, top: 70, bottom: 90 }, marks: [shapes[1]] }, { page: 1, frame: { left: 20, right: 25, top: 75, bottom: 85 }, marks: [] }];
  const result = framePaints([shapes], charts);
  assert.deepEqual(result.pages[0], shapes.slice(0, 3), 'the unclaimed page-CTM mark is included without relying on a clip');
  assert.deepEqual(result.classification, { chart_marks: 1, rectangular_container_paints: 1, additional_vector_paints: 1 });
});

test('coverage requires the compiled types and unique chart identities, while allowing two examples of one type', () => {
  const expected = ['line', 'wordcloud'];
  const charts = [{ id: 'a', type: 'line' }, { id: 'b', type: 'line' }, { id: 'c', type: 'wordcloud' }];
  assert.deepEqual(requireTypes(expected, charts), { charts: 3, types: 2 });
  assert.throws(() => requireTypes(expected, charts.slice(0, 2)), /missing=wordcloud/);
  assert.throws(() => requireTypes(expected, [...charts, { id: 'd', type: 'unknown' }]), /extra=unknown/);
  assert.throws(() => requireTypes(expected, [...charts, charts[0]]), /duplicate/);
  assert.throws(() => requireTypes(expected, charts.map(chart => ({ ...chart, id: '' }))), /identities/);
});

test('a live announcement names the actual selected observation and facet, not only a nonempty string', () => {
  const geometry = { panels: [{ facet: 'West', observations: [
    { readout: { heading: 'April', lines: [{ name: 'Revenue', value: '€12' }, { name: 'Share', value: '40%' }] } },
    { readout: { heading: 'May', lines: [{ name: 'Revenue', value: '€15' }] } },
  ] }] };
  assert.equal(announcement(geometry, { panel: 0, observation: 1 }), 'West, May, Revenue: €15');
  assert.throws(() => announcement(geometry, { panel: 0, observation: 2 }), /No observation/);
  assert.throws(() => announcement(geometry, null), /No observation/);
  delete geometry.panels[0].facet;
  assert.equal(announcement(geometry, { panel: 0, observation: 0 }), 'April, Revenue: €12, Share: 40%');
});

test('PDF paint matching is one-to-one and rejects moved, missing, extra and same-bounds changed curves', async () => {
  const { paintedShapes, paintedPaths } = await import('../../../scripts/export-types.mjs');
  const { matchPainted } = await import('../../../scripts/chart-type-evidence.mjs');
  const source='0 0 m 50 0 50 100 100 100 c S';
  const before=paintedShapes(source,100);
  assert.equal(matchPainted(before,paintedShapes(source,100)).ok,true);
  assert.equal(matchPainted(before,[]).ok,false);
  assert.equal(matchPainted(before,paintedShapes(source+'\n'+source,100)).ok,false);
  assert.equal(matchPainted(paintedShapes(source+'\n'+source,100),before).ok,false,'one actual paint cannot stand for two marks');
  assert.equal(matchPainted(before,paintedShapes('1 0 0 1 1 0 cm '+source,100)).ok,false);
  assert.equal(matchPainted(before,paintedShapes(source.replace('50 0 50 100','50.00000001 0 50 100'),100)).ok,false,'PDF controls compare exactly, with no Float32 or distance allowance');
  const changed=source.replace('50 0 50 100','0 50 100 50');
  assert.deepEqual(paintedPaths(changed,100),paintedPaths(source,100));
  assert.equal(matchPainted(before,paintedShapes(changed,100)).ok,false);
  const red='q 1 0 0 rg 0 0 10 10 re f Q',blue='q 0 0 1 rg 5 0 10 10 re f Q';
  const swapped=matchPainted(paintedShapes(red+' '+blue,100),paintedShapes(blue+' '+red,100));
  assert.equal(swapped.ok,false,'paint order changes the overlapping colours');
  assert.equal(swapped.matches.length,2,'both exact paints remain present');
  assert.equal(swapped.reordered.length,1);
});

test('PDF paint matching includes weight, affine stroke scaling and unresolved primitives', async () => {
  const { paintedShapes } = await import('../../../scripts/export-types.mjs');
  const { matchPainted } = await import('../../../scripts/chart-type-evidence.mjs');
  const source='1 w 0 0 m 10 10 l S',before=paintedShapes(source,100);
  assert.equal(matchPainted(before,paintedShapes(source.replace('1 w','2 w'),100)).ok,false);
  assert.equal(matchPainted(before,paintedShapes(source.replace(' S',' 2 0 0 2 0 0 cm S'),100)).ok,false,'paint-time CTM changes the stroke even when the path stays put');
  const clipped='0 0 10 10 re W n '+source;
  assert.equal(matchPainted(paintedShapes(clipped,100),paintedShapes(clipped.replace('10 10 re','5 5 re'),100)).ok,false,'unchanged marks with changed clipping are not equivalent');
  assert.equal(matchPainted([],[]).ok,false,'empty evidence is not acceptance');
  assert.equal(matchPainted(paintedShapes('/X1 Do',100),paintedShapes('/X1 Do',100)).ok,false,'equal unresolved externals cannot pass');
  const unresolved=paintedShapes('/G1 gs '+source,100);
  assert.equal(matchPainted(unresolved,unresolved).ok,false,'the same resource name does not establish the same paint');
});


test('intrinsic chart papers form one ordered bijection regardless of caption position', async () => {
  const { paintedShapes } = await import('../../../scripts/export-types.mjs');
  const { chartPaperRoots } = await import('../../../scripts/chart-type-evidence.mjs');
  const line = { width: 657.64, height: 340.82 }, bar = { width: 657.64, height: 381.92 };
  // Actual Skia scalar serialization and page transforms from the retained hand-drawn PDF.
  const page = paintedShapes('q .496270275322071 0 0 -.496270275322071 50.999997875 718.168228356323 cm 0 0 657.64001 340.82001 re f Q q .496270275322071 0 0 -.496270275322071 50.999997875 150 cm 0 0 657.64001 381.92001 re f Q', 841.91998);
  const roots = chartPaperRoots([page], [line, bar]);
  assert.equal(roots.length, 2);
  assert.equal(roots[0].shape, page[0]); assert.equal(roots[1].shape, page[1]);
  assert.ok(roots[0].shape.top < 292.496388, 'the actual line paper precedes its below-chart caption');
  assert.throws(() => chartPaperRoots([page.slice(1)], [line, bar]), /paper count/);
  assert.throws(() => chartPaperRoots([[...page, page[0]]], [line, bar]), /paper count/);
  assert.throws(() => chartPaperRoots([[page[1], page[0]]], [line, bar]), /order or dimensions/);
  assert.throws(() => chartPaperRoots([paintedShapes('0 0 657.64 340.81 re f', 841.91998)], [line]), /paper count/);
});

test('PDF reference cleanup bounds stalled close and refuses a changed process identity', { timeout: 20000 }, async () => {
  const { spawn } = await import('node:child_process');
  const { once } = await import('node:events');
  const { ownRoots, bounded, identity } = await import('../../../scripts/verify-chart-type-pdf.mjs');
  const roots = ownRoots();
  let child;
  try {
    child = roots.run(() => spawn(process.execPath, ['-e', 'setInterval(()=>{},1000)'], { detached: true, stdio: 'ignore' }));
    await once(child, 'spawn');
    const root = roots.snapshot()[0];
    assert.equal(root.identity.pid, child.pid);
    assert.equal(root.identity.ppid, process.pid);
    assert.equal(root.identity.pgid, child.pid);
    const birth = root.identity.birth;
    root.identity.birth = 'different process birth';
    assert.throws(() => roots.kill(), /identity changed/);
    root.identity.birth = birth;
    assert.ok(identity(child.pid), 'the refusal leaves the actual child alive');
    await assert.rejects(bounded(new Promise(() => {}), 100), /exceeded 100ms/);
    roots.kill();
    assert.equal(await roots.drained(5000), true);
    assert.equal(roots.snapshot()[0].closed, true);
    assert.equal(roots.snapshot()[0].groupAbsent, true);
  } finally {
    try { roots.kill(); await roots.drained(5000); } finally { roots.stop(); }
  }
});
