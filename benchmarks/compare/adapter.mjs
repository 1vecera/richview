// Library-comparison lab adapter (ARC-64, FEA-CH-75): turns one RichView chart block (type,
// encoding, data rows) into the options of the idiomatic Highcharts chart and ECharts chart for the
// same type, with the same data, title and size. Development tooling only; no product rule lives
// here (ARC-03). The libraries are benchmarks, never code or styling to copy (INT-12, INT-53):
// every builder writes the configuration its library's documentation shows for the type, with the
// library's default colours, fonts and axes.
//
// A builder that cannot map an encoding channel faithfully calls ctx.unmap(library, channel,
// reason); the channel is left out and the manifest records it, never an approximation. A type or
// library the types.json table maps to null returns { ok: false, reason }.
//
// Pure: no filesystem, network or clock access, so the unit tests run anywhere.

const isDate = (v) => typeof v === 'string' && /^\d{4}-\d{2}(-\d{2})?$/.test(v);
const toTs = (s) => {
	const [y, m, d] = s.split('-').map(Number);
	return Date.UTC(y, m - 1, d || 1);
};
const uniq = (rows, f) => [...new Set(rows.map((r) => r[f]))];
const num = (v) => (v === null || v === undefined || v === '' ? null : Number(v));
const sum = (a) => a.reduce((s, v) => s + v, 0);

/** The kind of an x field: calendar dates, numbers, or categories. */
export function xKind(rows, field) {
	if (rows.every((r) => isDate(r[field]))) return 'date';
	if (rows.every((r) => typeof r[field] === 'number')) return 'number';
	return 'category';
}

/** Rows grouped into series by the colour channel, x kind and categories resolved. */
export function groupSeries(rows, enc, fallbackName) {
	const kind = xKind(rows, enc.x);
	const cats = kind === 'category' ? uniq(rows, enc.x) : null;
	const names = enc.color ? uniq(rows, enc.color) : [fallbackName];
	const series = names.map((name) => {
		const mine = enc.color ? rows.filter((r) => r[enc.color] === name) : rows;
		let data;
		if (kind === 'category') {
			data = cats.map((c) => {
				const r = mine.find((q) => q[enc.x] === c);
				return r ? num(r[enc.y]) : null;
			});
		} else {
			const key = (r) => (kind === 'date' ? toTs(r[enc.x]) : r[enc.x]);
			data = mine.map((r) => [key(r), num(r[enc.y])]).sort((a, b) => a[0] - b[0]);
		}
		return { name: String(name), data };
	});
	return { kind, cats, series };
}

/** Axis bounds that enclose `values` on round ticks, the way the Highcharts 3-D scatter demo fixes each axis (min, max) to its data. */
export function niceBounds(values, ticks = 5) {
	const v = values.filter((x) => Number.isFinite(x));
	const lo = Math.min(...v);
	const hi = Math.max(...v);
	if (lo === hi) return { min: lo - 1, max: hi + 1 };
	const raw = (hi - lo) / ticks;
	const mag = 10 ** Math.floor(Math.log10(raw));
	const step = [1, 2, 2.5, 5, 10].map((m) => m * mag).find((x) => x >= raw);
	return { min: Math.floor(lo / step) * step, max: Math.ceil(hi / step) * step };
}

/** The symmetric extent of signed values: the diverging colour scales of both libraries are centred on zero. */
export function signedExtent(values) {
	const v = values.filter((x) => Number.isFinite(x));
	return { signed: Math.min(...v) < 0 && Math.max(...v) > 0, max: Math.max(...v.map(Math.abs)) };
}

/** R-7 quantile of a sorted array, the rule RichView's boxplot uses. */
export function quantile(sorted, p) {
	const h = (sorted.length - 1) * p;
	const lo = Math.floor(h);
	return sorted[lo] + (h - lo) * ((sorted[Math.min(lo + 1, sorted.length - 1)] ?? sorted[lo]) - sorted[lo]);
}

/** Five-number summary with observed whiskers inside the 1.5 IQR fences, and the outliers. */
export function boxStats(values) {
	const s = values.filter((v) => v !== null).sort((a, b) => a - b);
	const q1 = quantile(s, 0.25);
	const med = quantile(s, 0.5);
	const q3 = quantile(s, 0.75);
	const iqr = q3 - q1;
	const inside = s.filter((v) => v >= q1 - 1.5 * iqr && v <= q3 + 1.5 * iqr);
	return {
		box: [inside[0], q1, med, q3, inside[inside.length - 1]],
		outliers: s.filter((v) => v < q1 - 1.5 * iqr || v > q3 + 1.5 * iqr),
	};
}

/** Nested { name, value, children } tree from parent-child rows. */
function nest(rows, enc) {
	const byName = new Map(rows.map((r) => [r[enc.label], { name: r[enc.label], value: num(r[enc.y]), children: [] }]));
	const roots = [];
	for (const r of rows) {
		const node = byName.get(r[enc.label]);
		const parent = enc.parent ? byName.get(r[enc.parent]) : null;
		if (parent) parent.children.push(node);
		else roots.push(node);
	}
	const prune = (n) => {
		if (n.children.length) {
			n.children.forEach(prune);
			delete n.value;
		} else delete n.children;
		return n;
	};
	return roots.map(prune);
}

const label = (ctx, axis) => ctx.block.axes?.[axis]?.label;
const unit = (ctx, axis) => {
	const u = ctx.block.axes?.[axis]?.unit;
	return u === '%' || u === 'k' ? u : null;
};

// ---- Highcharts ------------------------------------------------------------------------------

function hcBase(ctx, extra = {}) {
	const { block } = ctx;
	return {
		chart: { height: ctx.h, animation: false, ...extra.chart },
		title: { text: block.title },
		subtitle: block.subtitle ? { text: block.subtitle } : undefined,
		credits: { enabled: false },
		plotOptions: { series: { animation: false }, ...extra.plotOptions },
		...Object.fromEntries(Object.entries(extra).filter(([k]) => k !== 'chart' && k !== 'plotOptions')),
	};
}

function hcXAxis(ctx, g) {
	const axis = { title: { text: label(ctx, 'x') } };
	if (g.kind === 'date') axis.type = 'datetime';
	if (g.kind === 'category') axis.categories = g.cats;
	return axis;
}

function hcYAxis(ctx, extra = {}) {
	const u = unit(ctx, 'y');
	return { title: { text: label(ctx, 'y') }, ...(u ? { labels: { format: `{value}${u}` } } : {}), ...extra };
}

/** Line annotations (kind line, at y) become plot lines; other kinds are recorded as unmapped. */
function hcPlotLines(ctx) {
	const lines = [];
	for (const a of ctx.block.annotations ?? []) {
		if (a.kind === 'line' && a.at?.y !== undefined) lines.push({ value: a.at.y, width: 1, dashStyle: 'Dash', color: '#666', label: { text: a.text } });
		else ctx.unmap('highcharts', 'annotations', `annotation kind ${a.kind} has no direct option`);
	}
	return lines;
}

