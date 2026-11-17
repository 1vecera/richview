// The site gate (FEA-SF-62, DEL-59): the route map, dead links, chart-type docs coverage, the
// first-report example in every style, and every page under the deployed headers, with no policy
// violation (FEA-SF-60), at phone and desktop widths. Runs against the built `dist/web` over loopback.
// Pass --repository-public when checking a public-repository build with installation disabled.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { cli, harness, root, settled, settledCharts, watchErrors } from './browser-harness.mjs';
import { siteModeExpectations } from './site-mode.mjs';

// ---- The site's look, as rules over the linked stylesheets (css-policy) ----
// Reports, the docs' rendered pages and the showcase frames keep their own style; only the files the pages link are the site's.
const stripped = css => css.replace(/\/\*[\s\S]*?\*\//g, '');
const rules = css => [...stripped(css).matchAll(/([^{}]+)\{([^{}]*)\}/g)].map(match => ({
  selectors: match[1].split(',').map(selector => selector.trim().replace(/^@[^{]*\s(?=\S)/, '')).filter(Boolean),
  declarations: [...match[2].matchAll(/([\w-]+)\s*:\s*([^;]+)/g)].map(declaration => [declaration[1].trim(), declaration[2].trim()]),
}));
// Radii a site surface may have (STY-66); drawn charts and style swatches are not surfaces.
const radiusExceptions = [
  ['.split__view i', 'bars of the drawn mini chart'],
  ['.demo-status[data-state="pending"] i', 'the tiny chart that is the loading indicator'],
  ['--tile-radius', "a style swatch shows that style's own corners (STY-36)"],
];
// The voices scene's pages float like the live example's report sheet (S27-926). Pinned, a voices sheet
// draws that one shadow on its own ::before, painted once instead of as a filter on every frame (S27-930).
const shadowSelectors = ['.sheet--source', '.sheet--report', '.voices__sheet', '.voices__sheet::before', '.citation-card'];
const linear = channel => (channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4);
const oklchToSrgb = (l, c, hue) => {
  const a = c * Math.cos((hue * Math.PI) / 180), b = c * Math.sin((hue * Math.PI) / 180);
  const [lp, mp, sp] = [l + 0.3963377774 * a + 0.2158037573 * b, l - 0.1055613458 * a - 0.0638541728 * b, l - 0.0894841775 * a - 1.291485548 * b].map(v => v ** 3);
  return [4.0767416621 * lp - 3.3077115913 * mp + 0.2309699292 * sp, -1.2684380046 * lp + 2.6097574011 * mp - 0.3413193965 * sp, -0.0041960863 * lp - 0.7034186147 * mp + 1.707614701 * sp];
};
const luminance = rgb => 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
const hexToLinear = hex => [1, 3, 5].map(index => linear(parseInt(hex.slice(index, index + 2), 16) / 255));
const srgbToHue = rgb => {
  const [r, g, b] = rgb;
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b), m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b), sv = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  return ((Math.atan2(0.0259040371 * l + 0.7827717662 * m - 0.808675766 * sv, 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * sv) * 180) / Math.PI + 360) % 360;
};

// ---- The hero's media, read from the files themselves (S27-959) ----
// ISO BMFF boxes (MP4, AVIF) inside a box, each as its four-letter type and the span of its contents. A full box
// starts with a version and flags, a sample description also counts its entries, and a visual sample entry has 78
// bytes of fields before its own boxes.
const boxSkip = { meta: 4, stsd: 8, avc1: 78, avc3: 78, hvc1: 78, hev1: 78, av01: 78 };
const boxesIn = (file, box) => {
  const found = [];
  for (let at = box.start + (boxSkip[box.type] ?? 0); at + 8 <= box.end;) {
    const short = file.readUInt32BE(at);
    const header = short === 1 ? 16 : 8;
    const size = short === 1 ? Number(file.readBigUInt64BE(at + 8)) : short === 0 ? box.end - at : short;
    assert.ok(size >= header && at + size <= box.end, `a malformed box at byte ${at}`);
    found.push({ type: file.toString('latin1', at + 4, at + 8), start: at + header, end: at + size });
    at += size;
  }
  return found;
};
const boxesAt = (file, path, from = { type: '', start: 0, end: file.length }) => path.reduce((boxes, type) => boxes.flatMap(box => boxesIn(file, box).filter(child => child.type === type)), [from]);
// A `colr` box's nclx colour description: CICP primaries, transfer and matrix, and whether the range is full.
const colourOf = (file, colr) => colr && file.toString('latin1', colr.start, colr.start + 4) === 'nclx'
  ? { primaries: file.readUInt16BE(colr.start + 4), transfer: file.readUInt16BE(colr.start + 6), matrix: file.readUInt16BE(colr.start + 8), full: file[colr.start + 10] >> 7 === 1 }
  : null;
// What a browser picks a clip by and shows it with: the RFC 6381 codec string its sample entry's configuration
// record makes (ISO/IEC 14496-15 for avcC and hvcC, the AV1 ISOBMFF binding for av1C), its frame size, its
// length from the video track's media header, and its colour description.
const clipFacts = file => {
  const track = boxesAt(file, ['moov', 'trak']).find(trak => boxesAt(file, ['mdia', 'hdlr'], trak).some(hdlr => file.toString('latin1', hdlr.start + 8, hdlr.start + 12) === 'vide'));
  assert.ok(track, 'the clip has a video track');
  const [mdhd] = boxesAt(file, ['mdia', 'mdhd'], track);
  const long = file[mdhd.start] === 1;
  const seconds = (long ? Number(file.readBigUInt64BE(mdhd.start + 24)) : file.readUInt32BE(mdhd.start + 16)) / file.readUInt32BE(mdhd.start + (long ? 20 : 12));
  const [entry] = boxesAt(file, ['mdia', 'minf', 'stbl', 'stsd'], track).flatMap(stsd => boxesIn(file, stsd));
  const record = type => boxesIn(file, entry).find(box => box.type === type)?.start;
  const hex = byte => byte.toString(16).toUpperCase();
  let codec = entry.type;
  if (record('avcC') !== undefined) {
    const at = record('avcC');
    codec += `.${[1, 2, 3].map(offset => hex(file[at + offset]).padStart(2, '0')).join('')}`;
  } else if (record('hvcC') !== undefined) {
    const at = record('hvcC');
    let compatibility = 0;
    for (let bit = 0; bit < 32; bit++) compatibility = compatibility * 2 + ((file.readUInt32BE(at + 2) >>> bit) & 1);
    const constraints = [...file.subarray(at + 6, at + 12)];
    while (constraints.at(-1) === 0) constraints.pop();
    codec += `.${['', 'A', 'B', 'C'][file[at + 1] >> 6]}${file[at + 1] & 31}.${hex(compatibility)}.${file[at + 1] & 32 ? 'H' : 'L'}${file[at + 12]}${constraints.map(byte => `.${hex(byte)}`).join('')}`;
  } else if (record('av1C') !== undefined) {
    const at = record('av1C');
    const profile = file[at + 1] >> 5, high = (file[at + 2] >> 6) & 1, twelve = (file[at + 2] >> 5) & 1;
    codec += `.${profile}.${String(file[at + 1] & 31).padStart(2, '0')}${file[at + 2] >> 7 ? 'H' : 'M'}.${high ? (profile === 2 && twelve ? '12' : '10') : '08'}`;
  }
  return { codec, width: file.readUInt16BE(entry.start + 24), height: file.readUInt16BE(entry.start + 26), seconds, colour: colourOf(file, boxesIn(file, entry).find(box => box.type === 'colr')) };
};
// An AVIF still's pixel size and colour description, from its item properties.
const stillFacts = file => {
  const properties = boxesAt(file, ['meta', 'iprp', 'ipco']).flatMap(ipco => boxesIn(file, ipco));
  const ispe = properties.find(box => box.type === 'ispe');
  return { width: file.readUInt32BE(ispe.start + 4), height: file.readUInt32BE(ispe.start + 8), colour: colourOf(file, properties.find(box => box.type === 'colr')) };
};

