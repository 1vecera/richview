// Host reason (ARC-03): check how the browser verifier judges a delivered pointer against chart geometry.
import assert from 'node:assert/strict';
import test from 'node:test';
import { displayedReadout, pointerReachesObservation } from '../../../scripts/chart-interaction-checks.ts';

test('the verifier expects the selected mark and preserves authored and series rows', () => {
  const value = { name: 'Readers', value: '210' };
  const derived = { name: 'Share of Email', value: '33.9%', derived: true };
  const second = { name: 'Share of Web', value: '38.9%', derived: true };
  const readout = (lines: { name: string; value: string; derived?: boolean; series?: number }[]) => ({ heading: 'Email and Web', lines });
  for (const kind of ['sankey', 'alluvial']) {
    assert.deepEqual(displayedReadout(readout([value, derived, second]), 'contain', kind), readout([value]));
    assert.deepEqual(displayedReadout(readout([value, derived]), 'contain', kind), readout([value, derived]));
    assert.deepEqual(displayedReadout(readout([value, derived, second]), 'nearest-x', kind), readout([value, derived, second]));
  }
  for (const kind of ['qq', 'gauge', 'ribbon', 'bin2d', 'hexbin', 'venn', 'flow', 'funnel', 'waffle']) {
    assert.deepEqual(displayedReadout(readout([value, derived, second]), 'contain', kind), readout([value, derived, second]));
  }
  const authored = { name: 'Source', value: 'survey' };
  assert.deepEqual(displayedReadout(readout([value, derived, authored]), 'contain', 'sankey'), readout([value, derived, authored]));
  const series = { ...second, series: 0 };
  assert.deepEqual(displayedReadout(readout([value, derived, series]), 'contain', 'sankey'), readout([value, derived, series]));
});

test('point hit uses the delivered coordinates and the existing four-pixel reach', () => {
  const box = { x: 392.359375, y: 229.1875 };
  const observation: Parameters<typeof pointerReachesObservation>[0] = { at: [242.29024715780014, 316.0376595944341], radius: 4 };
  assert.equal(pointerReachesObservation(observation, [641, 549], box, 1), true);
  assert.equal(pointerReachesObservation(observation, [641.4482369917156, 549.7882105951269], box, 1), false);
  assert.equal(pointerReachesObservation(observation, [650, 560], box, 1), false);
  assert.equal(pointerReachesObservation({ at: [10, 10], radius: 4 }, [107, 107], { x: 100, y: 100 }, 0.5), true);
  assert.equal(pointerReachesObservation({ at: [10, 10], radius: 4 }, [109, 109], { x: 100, y: 100 }, 0.5), false);
});

test('area hits still reject points outside the existing one-unit margin', () => {
  const observation: Parameters<typeof pointerReachesObservation>[0] = { at: [0, 0], radius: 4, area: { x: 10, y: 20, w: 30, h: 40 } };
  assert.equal(pointerReachesObservation(observation, [9, 19], { x: 0, y: 0 }, 1), true);
  assert.equal(pointerReachesObservation(observation, [8.9, 19], { x: 0, y: 0 }, 1), false);
});