function hcCartesian(seriesType, { stacking } = {}) {
	return (ctx) => {
		const g = groupSeries(ctx.rows, ctx.enc, label(ctx, 'y') ?? ctx.enc.y);
		for (const ch of ['size', 'label', 'detail', 'facet']) if (ctx.enc[ch]) ctx.unmap('highcharts', ch, 'channel not read by this series type');
		const stack = stacking && ctx.block.stack ? 'normal' : undefined;
		return hcBase(ctx, {
			chart: { type: seriesType },
			xAxis: hcXAxis(ctx, g),
			yAxis: hcYAxis(ctx, { plotLines: hcPlotLines(ctx) }),
			legend: { enabled: g.series.length > 1 },
			plotOptions: { series: { animation: false, stacking: stack } },
			series: g.series.map((s) => ({ type: seriesType, name: s.name, data: s.data })),
		});
	};
}

// ---- ECharts ---------------------------------------------------------------------------------

function ecBase(ctx, { legend = false, grid = true, extra = {} } = {}) {
	const { block } = ctx;
	const textWidth = ctx.w - 40;
	return {
		animation: false,
		title: {
			text: block.title,
			subtext: block.subtitle,
			left: 'center',
			textStyle: { width: textWidth, overflow: 'break' },
			subtextStyle: { width: textWidth, overflow: 'break' },
		},
		...(legend ? { legend: { bottom: 0 } } : {}),
		...(grid ? { grid: { top: ctx.w < 500 ? 110 : 80, bottom: legend ? 40 : 16, left: 16, right: 24, containLabel: true } } : {}),
		...extra,
	};
}

function ecXAxis(ctx, g) {
	const base = { name: label(ctx, 'x'), nameLocation: 'middle', nameGap: 28 };
	if (g.kind === 'date') return { ...base, type: 'time' };
	if (g.kind === 'category') return { ...base, type: 'category', data: g.cats };
	return { ...base, type: 'value', scale: true };
}

function ecYAxis(ctx, extra = {}) {
	const u = unit(ctx, 'y');
	return { type: 'value', name: label(ctx, 'y'), nameTextStyle: { align: 'left' }, ...(u ? { axisLabel: { formatter: `{value}${u}` } } : {}), ...extra };
}

function ecMarkLine(ctx) {
	const data = [];
	for (const a of ctx.block.annotations ?? []) {
		if (a.kind === 'line' && a.at?.y !== undefined) data.push({ yAxis: a.at.y, name: a.text, label: { formatter: a.text } });
		else ctx.unmap('echarts', 'annotations', `annotation kind ${a.kind} has no direct option`);
	}
	return data.length ? { symbol: 'none', data } : undefined;
}

function ecCartesian(seriesType, { area = false, stacking = false, extraSeries = {} } = {}) {
	return (ctx) => {
		const g = groupSeries(ctx.rows, ctx.enc, label(ctx, 'y') ?? ctx.enc.y);
		for (const ch of ['size', 'label', 'detail', 'facet']) if (ctx.enc[ch]) ctx.unmap('echarts', ch, 'channel not read by this series type');
		const stack = stacking && ctx.block.stack ? 'total' : undefined;
		return ecBase(ctx, {
			legend: g.series.length > 1,
			extra: {
				tooltip: { trigger: 'axis' },
				xAxis: ecXAxis(ctx, g),
				yAxis: ecYAxis(ctx),
				series: g.series.map((s, i) => ({
					type: seriesType,
					name: s.name,
					data: s.data,
					stack,
					...(area ? { areaStyle: {} } : {}),
					...(i === 0 ? { markLine: ecMarkLine(ctx) } : {}),
					...extraSeries,
				})),
			},
		});
	};
}

// ---- Shared preparation ----------------------------------------------------------------------

function categoryGrid(rows, enc) {
	const xs = uniq(rows, enc.x);
	const ys = uniq(rows, enc.y);
	const data = rows.map((r) => [xs.indexOf(r[enc.x]), ys.indexOf(r[enc.y]), num(r[enc.color])]);
	const vals = data.map((d) => d[2]);
	return { xs, ys, data, min: Math.min(...vals), max: Math.max(...vals) };
}

function topLevelOf(rows, enc) {
	const parentOf = new Map(rows.map((r) => [r[enc.label], r[enc.parent]]));
	return (name) => {
		let n = name;
		while (parentOf.get(n)) n = parentOf.get(n);
		return n;
	};
}

function stripSets(name) {
	return String(name).split(' & ');
}

function cumulative(values) {
	const total = sum(values);
	let run = 0;
	return values.map((v) => {
		run += v;
		return Math.round((run / total) * 1000) / 10;
	});
}

/** Largest-remainder counts out of 100, so 100 squares carry the shares. */
function hundred(values) {
	const total = sum(values);
	const raw = values.map((v) => (v / total) * 100);
	const out = raw.map(Math.floor);
	let left = 100 - sum(out);
	raw
		.map((v, i) => [v - Math.floor(v), i])
		.sort((a, b) => b[0] - a[0])
		.forEach(([, i]) => {
			if (left-- > 0) out[i] += 1;
		});
	return out;
}

// ---- Builders per RichView type ---------------------------------------------------------------

