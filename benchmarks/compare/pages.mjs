// Library-comparison lab pages (ARC-64): one static HTML page per Highcharts or ECharts chart. The
// page loads the library from the local evidence install by file URL and draws the adapter's
// options in a fixed-size box; nothing of the libraries is copied into the page or the repository.
// Development tooling only (ARC-03).

import path from 'node:path';
import { pathToFileURL } from 'node:url';

const productFile = { highcharts: 'highcharts.js', highstock: 'highstock.js', gantt: 'highcharts-gantt.js' };
const factory = { highcharts: 'chart', highstock: 'stockChart', gantt: 'ganttChart' };

const shell = (scripts, body) => `<!doctype html>
<meta charset="utf-8">
<style>html,body{margin:0;background:#fff}#c{width:var(--w);height:var(--h);overflow:hidden}</style>
${scripts.map((s) => `<script src="${s}"></script>`).join('\n')}
<div id="c"></div>
<script>
window.__done = false;
try {
${body}
} catch (e) { window.__error = String(e && e.stack || e); }
window.__done = true;
</script>
`;

/** The Highcharts page for adapter result `res` at `size`. */
export function highchartsPage(res, size, referenceDir) {
	const base = pathToFileURL(path.join(referenceDir, 'node_modules', 'highcharts')).href;
	const scripts = [`${base}/${productFile[res.product]}`, ...res.modules.map((m) => `${base}/${m}.js`)];
	const body = `Highcharts.setOptions({ chart: { animation: false } });
Highcharts.${factory[res.product]}('c', ${JSON.stringify(res.options)});`;
	return shell(scripts, body).replace('var(--w)', `${size.w}px`).replace('var(--h)', `${size.h}px`);
}

/** The ECharts page for adapter result `res` at `size`. */
export function echartsPage(res, size, referenceDir) {
	const file = pathToFileURL(path.join(referenceDir, 'node_modules', 'echarts', 'dist', 'echarts.js')).href;
	const body = `var chart = echarts.init(document.getElementById('c'), null, { width: ${size.w}, height: ${size.h} });
chart.setOption(${JSON.stringify(res.options)});`;
	return shell([file], body).replace('var(--w)', `${size.w}px`).replace('var(--h)', `${size.h}px`);
}
