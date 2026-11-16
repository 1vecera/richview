// Regenerates the site's share images, assets/share/home.png and flagship.png, and the launched
// home image, assets/site-home-launched.png (FEA-SF-55).
// Host reason: a browser rasterizes the pages; nothing here ships. The flagship image shows
// RichView rendering a neutral sample in three styles: no title, verdict or figure from the report (INT-45).
// The images follow the home page: its headline face and tracking, the header's wordmark and the
// site's colour tokens, so a link preview and the page it opens read as one brand.
import { launchBrowser } from "../tools/rv-gate/lib/browser-launch.mjs";
import { chromium, errors as playwrightErrors } from 'playwright';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync, copyFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const cli = join(root, 'target/release/richview');
// The site's faces, staged by the build from the report font builds and assets/site-fonts (STY-28).
const fonts = join(root, 'dist/web/fonts');
// The site's colours, read from the pages' own sheets: the tokens every page uses, and the home
// page's headline ink.
const sheets = ['assets/site.css', 'assets/landing/homepage.css'].map(file => readFileSync(join(root, file), 'utf8')).join('\n');
const token = name => {
  const found = sheets.match(new RegExp(`--${name}:\\s*(#[0-9a-fA-F]{6});`));
  if (!found) throw new Error(`the site sheets define no colour --${name}`);
  return found[1];
};
const [paper, ink, inkStrong, soft, blue] = ['paper', 'ink', 'ink-strong', 'soft', 'blue'].map(token);
const logo = readFileSync(join(root, 'assets/landing/favicon.svg'), 'utf8');
const work = mkdtempSync(join(tmpdir(), 'rv-share-'));
const sample = `---\ntitle: Sample\n---\n\n\`\`\`chart\ntype: line\ntitle: Sample series\ndata:\n  inline:\n${['A:12,18,15,26,31,38', 'B:20,22,27,25,30,29'].flatMap(row => { const [s, vals] = row.split(':'); return vals.split(',').map((v, i) => `    - { x: "2026-0${i + 1}", series: ${s}, v: ${v} }`); }).join('\n')}\nencoding: { x: x, y: v, color: series }\n\`\`\`\n`;
mkdirSync(join(work, 'report'));
writeFileSync(join(work, 'report/report.rmd'), sample);
const styles = [['sleek', 'Sleek'], ['air', 'Air'], ['brutalist', 'Brutalist']];
for (const [id] of styles) {
  const run = spawnSync(cli, ['generate', join(work, 'report/report.rmd'), '--style', id, '-o', `${id}.html`], { encoding: 'utf8' });
  if (run.status !== 0) throw new Error(run.stderr || run.stdout);
}
const face = (family, file, weight) => `@font-face{font-family:${family};font-weight:${weight};src:url(${pathToFileURL(join(fonts, file))}) format('woff2')}`;
// The wordmark is the site header's (.nav__brand): Geist 600 at 0.01em beside the mark, which stands
// 1.365 times the type size. The headline is the home page's h1: Bricolage Grotesque 600 at -0.04em,
// its last word in the brand blue.
const css = `${face('Bricolage', 'bricolage-grotesque-600.woff2', 600)}${face('Geist', 'geist-400.woff2', 400)}${face('Geist', 'geist-600.woff2', 600)}${face('Plex', 'richview-mono-600.woff2', 600)}
*{box-sizing:border-box;margin:0}html,body{width:1200px;height:630px;background:${paper};color:${ink};font-family:Geist,system-ui,sans-serif;overflow:hidden}
.mark{width:84px;height:84px}.word{font:600 62px/1 Geist,sans-serif;letter-spacing:0.01em}
.pill{display:inline-block;border:2px solid ${blue};border-radius:999px;padding:8px 22px;color:${blue};font:600 22px/1 Plex,monospace}`;
const home = `<style>${css}
body{display:flex;flex-direction:column;justify-content:center;padding:0 110px;gap:34px}
.brand{display:flex;align-items:center;gap:26px}h1{font:600 104px/1 Bricolage,Geist,sans-serif;letter-spacing:-0.04em;color:${inkStrong};white-space:nowrap}
h1 em{font-style:normal;color:${blue}}p{font-size:38px;color:${soft};margin-top:8px}</style>
<div class="brand"><div class="mark">${logo}</div><div class="word">RichView</div></div>
<div><h1>Let your agent <em>present.</em></h1><p>Designed, reproducible, versioned.</p></div>
<div><span class="pill">Coming soon · richview.ai</span></div>`;
const browser = await launchBrowser(chromium, { headless: true, executablePath: process.env.RICHVIEW_CHROMIUM || undefined, args: ['--no-sandbox'] }, { TimeoutError: playwrightErrors.TimeoutError });
const shots = [];
for (const [id, label] of styles) {
  const page = await browser.newPage({ viewport: { width: 520, height: 560 }, deviceScaleFactor: 1 });
  await page.goto(pathToFileURL(join(work, 'report/Rendered', `${id}.html`)).href);
  await page.waitForSelector('figure canvas', { timeout: 15000 });
  await page.addStyleTag({ content: 'figure button,figure [role=toolbar]{display:none!important}' });
  await page.waitForTimeout(800);
  const figure = page.locator('figure').first();
  const file = join(work, `${id}.png`);
  await figure.screenshot({ path: file });
  shots.push([label, pathToFileURL(file).href]);
  await page.close();
}
const cards = shots.map(([label, src]) => `<div class="card"><img src="${src}"><span>${label}</span></div>`).join('');
const flagship = `<style>${css}
body{padding:56px 60px;display:flex;flex-direction:column;gap:30px}
.top{display:flex;align-items:center;gap:18px}.top .mark{width:46px;height:46px}.top .word{font-size:34px}.top em{margin-left:auto;font:400 26px Geist;color:${soft};font-style:normal}
.row{display:flex;gap:26px;flex:1;min-height:0;align-items:center}.card{flex:1;display:flex;flex-direction:column;gap:14px;min-width:0}
.card img{width:100%;height:auto;border-radius:12px;box-shadow:0 2px 8px oklch(0 0 0 / 0.12)}
.card span{font:600 24px Geist;color:${soft}}</style>
<div class="top"><div class="mark">${logo}</div><div class="word">RichView</div><em>One report, many styles</em></div><div class="row">${cards}</div>`;
const launchedHome = home.replace('Coming soon · richview.ai', 'richview.ai');
for (const [name, html] of [['home', home], ['home-launched', launchedHome], ['flagship', flagship]]) {
  const page = await browser.newPage({ viewport: { width: 1200, height: 630 }, deviceScaleFactor: 1 });
  writeFileSync(join(work, `${name}.html`), `<!doctype html><meta charset="utf-8">${html}`);
  await page.goto(pathToFileURL(join(work, `${name}.html`)).href);
  await page.evaluate(() => document.fonts.ready);
  await page.screenshot({ path: name === 'home-launched' ? join(root, 'assets/site-home-launched.png') : join(root, 'assets/share', `${name}.png`) });
  await page.close();
}
await browser.close();
console.log('wrote both home share images and flagship.png');
