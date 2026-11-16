// Composite images and the contact sheet. Layouts are HTML rendered by the same
// headless browser, so labels are real text and the browser downscales captures.
import { closeSync, openSync, readSync, writeFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

const esc = value => String(value ?? '').replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);
const SHEET_SCALE = .25, STRIP_HEIGHT = 4000;
const css = `*{box-sizing:border-box}body{margin:0;background:#fff;color:#16181d;font:13px/1.45 system-ui,sans-serif;width:max-content}
main{padding:24px 28px 32px}h1{font-size:18px;font-weight:600;margin:0}h2{font-size:14px;font-weight:600;margin:28px 0 10px}
h3{font-size:13px;font-weight:600;margin:0}p{margin:2px 0 0;color:#5b6170;max-width:96ch}figure{margin:0}
figcaption{color:#5b6170;font-variant-numeric:tabular-nums;margin-top:3px}img{display:block}.frame img{border:1px solid #d9dce3}`;

function pngSize(file) {
  const header = Buffer.alloc(24), fd = openSync(file, 'r');
  try { readSync(fd, header, 0, 24, 0); } finally { closeSync(fd); }
  return { width: header.readUInt32BE(16), height: header.readUInt32BE(20) };
}

export async function composer(browser) {
  const context = await browser.newContext({ viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1 });
  return {
    // Pages that reference files on disk are written next to the output and opened
    // from file://; pages with inline images are set directly.
    async render(html, path, file) {
      const page = await context.newPage();
      try {
        if (file) { writeFileSync(file, html); await page.goto(pathToFileURL(file).href); }
        else await page.setContent(html);
        await page.evaluate(() => Promise.all([...document.images].map(image => image.decode().catch(() => null))));
        const size = await page.evaluate(() => { const r = document.body.getBoundingClientRect(); return { width: Math.ceil(r.width), height: Math.ceil(r.height) }; });
        await page.setViewportSize({ width: size.width, height: Math.min(size.height, 4000) });
        await page.screenshot({ path, fullPage: true });
      } finally {
        await page.close();
      }
    },
    close: () => context.close(),
  };
}

// Entrance frames in a 5-column grid and the pointer sequence in one row of equal width.
export function filmstripHtml({ heading, note, view, motion }) {
  const gap = 12, width = view.mobile ? 240 : 400, pointerWidth = Math.floor((5 * width - gap) / 6);
  const frame = (f, w) => `<figure class="frame"><img src="data:image/png;base64,${f.png.toString('base64')}" style="width:${w}px"><figcaption>${f.t} ms${f.readout ? ` · ${esc(f.readout.slice(0, 70))}` : ''}</figcaption></figure>`;
  const grid = (frames, columns, w) => `<div style="display:grid;grid-template-columns:repeat(${columns},${w}px);gap:${gap}px;align-items:start">${frames.map(f => frame(f, w)).join('')}</div>`;
  const target = motion.target;
  return `<!doctype html><meta charset="utf-8"><style>${css}</style><main><h1>${esc(heading)}</h1><p>${esc(note)}</p>
<h2>Entrance, from the chart scrolled into place (canvas only)</h2>${grid(motion.entrance, 5, width)}
<h2>${target ? `${target.input === 'tap' ? 'Tap' : 'Hover'} on ${esc(target.mark)}${target.label ? ` · ${esc(target.label.slice(0, 90))}` : ''}` : 'No pointer target found'}</h2>${target ? grid(motion.pointer, 6, pointerWidth) : ''}</main>`;
}

const summaryLine = s => s ? [`${s.glyph_findings} chart glyph`, `${s.text_overlaps} text overlap`, `${s.text_clipping} clipped text`, `${s.scroll_hidden_text} scroll-hidden`, `${s.overflow_px} px overflow`, `CLS ${s.cls}`, `${s.console_errors} console`, s.reduced_motion_failures ? `${s.reduced_motion_failures} reduced-motion failures` : 'reduced motion ok', s.settle_mismatches ? `${s.settle_mismatches} settle mismatches` : null].filter(Boolean).join(' · ') : 'capture failed';