export function sitePolicy({ sheets, classes }) {
  const problems = [];
  const all = [...sheets.entries()].map(([href, css]) => ({ href, text: stripped(css), rules: rules(css) }));
  // A voice of the home page's styles scene restyles it under `:root[data-voice="…"]` (S27-926); the
  // site's own tokens are the rest, and each voice's are checked below on their own.
  const voiced = rule => rule.selectors.every(selector => selector.includes('[data-voice'));
  const tokens = new Map();
  const voices = new Map();
  for (const sheet of all) for (const rule of sheet.rules) for (const [name, value] of rule.declarations) {
    if (!name.startsWith('--')) continue;
    if (!voiced(rule)) tokens.set(name, value);
    for (const voice of rule.selectors.map(selector => selector.match(/^:root\[data-voice="([\w-]+)"\]$/)?.[1]).filter(Boolean)) {
      if (!voices.has(voice)) voices.set(voice, new Map());
      voices.get(voice).set(name, value);
    }
  }
  const resolve = value => value.replace(/var\((--[\w-]+)\)/g, (whole, name) => tokens.get(name) ?? whole);
  for (const sheet of all) {
    const fail = (card, text) => problems.push(`${card} ${sheet.href}: ${text}`);
    // STY-47: light only.
    if (/prefers-color-scheme/.test(sheet.text) || /color-scheme:\s*[^;]*dark/.test(sheet.text)) fail('STY-47', 'the site follows no dark setting');
    // STY-48: opaque surfaces, no glass, lift or overshoot.
    if (/backdrop-filter|saturate\(/.test(sheet.text)) fail('STY-48', 'backdrop blur or saturation');
    if (/--(?:glass|lift|pop)\b/.test(sheet.text)) fail('STY-48', 'a glass, lift or pop token');
    for (const [, , , y2] of [...sheet.text.matchAll(/cubic-bezier\(\s*[-\d.]+\s*,\s*([-\d.]+)\s*,\s*[-\d.]+\s*,\s*([-\d.]+)\s*\)/g)].map(match => [0, 0, +match[1], +match[2]])) if (y2 < 0 || y2 > 1) fail('STY-48', `overshoot easing, y2 ${y2}`);
    for (const [, y1] of [...sheet.text.matchAll(/cubic-bezier\(\s*[-\d.]+\s*,\s*([-\d.]+)/g)].map(match => [0, +match[1]])) if (y1 < 0 || y1 > 1) fail('STY-48', `overshoot easing, y1 ${y1}`);
    for (const rule of sheet.rules) {
      for (const [name, value] of rule.declarations) {
        // STY-48: a soft shadow, as a box shadow or a drop-shadow filter, belongs to the floating
        // surfaces only; rings and bars are hard (blur 0).
        const layers = name === 'box-shadow' && value !== 'none' ? resolve(value).split(/,(?![^(]*\))/)
          : name === 'filter' ? [...resolve(value).matchAll(/drop-shadow\(((?:[^()]|\([^()]*\))*)\)/g)].map(match => match[1]) : [];
        const soft = layers.some(layer => { const lengths = layer.replace(/\([^)]*\)/g, '').trim().split(/\s+/).filter(token => /^-?[\d.]+(px|rem)?$/.test(token)); return lengths.length >= 3 && parseFloat(lengths[2]) > 0; });
        if (soft && !rule.selectors.every(selector => shadowSelectors.some(allowed => selector.endsWith(allowed)))) fail('STY-48', `${rule.selectors.join(', ')} casts a soft shadow`);
        // STY-66: three radii.
        if (/^border(-\w+){0,2}-radius$/.test(name)) {
          const exception = radiusExceptions.find(([key]) => rule.selectors.some(selector => selector.includes(key)) || value.includes(key));
          const parts = resolve(value).split(/\s+/);
          const bad = parts.filter(part => !['0', '50%', '999px', '12px', '4px', 'inherit'].includes(part));
          if (bad.length && !exception) fail('STY-66', `${rule.selectors.join(', ')} has radius ${value}`);
        }
      }
    }
    // STY-28: every face is a report font build under its report name, served from /fonts/.
    for (const face of sheet.rules.filter(rule => rule.selectors.includes('@font-face'))) {
      const family = face.declarations.find(([name]) => name === 'font-family')?.[1].replace(/^["']|["']$/g, '');
      const source = face.declarations.find(([name]) => name === 'src')?.[1] ?? '';
      if (!/^\/fonts\//.test(source.match(/url\(["']?([^)"']+)/)?.[1] ?? '')) fail('STY-28', `${family} is not loaded from /fonts/`);
    }
    if (/Docs Geist|Docs Mono|IBM Plex Mono|Source Serif 4/.test(sheet.text)) fail('STY-28', 'a reserved or private family name');
    // STY-46: the site's faces; the report's own faces appear only in the style swatches.
    for (const match of sheet.text.matchAll(/font-family\s*:\s*([^;}]+)/g)) {
      for (const family of match[1].split(',').map(name => name.trim().replace(/^["']|["']$/g, '')).filter(name => !/^(var\(|inherit$|serif$|sans-serif$|monospace$|Georgia$|Arial Narrow$|system-ui$|ui-)/.test(name)))
        if (!['Newsreader', 'Geist', 'RichView Mono', 'RichView Serif', 'Space Grotesk', 'Big Shoulders', 'Patrick Hand', 'Caveat', 'Caveat Brush', 'Bricolage Grotesque'].includes(family)) fail('STY-46', `font family ${family} is not one of the site's faces`);
    }
  }
  // STY-48: the cards of "What is RichView?" and the proof pair are rows of flat surfaces; the loop above already
  // refuses a soft shadow on any selector that is not a floating sheet.
  const homepage = all.find(sheet => sheet.href.endsWith('/homepage.css'));
  // STY-48: pinned, a voices sheet's one shadow is its ::before; its own drop-shadow filter must be off, or the
  // sheet casts two (S27-930).
  const pinned = '.voices[data-mode="pinned"] .voices__sheet';
  if (!homepage?.rules.some(rule => rule.selectors.includes(pinned) && rule.declarations.some(([name, value]) => name === 'filter' && value === 'none')))
    problems.push(`STY-48 homepage.css: ${pinned} must set filter: none, so its ::before casts its only shadow`);
  // STY-49: no eyebrow, overline, kicker or section number above a heading.
  for (const name of classes) if (/(^|[-_])(eyebrow|overline|kicker|badge|section-number)($|[-_])/i.test(name)) problems.push(`STY-49 pages: class ${name}`);
  // STY-59: one code palette, five roles at 4.5:1 on the code panel, keys in the brand blue.
  const hex = value => (/^#[0-9a-f]{6}$/i.test(resolve(value ?? '')) ? hexToLinear(resolve(value)) : undefined);
  const panel = hex(tokens.get('--code-bg')) ?? hex(tokens.get('--paper-2'));
  const roles = ['--hl-key', '--hl-str', '--hl-num', '--hl-punc', '--hl-comment'];
  if (!panel) problems.push('STY-59 site.css: the code panel colour --code-bg is not a hex colour');
  else for (const role of roles) {
    const match = resolve(tokens.get(role) ?? '').match(/^oklch\(\s*([\d.]+)\s+([\d.]+)\s+([\d.]+)\s*\)$/);
    if (!match) { problems.push(`STY-59 site.css: ${role} is not written in oklch()`); continue; }
    const ratio = (luminance(panel) + 0.05) / (luminance(oklchToSrgb(+match[1], +match[2], +match[3]).map(v => Math.min(1, Math.max(0, v)))) + 0.05);
    if (ratio < 4.5) problems.push(`STY-59 site.css: ${role} is ${ratio.toFixed(2)}:1 on the code panel, under 4.5:1`);
  }
  // Under a voice the page wears that style's paper, and the code below the scene sits on its panel:
  // the five roles keep 4.5:1 there too (S27-926).
  for (const [voice, own] of voices) {
    const value = own.get('--code-bg') ?? own.get('--paper-2');
    if (!value) continue;
    const match = value.match(/^oklch\(\s*([\d.]+)\s+([\d.]+)\s+([\d.]+)\s*\)$/);
    const voicePanel = match ? oklchToSrgb(+match[1], +match[2], +match[3]).map(v => Math.min(1, Math.max(0, v))) : hex(value);
    if (!voicePanel) { problems.push(`STY-59 voices: the ${voice} voice's code panel ${value} is neither oklch() nor hex`); continue; }
    for (const role of roles) {
      const colour = resolve(tokens.get(role) ?? '').match(/^oklch\(\s*([\d.]+)\s+([\d.]+)\s+([\d.]+)\s*\)$/);
      if (!colour) continue;
      const ratio = (luminance(voicePanel) + 0.05) / (luminance(oklchToSrgb(+colour[1], +colour[2], +colour[3]).map(v => Math.min(1, Math.max(0, v)))) + 0.05);
      if (ratio < 4.5) problems.push(`STY-59 voices: ${role} is ${ratio.toFixed(2)}:1 on the ${voice} voice's code panel, under 4.5:1`);
    }
  }
  const blue = hex(tokens.get('--blue')), key = resolve(tokens.get('--hl-key') ?? '').match(/oklch\(\s*[\d.]+\s+[\d.]+\s+([\d.]+)/);
  if (blue && key && Math.abs(srgbToHue(blue) - +key[1]) > 20) problems.push(`STY-59 site.css: --hl-key hue ${key[1]} is not the brand blue's ${srgbToHue(blue).toFixed(0)}`);
  return problems;
}

const map = JSON.parse(readFileSync(join(root, 'assets/routes.json'), 'utf8'));
const configuredInstall = JSON.parse(readFileSync(join(root, 'assets/install/release.json'), 'utf8')).enabled;
assert.equal(typeof configuredInstall, 'boolean', 'install release must explicitly be enabled or disabled');
const localPreview = process.env.RICHVIEW_DEV_PREVIEW;
if (localPreview !== undefined) {
  assert.equal(localPreview, '1', 'the development preview signal must be explicit');
  const target = process.env.CARGO_TARGET_DIR || join(root, 'target');
  const manifest = JSON.parse(readFileSync(join(target, 'release/richview-build-manifest.json'), 'utf8'));
  assert.equal(manifest.schema, 1, 'the local preview has a schema-1 build manifest');
  assert.equal(manifest.mode, 'local-preview', 'the development preview cannot use a full build manifest');
}
const installEnabled = configuredInstall && !localPreview;
// Install-on builds require a public repository; install-off public builds name that mode here too.
const repositoryPublic = installEnabled || process.argv.includes('--repository-public');
const siteMode = siteModeExpectations(installEnabled, repositoryPublic);
const stagedHome = readFileSync(join(root, 'dist/web/index.html'), 'utf8');
assert.ok(stagedHome.includes(`<label class="updates__label" for="interest-email">${installEnabled ? 'Get release updates.' : 'Coming soon.'}</label>`), 'signup copy follows the install switch');
assert.deepEqual(readFileSync(join(root, 'dist/web/share/home.png')), readFileSync(join(root, installEnabled ? 'assets/site-home-launched.png' : 'assets/share/home.png')), 'home share image follows the install switch');
// Observe the same site's startup, fonts and report paint before the layout/policy checks.
// The static routes have no site module; a declared module must actually start.
async function readyPage(page, route) {
  const timeout = 15_000;
  if (route === '/' && await page.locator('script[src="/landing/bootstrap.js"]').count()) {
    await page.keyboard.press('Shift');
    await page.waitForFunction(() => document.documentElement.dataset.homepageEngine === 'rust', null, { timeout });
    await page.waitForFunction(() => document.querySelector('#demo-themes button') && document.querySelector('.code__hl code span'), null, { timeout });
  } else if (await page.locator('script[src="/bootstrap.js"]').count()) {
    await page.waitForFunction(() => document.documentElement.dataset.richviewStarted === 'rust', null, { timeout });
  }
  if (route.startsWith('/docs/charts/')) {
    await page.locator('figure[data-rv-chart] canvas').first().waitFor({ state: 'attached', timeout });
    await page.waitForFunction(() => document.documentElement.dataset.rvRuntime === 'started' && !!window.__richview, null, { timeout });
    // Observe natural completion; settled() would forcibly finish below-fold entrances.
    await page.waitForFunction(() => {
      const models = window.__richview.charts();
      const figures = [...document.querySelectorAll('figure[data-rv-chart]')];
      return models.length === figures.length && figures.every(figure => {
        const model = models.find(value => value.figure === figure.id);
        const box = figure.querySelector('.rv-chart__frame').getBoundingClientRect();
        const visible = box.top < innerHeight && box.bottom > 0;
        return model && model.width > 0 && (!visible || model.settled);
      });
    }, null, { timeout });
  }
  if (route === '/' || route === '/playground' || route === '/showcase/openai-vs-anthropic/') {
    // Offscreen previews may defer drawing; their runtime must still start under the policy.
    const element = await page.locator('iframe:not([hidden])').first().elementHandle({ timeout: 15_000 });
    const frame = await element.contentFrame();
    assert.ok(frame, route + ': the preview frame is attached');
    await element.dispose();
    await frame.waitForFunction(() => document.documentElement.dataset.rvRuntime === 'started' && !!window.__richview && document.fonts.status === 'loaded', null, { timeout: 30_000, polling: 50 });
  }
  await settled(page); // Start any fonts selected by the first real layout.
  await page.waitForFunction(() => document.fonts.status === 'loaded', null, { timeout });
  await settled(page);
}

const h = await harness('site');
const evidence = { routes: [], gone: [], redirects: [], pages: [], links: 0, chart_pages: [], example_styles: [], violations: [], errors: [] };
const fetchStatus = async path => {
  const response = await fetch(h.base + path, { redirect: 'manual' });
  return { status: response.status, type: response.headers.get('content-type') || '', body: response.status === 200 ? await response.text() : '' };
};
try {
  // The switch controls the install guide; release assets are served by GitHub.
  assert.deepEqual(map.install, ['/install']);
  const page = await fetchStatus('/install');
  assert.equal(page.status, installEnabled ? 200 : 404);
  if (installEnabled) {
    assert.ok(page.body.includes('curl -fsSL https://github.com/1vecera/richview/releases/latest/download/install.sh | sh'), 'install page shows the GitHub release command');
    assert.ok(!page.body.includes('rv-agent-prompt'), 'install page renders the agent prompt');
  }
  for (const path of ['/install.sh', '/install-release.json', '/downloads/']) {
    assert.equal((await fetchStatus(path)).status, 404, `retired hosted route ${path}`);
  }
  assert.equal(existsSync(join(root, 'dist/web/downloads')), false, 'no hosted download folder');
  const agents = await fetchStatus('/llms.txt');
  assert.equal(agents.body.includes('https://github.com/1vecera/richview/releases/latest'), installEnabled, 'agent install instructions follow the switch');
  const guide = await fetchStatus('/docs/install');
  assert.equal(guide.body.includes('https://github.com/1vecera/richview/releases/latest/download/install.sh'), installEnabled, 'documentation install command follows the switch');
  // Every served route answers; every removed one is 404. `/api/` is the Worker's, not the static build's.
  for (const route of map.serve) {
    const { status, type, body } = await fetchStatus(route);
    assert.equal(status, 200, `${route} must be served`);
    if (route === '/llms.txt' || route === '/skill/SKILL.md') {
      assert.match(type, /^text\/(?:plain|markdown)/, `${route} must be a text document`);
      assert.doesNotMatch(body, /<!doctype html>/i, `${route} must not be an HTML fallback`);
    }
    evidence.routes.push(route);
  }
  for (const route of map.gone.filter(route => !route.startsWith('/api/'))) {
    assert.equal((await fetchStatus(route)).status, 404, `${route} must answer 404`);
    evidence.gone.push(route);
  }
  // The static loopback cannot redirect: the redirects are the committed `_redirects` rules.
  const rules = new Map(readFileSync(join(root, 'assets/_redirects'), 'utf8').split('\n').filter(line => line.startsWith('/')).map(line => line.split(/\s+/)));
  for (const [from, to] of Object.entries(map.redirect)) {
    assert.equal(rules.get(from) ?? rules.get(from.replace(/\/$/, '')), to, `${from} must redirect to ${to}`);
    assert.equal((await fetchStatus(to)).status, 200, `${to} (target of ${from}) must be served`);
    evidence.redirects.push(`${from} -> ${to}`);
  }
  // No file outside the map: the top level of the build holds only what the map keeps.
  const unexpected = readdirSync(join(root, 'dist/web')).filter(name => !map.keep.includes(name));
  assert.deepEqual(unexpected, [], 'dist/web holds files the route map does not keep');

  // Every chart type has a docs page that is served.
  const registry = readFileSync(join(root, 'crates/richview-core/src/registry.rs'), 'utf8');
  const block = registry.slice(registry.indexOf('pub const CHART_TYPES'));
  const landed = [...block.slice(0, block.indexOf('\n];')).matchAll(/^    ChartType \{\n        name: "([^"]+)"/gm)].map(match => match[1]);
  // Only the types the release run accepted are public (FEA-CH-75); no list means every landed type.
  const acceptedFile = join(root, 'docs/public/charts/accepted.txt');
  const accepted = existsSync(acceptedFile) ? readFileSync(acceptedFile, 'utf8').split('\n').map(line => line.trim()).filter(line => line && !line.startsWith('#')) : null;
  const types = accepted ? landed.filter(type => accepted.includes(type)) : landed;
  assert.ok(types.length >= 9, 'chart types were not read');
  const missing = [];
  for (const type of types) {
    if ((await fetchStatus(`/docs/charts/${type}`)).status !== 200) missing.push(type);
    else evidence.chart_pages.push(type);
  }
  assert.deepEqual(missing, [], `chart types without a docs page: ${missing.join(', ')}`);
  // The landing ships static until Daniel accepts its scroll story (STY-44).
  const home = readFileSync(join(root, 'dist/web/index.html'), 'utf8');
  assert.ok(home.includes('data-landing="static"') && !home.includes('id="rv-stage"') && !home.includes('id="rv-field"'), 'the built landing is the static one (set RICHVIEW_LANDING=story to build the story)');

  // The first screen (W4, S27-959): the painting agent's still, and its clip in AV1, then HEVC, then H.264 for
  // each framing. Every clip asks for motion, so a reader with reduced motion loads none and gets the clip's last
  // frame as the still instead; the clip plays once, muted and inline, without controls; and each framing's still
  // and largest clip stay inside the hero's share of the first visit's 1 MB (PRF-36), the rest of the first screen
  // taking about 0.25 MB.
  {
    const at = home.indexOf('<figure class="hero__scene">');
    assert.ok(at > 0, 'the home page has its hero scene');
    const scene = home.slice(at, home.indexOf('</figure>', at));
    const video = scene.match(/<video[^>]*>/)?.[0] ?? '';
    for (const attribute of ['muted', 'playsinline', 'autoplay']) assert.match(video, new RegExp(`\\s${attribute}[\\s>]`), `the hero clip must be ${attribute}`);
    assert.doesNotMatch(video, /\s(loop|controls)[\s>=]/, 'the hero clip plays once, without controls');
    const sources = [...scene.matchAll(/<source media="([^"]+)" type='video\/mp4; codecs="([^"]+)"' src="([^"]+)"/g)].map(([, media, codec, src]) => ({ media, codec, src }));
    assert.equal(sources.length, 6, 'the hero has three clips for each of its two framings');
    for (const source of sources) assert.match(source.media, /prefers-reduced-motion: no-preference/, `${source.src} must load only for readers who allow motion`);
    const stills = [...scene.matchAll(/<source (?:media="([^"]+)" )?type="image\/avif" srcset="([^"]+)"/g)].map(([, media = '', src]) => ({ media, src }));
    const still = scene.match(/<img[^>]*class="hero__still"[^>]*>/s)?.[0] ?? '';
    assert.match(still, /\balt="[^"]{20,}"/, 'the hero still says what it shows');
    assert.match(still, /fetchpriority="high"/, 'the hero still is fetched first, as the largest image on the first screen');
    const bytes = path => statSync(join(root, 'dist/web', path)).size;
    const media = path => readFileSync(join(root, 'dist/web', path));
    const sizeOf = tag => ({ width: Number(tag.match(/\bwidth="(\d+)"/)?.[1]), height: Number(tag.match(/\bheight="(\d+)"/)?.[1]) });
    // The stills' colour description (BT.709 primaries and matrix, the sRGB curve, limited range), which every clip
    // must carry too: untagged, or tagged with BT.709's own curve, a clip shows about 9 levels lighter in the
    // midtones than its still in WebKit, so it would visibly brighten as it took the still's place.
    const tagged = { primaries: 1, transfer: 13, matrix: 1, full: false };
    for (const [framing, poster, rest] of [['(min-width: 700px)', '/landing/hero/pip.avif', '/landing/hero/pip-rest.avif'], ['(max-width: 699px)', '/landing/hero/pip-portrait.avif', '/landing/hero/pip-portrait-rest.avif']]) {
      const clips = sources.filter(source => source.media.startsWith(framing));
      assert.deepEqual(clips.map(source => source.codec.slice(0, 4)), ['av01', 'hvc1', 'avc1'], `${framing}: AV1, then HEVC, then H.264`);
      assert.ok(stills.some(source => source.src === poster) && stills.some(source => source.src === rest && /prefers-reduced-motion: reduce/.test(source.media)), `${framing}: the still is the clip's first frame, and with reduced motion its last`);
      // Each file as a browser meets it: the stills and clips have the frame size the markup reserves; each clip's
      // type names its exact codec, profile and level, so a browser is never offered a clip its decoder cannot
      // take; and each clip runs under 5 s, because it moves without a pause control (WCAG 2.2.2).
      const posterTag = scene.match(new RegExp(`<source[^>]*srcset="${poster}"[^>]*>`))?.[0] ?? '';
      const frame = sizeOf(/\bwidth=/.test(posterTag) ? posterTag : still);
      for (const path of [poster, rest]) assert.deepEqual(stillFacts(media(path)), { ...frame, colour: tagged }, `${path} must be ${frame.width}x${frame.height}, in BT.709 with the sRGB curve and limited range`);
      const read = {};
      for (const clip of clips) {
        const facts = clipFacts(media(clip.src));
        assert.equal(facts.codec.toLowerCase(), clip.codec.toLowerCase(), `${clip.src} is ${facts.codec}, but its type says ${clip.codec}`);
        assert.ok(facts.seconds < 5, `${clip.src} runs ${facts.seconds} s; motion of 5 s or more needs a pause control (WCAG 2.2.2)`);
        assert.deepEqual({ width: facts.width, height: facts.height, colour: facts.colour }, { ...frame, colour: tagged }, `${clip.src} must have its still's frame size and colour description`);
        read[clip.src] = { bytes: bytes(clip.src), codec: facts.codec, seconds: facts.seconds };
      }
      const largest = Math.max(...clips.map(source => bytes(source.src)));
      assert.ok(bytes(poster) + largest <= 750 * 1024, `${framing}: the still and its largest clip are ${Math.round((bytes(poster) + largest) / 1024)} KB, over the hero's 750 KB`);
      evidence.hero = { ...evidence.hero, [framing]: { still: bytes(poster), rest: bytes(rest), clips: read } };
    }
  }

  // STY-28: one licence page, linked from the footer, lists every family the pages load; the landing's own font folder is gone.
  const licences = (await fetchStatus('/licenses')).body;
  for (const family of ['Newsreader', 'Geist', 'RichView Mono', 'RichView Serif', 'Space Grotesk', 'Big Shoulders', 'Patrick Hand', 'Caveat', 'Caveat Brush', 'Bricolage Grotesque']) assert.ok(licences.includes(`<h2>${family}</h2>`), `the licence page lists no ${family}`);
  assert.ok(home.includes('href="/licenses"'), 'the footer links the licence page');
  // The About page names the founder and the open-source repository, and the footer links it.
  const about = (await fetchStatus('/about')).body;
  assert.ok(about.includes('founder of RichView') && about.includes('href="https://github.com/1vecera/richview"'), 'the About page names the founder and links the repository');
  assert.ok(home.includes('href="/about"'), 'the footer links the About page');
  assert.equal((await fetchStatus('/landing/fonts/geist-400.woff2')).status, 404, 'the landing keeps no font folder of its own');
  assert.ok(!existsSync(join(root, 'assets/landing/fonts')), 'assets/landing/fonts is deleted');

  // Crawl: every same-origin link and asset on every page reachable from the served routes resolves.
  const seen = new Set();
  const queue = [...map.serve.filter(route => !route.endsWith('.rmd') && route !== '/robots.txt'), ...(installEnabled ? map.install : []), ...types.map(type => `/docs/charts/${type}`)];
  const redirects = new Map(Object.entries(map.redirect));
  const pages = [];
  const sheets = new Map();
  const classes = new Set();
  while (queue.length) {
    const route = queue.shift();
    if (seen.has(route)) continue;
    seen.add(route);
    const { status, type, body } = await fetchStatus(route);
    assert.equal(status, 200, `dead link: ${route}`);
    if (!type.includes('html')) continue;
    pages.push(route);
    for (const tag of body.match(/<link[^>]*rel="stylesheet"[^>]*>/g) ?? []) {
      const href = tag.match(/href="([^"]+)"/)?.[1];
      if (href?.startsWith('/') && !href.includes('richview-fonts')) sheets.set(href, (await fetchStatus(href)).body);
    }
    for (const attribute of body.matchAll(/\bclass="([^"]*)"/g)) for (const name of attribute[1].split(/\s+/)) classes.add(name);
    assert.equal((body.match(/<link[^>]*rel="icon"/g) ?? []).length, 1, `${route} must name exactly one icon`);
    for (const match of body.matchAll(/(?:href|src)="([^"#]+)(?:#[^"]*)?"/g)) {
      const target = match[1];
      if (/^(?:[a-z]+:|\/\/|data:)/i.test(target) && !target.startsWith('/')) continue;
      if (target.startsWith('/api/') || target.startsWith('mailto:')) continue;
      const url = new URL(target, `http://x${route.endsWith('/') ? route : `${route}/`.replace(/\/[^/]*\/$/, '/')}`);
      let path = url.pathname;
      if (route.endsWith('/') ? false : !target.startsWith('/')) path = new URL(target, `http://x${route}`).pathname;
      path = redirects.get(path) ?? path;
      evidence.links++;
      if (!seen.has(path)) queue.push(path);
    }
  }
  evidence.pages = pages;
  evidence.stylesheets = [...sheets.keys()];
  // The site's look rules (STY-46 to STY-49, STY-59, STY-66) hold in every stylesheet the pages link.
  const looks = sitePolicy({ sheets, classes });
  assert.deepEqual(looks, [], `the site stylesheets break the look rules:\n${looks.join('\n')}`);

  // The first-report example renders in every site style with no error and no undeclared warning (FEA-SF-42).
  const folder = join(h.dir, 'first-report');
  mkdirSync(folder, { recursive: true });
  cpSync(join(root, 'examples/first-report'), folder, { recursive: true });
  for (const style of ['sleek', 'air', 'scientific', 'brutalist', 'hand-drawn']) {
    const run = spawnSync(cli, ['check', join(folder, 'report.rmd'), '--style', style, '--format', 'json'], { encoding: 'utf8', timeout: 60_000 });
    assert.equal(run.status, 0, `first-report in ${style}: ${run.stderr || run.stdout}`);
    const diagnostics = JSON.parse(run.stdout).diagnostics ?? [];
    const bad = diagnostics.filter(d => d.level === 'error' || d.level === 'warning');
    assert.deepEqual(bad.map(d => `${d.code}: ${d.message}`), [], `first-report in ${style} has findings`);
    evidence.example_styles.push(style);
  }

  // Shell commands must fit without horizontal scrolling at desktop and phone widths.
  if (installEnabled) {
    for (const width of [1280, 390]) {
      const page = await h.browser.newPage({ viewport: { width, height: 900 } });
      for (const [route, selector] of [
        ['/install', 'pre.shell'],
        ['/docs/install', 'pre:has(> code.language-sh), pre:has(> code.language-bash)'],
        ['/docs/publishing', 'pre:has(> code.language-sh), pre:has(> code.language-bash)'],
      ]) {
        await page.goto(h.base + route);
        await page.evaluate(() => document.fonts.ready);
        const blocks = await page.locator(selector).evaluateAll(nodes => nodes.map(node => ({ width: node.clientWidth, content: node.scrollWidth })));
        assert.ok(blocks.length, `No shell commands on ${route}`);
        assert.deepEqual(blocks.filter(block => block.content > block.width + 1), [], `Shell command overflows on ${route} at ${width}px`);
      }
      await page.close();
    }
  }

  // Each page loads in a browser under the deployed policy: no violation, no console error, no sideways scroll.
  let chromeLook;
  const widths = [['desktop', 1440, 900], ['phone', 390, 844], ['narrow', 320, 640]];
  const browserPages = ['/', '/docs/', '/docs/charts', '/docs/reference', '/playground', ...(installEnabled ? ['/install'] : []), '/about', '/impressum', '/privacy', '/licenses', '/showcase/openai-vs-anthropic/', ...map.serve.filter(route => route.startsWith('/docs/examples/')), ...types.map(type => `/docs/charts/${type}`)];
  for (const [name, width, height] of widths) {
    const context = await h.browser.newContext({ viewport: { width, height }, isMobile: name !== 'desktop', hasTouch: name !== 'desktop' });
    for (const route of browserPages) {
      const page = await context.newPage();
      const errors = [];
      watchErrors(page, errors, `${name} ${route}: `);
      await page.goto(h.base + route, { waitUntil: 'load' });
      await readyPage(page, route);
      // TYP-75: exercise the actual shipped facade once, and keep retired roots out of Wasm.
      if (name === 'desktop' && route === '/') {
        await page.waitForFunction(() => document.documentElement.dataset.richviewStarted === 'rust');
        evidence.facade = await page.evaluate(() => {
          const api = wasm_bindgen;
          const retired = ['render_compat', 'parse', 'createPublishRequest', 'learn', 'diagnose', 'resolveRatio', 'nominalHeight', 'renderChart', 'resolveChartTheme', 'chart_constants', 'NativeSchema', 'resolveStyles', 'customPropertiesToCss', 'learnBundle', 'learnSection'];
          const retained = ['provide_font_catalog', 'render_source', 'worker_request', 'render', 'check', 'validate', 'core_constants', 'start'];
          const options = { report_folder: { root: 'site-facade', source: 'report.rmd', files: ['report.rmd'] } };
          const source = '# API contract\n\nHello *reader*.\n';
          const rendered = api.render(source, options);
          const direct = JSON.parse(api.render_source(source, JSON.stringify(options)));
          const checked = api.check(source, options);
          const validated = api.validate(source, options);
          const worker = JSON.parse(api.worker_request(source, JSON.stringify(options)));
          const body = value => typeof value?.html === 'string' && value.html.includes('Hello <em>reader</em>.');
          return {
            retired: retired.filter(key => key in api),
            missing: retained.filter(key => typeof api[key] !== 'function'),
            constants: api.core_constants(),
            calls: { render: body(rendered), render_source: body(direct), worker_request: body(worker.report), check: checked.ok === true && Array.isArray(checked.diagnostics), validate: validated.ok === true && Array.isArray(validated.diagnostics) },
          };
        });
        assert.deepEqual(evidence.facade.retired, [], 'retired compatibility exports returned to the site Wasm');
        assert.deepEqual(evidence.facade.missing, [], 'the shipped site lost a live facade function');
        assert.deepEqual(Object.keys(evidence.facade.constants).sort(), ['BUILTIN_STYLE_NAMES', 'RICH_TAGS', 'VERSION']);
        assert.equal(typeof evidence.facade.constants.VERSION, 'string');
        assert.ok(evidence.facade.constants.RICH_TAGS.includes('chart'));
        assert.ok(evidence.facade.constants.BUILTIN_STYLE_NAMES.includes('air'));
        assert.deepEqual(evidence.facade.calls, { render: true, render_source: true, worker_request: true, check: true, validate: true });
      }
      // A phone zooms a page that is too wide out, and `innerWidth` grows with it: the width that was set is the measure.
      const overflow = await page.evaluate(set => document.documentElement.scrollWidth - Math.min(window.innerWidth, set), width);
      assert.ok(overflow <= 1, `${name} ${route} scrolls sideways by ${overflow}px`);
      assert.ok(await page.locator('header, nav').count(), `${name} ${route} has no header`);
      // Gutters and page width (STY-49): the page's content keeps off both edges and never exceeds the viewport.
      const edge = await page.evaluate(() => { const r = document.querySelector('main, article, .docs-layout')?.getBoundingClientRect(); return r ? { left: r.left, right: innerWidth - r.right } : null; });
      assert.ok(!edge || (edge.left >= 0 && edge.right >= 0), `${name} ${route} content touches or passes a viewport edge (${JSON.stringify(edge)})`);
      // STY-46: the header and footer links look the same on every page, except the flagship, whose frame follows the active style (STY-60).
      // Let each link's initial color transition finish before comparing its computed color.
      await page.waitForFunction(() => ['.nav__brand', '.nav__links a', '.foot a'].every(selector => {
        const node = document.querySelector(selector);
        return node && !node.getAnimations().some(animation => animation.playState === 'running' && ['color', 'text-decoration-color'].includes(animation.transitionProperty));
      }));
      const chrome = await page.evaluate(() => ['.nav__brand', '.nav__links a', '.foot a'].map(selector => { const style = getComputedStyle(document.querySelector(selector)); return `${selector} ${style.color} ${style.textDecorationLine}`; }).join('; '));
      if (!route.startsWith('/showcase/')) chromeLook ??= chrome;
      if (!route.startsWith('/showcase/')) assert.equal(chrome, chromeLook, `${name} ${route} sets the header or footer links differently from the home page`);
      // STY-49: no uppercase or letter-spaced line sits directly above a heading, and no heading overlaps its neighbour.
      const above = await page.evaluate(() => [...document.querySelectorAll('main h1, main h2, article h1, article h2')].flatMap(heading => {
        const before = heading.previousElementSibling;
        if (!before || /^H[1-6]$/.test(before.tagName)) return [];
        const style = getComputedStyle(before);
        const box = before.getBoundingClientRect(), own = heading.getBoundingClientRect();
        const overlaps = box.height > 0 && own.height > 0 && box.bottom > own.top + 1 && box.top < own.bottom - 1 && box.right > own.left && box.left < own.right;
        return style.textTransform === 'uppercase' || overlaps ? [`${heading.textContent.trim().slice(0, 40)}`] : [];
      }));
      assert.deepEqual(above, [], `${name} ${route} has an overline or overlapping text above a heading`);
      if (route.startsWith('/docs/examples/')) {
        await settledCharts(page, 1);
        assert.equal(await page.locator('figure[data-rv-chart] canvas.rv-chart__canvas').count(), 1, `${name} ${route} mounts its time-series example under the site policy`);
      }
      if (route === '/showcase/openai-vs-anthropic/') {
        // The report frame fills its pane: the closed comparison pane takes no room (STY-60).
        const room = await page.evaluate(() => ({ frame: document.querySelector('#report-frame').getBoundingClientRect().height, pane: document.querySelector('#showcase-panes').getBoundingClientRect().height }));
        assert.ok(room.frame >= room.pane - 2, `${name} ${route}: the report frame is ${room.frame}px of a ${room.pane}px pane`);
        // The document is never wider than the window: the style row scrolls inside its own strip, and
        // nothing of it widens the page, the hidden name of its menu included. A phone zooms a wider page
        // out and `innerWidth` grows with it, so the width that was set is the measure, at a tablet's too.
        const documentWidths = [[width, await page.evaluate(() => document.documentElement.scrollWidth)]];
        if (name === 'phone') {
          await page.setViewportSize({ width: 768, height: 1024 });
          documentWidths.push([768, await page.evaluate(() => document.documentElement.scrollWidth)]);
          await page.setViewportSize({ width, height });
        }
        for (const [set, measured] of documentWidths) assert.ok(measured <= set, `${name} ${route}: the document is ${measured}px wide in a ${set}px window`);
        // Up to 640 px the top is one line (Daniel, 17 Nov: "top area is all single line with a dropdown to change
        // styles ... no bulky bottom area"): every style is in one named menu in the header's own row, between the
        // mark and the links and over neither, the report starts right under that line and ends at the screen's
        // bottom, and the page itself has nothing more to scroll to. A wider window keeps the row of styles, the
        // extended themes' own menu and the note under the report.
        const layout = await page.evaluate(() => {
          const box = selector => { const node = document.querySelector(selector), r = node.getBoundingClientRect(); return { shown: getComputedStyle(node).display !== 'none' && r.width > 0 && r.height > 0, left: r.left, top: r.top, right: r.right, bottom: r.bottom }; };
          const values = selector => [...document.querySelectorAll(selector)].map(node => node.value ?? node.dataset.theme);
          return {
            header: box('.nav'), mark: box('.nav__brand'), menu: box('#style-menu'), links: box('.nav__links'), rail: box('.style-rail'), note: box('.flag-foot'), footer: box('.foot'), frame: box('#report-frame'),
            page: document.documentElement.scrollHeight, screen: innerHeight,
            offered: values('#style-menu option'), styles: [...document.querySelectorAll('#style-pills [data-theme]')].map(node => node.dataset.theme).concat(values('#theme option:not([value=""])')),
          };
        });
        assert.deepEqual(layout.offered, layout.styles, `${name} ${route}: the phone's menu offers the styles of the row and of the extended themes' menu`);
        if (width <= 640) {
          assert.ok(layout.menu.shown && !layout.rail.shown, `${name} ${route}: one menu stands for the row of styles: ${JSON.stringify(layout)}`);
          assert.ok(layout.menu.top >= layout.header.top && layout.menu.bottom <= layout.header.bottom && layout.menu.left >= layout.mark.right && layout.menu.right <= layout.links.left, `${name} ${route}: the menu is in the header's line, between the mark and the links: ${JSON.stringify(layout)}`);
          assert.ok(Math.abs(layout.frame.top - layout.header.bottom) <= 1, `${name} ${route}: the report starts ${layout.frame.top - layout.header.bottom}px under the one line`);
          assert.ok(!layout.note.shown && !layout.footer.shown && Math.abs(layout.frame.bottom - layout.screen) <= 1 && layout.page <= layout.screen + 1, `${name} ${route}: something stands under the report: ${JSON.stringify(layout)}`);
          assert.equal(await page.getByRole('combobox', { name: 'Style', exact: true }).count(), 1, `${name} ${route}: the style menu has no name for a screen reader`);
          // A finger's target, and an open list that names each style in its own face and checks the current one (Chart design, 17 Nov).
          assert.ok(layout.menu.bottom - layout.menu.top >= 44, `${name} ${route}: the style menu is ${layout.menu.bottom - layout.menu.top}px tall`);
          const list = await page.evaluate(() => {
            const options = [...document.querySelectorAll('#style-menu optgroup:first-of-type option')];
            return { drawn: CSS.supports('appearance', 'base-select'), faces: new Set(options.map(o => getComputedStyle(o).fontFamily)).size, count: options.length, check: getComputedStyle(document.querySelector('#style-menu option:checked'), '::checkmark').content };
          });
          if (list.drawn) {
            assert.ok(list.faces >= list.count - 1, `${name} ${route}: the open list sets ${list.faces} faces for ${list.count} styles`);
            assert.match(list.check, /✓/, `${name} ${route}: the current style carries no check`);
          }
          // The menu changes the style and names the one shown.
          await page.waitForFunction(() => document.documentElement.dataset.rvTheme);
          const [first, second] = layout.offered;
          for (const style of [second, first]) {
            await page.locator('#style-menu').selectOption(style);
            await page.waitForFunction(style => document.documentElement.dataset.rvTheme === style && document.querySelector('#style-menu').value === style && document.querySelector('#report-frame')?.getAttribute('src')?.endsWith(`${style}.html`), style);
          }
        } else {
          assert.ok(!layout.menu.shown && layout.rail.shown && layout.note.shown && layout.footer.shown, `${name} ${route}: a wide window keeps the row of styles and the note under the report: ${JSON.stringify(layout)}`);
          assert.equal(await page.getByRole('combobox', { name: 'Extended themes', exact: true }).count(), 1, `${name} ${route}: the extended themes menu has lost its name for a screen reader`);
        }
      }
      if (route.startsWith('/docs/charts/')) {
        if (route === '/docs/charts/bar' && name === 'desktop') {
          // The style row fills the frame with the example in the chosen style, drawn by the report runtime.
          const looks = {};
          for (const style of ['Air', 'Brutalist']) {
            await page.getByRole('link', { name: style, exact: true }).click();
            const frameElement = await page.locator('.docs-styles__frame').elementHandle({ timeout: 20_000 });
            const frame = await frameElement.contentFrame();
            assert.ok(frame, 'the docs style preview frame is attached');
            await frameElement.dispose();
            await frame.locator('figure[data-rv-chart] canvas').first().waitFor({ state: 'attached', timeout: 20_000 });
            // The previous style's document stays until the new one replaces it.
            for (let attempt = 0; attempt < 60; attempt++) {
              looks[style] = await frame.locator('body').evaluate(body => getComputedStyle(body).fontFamily + '|' + getComputedStyle(body).backgroundColor).catch(() => '');
              if (looks[style] && !Object.values(looks).slice(0, -1).includes(looks[style])) break;
              await page.waitForTimeout(250);
            }
            assert.equal(await page.locator('[data-style-link][aria-current="true"]').innerText(), style);
          }
          assert.notEqual(looks.Air, looks.Brutalist, 'the frame follows the chosen style');
        }
        // The gallery interactions: the chart mounts and answers a pointer (desktop) or a tap (phone).
        const canvas = page.locator('figure[data-rv-chart] canvas').first();
        await canvas.waitFor({ state: 'attached', timeout: 15_000 });
        await canvas.scrollIntoViewIfNeeded();
        const box = await canvas.boundingBox();
        assert.ok(box && box.width > 40 && box.height > 40, `${name} ${route} chart has no size`);
        if (name === 'phone') await page.touchscreen.tap(box.x + box.width / 2, box.y + box.height / 2);
        else await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
        await page.waitForTimeout(150);
      }
      const policy = errors.filter(error => /Content Security Policy|violates the following/i.test(error));
      evidence.violations.push(...policy);
      evidence.errors.push(...errors.filter(error => !policy.includes(error)));
      await page.close();
    }
    await context.close();
  }
  // Sandboxed previews inherit the page's policy: the home demo and the playground must draw their charts under it.
  // Playwright exposes different clipboard grants per engine; all three exercise the native clipboard below.
  const permissions = h.engine === 'chromium' ? ['clipboard-read', 'clipboard-write'] : h.engine === 'webkit' ? ['clipboard-read'] : [];
  const context = await h.browser.newContext({ viewport: { width: 1440, height: 900 }, permissions });
  {
    const page = await context.newPage();
    await page.goto(h.base + '/', { waitUntil: 'load' });
    if (await page.evaluate(() => document.documentElement.dataset.landing === 'static')) {
      await page.mouse.wheel(0, 600);
      await page.waitForFunction(() => window.scrollY > 0);
      await page.waitForTimeout(500);
      assert.equal(await page.evaluate(() => document.documentElement.dataset.homepageEngine ?? null), null, 'scrolling the static landing does not start the editor');
      await page.locator('#demo-source').click();
      await page.waitForFunction(() => document.documentElement.dataset.homepageEngine === 'rust', null, { timeout: 15_000 });
    }
    await page.close();
  }
  for (const route of ['/', '/playground']) {
    const page = await context.newPage();
    const errors = [];
    watchErrors(page, errors, `${route}: `);
    await page.goto(h.base + route, { waitUntil: 'load' });
    if (route === '/') {
      // Typing renders the demo in the browser into a srcdoc frame, which inherits the page's policy.
      await page.locator('#demo-source').scrollIntoViewIfNeeded();
      await page.locator('#demo-source').press('End');
      await page.locator('#demo-source').type(' ');
      await page.waitForFunction(() => document.querySelector('#demo-result')?.hasAttribute('srcdoc'), null, { timeout: 15_000 });
    }
    // On the home page the typed source renders into the live example's frame; elsewhere the page's first frame.
    const frameElement = page.locator(route === '/' ? '#demo-result' : 'iframe').first();
    await frameElement.waitFor({ state: 'attached', timeout: 15_000 });
    const frame = await (await frameElement.elementHandle()).contentFrame();
    await frame.waitForSelector('figure[data-rv-chart] canvas', { state: 'attached', timeout: 30_000 }).catch(() => {});
    const drawn = await frame.locator('figure[data-rv-chart] canvas').count();
    evidence.previews = { ...evidence.previews, [route]: drawn };
    assert.ok(drawn > 0, `${route}: the sandboxed preview draws no chart under the page policy: ${errors.join(' | ')}`);
    evidence.violations.push(...errors.filter(error => /Content Security Policy|violates the following/i.test(error)));
    await page.close();
  }
  // The home page's story, top to bottom (Daniel, 9 Nov): the hero with the prompt box, the problem and the better way, the
  // live example, the brand and its styles, the best elements, the ecosystem and the close.
  {
    const prompt = siteMode.prompt;
    const page = await context.newPage();
    const errors = [];
    watchErrors(page, errors, 'home story: ');
    await page.goto(h.base + '/', { waitUntil: 'load' });
    const story = await page.evaluate(() => {
      const text = element => element?.textContent.replace(/\s+/g, ' ').trim();
      return {
        sections: [...document.querySelectorAll('main > section')].map(section => section.className.split(' ')[0]),
        headings: [...document.querySelectorAll('main h1, main h2')].map(text),
        sub: text(document.querySelector('.hero__sub')),
        try: document.querySelector('.hero__try a')?.getAttribute('href'),
        pills: document.querySelectorAll('.hero__pills').length,
        heroLinks: [...document.querySelectorAll('.hero a')].map(link => link.getAttribute('href')),
        prompts: [...document.querySelectorAll('.prompt')].map(box => ({ label: text(box.querySelector('.prompt__label')), words: box.querySelector('.prompt__text code').textContent, copy: text(box.querySelector('[data-copy]')) })),
        promptsIn: [...document.querySelectorAll('.prompt')].map(box => box.closest('section').className.split(' ')[0]),
        leads: [...document.querySelectorAll('.proof__lead')].map(text),
        sizes: [...document.querySelectorAll('[data-size]')].map(size => ({ text: text(size), hidden: size.hidden })),
        notes: [...document.querySelectorAll('.proof__note')].map(text),
        excerpts: [...document.querySelectorAll('.proof__excerpt')].map(text),
        pips: [...document.querySelectorAll('.pip-slot')].map(slot => ({ pip: slot.dataset.pip, clip: slot.dataset.clip, hidden: slot.getAttribute('aria-hidden'), still: slot.querySelector('img')?.getAttribute('src'), in: slot.closest('.sheet--source, .brand__file, .close__sheet')?.className.split(' ')[0] })),
        brand: (slot => slot && { file: text(slot.querySelector('.brand__file pre code'))?.startsWith('# Your Company Name Inc.'), page: slot.querySelector('.brand__page img')?.getAttribute('src'), before: !!(slot.compareDocumentPosition(document.querySelector('.voices__label')) & Node.DOCUMENT_POSITION_FOLLOWING) })(document.querySelector('.brand-slot[data-brand]')),
        lede: text(document.querySelector('.voices__head p')),
        rowLabel: text(document.querySelector('.voices__label')),
        eco: [...document.querySelectorAll('.eco__card h3')].map(heading => text(heading).replace(/\s*↑$/, '')),
        ecoLines: [...document.querySelectorAll('.eco__card > p')].map(text),
        ecoLink: document.querySelector('.eco__card h3 a')?.getAttribute('href'),
        elementsTarget: document.querySelector('#elements')?.className,
        elements: [...document.querySelectorAll('.element h3')].map(text),
        specimens: { lines: document.querySelectorAll('.spec-chart polyline').length, rows: document.querySelectorAll('.spec-table tbody tr').length, colours: document.querySelectorAll('.spec-colour li').length },
        elementsMore: (link => link && { text: text(link), href: link.getAttribute('href') })(document.querySelector('.best__more a')),
        close: { note: text(document.querySelector('.close__note')), open: text(document.querySelector('.close__open')), updates: text(document.querySelector('.updates__label')), formInside: !!document.querySelector('.updates #interest-form'), fields: [...document.querySelectorAll('#interest-form input:not([tabindex="-1"])')].map(input => input.type), terms: !!document.querySelector('.updates__terms a[href="/privacy"]') },
        removed: ['#how', '#why', '#flagship', '.signup', '.workflow', '.why'].filter(selector => document.querySelector(selector)),
        header: { links: [...document.querySelectorAll('.nav__links a')].map(link => text(link) || link.getAttribute('aria-label')), github: document.querySelector('.nav__github')?.getAttribute('href') ?? null, motto: document.querySelectorAll('.nav__motto').length },
        frames: document.querySelectorAll('.best__frame iframe').length,
      };
    });
    const hostedOrigin = (process.env.RICHVIEW_HOSTED_ORIGIN || '').trim().replace(/\/+$/, '');
    const hostedSections = hostedOrigin ? ['ecosystem'] : [];
    assert.deepEqual(story.sections, ['hero', 'problem', 'demo', 'voices', 'best', 'ecosystem', ...hostedSections, 'close'], 'the home page sections, in order');
    const hosted = page.locator('#hosted');
    assert.equal(await hosted.count(), hostedOrigin ? 1 : 0, 'the hosted section follows the configured origin');
    if (hostedOrigin) assert.equal(await hosted.locator('a').getAttribute('href'), `${hostedOrigin}/sign-up`, 'the hosted link uses the configured origin');
    assert.deepEqual(story.headings, [
      'Let your agent present.',
      "Agents create HTML reports by default. They're large, hard to edit, hard to version and iterate on, and different every time.",
      'Markdown in. Interactive report out.',
      'Your brand',
      'The best elements of Rich Markdown',
      'RichView is a whole ecosystem.',
      ...(hostedOrigin ? ['Publish and share your reports.'] : []),
      'Try it right now and see how simple it is.',
    ]);
    assert.equal(story.sub, 'RichView is a presentation layer for your agent.');
    assert.equal(story.try, '#demo');
    assert.equal(story.pills, 0, 'the hero has no pills');
    assert.ok(!story.heroLinks.includes('/showcase/openai-vs-anthropic/') && !story.heroLinks.includes('#signup'), 'the hero names neither the flagship nor the updates');
    assert.deepEqual(story.prompts.map(box => box.label), ['Let your agent make your first report.', 'Let your agent make your first report.']);
    assert.deepEqual(story.prompts.map(box => box.words), [prompt, prompt]);
    assert.deepEqual(story.prompts.map(box => box.copy), ['Copy', 'Copy']);
    assert.deepEqual(story.promptsIn, ['hero', 'close']);
    assert.deepEqual(story.leads, ['Megabytes of generated HTML', 'Kilobytes of text and data']);
    assert.deepEqual(story.notes, ['what your agent hands you', 'what you keep and review in Git']);
    // No numbers yet: the size slots wait, empty and hidden.
    assert.deepEqual(story.sizes, [{ text: '', hidden: true }, { text: '', hidden: true }]);
    assert.ok(!/\d/.test(story.leads.concat(story.notes).join(' ')), 'the proof pair carries no numbers');
    assert.deepEqual(story.excerpts, ['Example excerpt', 'Example excerpt']);
    // Vee's three small clips, each standing on the page's paper: typing on the editor, a brush in the brand's style file, a wave on the close's sheet.
    assert.deepEqual(story.pips, [
      { pip: 'typing', clip: '/landing/vee/typing', hidden: 'true', still: '/landing/vee/typing-first.webp', in: 'sheet' },
      { pip: 'brush', clip: '/landing/vee/brand', hidden: 'true', still: '/landing/vee/brand-first.webp', in: 'brand__file' },
      { pip: 'wave', clip: '/landing/vee/wave', hidden: 'true', still: '/landing/vee/wave-first.webp', in: 'close__sheet' },
    ]);
    // Your brand: an example company's style file above the prebuilt styles, with the report it makes.
    assert.deepEqual(story.brand, { file: true, page: '/landing/brand/company-report-1280.webp', before: true });
    assert.equal(story.lede, 'Set your typeface and colours once. Reuse them in every report.');
    assert.equal(story.rowLabel, 'Prebuilt styles');
    assert.deepEqual(story.eco, ['Rich Markdown', 'Renderer', 'Styles', 'VS Code extension', 'Report server']);
    assert.deepEqual(story.ecoLines, ['Your agent writes it, guided by richview learn.', 'Validation and chart diagnostics, then one self-contained HTML report.', 'Your brand, decided once and reused every time.', 'Live preview beside the source. Build it from source for now.', 'Share a stable, versioned link.']);
    assert.equal(story.ecoLink, '#elements');
    assert.equal(story.elementsTarget, 'best', 'the Rich Markdown card leads to the best elements');
    assert.deepEqual(story.elementsMore, { text: 'All elements in the docs →', href: '/docs/' });
    // The best elements are named, each with a specimen from the example's data (Daniel, 9 Nov), not a report in a frame.
    assert.deepEqual(story.elements, ['Charts', 'Metrics', 'Callouts', 'Tables', 'Images', 'Typography', 'Colour', 'And more']);
    assert.deepEqual(story.specimens, { lines: 4, rows: 4, colours: 5 });
    // Release updates take an email and nothing else; one line says what the reader signs up for, with the privacy link.
    assert.deepEqual(story.close, { note: "Of course, you won't be doing any of this: it's for your agent.", open: 'RichView is open source and MIT licensed.', updates: installEnabled ? 'Get release updates.' : 'Coming soon.', formInside: true, fields: ['email'], terms: true });
    assert.deepEqual(story.removed, [], 'the how, why, flagship and sign-up sections are gone');
    assert.deepEqual(story.header, siteMode.header);
    assert.equal(story.frames, 0, 'the best elements are named, not a report in a frame');
    // Pages of the styles row stand still and show no scroll bar: each frame is wider than its page by more than its own bar.
    await page.locator('#styles').scrollIntoViewIfNeeded();
    await page.waitForFunction(() => document.querySelectorAll('.voices__sheet[data-ready]').length === 5, null, { timeout: 60_000 });
    const clipped = await page.evaluate(() => [...document.querySelectorAll('.voices__page > iframe')].map(frame => {
      const win = frame.contentWindow, bar = (win.innerWidth - win.document.documentElement.clientWidth) / 3;
      return frame.getBoundingClientRect().right - frame.parentElement.getBoundingClientRect().right - bar;
    }));
    assert.equal(clipped.length, 5);
    for (const room of clipped) assert.ok(room >= -0.5, `a style page shows its report's scroll bar (${room}px short)`);
    // The runtime gives the typing slot its clip as the reader nears the live example and shows it once it plays, from its first frame.
    await page.locator('#demo').scrollIntoViewIfNeeded();
    await page.waitForFunction(() => document.querySelector('.pip-slot[data-pip="typing"][data-shown] video'), null, { timeout: 30_000 });
    evidence.clips = await page.evaluate(() => [...document.querySelectorAll('.pip-slot video')].map(video => ({ src: video.currentSrc.split('/').pop(), types: [...video.querySelectorAll('source')].map(source => source.type) })));
    evidence.best = story.specimens;
    evidence.violations.push(...errors.filter(error => /Content Security Policy|violates the following/i.test(error)));
    await page.close();
  }
  // The first press on Copy copies, though it is also what starts the site runtime: the bootstrap marks the button and the
  // runtime answers the press when it starts.
  for (const where of ['.hero [data-copy]', '.close [data-copy]']) {
    const page = await context.newPage();
    await page.goto(h.base + '/', { waitUntil: 'load' });
    assert.equal(await page.evaluate(() => document.documentElement.dataset.homepageEngine ?? null), null, 'the runtime has not started yet');
    await page.locator(where).scrollIntoViewIfNeeded();
    await page.evaluate(() => navigator.clipboard.writeText('before'));
    await page.locator(where).click();
    await page.waitForFunction(selector => document.querySelector(selector)?.textContent === 'Copied', where, { timeout: 15_000 });
    const copied = await page.evaluate(() => navigator.clipboard.readText());
    const shown = await page.locator(where.replace('[data-copy]', '.prompt__text code')).textContent();
    assert.equal(copied, shown, `${where}: the first press copies the prompt`);
    assert.equal(copied, siteMode.prompt, `${where}: the prompt matches the install mode`);
    await page.close();
  }
  // The demo's style buttons and highlighting start without the motion layer (FEA-SF-29): load the home page without its canvas.
  {
    const page = await context.newPage();
    await page.route('**/', async route => {
      const response = await route.fetch();
      const body = (await response.text()).replace(/<canvas id="rv-field"[^>]*><\/canvas>/, '').replace(/<svg id="rv-stage"[\s\S]*?<\/svg>/, '');
      await route.fulfill({ response, body });
    });
    await page.goto(h.base + '/', { waitUntil: 'load' });
    await page.keyboard.press('Shift');
    await page.waitForFunction(() => document.documentElement.dataset.homepageEngine === 'rust');
    assert.equal(await page.locator('#rv-field').count(), 0, 'the canvas was not removed');
    const buttons = page.locator('#demo-themes button');
    assert.ok(await buttons.count() >= 5, 'the demo has its five style buttons');
    assert.ok(await buttons.first().isVisible(), 'the demo style buttons are visible without the motion layer');
    assert.ok(await page.locator('.code__hl code span').count() > 0, 'the demo source is highlighted without the motion layer');
    await page.close();
  }
  // The live example's tools edit its source: the picker lists the chart types the core draws for the chart
  // without a finding, and a button adds a block that renders without one.
  {
    const page = await context.newPage();
    const errors = [];
    watchErrors(page, errors, 'editor tools: ');
    await page.goto(h.base + '/', { waitUntil: 'load' });
    await page.keyboard.press('Shift');
    await page.waitForFunction(() => document.documentElement.dataset.homepageEngine === 'rust', null, { timeout: 15_000 });
    await page.locator('#demo-tools').scrollIntoViewIfNeeded();
    const picker = page.locator('#demo-chart-type');
    await page.waitForFunction(() => !document.querySelector('#demo-chart-type').disabled, null, { timeout: 30_000 });
    const offered = await picker.locator('option').evaluateAll(options => options.map(option => option.value));
    assert.deepEqual(offered.slice(0, 4), ['line', 'bar', 'scatter', 'area'], 'the picker offers the types that fit the example');
    for (const refused of ['pie', 'heatmap', 'waterfall']) assert.ok(!offered.includes(refused), `${refused} does not fit the example's data`);
    const settledStatus = () => page.waitForFunction(() => document.querySelector('#demo-status').dataset.state === 'live', null, { timeout: 15_000 });
    const source = page.locator('#demo-source');
    await page.frameLocator('#demo-result').locator('body').waitFor();
    await page.locator('#demo-result').evaluate(frame => frame.setAttribute('data-last-good', 'yes'));
    await source.click();
    await source.evaluate(area => {
      const start = area.value.indexOf('y: life') + 3;
      if (start < 3) throw new Error('the example has no life encoding');
      area.setSelectionRange(start, start + 4);
    });
    await page.keyboard.type('lif');
    await page.waitForFunction(() => document.querySelector('#demo-status').dataset.state === 'error', null, { timeout: 15_000 });
    assert.match(await page.locator('#demo-status').innerText(), /This chart can't be drawn yet:.*lif/);
    await page.waitForTimeout(1000); // The picker also tries the edited chart after its debounce.
    assert.equal(await page.locator('#demo-result').getAttribute('data-last-good'), 'yes', 'invalid encoding keeps the last good report');
    assert.equal(await page.locator('#demo-veil').isHidden(), true, 'the last good report remains visible');
    await page.keyboard.type('e');
    await settledStatus();
    await page.waitForFunction(() => document.querySelector('#demo-result')?.hasAttribute('srcdoc'), null, { timeout: 15_000 });
    assert.match(await source.inputValue(), /y: life/);
    assert.equal(await page.locator('#demo-diagnostics').isHidden(), true, 'the report recovers after the field is corrected');
    await picker.selectOption('area');
    await settledStatus();
    assert.match(await page.locator('#demo-source').inputValue(), /^type: area$/m, 'the picker writes the type into the source');
    await source.focus();
    await source.evaluate(area => area.setSelectionRange(area.value.length, area.value.length));
    await page.getByRole('button', { name: '+ Metric' }).click();
    await settledStatus();
    assert.ok((await page.locator('#demo-source').inputValue()).endsWith('```metric\nvalue: 78.8\nlabel: Years a newborn in Europe could expect to live, 2019\ndelta: 0.22\ndelta_label: since 1950\n```\n'), 'a button adds its block at the end');
    assert.equal(await page.locator('#demo-diagnostics').isHidden(), true, 'the added block renders without a finding');
    for (const [name, block, selector, text] of [
      ['Heading', '## New heading', 'h2', 'New heading'],
      ['Paragraph', 'A new paragraph.', 'p', 'A new paragraph.'],
    ]) {
      const source = page.locator('#demo-source');
      await source.focus();
      const caret = (await source.inputValue()).indexOf("Gapminder's regions are wide") + 10;
      await source.evaluate((area, at) => area.setSelectionRange(at, at), caret);
      await page.getByRole('button', { name: `+ ${name}` }).click();
      await settledStatus();
      const edited = await source.inputValue();
      assert.ok(edited.indexOf(block) > caret && edited.indexOf(block) < edited.indexOf('```table'), `${name} is inserted at the caret's block`);
      await page.frameLocator('#demo-result').locator(selector, { hasText: text }).waitFor();
      assert.equal(await page.locator('#demo-diagnostics').isHidden(), true, `${name} renders without a finding`);
    }
    const policy = errors.filter(error => /Content Security Policy|violates the following/i.test(error));
    evidence.violations.push(...policy);
    evidence.errors.push(...errors.filter(error => !policy.includes(error)));
    await page.close();
  }
  // Both editors must show and insert the same choices and paint the same syntax for the same source.
  {
    const home = await context.newPage();
    const playground = await context.newPage();
    await home.goto(h.base + '/', { waitUntil: 'load' });
    await home.keyboard.press('Shift');
    await home.waitForFunction(() => document.documentElement.dataset.homepageEngine === 'rust', null, { timeout: 30_000 });
    await playground.goto(h.base + '/playground', { waitUntil: 'load' });
    await playground.waitForFunction(() => document.querySelector('#source')?.getAttribute('aria-autocomplete') === 'list', null, { timeout: 30_000 });
    const original = readFileSync(join(root, 'assets/landing/anchor/report.rmd'), 'utf8');
    for (const [needle, prefix, typed] of [['type: line', 'type: ', 'l'], ['x: year', 'x: ', 'y']]) {
      const results = [];
      for (const [page, selector] of [[home, '#demo-source'], [playground, '#source']]) {
        const area = page.locator(selector);
        await area.fill(original);
        const start = original.indexOf(needle) + prefix.length;
        assert.ok(start >= prefix.length, `fixture contains ${needle}`);
        await area.evaluate((el, range) => el.setSelectionRange(range[0], range[1]), [start, start + needle.length - prefix.length]);
        await page.keyboard.type(typed);
        const list = page.locator('#demo-complete');
        await list.waitFor({ state: 'visible', timeout: 15_000 });
        const choices = await list.locator('[role=option]').allTextContents();
        const painted = await area.evaluate(el => {
          const code = el.parentElement.querySelector('.code__hl code');
          return { html: code.innerHTML, colours: [...code.querySelectorAll('span')].map(span => [span.className, getComputedStyle(span).color]) };
        });
        assert.ok(choices.length > 0, `${selector} offers ${needle}`);
        assert.ok(painted.colours.length > 0, `${selector} highlights ${needle}`);
        await page.keyboard.press('Enter');
        results.push({ choices, painted, inserted: await area.inputValue() });
      }
      assert.deepEqual(results[1], results[0], `${needle}: playground and home completions, insertion and syntax colours match`);
    }
    // Completion on a later page must use the full document and preserve off-screen source,
    // including when the browser needs the setRangeText fallback.
    const longSource = Array.from({ length: 530 }, (_, i) => `Paragraph ${i} 📈`).join('\n') + '\n```chart\ntype: line\n```\n';
    const area = playground.locator('#source');
    await area.fill(longSource);
    await playground.waitForFunction(() => document.querySelector('#source').dataset.sourcePaged === 'true');
    const jump = playground.locator('.rvc-source-jump');
    await jump.fill('531');
    await jump.dispatchEvent('change');
    const visible = await area.inputValue();
    const typeAt = visible.indexOf('type: line');
    assert.ok(typeAt >= 0, 'the second source window contains the chart');
    await area.focus();
    await area.evaluate((el, at) => el.setSelectionRange(at + 6, at + 10), typeAt);
    await playground.keyboard.type('l');
    await playground.locator('#demo-complete').waitFor({ state: 'visible' });
    assert.ok((await playground.locator('#demo-complete [role=option]').allTextContents()).includes('line'), 'paged source offers chart types');
    await playground.evaluate(() => { document.execCommand = () => false; });
    await playground.keyboard.press('Enter');
    await playground.locator('#copy-source').click();
    await playground.waitForFunction(() => document.querySelector('#pg-status')?.textContent.includes('Source copied'));
    assert.equal(await playground.evaluate(() => navigator.clipboard.readText()), longSource, 'fallback completion preserves the full paged source');
    await home.close();
    await playground.close();
  }
  await context.close();
  assert.deepEqual(evidence.violations, [], 'policy violations');
  assert.deepEqual(evidence.errors, [], 'console errors');
  console.log(JSON.stringify({ status: 'passed', ...evidence, pages: evidence.pages.length, chart_pages: evidence.chart_pages.length }, null, 2));
} catch (error) {
  console.error(JSON.stringify({ status: 'failed', failure: String(error.stack || error), ...evidence }, null, 2));
  process.exitCode = 1;
} finally {
  await h.close();
}
