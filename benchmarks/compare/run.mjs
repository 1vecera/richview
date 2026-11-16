// Library-comparison lab harness (ARC-64, FEA-CH-75): renders every chart type of
// examples/every-block that Highcharts or ECharts also draws, in RichView, Highcharts and ECharts
// on the same data, captures all three with headless Chromium at 720x440 and 390x440 with reduced
// motion, and writes one sheet per type, comparison.json and the contact sheet index.html.
// Runs in no gate tier. Development tooling only (ARC-03); outputs never enter the repository.
//
//   node benchmarks/compare/run.mjs [--evidence DIR] [--richview DIR] [--out DIR] [--types a,b]
//        [--styles sleek,air] [--keep]
//
// Highcharts is commercially licensed and runs only as a local reference (INT-53): it is read from
// the evidence install and never vendored, committed or added to a package.json.

import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { chromium } from 'playwright';
import { adapt } from './adapter.mjs';
import { loadYaml, readCases } from './corpus.mjs';
import { echartsPage, highchartsPage } from './pages.mjs';
import { indexHtml, sheetHtml } from './sheet.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, '..', '..');
const args = process.argv.slice(2);
const opt = (name, fallback) => {
	const i = args.indexOf(`--${name}`);
	return i >= 0 ? args[i + 1] : fallback;
};
const home = os.homedir();
const evidence = path.resolve(opt('evidence', process.env.RICHVIEW_EVIDENCE ?? path.join(home, 'tmp/richview-rewrite/labs')));
const richviewDir = path.resolve(opt('richview', process.env.RICHVIEW_BIN ?? path.join(home, 'tmp/richview-rewrite/bin-release/8008603c43')));
// RichView is captured once per style, so a reviewer can compare light against light (Air) and still judge Sleek.
// The first style keeps the image key `richview`; each further one is `richview-<style>`.
const styles = (opt('styles', opt('style', 'sleek,air')) ?? '').split(',').filter(Boolean);
const style = styles[0];
const only = opt('types', '')?.split(',').filter(Boolean);
const keep = args.includes('--keep');
const commit = opt('commit', path.basename(richviewDir));
const out = path.resolve(opt('out', path.join(evidence, 'arc64', commit)));
const referenceDir = path.join(evidence, 'reference');

const HEIGHT = 440;
// RichView's reading column caps a figure at 560 px; `wide: true` lets it fill the page minus 24 px
// of margin on each side, so the desktop viewport is the target width plus 48. The phone figure fills
// the viewport minus 16 px of margin on each side, so its viewport is 32 wider.
const WIDTHS = { 720: { viewport: 720 + 48 }, 390: { viewport: 390 + 32 } };

const pkgVersion = (name) => JSON.parse(fs.readFileSync(path.join(referenceDir, 'node_modules', name, 'package.json'), 'utf8')).version;
const yaml = loadYaml(referenceDir);
const typesTable = JSON.parse(fs.readFileSync(path.join(here, 'types.json'), 'utf8')).types;
const rmd = path.join(repo, 'examples', 'every-block', 'report.rmd');
let cases = readCases(rmd, yaml);
if (only?.length) cases = cases.filter((c) => only.includes(c.id) || only.includes(c.type));

const work = path.join(out, '.work');
fs.rmSync(work, { recursive: true, force: true });
fs.mkdirSync(path.join(work, 'pages'), { recursive: true });
fs.mkdirSync(path.join(out, 'sheets'), { recursive: true });

const richviewVersion = execFileSync(path.join(richviewDir, 'richview'), ['--version']).toString().trim();
const sh = (cmd, a) => {
	try {
		return execFileSync(cmd, a, { cwd: repo }).toString().trim();
	} catch {
		return null;
	}
};