export const builders = {
	line: { hc: hcCartesian('line'), ec: ecCartesian('line') },
	area: { hc: hcCartesian('area', { stacking: true }), ec: ecCartesian('line', { area: true, stacking: true }) },
	bar: { hc: hcCartesian('column', { stacking: true }), ec: ecCartesian('bar', { stacking: true }) },
	lollipop: {
		hc: hcCartesian('lollipop'),
		ec: null,
	},
	pie: {
		hc(ctx) {
			const { enc, rows } = ctx;
			return hcBase(ctx, {
				series: [{ type: 'pie', name: enc.y, data: rows.map((r) => ({ name: String(r[enc.x]), y: num(r[enc.y]) })) }],
			});
		},
		ec(ctx) {
			const { enc, rows } = ctx;
			return ecBase(ctx, {
				grid: false,
				extra: {
					tooltip: { trigger: 'item' },
					legend: { bottom: 0 },
					series: [{ type: 'pie', radius: '55%', center: ['50%', '55%'], data: rows.map((r) => ({ name: String(r[enc.x]), value: num(r[enc.y]) })) }],
				},
			});
		},
	},
	scatter: {
		hc(ctx) {
			const { enc, rows } = ctx;
			const groups = enc.color ? uniq(rows, enc.color) : [null];
			const bubble = Boolean(enc.size);
			return hcBase(ctx, {
				chart: { type: bubble ? 'bubble' : 'scatter', zoomType: undefined },
				xAxis: { title: { text: label(ctx, 'x') } },
				yAxis: hcYAxis(ctx),
				legend: { enabled: groups.length > 1 },
				series: groups.map((gname) => ({
					name: gname ?? enc.y,
					type: bubble ? 'bubble' : 'scatter',
					data: rows
						.filter((r) => gname === null || r[enc.color] === gname)
						.map((r) => ({ x: r[enc.x], y: r[enc.y], ...(bubble ? { z: r[enc.size] } : {}), ...(enc.label ? { name: String(r[enc.label]) } : {}) })),
					dataLabels: enc.label ? { enabled: true, format: '{point.name}' } : undefined,
				})),
			});
		},
		ec(ctx) {
			const { enc, rows } = ctx;
			const groups = enc.color ? uniq(rows, enc.color) : [null];
			const maxSize = enc.size ? Math.max(...rows.map((r) => r[enc.size])) : 1;
			return ecBase(ctx, {
				legend: groups.length > 1,
				extra: {
					tooltip: { trigger: 'item' },
					xAxis: { type: 'value', name: label(ctx, 'x'), nameLocation: 'middle', nameGap: 28, scale: true },
					yAxis: ecYAxis(ctx, { scale: true }),
					series: groups.map((gname) => ({
						type: 'scatter',
						name: gname ?? enc.y,
						data: rows
							.filter((r) => gname === null || r[enc.color] === gname)
							.map((r) => ({
								value: [r[enc.x], r[enc.y]],
								name: enc.label ? String(r[enc.label]) : undefined,
								symbolSize: enc.size ? 8 + 40 * Math.sqrt(r[enc.size] / maxSize) : 10,
							})),
						label: enc.label ? { show: true, formatter: '{b}', position: 'top' } : undefined,
					})),
				},
			});
		},
	},
	strip: {
		hc(ctx) {
			const { enc, rows } = ctx;
			return hcBase(ctx, {
				chart: { type: 'scatter' },
				xAxis: { title: { text: label(ctx, 'x') ?? enc.x } },
				yAxis: hcYAxis(ctx),
				legend: { enabled: false },
				series: [{ type: 'scatter', name: enc.y, data: rows.map((r) => [r[enc.x], r[enc.y]]) }],
			});
		},
		ec(ctx) {
			const { enc, rows } = ctx;
			return ecBase(ctx, {
				extra: {
					xAxis: { type: 'value', name: label(ctx, 'x') ?? enc.x, nameLocation: 'middle', nameGap: 28, scale: true },
					yAxis: ecYAxis(ctx, { scale: true }),
					series: [{ type: 'scatter', symbolSize: 6, data: rows.map((r) => [r[enc.x], r[enc.y]]) }],
				},
			});
		},
	},
	heatmap: {
		hc(ctx) {
			const g = categoryGrid(ctx.rows, ctx.enc);
			const ext = signedExtent(g.data.map((d) => d[2]));
			if (ext.signed) ctx.note('highcharts', 'signed values: colorAxis is symmetric about zero (min -max, max +max) with the documented three-stop diverging stops, so zero is the middle colour');
			return hcBase(ctx, {
				chart: { type: 'heatmap' },
				xAxis: { categories: g.xs.map(String), title: { text: label(ctx, 'x') } },
				yAxis: { categories: g.ys.map(String), title: { text: label(ctx, 'y') }, reversed: true },
				colorAxis: ext.signed ? { min: -ext.max, max: ext.max, stops: [[0, '#3060cf'], [0.5, '#fffbbc'], [1, '#c4463a']] } : { min: Math.min(0, g.min), minColor: '#FFFFFF', maxColor: '#7cb5ec' },
				legend: { align: 'right', layout: 'vertical', verticalAlign: 'middle' },
				series: [{ type: 'heatmap', name: label(ctx, 'color') ?? ctx.enc.color, data: g.data, borderWidth: 1, dataLabels: { enabled: true } }],
			});
		},
		ec(ctx) {
			const g = categoryGrid(ctx.rows, ctx.enc);
			const ext = signedExtent(g.data.map((d) => d[2]));
			if (ext.signed) ctx.note('echarts', 'signed values: visualMap is symmetric about zero (min -max, max +max) with a three-colour diverging inRange, so zero is the middle colour');
			return ecBase(ctx, {
				extra: {
					tooltip: { position: 'top' },
					xAxis: { type: 'category', data: g.xs.map(String), name: label(ctx, 'x'), splitArea: { show: true } },
					yAxis: { type: 'category', data: g.ys.map(String), name: label(ctx, 'y'), inverse: true, splitArea: { show: true } },
					visualMap: { min: ext.signed ? -ext.max : g.min, max: ext.signed ? ext.max : g.max, calculable: true, orient: 'horizontal', left: 'center', bottom: 0, ...(ext.signed ? { inRange: { color: ['#4575b4', '#ffffbf', '#d73027'] } } : {}) },
					grid: { top: ctx.w < 500 ? 110 : 80, bottom: 60, left: 16, right: 24, containLabel: true },
					series: [{ type: 'heatmap', data: g.data, label: { show: true } }],
				},
			});
		},
	},
	histogram: {
		hc(ctx) {
			const bins = ctx.block.binning?.count;
			if (ctx.block.binning?.measure) ctx.unmap('highcharts', 'binning.measure', 'histogram series counts only');
			return hcBase(ctx, {
				xAxis: { title: { text: label(ctx, 'x') } },
				yAxis: { title: { text: label(ctx, 'y') } },
				legend: { enabled: false },
				series: [
					{ type: 'histogram', name: label(ctx, 'y') ?? 'Count', baseSeries: 'raw', ...(bins ? { binsNumber: bins } : {}) },
					{ type: 'scatter', id: 'raw', visible: false, showInLegend: false, data: ctx.rows.map((r) => r[ctx.enc.x]) },
				],
			});
		},
		ec: null,
	},
	boxplot: {
		hc(ctx) {
			const { enc, rows } = ctx;
			const cats = enc.x ? uniq(rows, enc.x) : [enc.y];
			const stats = cats.map((c) => boxStats(rows.filter((r) => !enc.x || r[enc.x] === c).map((r) => num(r[enc.y]))));
			return hcBase(ctx, {
				chart: { type: 'boxplot' },
				xAxis: { categories: cats.map(String), title: { text: label(ctx, 'x') } },
				yAxis: hcYAxis(ctx),
				legend: { enabled: false },
				series: [
					{ type: 'boxplot', name: enc.y, data: stats.map((s) => s.box) },
					{ type: 'scatter', name: 'Outliers', marker: { symbol: 'circle' }, data: stats.flatMap((s, i) => s.outliers.map((v) => [i, v])) },
				],
			});
		},
		ec(ctx) {
			const { enc, rows } = ctx;
			const cats = enc.x ? uniq(rows, enc.x) : [enc.y];
			const stats = cats.map((c) => boxStats(rows.filter((r) => !enc.x || r[enc.x] === c).map((r) => num(r[enc.y]))));
			return ecBase(ctx, {
				extra: {
					tooltip: { trigger: 'item' },
					xAxis: { type: 'category', data: cats.map(String), name: label(ctx, 'x') },
					yAxis: ecYAxis(ctx, { scale: true }),
					series: [
						{ type: 'boxplot', name: enc.y, data: stats.map((s) => s.box) },
						{ type: 'scatter', name: 'Outliers', data: stats.flatMap((s, i) => s.outliers.map((v) => [i, v])) },
					],
				},
			});
		},
	},
	gauge: {
		hc(ctx) {
			const { rows, enc, block } = ctx;
			const st = block.style ?? {};
			const colors = ['#DF5353', '#DDDF0D', '#55BF3B'];
			let from = st.min ?? 0;
			const plotBands = (st.bands ?? []).map((b, i) => {
				const band = { from, to: b.to, color: colors[i % 3], label: { text: b.label } };
				from = b.to;
				return band;
			});
			return hcBase(ctx, {
				chart: { type: 'gauge' },
				pane: { startAngle: -150, endAngle: 150 },
				yAxis: { min: st.min ?? 0, max: st.max ?? 100, plotBands, title: { text: label(ctx, 'y') } },
				series: [{ type: 'gauge', name: String(rows[0][enc.x]), data: [rows[0][enc.y]] }],
			});
		},
		ec(ctx) {
			const { rows, enc, block } = ctx;
			const st = block.style ?? {};
			const min = st.min ?? 0;
			const max = st.max ?? 100;
			const colors = ['#fd666d', '#37a2da', '#67e0e3'];
			const stops = (st.bands ?? []).map((b, i) => [(b.to - min) / (max - min), colors[i % 3]]);
			ctx.note('echarts', 'band names (Poor, Fair, Good) have no label slot on a gauge axis line');
			return ecBase(ctx, {
				grid: false,
				extra: {
					series: [
						{
							type: 'gauge',
							min,
							max,
							center: ['50%', '62%'],
							axisLine: stops.length ? { lineStyle: { width: 14, color: stops } } : undefined,
							detail: { valueAnimation: false, formatter: '{value}' },
							data: [{ value: rows[0][enc.y], name: String(rows[0][enc.x]) }],
						},
					],
				},
			});
		},
	},
	calendar: {
		hc: null,
		ec(ctx) {
			const { rows, enc } = ctx;
			const days = rows.map((r) => r[enc.x]).sort();
			const vals = rows.map((r) => num(r[enc.y]));
			return ecBase(ctx, {
				grid: false,
				extra: {
					tooltip: {},
					visualMap: { min: Math.min(...vals), max: Math.max(...vals), type: 'piecewise', orient: 'horizontal', left: 'center', bottom: 0 },
					calendar: { top: ctx.w < 500 ? 120 : 90, left: 40, right: 20, cellSize: ['auto', ctx.w < 500 ? 22 : 30], range: [days[0], days[days.length - 1]], itemStyle: { borderWidth: 0.5 }, yearLabel: { show: false } },
					series: [{ type: 'heatmap', coordinateSystem: 'calendar', data: rows.map((r) => [r[enc.x], num(r[enc.y])]) }],
				},
			});
		},
	},
	streamgraph: {
		hc(ctx) {
			const g = groupSeries(ctx.rows, ctx.enc, ctx.enc.y);
			return hcBase(ctx, {
				chart: { type: 'streamgraph' },
				xAxis: { ...hcXAxis(ctx, g), maxPadding: 0 },
				yAxis: { visible: false, startOnTick: false, endOnTick: false },
				legend: { enabled: true },
				series: g.series.map((s) => ({ type: 'streamgraph', name: s.name, data: s.data })),
			});
		},
		ec(ctx) {
			const { rows, enc } = ctx;
			return ecBase(ctx, {
				grid: false,
				legend: true,
				extra: {
					tooltip: { trigger: 'axis' },
					singleAxis: { type: 'time', top: ctx.w < 500 ? 110 : 80, bottom: 50 },
					series: [{ type: 'themeRiver', data: rows.map((r) => [r[enc.x], num(r[enc.y]), String(r[enc.color])]) }],
				},
			});
		},
	},
	gantt: {
		hc(ctx) {
			const { rows, enc } = ctx;
			const tasks = uniq(rows, enc.label);
			const phases = enc.color ? uniq(rows, enc.color) : [null];
			return hcBase(ctx, {
				yAxis: { type: 'category', categories: tasks.map(String), reversed: true },
				legend: { enabled: phases.length > 1 },
				series: phases.map((p) => ({
					name: p === null ? 'Tasks' : String(p),
					data: rows
						.filter((r) => p === null || r[enc.color] === p)
						.map((r) => ({ name: String(r[enc.label]), y: tasks.indexOf(r[enc.label]), start: toTs(r[enc.start]), end: toTs(r[enc.end]), ...(r[enc.start] === r[enc.end] ? { milestone: true } : {}) })),
				})),
			});
		},
		ec: null,
	},
	parallel: {
		hc(ctx) {
			const { rows, enc } = ctx;
			ctx.unmap('highcharts', 'color', 'the parallel-coordinates module has no colour-by-field channel; each row is its own series');
			return hcBase(ctx, {
				chart: { type: 'line', parallelCoordinates: true, parallelAxes: { lineWidth: 2 } },
				xAxis: { categories: enc.dimensions, labels: { format: '{text}' } },
				yAxis: enc.dimensions.map(() => ({ title: { text: '' } })),
				legend: { enabled: true },
				series: rows.map((r) => ({ type: 'line', name: String(r[enc.label]), data: enc.dimensions.map((d) => r[d]) })),
			});
		},
		ec(ctx) {
			const { rows, enc } = ctx;
			const classes = uniq(rows, enc.color);
			ctx.unmap('echarts', 'label', 'parallel series carry no per-line name; rows are grouped by class');
			return ecBase(ctx, {
				legend: true,
				grid: false,
				extra: {
					legend: { bottom: 0 },
					parallelAxis: enc.dimensions.map((d, i) => ({ dim: i, name: d })),
					parallel: { top: ctx.w < 500 ? 120 : 100, bottom: 50, left: 40, right: 60 },
					series: classes.map((c) => ({ type: 'parallel', name: String(c), data: rows.filter((r) => r[enc.color] === c).map((r) => enc.dimensions.map((d) => r[d])) })),
				},
			});
		},
	},
	sankey: {
		hc(ctx) {
			const { rows, enc } = ctx;
			return hcBase(ctx, { series: [{ type: 'sankey', name: label(ctx, 'y') ?? enc.y, keys: ['from', 'to', 'weight'], data: rows.map((r) => [String(r[enc.source]), String(r[enc.target]), num(r[enc.y])]) }] });
		},
		ec(ctx) {
			const { rows, enc } = ctx;
			const names = [...new Set(rows.flatMap((r) => [r[enc.source], r[enc.target]]))];
			return ecBase(ctx, {
				grid: false,
				extra: {
					tooltip: { trigger: 'item' },
					series: [{ type: 'sankey', top: ctx.w < 500 ? 110 : 80, bottom: 16, left: 16, right: 100, data: names.map((n) => ({ name: String(n) })), links: rows.map((r) => ({ source: String(r[enc.source]), target: String(r[enc.target]), value: num(r[enc.y]) })) }],
				},
			});
		},
	},
	chord: {
		hc(ctx) {
			const { rows, enc } = ctx;
			return hcBase(ctx, { series: [{ type: 'dependencywheel', name: label(ctx, 'y') ?? enc.y, keys: ['from', 'to', 'weight'], data: rows.map((r) => [String(r[enc.source]), String(r[enc.target]), num(r[enc.y])]) }] });
		},
		ec(ctx) {
			const { rows, enc } = ctx;
			const names = [...new Set(rows.flatMap((r) => [r[enc.source], r[enc.target]]))];
			return ecBase(ctx, {
				grid: false,
				extra: {
					tooltip: { trigger: 'item' },
					series: [{ type: 'chord', center: ['50%', '58%'], data: names.map((n) => ({ name: String(n) })), links: rows.map((r) => ({ source: String(r[enc.source]), target: String(r[enc.target]), value: num(r[enc.y]) })) }],
				},
			});
		},
	},
	network: {
		hc(ctx) {
			const { rows, enc } = ctx;
			ctx.unmap('highcharts', 'y', 'networkgraph links carry no weight channel');
			return hcBase(ctx, {
				plotOptions: { networkgraph: { keys: ['from', 'to'], layoutAlgorithm: { enableSimulation: false, integration: 'verlet', linkLength: 90, maxIterations: 400 } } },
				series: [{ type: 'networkgraph', dataLabels: { enabled: true, linkFormat: '' }, data: rows.map((r) => [String(r[enc.source]), String(r[enc.target])]) }],
			});
		},
		ec(ctx) {
			const { rows, enc } = ctx;
			const names = [...new Set(rows.flatMap((r) => [r[enc.source], r[enc.target]]))];
			const max = Math.max(...rows.map((r) => num(r[enc.y])));
			return ecBase(ctx, {
				grid: false,
				extra: {
					series: [
						{
							type: 'graph',
							layout: 'circular',
							top: ctx.w < 500 ? 110 : 80,
							roam: false,
							zoom: 0.7,
							label: { show: true },
							symbolSize: 30,
							data: names.map((n) => ({ name: String(n) })),
							links: rows.map((r) => ({ source: String(r[enc.source]), target: String(r[enc.target]), lineStyle: { width: 1 + 5 * (num(r[enc.y]) / max) } })),
						},
					],
				},
			});
		},
	},
	'packed-bubble': {
		hc(ctx) {
			const { rows, enc } = ctx;
			const groups = enc.color ? uniq(rows, enc.color) : [null];
			return hcBase(ctx, {
				chart: { type: 'packedbubble' },
				plotOptions: { packedbubble: { minSize: '20%', maxSize: '120%', layoutAlgorithm: { enableSimulation: false, gravitationalConstant: 0.05, splitSeries: false }, dataLabels: { enabled: true, format: '{point.name}' } } },
				series: groups.map((gname) => ({ type: 'packedbubble', name: gname === null ? enc.y : String(gname), data: rows.filter((r) => gname === null || r[enc.color] === gname).map((r) => ({ name: String(r[enc.label]), value: num(r[enc.y]) })) })),
			});
		},
		ec: null,
	},
	wordcloud: {
		hc(ctx) {
			const { rows, enc } = ctx;
			return hcBase(ctx, { series: [{ type: 'wordcloud', name: label(ctx, 'y') ?? enc.y, data: rows.map((r) => ({ name: String(r[enc.label]), weight: num(r[enc.y]) })) }] });
		},
		ec: null,
	},
	venn: {
		hc(ctx) {
			const { rows, enc } = ctx;
			return hcBase(ctx, { series: [{ type: 'venn', name: label(ctx, 'y') ?? enc.y, data: rows.map((r) => ({ sets: stripSets(r[enc.sets]), value: num(r[enc.y]), ...(stripSets(r[enc.sets]).length === 1 ? { name: r[enc.sets] } : {}) })) }] });
		},
		ec: null,
	},
	vector: {
		hc(ctx) {
			const { rows, enc } = ctx;
			// Verified in Highcharts 13.1.1: direction 0 draws an arrow pointing down (south) and the angle grows clockwise (90 is
			// west), the meteorological "wind from" convention of the vector demo. RichView's angle is counter-clockwise from east.
			ctx.note('highcharts', 'direction converted from RichView degrees counter-clockwise from east to Highcharts degrees clockwise from south (270 - angle), so the arrows point where RichView points them');
			// The series puts length and direction into the y data range, so the axes are fixed to the grid as the demo does.
			const bx = niceBounds(rows.map((r) => r[enc.x]), 6);
			const by = niceBounds(rows.map((r) => r[enc.y]), 4);
			const gapx = (bx.max - bx.min) / 12;
			const gapy = (by.max - by.min) / 8;
			const dir = (a) => (((270 - num(a)) % 360) + 360) % 360;
			return hcBase(ctx, {
				xAxis: { min: Math.min(bx.min, Math.min(...rows.map((r) => r[enc.x])) - gapx), max: Math.max(bx.max, Math.max(...rows.map((r) => r[enc.x])) + gapx), title: { text: label(ctx, 'x') } },
				yAxis: { min: Math.min(by.min, Math.min(...rows.map((r) => r[enc.y])) - gapy), max: Math.max(by.max, Math.max(...rows.map((r) => r[enc.y])) + gapy), title: { text: label(ctx, 'y') } },
				legend: { enabled: false },
				series: [{ type: 'vector', rotationOrigin: 'start', vectorLength: 20, data: rows.map((r) => [r[enc.x], r[enc.y], num(r[enc.size] ?? 1), dir(r[enc.angle])]) }],
			});
		},
		ec: null,
	},
	contour: {
		hc(ctx) {
			const { rows, enc } = ctx;
			return hcBase(ctx, {
				xAxis: { title: { text: label(ctx, 'x') } },
				yAxis: { title: { text: label(ctx, 'y') } },
				colorAxis: { stops: [[0, '#FFFFFF'], [1, '#7cb5ec']] },
				series: [{ type: 'contour', name: enc.z, data: rows.map((r) => [r[enc.x], r[enc.y], r[enc.z]]) }],
			});
		},
		ec: null,
	},
	interval: {
		hc(ctx) {
			const { rows, enc } = ctx;
			const cats = uniq(rows, enc.x);
			if (ctx.block.axes?.y?.format === 'percent') ctx.note('highcharts', 'axes.y.format percent is not applied; the axis shows fractions');
			return hcBase(ctx, {
				xAxis: { categories: cats.map(String), title: { text: label(ctx, 'x') } },
				yAxis: hcYAxis(ctx, { plotLines: hcPlotLines(ctx) }),
				legend: { enabled: false },
				series: [
					{ type: 'scatter', name: 'Estimate', data: cats.map((c) => num(rows.find((r) => r[enc.x] === c)[enc.y])), id: 'est' },
					{ type: 'errorbar', name: 'Range', data: cats.map((c) => { const r = rows.find((q) => q[enc.x] === c); return [num(r[enc.lower]), num(r[enc.upper])]; }) },
				],
			});
		},
		ec: null,
	},
	waterfall: {
		hc(ctx) {
			const { rows, enc } = ctx;
			return hcBase(ctx, {
				xAxis: { type: 'category', title: { text: label(ctx, 'x') } },
				yAxis: hcYAxis(ctx),
				legend: { enabled: false },
				series: [{ type: 'waterfall', name: enc.y, upColor: undefined, data: rows.map((r) => ({ name: String(r[enc.x]), y: num(r[enc.y]), ...(r[enc.measure] === 'total' ? { isSum: true } : {}) })) }],
			});
		},
		ec(ctx) {
			const { rows, enc } = ctx;
			ctx.note('echarts', 'no waterfall series: the documented stacked-bar idiom with a transparent base series');
			let run = 0;
			const base = [];
			const up = [];
			const down = [];
			for (const r of rows) {
				const v = num(r[enc.y]);
				if (r[enc.measure] === 'total') {
					base.push(0);
					up.push(v);
					down.push('-');
					run = v;
				} else {
					const next = run + v;
					base.push(Math.min(run, next));
					(v >= 0 ? up : down).push(Math.abs(v));
					(v >= 0 ? down : up).push('-');
					run = next;
				}
			}
			return ecBase(ctx, {
				extra: {
					tooltip: { trigger: 'axis' },
					xAxis: { type: 'category', data: rows.map((r) => String(r[enc.x])), name: label(ctx, 'x') },
					yAxis: ecYAxis(ctx),
					series: [
						{ type: 'bar', stack: 'total', itemStyle: { borderColor: 'transparent', color: 'transparent' }, emphasis: { itemStyle: { borderColor: 'transparent', color: 'transparent' } }, data: base },
						{ type: 'bar', stack: 'total', name: 'Increase', data: up },
						{ type: 'bar', stack: 'total', name: 'Decrease', data: down },
					],
				},
			});
		},
	},
	treemap: {
		hc(ctx) {
			const { rows, enc } = ctx;
						return hcBase(ctx, {
				series: [
					{
						type: 'treemap',
						colorByPoint: true,
						layoutAlgorithm: 'squarified',
						allowTraversingTree: true,
						levels: [{ level: 1, dataLabels: { enabled: true, align: 'left', verticalAlign: 'top' }, borderWidth: 3 }],
						data: rows.map((r) => ({ id: String(r[enc.label]), name: String(r[enc.label]), ...(r[enc.parent] ? { parent: String(r[enc.parent]), value: num(r[enc.y]) } : {}) })),
					},
				],
			});
		},
		ec(ctx) {
			return ecBase(ctx, { grid: false, extra: { series: [{ type: 'treemap', top: ctx.w < 500 ? 110 : 80, bottom: 10, roam: false, breadcrumb: { show: false }, data: nest(ctx.rows, ctx.enc) }] } });
		},
	},
	sunburst: {
		hc(ctx) {
			const { rows, enc } = ctx;
			return hcBase(ctx, {
				series: [
					{
						type: 'sunburst',
						allowTraversingTree: true,
						dataLabels: { rotationMode: 'circular' },
						levels: [{ level: 1, levelIsConstant: false }],
						data: [{ id: 'root', name: '' }, ...rows.map((r) => ({ id: String(r[enc.label]), name: String(r[enc.label]), parent: r[enc.parent] ? String(r[enc.parent]) : 'root', ...(r[enc.parent] ? { value: num(r[enc.y]) } : {}) }))],
					},
				],
			});
		},
		ec(ctx) {
			return ecBase(ctx, { grid: false, extra: { series: [{ type: 'sunburst', center: ['50%', '58%'], radius: [0, '70%'], label: { rotate: 'radial' }, data: nest(ctx.rows, ctx.enc) }] } });
		},
	},
	funnel: {
		hc(ctx) {
			const { rows, enc } = ctx;
			return hcBase(ctx, { series: [{ type: 'funnel', name: enc.y, data: rows.map((r) => [String(r[enc.x]), num(r[enc.y])]), dataLabels: { enabled: true, format: '<b>{point.name}</b> ({point.y:,.0f})' } }] });
		},
		ec(ctx) {
			const { rows, enc } = ctx;
			return ecBase(ctx, { grid: false, extra: { tooltip: { trigger: 'item' }, series: [{ type: 'funnel', top: ctx.w < 500 ? 110 : 80, bottom: 16, left: '10%', width: '80%', label: { show: true, position: 'inside' }, data: rows.map((r) => ({ name: String(r[enc.x]), value: num(r[enc.y]) })) }] } });
		},
	},
	dumbbell: {
		hc(ctx) {
			const { rows, enc } = ctx;
			return hcBase(ctx, {
				chart: { type: 'dumbbell' },
				xAxis: { type: 'category', title: { text: label(ctx, 'x') } },
				yAxis: hcYAxis(ctx),
				legend: { enabled: true },
				series: [{ type: 'dumbbell', name: `${enc.lower} to ${enc.upper}`, data: rows.map((r) => ({ name: String(r[enc.x]), low: num(r[enc.lower]), high: num(r[enc.upper]) })) }],
			});
		},
		ec: null,
	},
	band: {
		hc(ctx) {
			const { rows, enc } = ctx;
			const g = groupSeries(rows, { x: enc.x, y: enc.y ?? enc.upper }, enc.y);
			const key = (r) => (g.kind === 'date' ? toTs(r[enc.x]) : r[enc.x]);
			return hcBase(ctx, {
				xAxis: hcXAxis(ctx, g),
				yAxis: hcYAxis(ctx),
				series: [
					{ type: 'arearange', name: `${enc.lower} to ${enc.upper}`, data: rows.map((r) => [key(r), num(r[enc.lower]), num(r[enc.upper])]), lineWidth: 0, fillOpacity: 0.3 },
					...(enc.y ? [{ type: 'line', name: enc.y, data: rows.map((r) => [key(r), num(r[enc.y])]) }] : []),
				],
			});
		},
		ec(ctx) {
			const { rows, enc } = ctx;
			const g = groupSeries(rows, { x: enc.x, y: enc.upper }, enc.upper);
			const key = (r) => (g.kind === 'date' ? r[enc.x] : r[enc.x]);
			ctx.note('echarts', 'no range-area series: the documented two stacked line series with a transparent base');
			return ecBase(ctx, {
				extra: {
					tooltip: { trigger: 'axis' },
					xAxis: { ...ecXAxis(ctx, g), ...(g.kind === 'date' ? { type: 'category', data: rows.map(key) } : {}) },
					yAxis: ecYAxis(ctx, { scale: true }),
					series: [
						{ type: 'line', stack: 'band', symbol: 'none', lineStyle: { opacity: 0 }, data: rows.map((r) => num(r[enc.lower])) },
						{ type: 'line', stack: 'band', symbol: 'none', lineStyle: { opacity: 0 }, areaStyle: {}, name: `${enc.lower} to ${enc.upper}`, data: rows.map((r) => num(r[enc.upper]) - num(r[enc.lower])) },
						...(enc.y ? [{ type: 'line', name: enc.y, data: rows.map((r) => num(r[enc.y])) }] : []),
					],
				},
			});
		},
	},
	bullet: {
		hc(ctx) {
			const { rows, enc, block } = ctx;
			const ranges = block.style?.ranges ?? [];
			let from = 0;
			const plotBands = ranges.map((to, i) => {
				const band = { from, to, color: ['#666666', '#999999', '#bbbbbb'][i % 3] };
				from = to;
				return band;
			});
			if (enc.lower || enc.upper) ctx.unmap('highcharts', 'lower/upper', 'ranges come from style.ranges plot bands');
			return hcBase(ctx, {
				chart: { type: 'bullet', inverted: true },
				xAxis: { categories: rows.map((r) => String(r[enc.x])) },
				yAxis: { plotBands, title: { text: label(ctx, 'y') } },
				legend: { enabled: false },
				plotOptions: { series: { animation: false, pointPadding: 0.25, borderWidth: 0, targetOptions: { width: '200%' } } },
				series: [{ type: 'bullet', name: enc.y, data: rows.map((r) => ({ y: num(r[enc.y]), target: num(r[enc.target]) })) }],
			});
		},
		ec: null,
	},
	candlestick: {
		hc(ctx) {
			const { rows, enc } = ctx;
			return {
				...hcBase(ctx, {
					xAxis: { type: 'datetime' },
					legend: { enabled: false },
					series: [{ type: 'candlestick', name: 'Price', data: rows.map((r) => [toTs(r[enc.x]), r[enc.open], r[enc.high], r[enc.low], r[enc.close]]) }],
				}),
				rangeSelector: { enabled: false },
				navigator: { enabled: false },
				scrollbar: { enabled: false },
			};
		},
		ec(ctx) {
			const { rows, enc } = ctx;
			return ecBase(ctx, {
				extra: {
					tooltip: { trigger: 'axis' },
					xAxis: { type: 'category', data: rows.map((r) => String(r[enc.x])) },
					yAxis: { scale: true },
					series: [{ type: 'candlestick', data: rows.map((r) => [r[enc.open], r[enc.close], r[enc.low], r[enc.high]]) }],
				},
			});
		},
	},
	ohlc: {
		hc(ctx) {
			const { rows, enc } = ctx;
			return {
				...hcBase(ctx, {
					xAxis: { type: 'datetime' },
					legend: { enabled: false },
					series: [{ type: 'ohlc', name: 'Price', data: rows.map((r) => [toTs(r[enc.x]), r[enc.open], r[enc.high], r[enc.low], r[enc.close]]) }],
				}),
				rangeSelector: { enabled: false },
				navigator: { enabled: false },
				scrollbar: { enabled: false },
			};
		},
		ec: null,
	},
	tree: {
		hc(ctx) {
			const { rows, enc } = ctx;
			ctx.unmap('highcharts', 'y', 'treegraph nodes carry no size channel');
			return hcBase(ctx, {
				series: [
					{
						type: 'treegraph',
						keys: ['id', 'parent', 'name'],
						marker: { symbol: 'circle', radius: 6 },
						dataLabels: { align: 'left', x: 14, crop: false, overflow: 'allow' },
						data: rows.map((r) => ({ id: String(r[enc.label]), parent: r[enc.parent] ? String(r[enc.parent]) : undefined, name: String(r[enc.label]) })),
					},
				],
			});
		},
		ec(ctx) {
			const { rows, enc } = ctx;
			ctx.unmap('echarts', 'y', 'tree nodes carry no size channel');
			ctx.note('echarts', 'the tree series needs one root; the adapter adds an unnamed root above the two top-level units');
			return ecBase(ctx, { grid: false, extra: { series: [{ type: 'tree', top: ctx.w < 500 ? 120 : 90, bottom: 20, left: '10%', right: '20%', symbolSize: 8, data: [{ name: '', children: nest(rows, enc).map(stripValues) }] }] } });
		},
	},
	radar: {
		hc(ctx) {
			const { rows, enc } = ctx;
			const cats = uniq(rows, enc.x);
			const teams = enc.color ? uniq(rows, enc.color) : [enc.y];
			return hcBase(ctx, {
				chart: { polar: true, type: 'line' },
				xAxis: { categories: cats.map(String), tickmarkPlacement: 'on', lineWidth: 0 },
				yAxis: { gridLineInterpolation: 'polygon', lineWidth: 0, min: 0 },
				legend: { enabled: teams.length > 1 },
				series: teams.map((t) => ({ type: 'line', name: String(t), pointPlacement: 'on', data: cats.map((c) => num(rows.find((r) => r[enc.x] === c && (!enc.color || r[enc.color] === t))?.[enc.y])) })),
			});
		},
		ec(ctx) {
			const { rows, enc } = ctx;
			const cats = uniq(rows, enc.x);
			const teams = enc.color ? uniq(rows, enc.color) : [enc.y];
			const max = Math.max(...rows.map((r) => num(r[enc.y])));
			return ecBase(ctx, {
				grid: false,
				legend: teams.length > 1,
				extra: {
					tooltip: {},
					radar: { indicator: cats.map((c) => ({ name: String(c), max })), center: ['50%', '55%'], radius: '58%' },
					series: [{ type: 'radar', data: teams.map((t) => ({ name: String(t), value: cats.map((c) => num(rows.find((r) => r[enc.x] === c && (!enc.color || r[enc.color] === t))?.[enc.y])) })) }],
				},
			});
		},
	},
	pareto: {
		hc(ctx) {
			const { rows, enc } = ctx;
			const sorted = [...rows].sort((a, b) => num(b[enc.y]) - num(a[enc.y]));
			return hcBase(ctx, {
				xAxis: { categories: sorted.map((r) => String(r[enc.x])), title: { text: label(ctx, 'x') } },
				yAxis: [{ title: { text: label(ctx, 'y') } }, { title: { text: '' }, minPadding: 0, maxPadding: 0, max: 100, min: 0, opposite: true, labels: { format: '{value}%' } }],
				legend: { enabled: true },
				series: [
					{ type: 'column', id: 'base', name: enc.y, zIndex: 2, data: sorted.map((r) => num(r[enc.y])) },
					{ type: 'pareto', name: 'Cumulative', yAxis: 1, zIndex: 10, baseSeries: 'base', tooltip: { valueDecimals: 1, valueSuffix: '%' } },
				],
			});
		},
		ec(ctx) {
			const { rows, enc } = ctx;
			const sorted = [...rows].sort((a, b) => num(b[enc.y]) - num(a[enc.y]));
			const vals = sorted.map((r) => num(r[enc.y]));
			ctx.note('echarts', 'no pareto series: the documented bar plus cumulative-share line on a second axis');
			return ecBase(ctx, {
				legend: true,
				extra: {
					tooltip: { trigger: 'axis' },
					xAxis: { type: 'category', data: sorted.map((r) => String(r[enc.x])), name: label(ctx, 'x') },
					yAxis: [ecYAxis(ctx), { type: 'value', max: 100, min: 0, axisLabel: { formatter: '{value}%' } }],
					series: [
						{ type: 'bar', name: enc.y, data: vals },
						{ type: 'line', name: 'Cumulative', yAxisIndex: 1, data: cumulative(vals) },
					],
				},
			});
		},
	},
	combo: {
		hc(ctx) {
			const { rows, enc } = ctx;
			return hcBase(ctx, {
				xAxis: { categories: rows.map((r) => String(r[enc.x])), crosshair: true },
				yAxis: [{ title: { text: label(ctx, 'y') } }, { title: { text: ctx.block.axes?.y2?.label }, opposite: true }],
				legend: { enabled: true },
				series: [
					{ type: 'column', name: enc.y, data: rows.map((r) => num(r[enc.y])) },
					{ type: 'spline', name: enc.line, yAxis: 1, data: rows.map((r) => num(r[enc.line])) },
				],
			});
		},
		ec(ctx) {
			const { rows, enc } = ctx;
			return ecBase(ctx, {
				legend: true,
				extra: {
					tooltip: { trigger: 'axis' },
					xAxis: { type: 'category', data: rows.map((r) => String(r[enc.x])) },
					yAxis: [{ type: 'value', name: label(ctx, 'y') }, { type: 'value', name: ctx.block.axes?.y2?.label }],
					series: [
						{ type: 'bar', name: enc.y, data: rows.map((r) => num(r[enc.y])) },
						{ type: 'line', name: enc.line, yAxisIndex: 1, smooth: true, data: rows.map((r) => num(r[enc.line])) },
					],
				},
			});
		},
	},
	rose: {
		hc(ctx) {
			const { rows, enc } = ctx;
			return hcBase(ctx, {
				chart: { polar: true, type: 'column' },
				xAxis: { categories: rows.map((r) => String(r[enc.x])), tickmarkPlacement: 'on' },
				yAxis: { min: 0, title: { text: label(ctx, 'y') } },
				legend: { enabled: false },
				plotOptions: { series: { animation: false, pointPlacement: 'on' }, column: { pointPadding: 0, groupPadding: 0 } },
				series: [{ type: 'column', name: enc.y, data: rows.map((r) => num(r[enc.y])) }],
			});
		},
		ec(ctx) {
			const { rows, enc } = ctx;
			return ecBase(ctx, { grid: false, extra: { tooltip: { trigger: 'item' }, series: [{ type: 'pie', roseType: 'radius', radius: [20, '65%'], center: ['50%', '58%'], data: rows.map((r) => ({ name: String(r[enc.x]), value: num(r[enc.y]) })) }] } });
		},
	},
	waffle: {
		hc(ctx) {
			const { rows, enc } = ctx;
			const counts = hundred(rows.map((r) => num(r[enc.y])));
			ctx.note('highcharts', 'values rescaled to 100 squares (largest remainder), as the RichView subtitle states; item series draws one marker per count');
			return hcBase(ctx, {
				legend: { enabled: true },
				series: [{ type: 'item', name: enc.y, rows: 10, marker: { symbol: 'square' }, data: rows.map((r, i) => [String(r[enc.x]), counts[i]]), dataLabels: { enabled: false }, showInLegend: true }],
			});
		},
		ec: null,
	},
	bar3d: {
		hc(ctx) {
			const g = groupSeries(ctx.rows, ctx.enc, ctx.enc.y);
			return hcBase(ctx, {
				chart: { type: 'column', options3d: { enabled: true, alpha: 15, beta: 15, depth: 60, viewDistance: 25 } },
				xAxis: { categories: g.cats },
				yAxis: hcYAxis(ctx),
				legend: { enabled: g.series.length > 1 },
				plotOptions: { column: { depth: 25 } },
				series: g.series.map((s) => ({ type: 'column', name: s.name, data: s.data })),
			});
		},
		ec: null,
	},
	scatter3d: {
		hc(ctx) {
			const { rows, enc } = ctx;
			const groups = enc.color ? uniq(rows, enc.color) : [null];
			if (enc.label) ctx.unmap('highcharts', 'label', 'no data-label slot on scatter3d points');
			// Highcharts' 3-D axes are x (across), y (up) and z (depth). RichView draws the z measure up and y in depth, so the
			// vertical axis here is the z measure, as in RichView; the demo's explicit min and max per axis keep the box on the data.
			ctx.note('highcharts', 'axes assigned as RichView draws them: x across, the z measure up (Highcharts y), y in depth (Highcharts z); each axis min and max fixed to the data as in the demo');
			const bx = niceBounds(rows.map((r) => r[enc.x]));
			const by = niceBounds(rows.map((r) => r[enc.z]));
			const bz = niceBounds(rows.map((r) => r[enc.y]));
			return hcBase(ctx, {
				chart: { type: 'scatter3d', options3d: { enabled: true, alpha: 10, beta: 30, depth: 250, viewDistance: 5, fitToPlot: false, frame: { bottom: { size: 1, color: 'rgba(0,0,0,0.02)' }, back: { size: 1, color: 'rgba(0,0,0,0.04)' }, side: { size: 1, color: 'rgba(0,0,0,0.06)' } } } },
				xAxis: { ...bx, gridLineWidth: 1, title: { text: label(ctx, 'x') } },
				yAxis: { ...by, title: { text: label(ctx, 'z') ?? label(ctx, 'color') ?? enc.z } },
				zAxis: { ...bz, showFirstLabel: false, title: { text: label(ctx, 'y') } },
				legend: { enabled: groups.length > 1 },
				series: groups.map((gn) => ({ type: 'scatter3d', name: gn === null ? enc.z : String(gn), data: rows.filter((r) => gn === null || r[enc.color] === gn).map((r) => [r[enc.x], r[enc.z], r[enc.y]]) })),
			});
		},
		ec: null,
	},
};

