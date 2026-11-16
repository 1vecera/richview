// Unit test of the library-comparison adapter (ARC-64): bar, line and pie become the options of the
// idiomatic Highcharts and ECharts charts, and an unmapped type reports its reason. Pure, needs no
// libraries, browser or build. Run: node --test benchmarks/compare/test/*.test.mjs

import assert from 'node:assert/strict';
import fs from 'node:fs';
import test from 'node:test';
import { adapt, niceBounds, signedExtent } from '../adapter.mjs';

const types = JSON.parse(fs.readFileSync(new URL('../types.json', import.meta.url), 'utf8')).types;
const size = { w: 720, h: 440 };

test('bar: a column series per colour value, categories from x, ECharts bar series', () => {
	const rows = [
		{ region: 'North', year: '2024', tickets: 42 },
		{ region: 'North', year: '2025', tickets: 35 },
		{ region: 'South', year: '2024', tickets: 38 },
		{ region: 'South', year: '2025', tickets: 31 },
	];
	const block = { type: 'bar', title: 'T', subtitle: 'S', encoding: { x: 'region', y: 'tickets', color: 'year' } };
	const r = adapt(block, rows, types, size);
	assert.equal(r.highcharts.options.series.length, 2);
	assert.equal(r.highcharts.options.series[0].type, 'column');
	assert.deepEqual(r.highcharts.options.xAxis.categories, ['North', 'South']);
	assert.deepEqual(r.highcharts.options.series[1].data, [35, 31]);
	assert.equal(r.echarts.options.series[0].type, 'bar');
	assert.deepEqual(r.echarts.options.xAxis.data, ['North', 'South']);
	assert.equal(r.highcharts.options.title.text, 'T');
	assert.equal(r.echarts.options.title.subtext, 'S');
});

test('line: calendar months become a datetime axis, one series per plan', () => {
	const rows = [
		{ month: '2025-01', plan: 'Solo', n: 1 },
		{ month: '2025-02', plan: 'Solo', n: 2 },
		{ month: '2025-01', plan: 'Team', n: 3 },
		{ month: '2025-02', plan: 'Team', n: 4 },
	];
	const r = adapt({ type: 'line', title: 'T', encoding: { x: 'month', y: 'n', color: 'plan' } }, rows, types, size);
	assert.equal(r.highcharts.options.xAxis.type, 'datetime');
	assert.equal(r.highcharts.options.series[0].data[1][0], Date.UTC(2025, 1, 1));
	assert.equal(r.echarts.options.xAxis.type, 'time');
	assert.equal(r.echarts.options.series.length, 2);
});

test('pie: slices keep order and values; an unmapped encoding is recorded, not approximated', () => {
	const rows = [
		{ c: 'Search', n: 412 },
		{ c: 'Referral', n: 238 },
	];
	const r = adapt({ type: 'pie', title: 'T', encoding: { x: 'c', y: 'n' } }, rows, types, size);
	assert.deepEqual(r.highcharts.options.series[0].data, [{ name: 'Search', y: 412 }, { name: 'Referral', y: 238 }]);
	assert.deepEqual(r.echarts.options.series[0].data[1], { name: 'Referral', value: 238 });
	const rec = adapt({ type: 'line', title: 'T', encoding: { x: 'month', y: 'n', size: 's' } }, [{ month: '2025-01', n: 1, s: 1 }], types, size);
	assert.equal(rec.unmapped.highcharts[0].channel, 'size');
});

test('a type neither library defines reports the reason', () => {
	const r = adapt({ type: 'qq', title: 'T', encoding: { y: 'ms' } }, [{ ms: 1 }], types, size);
	assert.equal(r.highcharts.ok, false);
	assert.match(r.echarts.reason, /quantile/);
});

test('niceBounds: round axis bounds that enclose the values', () => {
	assert.deepEqual(niceBounds([14.6, 18.9]), { min: 14, max: 19 });
	assert.deepEqual(niceBounds([35.4, 51.7]), { min: 35, max: 55 });
	assert.deepEqual(niceBounds([3, 3]), { min: 2, max: 4 });
});

test('signedExtent: signed only when values cross zero, extent is the largest magnitude', () => {
	assert.deepEqual(signedExtent([2, -6, 5]), { signed: true, max: 6 });
	assert.deepEqual(signedExtent([0, 3, 14]), { signed: false, max: 14 });
});

test('heatmap with signed values: both libraries get a zero-centred diverging scale', () => {
	const rows = [{ x: 'a', y: 'p', v: -6 }, { x: 'b', y: 'p', v: 3 }, { x: 'a', y: 'q', v: 6 }, { x: 'b', y: 'q', v: -1 }];
	const r = adapt({ type: 'heatmap', title: 'T', encoding: { x: 'x', y: 'y', color: 'v' } }, rows, types, size);
	assert.equal(r.highcharts.options.colorAxis.min, -6);
	assert.equal(r.highcharts.options.colorAxis.max, 6);
	assert.equal(r.highcharts.options.colorAxis.stops.length, 3);
	assert.equal(r.echarts.options.visualMap.min, -6);
	assert.equal(r.echarts.options.visualMap.max, 6);
	assert.equal(r.echarts.options.visualMap.inRange.color.length, 3);
	const unsigned = adapt({ type: 'heatmap', title: 'T', encoding: { x: 'x', y: 'y', color: 'v' } }, rows.map((q) => ({ ...q, v: Math.abs(q.v) })), types, size);
	assert.equal(unsigned.highcharts.options.colorAxis.stops, undefined);
	assert.equal(unsigned.echarts.options.visualMap.inRange, undefined);
});

test('vector: direction turned to Highcharts clockwise-from-south, axes fixed to the grid', () => {
	const rows = [{ e: 0, n: 0, d: 0, s: 2 }, { e: 5, n: 5, d: 90, s: 3 }];
	const r = adapt({ type: 'vector', title: 'T', encoding: { x: 'e', y: 'n', angle: 'd', size: 's' } }, rows, types, size);
	assert.deepEqual(r.highcharts.options.series[0].data, [[0, 0, 2, 270], [5, 5, 3, 180]]);
	assert.ok(r.highcharts.options.yAxis.max < 20);
});

test('scatter3d: the z measure is the Highcharts vertical axis, y is depth, every axis bounded', () => {
	const rows = [{ g: 'A', bl: 38, bd: 18, fl: 190 }, { g: 'B', bl: 46, bd: 15, fl: 220 }];
	const r = adapt({ type: 'scatter3d', title: 'T', encoding: { x: 'bl', y: 'bd', z: 'fl', color: 'g' } }, rows, types, size);
	assert.deepEqual(r.highcharts.options.series[0].data, [[38, 190, 18]]);
	for (const a of ['xAxis', 'yAxis', 'zAxis']) assert.ok(Number.isFinite(r.highcharts.options[a].min) && Number.isFinite(r.highcharts.options[a].max));
	assert.ok(r.highcharts.options.yAxis.min <= 190 && r.highcharts.options.yAxis.max >= 220);
});