// One image per case: for each width, every theme's full page side by side, scaled to a
// quarter of its pixels and wrapped into strips so long reports stay viewable.
// `shots(theme, view)` returns [{ label, file, summary }] (baseline first when present).
export function sheetHtml({ item, title, views, themes, shots }) {
  const column = shot => {
    const label = `<b>${esc(shot.label)}</b> · ${esc(summaryLine(shot.summary))}`;
    if (!shot.file) return `<div><p style="width:220px">${label}</p><p>No capture</p></div>`;
    const size = pngSize(shot.file), width = Math.round(size.width * SHEET_SCALE), height = Math.round(size.height * SHEET_SCALE);
    const count = Math.ceil(height / STRIP_HEIGHT), src = pathToFileURL(shot.file).href;
    const strips = Array.from({ length: count }, (_, i) => `<div style="width:${width}px;height:${Math.min(STRIP_HEIGHT, height - i * STRIP_HEIGHT)}px;overflow:hidden;outline:1px solid #d9dce3"><img src="${src}" style="width:${width}px;height:${height}px;margin-top:${-i * STRIP_HEIGHT}px"></div>`);
    return `<div><p style="width:${Math.max(220, count * (width + 6) - 6)}px">${label}</p><div style="display:flex;gap:6px;margin-top:6px">${strips.join('')}</div></div>`;
  };
  return `<!doctype html><meta charset="utf-8"><style>${css}</style><main><h1>${esc(item.id)}${title ? ` · ${esc(title)}` : ''}</h1><p>${esc(item.about)} · ${esc(item.source ?? item.page)}</p>
${views.map(view => `<h2>${view.width} × ${view.height} · ${view.dpr}x${view.mobile ? ' touch' : ''}</h2><div style="display:flex;gap:28px;align-items:start">${themes.map(theme => `<section><h3>${esc(theme)}</h3><div style="display:flex;gap:14px;margin-top:4px">${shots(theme, view).map(column).join('')}</div></section>`).join('')}</div>`).join('')}</main>`;
}

