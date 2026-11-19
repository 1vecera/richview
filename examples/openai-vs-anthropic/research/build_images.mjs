// Original editorial artwork from the frozen CSV, using the real Rust chart and SVG export.
// Run from the repository root after its release build. No source collection or network fetch.
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { chromium } from 'playwright';
import { chromiumPath, root, serveStatic, settled } from '../../../scripts/browser-harness.mjs';

const report = join(root, 'examples/openai-vs-anthropic');
const assets = join(report, 'assets'); mkdirSync(assets, { recursive: true });
const scratch = mkdtempSync(join(root, 'tmp/editorial-images-'));
const source = join(report, 'assets/editorial-charts-source-2026-10-02.csv');
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const rows = JSON.parse(execFileSync('uv', ['run', '--no-project', 'python', '-c', 'import csv,json,sys;print(json.dumps(list(csv.DictReader(open(sys.argv[1])))))', source], { encoding: 'utf8' }));
const row = name => { const value = rows.find(row => row.measure === name); if (!value) throw new Error(`Missing frozen measure: ${name}`); return value; };
const audiences = [
  ['US iOS ratings', 'US App Store ratings'],
  ['Firms paying', 'US businesses on Ramp'],
  ['Agent at work', 'Coding agents at work'],
].map(([measure, audience]) => ({ audience: `${audience} · ${row(measure).anthropic_share_pct}%`, share: row(measure).anthropic_share_pct, source: row(measure).source_id }));
const ratings = row('US iOS ratings');
const specifications = [
  { id: 'I-AUDIENCES', file: 'audiences.svg', chart: {
    type: 'bar', title: 'Different audiences, different leaders', subtitle: 'Anthropic ÷ (Anthropic + OpenAI), across distinct measures',
    data: { inline: audiences }, encoding: { x: 'audience', y: 'share' },
    axes: { y: { min: 0, max: 100, unit: '%' } }, style: { orientation: 'horizontal' },
  }, sources: audiences.map(row => row.source) },
  { id: 'I-RATINGS', file: 'app-store-ratings.svg', chart: {
    type: 'bar', title: 'ChatGPT has a much larger ratings footprint', subtitle: 'Cumulative US App Store ratings · 2 October 2026',
    data: { inline: [{ product: 'ChatGPT', ratings: ratings.openai_value }, { product: 'Claude', ratings: ratings.anthropic_value }] },
    encoding: { x: 'product', y: 'ratings' }, style: { orientation: 'horizontal' },
    annotations: [{ kind: 'point', at: { x: 'ChatGPT', y: Number(ratings.openai_value) }, text: BigInt(ratings.openai_value).toLocaleString('en-US') }, { kind: 'point', at: { x: 'Claude', y: Number(ratings.anthropic_value) }, text: BigInt(ratings.anthropic_value).toLocaleString('en-US') }],
  }, sources: [ratings.source_id] },
];
const routes = {};
for (const item of specifications) {
  const folder = join(scratch, item.id); mkdirSync(folder);
  const rmd = '---\ntitle: Editorial artwork\nversion: "1"\n---\n\n```chart\n' + JSON.stringify(item.chart, null, 2) + '\n```\n';
  writeFileSync(join(folder, 'report.rmd'), rmd);
  execFileSync(join(root, 'target/release/richview'), ['generate', join(folder, 'report.rmd'), '--style', 'scientific', '--public-dir', join(root, 'dist/web')], { cwd: root, stdio: 'pipe' });
  routes[`/${item.id}/`] = join(folder, 'Rendered');
}
const fontSource = readFileSync(join(root, 'crates/richview-core/assets/fonts-scientific.css'), 'utf8');
const fonts = [...fontSource.matchAll(/@font-face\s*\{[^}]+\}/g)].map(match => match[0]).filter(face => {
  const field = name => face.match(new RegExp(`${name}\\s*:\\s*([^;]+)`))?.[1].trim().replaceAll(/['"]/g, '');
  return field('font-style') === 'normal' && (field('font-family') === 'RichView Serif' && ['400', '600'].includes(field('font-weight')) || field('font-family') === 'RichView Code' && field('font-weight') === '400');
}).join('\n');
if ((fonts.match(/@font-face/g) || []).length !== 3) throw new Error('Expected the three pinned scientific text faces');
const notices = [readFileSync(join(root, 'LICENSE'), 'utf8'), ...['source-serif-4', 'source-code-pro'].map(name => readFileSync(join(root, `crates/richview-core/assets/font-licenses/${name}.txt`), 'utf8'))].join('\n\n');
const xml = text => text.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;');
const server = await serveStatic(routes);
const browser = await chromium.launch({ headless: true, executablePath: chromiumPath(), args: ['--no-sandbox'] });
const evidence = { snapshot: '2026-10-02', imageRevision: '2026-10-15', source: 'assets/editorial-charts-source-2026-10-02.csv', source_sha256: hash(readFileSync(source)), renderer_commit: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim(), runtime_sha256: hash(readFileSync(join(root, 'dist/web/richview_site_bg.wasm'))), browser: browser.version(), assets: [] };
try {
  const page = await browser.newPage({ viewport: { width: 1200, height: 1000 }, reducedMotion: 'reduce', acceptDownloads: true });
  for (const item of specifications) {
    await page.goto(`${server.base}/${item.id}/report.html`);
    await page.waitForFunction(() => window.__richview);
    await page.evaluate(() => window.__richview.settled());
    const figure = page.locator('figure.rv-chart');
    // A phone-sized original keeps chart labels legible when embedded in narrow report text.
    await figure.evaluate(figure => { figure.style.width = '420px'; figure.style.maxWidth = 'none'; });
    await settled(page);
    const background = await figure.evaluate(figure => getComputedStyle(figure).getPropertyValue('--rv-bg').trim() || '#fff');
    const [download] = await Promise.all([page.waitForEvent('download'), figure.getByRole('button', { name: 'Download SVG' }).click()]);
    let svg = readFileSync(await download.path(), 'utf8');
    const dimensions = svg.match(/viewBox="0 0 ([\d.]+) ([\d.]+)"/).slice(1).map(Number);
    const size = dimensions.map(Math.ceil);
    // Round the outer canvas only, leaving all Rust mark and text geometry unchanged.
    svg = svg.replace(/viewBox="0 0 [\d.]+ [\d.]+"/, `viewBox="0 0 ${size[0]} ${size[1]}"`).replace(/width="[\d.]+"/, `width="${size[0]}"`).replace(/height="[\d.]+"/, `height="${size[1]}"`);
    const provenance = { id: item.id, creator: 'RichView', license: 'MIT; embedded fonts SIL OFL 1.1', snapshot: evidence.snapshot, imageRevision: evidence.imageRevision, source: evidence.source, source_sha256: evidence.source_sha256, source_ids: item.sources, runtime_sha256: evidence.runtime_sha256 };
    const at = svg.indexOf('>') + 1;
    svg = svg.slice(0, at) + `<metadata>${xml(JSON.stringify(provenance, null, 2) + '\n\n' + notices)}</metadata><style><![CDATA[${fonts}]]></style><rect width="100%" height="100%" fill="${xml(background)}"/>` + svg.slice(at);
    writeFileSync(join(assets, item.file), svg + '\n');
    await figure.screenshot({ path: join(scratch, `${item.id}-live.png`) });
    await page.setContent(`<img alt="${item.chart.title}" src="data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}">`);
    await page.locator('img').evaluate(image => image.decode());
    await page.locator('img').screenshot({ path: join(scratch, `${item.id}-standalone.png`) });
    evidence.assets.push({ id: item.id, file: item.file, width: size[0], height: size[1], sha256: hash(Buffer.from(svg + '\n')), source_ids: item.sources, chart: item.chart });
  }
} finally { await browser.close(); await server.close(); }
writeFileSync(join(assets, 'editorial-charts.json'), JSON.stringify(evidence, null, 2) + '\n');
console.log(JSON.stringify({ scratch, assets: evidence.assets.map(({ chart, ...asset }) => asset) }, null, 2));