function stripValues(n) {
	if (n.children) n.children = n.children.map(stripValues);
	return n;
}

/**
 * Adapt one block for both libraries.
 * @param {object} block parsed chart block
 * @param {object[]} rows data rows (inline or CSV, typed)
 * @param {object} types the types.json table
 * @param {{w:number,h:number}} size chart size in px
 */
export function adapt(block, rows, types, size) {
	const entry = types[block.type] ?? { highcharts: null, echarts: null };
	const unmapped = { highcharts: [], echarts: [] };
	const notes = { highcharts: [], echarts: [] };
	const ctx = {
		block,
		rows,
		enc: block.encoding ?? {},
		w: size.w,
		h: size.h,
		unmap: (lib, channel, reason) => unmapped[lib].push({ channel, reason }),
		note: (lib, text) => notes[lib].push(text),
	};
	const out = {};
	for (const [lib, key, short] of [['highcharts', 'highcharts', 'hc'], ['echarts', 'echarts', 'ec']]) {
		const mapped = entry[key];
		const build = builders[block.type]?.[short];
		if (!mapped) out[lib] = { ok: false, reason: entry.why?.[key] ?? 'the library does not define this type' };
		else if (!build) out[lib] = { ok: false, reason: 'the table maps the type but the adapter has no builder' };
		else {
			const opts = JSON.parse(JSON.stringify(build(ctx)));
			out[lib] = { ok: true, series: mapped.series, modules: mapped.modules ?? [], product: mapped.product ?? 'highcharts', doc: mapped.doc, options: opts };
		}
	}
	// A channel counts as unmapped for a library only where it was recorded for that library.
	return { ...out, unmapped, notes };
}