// The contact sheet: run facts, a findings table, then per case a matrix with the page
// and each chart as rows and theme/width captures as columns, followed by findings and
// pacing. `baseline` is the baseline manifest; `link(path)` resolves a baseline path
// relative to the output.
const columns = ['glyph_findings', 'text_overlaps', 'text_clipping', 'scroll_hidden_text', 'overflow_px', 'cls', 'console_errors', 'reduced_motion_failures', 'settle_mismatches', 'mount_dh', 'settled_ms', 'settled_dh', 'interaction_failures', 'native_warnings'];
const headings = ['Chart glyph', 'Text overlap', 'Clipped text', 'Scroll-hidden text', 'Overflow px', 'CLS', 'Console', 'Reduced motion', 'Settle', 'Mount Δh px', 'Settled ms', 'Settled Σ|Δh| px', 'Tap / focus failures', 'Native'];
// Scroll-hidden text and native estimates are context; the other counts are defects.
// CLS varies slightly with load timing, so only a rise above 0.02 counts as worse. A first
// runtime frame may differ from the page without JavaScript on at most 0.5 % of its pixels.
const alarming = new Set(['glyph_findings', 'text_overlaps', 'text_clipping', 'overflow_px', 'console_errors', 'reduced_motion_failures', 'settle_mismatches', 'mount_dh', 'settled_dh', 'interaction_failures']);
const limits = { cls: .1 };
const change = (now, was) => `${was !== undefined && was !== null && was !== now ? `${esc(was)} → ` : ''}${esc(now ?? '–')}`;
export const baselineCapture = (baseline, item, capture) => baseline?.cases.find(c => c.id === item.id)?.captures.find(c => c.theme === capture.theme && c.width === capture.width && !c.error);
export function indexHtml({ manifest, baseline, link }) {
  const { build, device, options } = manifest;
  const baseItem = item => baseline?.cases.find(c => c.id === item.id);
  const base = (item, capture) => baselineCapture(baseline, item, capture);
  const value = (key, now, before) => {
    const v = now.summary?.[key], was = before?.summary?.[key];
    const worse = was !== undefined && was !== null && v !== null && (key === 'cls' ? v > was + .02 : Math.abs(v) > Math.abs(was));
    const flag = worse ? 'worse' : (key in limits ? v > limits[key] : v !== 0 && v !== null && alarming.has(key)) ? 'bad' : '';
    return `<td class="${flag}">${change(v, was)}</td>`;
  };
  const summary = manifest.cases.flatMap(item => item.captures.map(c => `<tr><td><a href="#${esc(item.id)}">${esc(item.id)}</a></td><td>${esc(c.theme)}</td><td>${c.width}</td>${c.error ? `<td colspan="${columns.length}" class="bad">${esc(c.error)}</td>` : columns.map(k => value(k, c, base(item, c))).join('')}</tr>`)).join('');
  const image = (href, width, tall) => `<a href="${esc(href)}"><img loading="lazy" src="${esc(href)}" alt="" style="width:${width}px${tall ? ';max-height:480px;object-fit:cover;object-position:top' : ''}"></a>`;
  const pair = (now, before, width, tall) => `<div class="pair">${before ? `<figure>${image(link(before), width, tall)}<figcaption>Baseline</figcaption></figure>` : ''}${now ? `<figure>${image(now, width, tall)}${before ? '<figcaption>Current</figcaption>' : ''}</figure>` : ''}</div>`;
  const flags = chart => [
    chart.glyph_checks?.length && `${chart.glyph_checks.length} glyph`, chart.reduced_motion?.error && `not painted: ${chart.reduced_motion.error}`,
    chart.reduced_motion?.equal === false && !chart.reduced_motion.error && 'reduced motion differs', chart.settles_to_reduced && !chart.settles_to_reduced.matches && 'settle mismatch',
    chart.motion_error && `filmstrip failed: ${chart.motion_error}`,
  ].filter(Boolean).join(' · ');
  const matrix = item => {
    const captures = [...item.captures].sort((a, b) => b.width - a.width);
    const charts = [...new Map(captures.flatMap(c => c.charts ?? []).map(chart => [chart.id, chart])).values()];
    const width = c => (c.width < 600 ? 120 : 240);
    const cell = (c, chart) => {
      const now = c.charts?.find(x => x.id === chart.id), before = base(item, c)?.charts?.find(x => x.id === chart.id);
      if (!now) return '<td></td>';
      const film = [now.filmstrip && `<a href="${esc(now.filmstrip)}">Filmstrip</a>`, before?.filmstrip && `<a href="${esc(link(before.filmstrip))}">baseline</a>`].filter(Boolean).join(' · ');
      return `<td>${pair(now.crop, before?.crop, width(c))}<p>${esc(flags(now))}${film ? `<br>${film}` : ''}</p></td>`;
    };
    return `<div class="scroll"><table class="matrix"><thead><tr><th></th>${captures.map(c => `<th>${esc(c.theme)}<br>${c.width}px</th>`).join('')}</tr></thead><tbody>
<tr><th>Page</th>${captures.map(c => `<td>${c.error ? `<p class="bad">${esc(c.error)}</p>` : pair(c.page, base(item, c)?.page, width(c), true)}</td>`).join('')}</tr>
${charts.map(chart => `<tr><th>${esc(chart.id)}<br>${esc(chart.type)}<br><small>${esc(chart.title.slice(0, 80))}</small></th>${captures.map(c => cell(c, chart)).join('')}</tr>`).join('')}</tbody></table></div>`;
  };
  const findings = item => item.captures.map(c => {
    const d = c.diagnostics;
    if (!d) return '';
    const list = [
      ...d.console.map(m => `Console: ${m}`),
      ...d.text.overlap_examples.map(o => `Text overlap: “${o.a}” and “${o.b}” (${o.elements.join(', ')})`),
      ...d.text.clipping_examples.map(o => `Clipped: “${o.text}” in ${o.container}${o.ellipsis ? ' with ellipsis' : ''}`),
      ...d.text.scroll_hidden_examples.map(o => `Out of view in ${o.container}: “${o.text}”`),
      ...d.overflow.offenders.map(o => `Overflow: ${o.element} reaches ${o.right} px`),
      ...(c.charts ?? []).flatMap(chart => (chart.glyph_checks ?? []).map(g => `${chart.id}: ${g.message}`)),
      ...(c.native?.findings ?? []).map(f => `Native ${f.level} ${f.chart ?? ''}: ${f.message}`),
      ...(c.mount?.error ? [`Mount continuity: ${c.mount.error}`] : c.mount ? [`Mount continuity ${c.mount.chart}: canvas first sized at ${c.mount.mount_ms} ms with Δh ${c.mount.first_frame.dh} px against the page without JavaScript, settled() resolved at ${c.mount.settled_ms} ms; settled Σ|Δh| ${c.mount.settled.summed_abs_dh} px over ${c.mount.settled.figures} figures`] : []),
      ...(c.interaction ?? []).map(r => r.error ? `${r.probe} ${r.chart}: ${r.error}` : r.probe === 'tap' ? `Tap ${r.chart} (${r.mark}): readout ${r.shown.ms100} at 100 ms, ${r.shown.ms1500} at 1500 ms, ${r.released ? 'released' : 'not released'} by a second tap${r.ok ? '' : ' (fails)'}` : `Legend focus release ${r.chart}: ${(r.focused * 100).toFixed(2)} % of the canvas changes while hovering, ${(r.after * 100).toFixed(2)} % differs after leaving${r.ok ? '' : ' (fails)'}`),
    ];
    return list.length ? `<details><summary>${esc(c.theme)} · ${c.width}px: ${list.length} findings</summary><ul>${list.map(x => `<li>${esc(x)}</li>`).join('')}</ul></details>` : '';
  }).join('');
  const pacing = item => {
    if (!item.pacing.length) return '';
    const head = ['Chart', 'Type', 'Width', 'CPU', 'Load long frames', 'Load longest ms', 'Entered by', 'Entrance ms', 'Entrance p95 ms', 'Entrance max ms', 'Dropped %', 'Long frames', 'Longest frame ms', 'Pointer p95 ms', 'Pointer dropped %'];
    const row = p => {
      if (p.error) return `<tr><td>${esc(p.chart)}</td><td colspan="${head.length - 1}" class="bad">${esc(p.error)}</td></tr>`;
      const b = baseItem(item)?.pacing?.find(x => x.chart === p.chart && x.width === p.width && x.throttle === p.throttle && !x.error);
      const cells = [
        change(p.load?.long_animation_frames, b?.load?.long_animation_frames), change(p.load?.loaf_max_ms, b?.load?.loaf_max_ms),
        p.scrolled ? (p.entrance.played ? 'scroll' : 'scroll, no entrance') : 'first screen', change(p.entrance.settled_after_ms, b?.entrance.settled_after_ms),
        change(p.entrance.p95_ms, b?.entrance.p95_ms), change(p.entrance.max_ms, b?.entrance.max_ms),
        change(p.entrance.dropped_pct, b?.entrance.dropped_pct), change(p.entrance.long_animation_frames, b?.entrance.long_animation_frames),
        change(p.entrance.loaf_max_ms, b?.entrance.loaf_max_ms), change(p.pointer?.p95_ms, b?.pointer?.p95_ms), change(p.pointer?.dropped_pct, b?.pointer?.dropped_pct),
      ];
      const bad = [p.load?.loaf_max_ms > (p.throttle > 1 ? 120 : 50), false, p.scrolled && !p.entrance.played, p.scrolled && p.entrance.played && p.entrance.settled_after_ms === null, false, p.entrance.max_ms > 33.4, p.entrance.dropped_pct > 5, p.entrance.long_animation_frames > 0];
      return `<tr><td>${esc(p.chart)}</td><td>${esc(p.type)}</td><td>${p.width}</td><td>${p.throttle}x</td>${cells.map((v, i) => `<td class="${bad[i] ? 'bad' : ''}">${v}</td>`).join('')}</tr>`;
    };
    return `<table><thead><tr>${head.map(h => `<th>${h}</th>`).join('')}</tr></thead><tbody>${item.pacing.map(row).join('')}</tbody></table>`;
  };
  const exposed = manifest.themes.map(t => (t.aliases.length ? `${t.id} (also ${t.aliases.join(', ')})` : t.id)).join(', ');
  const themes = options.themes.length === manifest.themes.length ? `Themes ${exposed}` : `Themes ${options.themes.join(', ')} of ${exposed}`;
  const gpu = Object.entries(device.gpu.features).filter(([k]) => ['Compositing', 'Rasterization', 'WebGL'].includes(k)).map(([k, v]) => `${k}: ${v}`).join('; ');
  return `<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Design review ${esc(build.commit.slice(0, 8))}</title>
<style>body{margin:0;background:#fff;color:#16181d;font:14px/1.5 system-ui,sans-serif}main{max-width:1760px;margin:0 auto;padding:32px 24px 72px}
h1{font-size:22px;margin:0 0 6px}h2{font-size:18px;margin:56px 0 4px}p{margin:0 0 6px;color:#5b6170;max-width:120ch}a{color:#174da3}
table{border-collapse:collapse;font-variant-numeric:tabular-nums;margin:12px 0}th,td{padding:4px 10px;border-bottom:1px solid #e3e5ea;text-align:right;white-space:nowrap;vertical-align:top}
th:first-child,td:first-child,th:nth-child(2),td:nth-child(2){text-align:left}thead th{font-weight:600;font-size:12px;color:#5b6170}.bad{color:#a1260d}.worse{color:#a1260d;font-weight:700}
.scroll{overflow-x:auto}.matrix th,.matrix td{text-align:left;white-space:normal;padding:8px 10px 12px 0}.matrix tbody th{font-weight:600;font-size:12px;width:170px}.matrix small{font-weight:400;color:#5b6170}
.matrix p{font-size:12px;margin:4px 0 0}.pair{display:flex;gap:6px}figure{margin:0}figcaption{font-size:12px;color:#5b6170}img{display:block;border:1px solid #d9dce3;height:auto}
details{margin:6px 0}summary{cursor:pointer}li{font-size:12px}</style>
<main><h1>Design review</h1>
<p>${esc(build.branch)} at ${esc(build.commit.slice(0, 12))}${build.dirty ? ' with uncommitted changes' : ''} · ${esc(build.version)} · captured ${esc(manifest.captured_at)} in ${manifest.duration_s} s${options.quick ? ' · quick' : ''}${options.baseline ? ` · baseline <a href="${esc(link('index.html'))}">${esc(options.baseline)}</a>` : ''}</p>
<p>${esc(device.cpu)}, ${device.logical_cores} threads, load ${esc(device.load_average.join(' '))} · Chromium ${esc(device.chromium)} headless · ${esc(gpu)} · frame interval ${device.frame_interval_ms} ms</p>
<p>${esc(themes)} · viewports ${esc(manifest.viewports.map(v => `${v.width}×${v.height} @${v.dpr}x`).join(', '))} · entrance frames ${manifest.timeline.entrance_ms.join(', ')} ms · pointer frames ${manifest.timeline.pointer_ms.join(', ')} ms</p>
<table><thead><tr><th>Case</th><th>Theme</th><th>Width</th>${headings.map(h => `<th>${h}</th>`).join('')}</tr></thead><tbody>${summary}</tbody></table>
${manifest.cases.map(item => `<section id="${esc(item.id)}"><h2>${esc(item.id)}</h2><p>${esc(item.about)} · <code>${esc(item.source ?? item.page)}</code>${item.sheet ? ` · <a href="${esc(item.sheet)}">All themes side by side</a>` : ''}</p>${matrix(item)}${findings(item)}${pacing(item)}</section>`).join('')}
</main></html>`;
}