/** Write a one-chart report folder and render it with `richview generate`; returns the html path and warnings. */
function renderRichView(c, width, ratio, style) {
	const dir = path.join(work, `${c.id}-${width}-${style}`);
	fs.mkdirSync(dir, { recursive: true });
	const block = { ...c.block, wide: true, style: { ...(c.block.style ?? {}), ratio } };
	if (c.csv) {
		fs.mkdirSync(path.join(dir, path.dirname(c.csv)), { recursive: true });
		fs.copyFileSync(path.join(path.dirname(rmd), c.csv), path.join(dir, c.csv));
	}
	const front = yaml.stringify({ title: `Library comparison, ${c.id}`, description: 'One chart for the ARC-64 library-comparison lab.', version: '0.0.1', slug: `compare-${c.id}`, author: 'RichView lab', date: '2026-10-28' });
	fs.writeFileSync(path.join(dir, 'report.rmd'), `---\n${front}---\n\n\`\`\`chart\n${yaml.stringify(block, { lineWidth: 0 })}\`\`\`\n`);
	const res = execFileSync(path.join(richviewDir, 'richview'), ['generate', '--style', style, dir], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
	return { html: path.join(dir, 'Rendered', 'report.html'), log: res };
}

async function figureBox(page, html) {
	await page.goto(pathToFileURL(html).href);
	const fig = page.locator('figure[data-rv-chart], figure.rv-chart').first();
	await fig.waitFor({ state: 'visible', timeout: 15000 });
	await page.evaluate(() => document.fonts.ready);
	await page.waitForTimeout(1200);
	return { fig, box: await fig.boundingBox() };
}

async function captureRichView(browser, c, width, style, shotPath) {
	const ctx = await browser.newContext({ viewport: { width: WIDTHS[width].viewport, height: 1000 }, reducedMotion: 'reduce', deviceScaleFactor: 1 });
	const page = await ctx.newPage();
	try {
		let ratio = width / (HEIGHT - 114);
		let { html } = renderRichView(c, width, ratio, style);
		let { fig, box } = await figureBox(page, html);
		for (let pass = 0; pass < 2 && Math.abs(box.height - HEIGHT) > 3; pass++) {
			const chrome = box.height - width / ratio;
			ratio = Math.min(6, Math.max(0.5, width / (HEIGHT - chrome)));
			({ html } = renderRichView(c, width, ratio, style));
			({ fig, box } = await figureBox(page, html));
		}
		await fig.screenshot({ path: shotPath });
		return { ok: true, figure: { w: Math.round(box.width), h: Math.round(box.height) }, ratio: Math.round(ratio * 1000) / 1000 };
	} catch (e) {
		return { ok: false, error: String(e.message ?? e).split('\n')[0] };
	} finally {
		await ctx.close();
	}
}

async function captureLibrary(browser, lib, res, c, width, shotPath) {
	const size = { w: width, h: HEIGHT };
	const html = lib === 'highcharts' ? highchartsPage(res, size, referenceDir) : echartsPage(res, size, referenceDir);
	const file = path.join(work, 'pages', `${c.id}-${lib}-${width}.html`);
	fs.writeFileSync(file, html);
	const ctx = await browser.newContext({ viewport: { width: width + 40, height: HEIGHT + 40 }, reducedMotion: 'reduce', deviceScaleFactor: 1 });
	const page = await ctx.newPage();
	const errors = [];
	page.on('pageerror', (e) => errors.push(String(e.message).split('\n')[0]));
	page.on('console', (m) => m.type() === 'error' && errors.push(m.text().split('\n')[0]));
	try {
		await page.goto(pathToFileURL(file).href);
		await page.waitForFunction(() => window.__done === true, null, { timeout: 15000 });
		await page.waitForTimeout(800);
		const err = await page.evaluate(() => window.__error ?? null);
		if (err) return { ok: false, error: err.split('\n')[0] };
		await page.locator('#c').screenshot({ path: shotPath });
		return { ok: true, warnings: errors };
	} catch (e) {
		return { ok: false, error: String(e.message ?? e).split('\n')[0] };
	} finally {
		await ctx.close();
	}
}

let browser = await chromium.launch();
// Highcharts' contour series draws with WebGPU; headless Chromium needs a software adapter for it.
// Only the types that need it pay for the slower software path.
const gpuTypes = new Set(['contour']);
let gpuBrowser = null;

/** Runs a capture with a watchdog: a page that never settles costs one retry on a fresh browser, then an error. */
async function guarded(run) {
	for (let attempt = 0; ; attempt++) {
		let timer;
		const timeout = new Promise((resolve) => {
			timer = setTimeout(() => resolve({ ok: false, error: 'capture timed out after 60 s', timedOut: true }), 60000);
		});
		const r = await Promise.race([run(browser), timeout]);
		clearTimeout(timer);
		if (!r.timedOut) return r;
		browser.close().catch(() => {});
		browser = await chromium.launch();
		if (attempt >= 1) return r;
	}
}
const libBrowser = async (type, b) => (gpuTypes.has(type) ? (gpuBrowser ??= await chromium.launch({ args: ['--enable-unsafe-webgpu', '--enable-unsafe-swiftshader', '--use-webgpu-adapter=swiftshader', '--use-angle=swiftshader'] })) : b);
const started = Date.now();
const entries = [];
for (const c of cases) {
	// A fresh browser per type: a page left spinning by one chart must not starve the next.
	const previous = browser;
	browser = await chromium.launch();
	await Promise.race([previous.close().catch(() => {}), new Promise((r) => setTimeout(r, 5000))]);
	const entry = {
		id: c.id,
		type: c.type,
		libraries: [],
		richviewStyles: styles.map((st) => ({ style: st, key: st === style ? 'richview' : `richview-${st}` })),
		series: {},
		doc: {},
		unmapped: { highcharts: [], echarts: [] },
		notes: { highcharts: [], echarts: [] },
		skipped: {},
		errors: {},
		widths: Object.fromEntries(Object.keys(WIDTHS).map((w) => [w, { height: HEIGHT, richviewViewport: WIDTHS[w].viewport }])),
		images: {},
	};
	const results = {};
	for (const w of Object.keys(WIDTHS)) results[w] = adapt(c.block, c.rows, typesTable, { w: Number(w), h: HEIGHT });
	const first = results[Object.keys(WIDTHS)[0]];
	for (const lib of ['highcharts', 'echarts']) {
		if (first[lib].ok) {
			entry.series[lib] = first[lib].series;
			entry.doc[lib] = first[lib].doc;
			if (lib === 'highcharts') entry.modules = { product: first[lib].product, modules: first[lib].modules };
		} else entry.skipped[lib] = first[lib].reason;
	}
	entry.unmapped = first.unmapped;
	entry.notes = first.notes;
	const wanted = ['highcharts', 'echarts'].filter((l) => first[l].ok);
	if (!wanted.length) {
		entries.push(entry);
		console.log(`${c.id}: neither library defines it`);
		continue;
	}
	const capDir = path.join(out, 'captures', c.id);
	fs.mkdirSync(capDir, { recursive: true });
	const richviewKey = (st) => (st === style ? 'richview' : `richview-${st}`);
	const shot = (lib, w) => path.join(capDir, `${lib}-${w}.png`);
	for (const w of Object.keys(WIDTHS)) {
		for (const st of styles) {
			const key = richviewKey(st);
			const rv = await guarded((b) => captureRichView(b, c, w, st, shot(key, w)));
			if (rv.ok) {
				entry.images[key] ??= {};
				entry.images[key][w] = path.relative(out, shot(key, w));
				entry.widths[w][key] = { figure: rv.figure, ratio: rv.ratio };
			} else entry.errors[key] = rv.error;
		}
		for (const lib of wanted) {
			const r = await guarded(async (b) => captureLibrary(await libBrowser(c.type, b), lib, results[w][lib], c, Number(w), shot(lib, w)));
			if (r.ok) {
				entry.images[lib] ??= {};
				entry.images[lib][w] = path.relative(out, shot(lib, w));
				if (r.warnings.length) (entry.warnings ??= {})[lib] = [...new Set(r.warnings)];
			} else entry.errors[lib] = r.error;
		}
	}
	entry.libraries = wanted.filter((l) => entry.images[l]);
	entries.push(entry);
	console.log(`${c.id}: ${wanted.join('+')}${Object.keys(entry.errors).length ? ' errors ' + JSON.stringify(entry.errors) : ''}`);
}

// Sheets: one PNG per drawn type, composed from the captures.
const sheetPage = await (await browser.newContext({ viewport: { width: 3200, height: 1000 }, deviceScaleFactor: 1 })).newPage();
for (const e of entries) {
	if (!Object.keys(e.images).length) continue;
	const html = sheetHtml(e, (lib, w) => (e.images[lib]?.[w] ? pathToFileURL(path.join(out, e.images[lib][w])).href : null));
	const file = path.join(work, `sheet-${e.id}.html`);
	fs.writeFileSync(file, html);
	await sheetPage.goto(pathToFileURL(file).href);
	await sheetPage.waitForLoadState('load');
	const target = path.join(out, 'sheets', `${e.id}.png`);
	await sheetPage.screenshot({ path: target, fullPage: true });
	e.images.sheet = path.relative(out, target);
}
await browser.close();
await gpuBrowser?.close();

const drawn = entries.filter((e) => e.libraries.length);
const has = (e, l) => e.libraries.includes(l) || Boolean(e.series[l]);
const counts = {
	types: entries.length,
	both: entries.filter((e) => has(e, 'highcharts') && has(e, 'echarts')).length,
	highchartsOnly: entries.filter((e) => has(e, 'highcharts') && !has(e, 'echarts')).length,
	echartsOnly: entries.filter((e) => !has(e, 'highcharts') && has(e, 'echarts')).length,
	neither: entries.filter((e) => !has(e, 'highcharts') && !has(e, 'echarts')).length,
	highcharts: entries.filter((e) => has(e, 'highcharts')).length,
	echarts: entries.filter((e) => has(e, 'echarts')).length,
	drawn: drawn.length,
};
const manifest = {
	schema: 1,
	lab: 'ARC-64 library comparison (FEA-CH-75)',
	generatedAt: new Date().toISOString(),
	richview: { commit, version: richviewVersion, style, styles: styles.map((st) => ({ style: st, key: st === style ? 'richview' : `richview-${st}`, scheme: st === 'air' ? 'light' : st === 'sleek' ? 'dark' : null })), repoCommit: sh('git', ['rev-parse', '--short=10', 'HEAD']), source: 'examples/every-block/report.rmd' },
	libraries: { highcharts: { version: pkgVersion('highcharts') }, echarts: { version: pkgVersion('echarts') } },
	capture: { widths: Object.keys(WIDTHS).map(Number), height: HEIGHT, reducedMotion: true, deviceScaleFactor: 1, browser: 'Playwright Chromium' },
	counts,
	types: entries,
};
fs.writeFileSync(path.join(out, 'comparison.json'), JSON.stringify(manifest, null, '\t') + '\n');
fs.writeFileSync(path.join(out, 'index.html'), indexHtml(manifest));
fs.copyFileSync(path.join(here, 'REVIEW-PROMPT.md'), path.join(out, 'REVIEW-PROMPT.md'));
fs.copyFileSync(path.join(here, 'verdicts.schema.json'), path.join(out, 'verdicts.schema.json'));
if (!keep) fs.rmSync(work, { recursive: true, force: true });
console.log(JSON.stringify(counts), `${Math.round((Date.now() - started) / 1000)} s`, path.join(out, 'index.html'));
