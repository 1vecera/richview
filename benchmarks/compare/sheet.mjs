// Library-comparison lab sheets (ARC-64): the HTML of one type's sheet (three columns, RichView,
// Highcharts and ECharts, at two widths) and of the contact sheet index.html. Image references are
// file URLs for the sheet, relative paths for the index. Development tooling only (ARC-03).

const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);
const SCHEME = { sleek: 'dark', air: 'light' };
const cap = (s) => s.charAt(0).toUpperCase() + s.slice(1);

/** Columns of a sheet: one per RichView style (`richview`, `richview-<style>`), then the two libraries. Entries without `richviewStyles` (older manifests) have the single Sleek column. */
export function columns(entry) {
	const styles = entry.richviewStyles ?? [{ style: 'sleek', key: 'richview' }];
	return [
		...styles.map((s) => [s.key, `RichView ${cap(s.style)}${SCHEME[s.style] ? ` (${SCHEME[s.style]})` : ''}`]),
		['highcharts', 'Highcharts (light)'],
		['echarts', 'ECharts (light)'],
	];
}

const css = `body{margin:0;padding:24px;background:#e9e9ec;font:14px/1.4 -apple-system,Helvetica,Arial,sans-serif;color:#222}
h1{font-size:20px;margin:0 0 4px}.meta{margin:0 0 16px;color:#555}.grid{display:grid;grid-template-columns:repeat(var(--n),max-content);gap:12px 16px;align-items:start}
.col{font-weight:600;font-size:13px;text-transform:uppercase;letter-spacing:.04em}.cell{background:#fff;min-height:40px}
.none{padding:16px;color:#666;width:calc(100% - 32px);box-sizing:content-box}.note{font-size:12px;color:#555;margin-top:6px;max-width:720px}
.tag{display:inline-block;padding:1px 8px;border-radius:9px;font-size:12px;background:#ccd}.tag.hc{background:#bde}.tag.ec{background:#bec}.tag.no{background:#ddd}`;

function cellFor(entry, lib, w, src) {
	if (src) return `<div class="cell"><img src="${esc(src)}" alt=""></div>`;
	const why = lib.startsWith('richview') ? 'RichView capture failed' : entry.skipped?.[lib] ?? entry.errors?.[lib] ?? 'not drawn';
	return `<div class="cell"><div class="none" style="width:${w - 32}px">${esc(why)}</div></div>`;
}

/** The sheet of one type; `imgSrc(lib, width)` returns an image reference or null. */
export function sheetHtml(entry, imgSrc) {
	const notes = [];
	for (const lib of ['highcharts', 'echarts']) {
		for (const u of entry.unmapped[lib]) notes.push(`${lib}: encoding ${u.channel} not mapped: ${u.reason}`);
		for (const n of entry.notes[lib]) notes.push(`${lib}: ${n}`);
	}
	const widths = Object.keys(entry.widths).sort((a, b) => b - a);
	const LIBS = columns(entry);
	const cells = widths.flatMap((w) => LIBS.map(([lib]) => cellFor(entry, lib, Number(w), imgSrc(lib, Number(w)))));
	return `<!doctype html><meta charset="utf-8"><style>${css}</style>
<h1>${esc(entry.id)} <small>(${esc(entry.type)})</small></h1>
<p class="meta">Same data, title and size in every column; RichView is shown in each style it was captured in, so Air (light) can be judged against the light references. Sizes: ${widths.map((w) => `${w}x${entry.widths[w].height}`).join(' and ')}. Libraries drawing it: ${entry.libraries.join(' and ') || 'none'}.</p>
<div class="grid" style="--n:${LIBS.length}">${LIBS.map(([, n]) => `<div class="col">${n}</div>`).join('')}${cells.join('')}</div>
${notes.map((n) => `<div class="note">${esc(n)}</div>`).join('')}`;
}

/** The contact sheet: a counts header and, per type, the desktop captures and a link to the sheet. */
export function indexHtml(manifest) {
	const c = manifest.counts;
	const rows = manifest.types
		.map((t) => {
			const tag = t.libraries.length === 2 ? '<span class="tag hc">both</span>' : t.libraries.length ? `<span class="tag ec">${t.libraries[0]} only</span>` : '<span class="tag no">neither</span>';
			const LIBS = columns(t);
			const img = (lib) => (t.images?.[lib]?.['720'] ? `<img src="${esc(t.images[lib]['720'])}" loading="lazy" alt="">` : '<span class="no">-</span>');
			return `<tr><td><a href="${esc(t.images?.sheet ?? '#')}">${esc(t.id)}</a><br>${tag}</td>${t.libraries.length ? LIBS.map(([lib]) => `<td>${img(lib)}</td>`).join('') : `<td colspan="${LIBS.length}" class="none">${esc(Object.values(t.skipped ?? {}).join('; '))}</td>`}</tr>`;
		})
		.join('\n');
	return `<!doctype html><meta charset="utf-8"><title>RichView library comparison</title>
<style>${css}table{border-collapse:collapse;width:100%}td,th{vertical-align:top;padding:6px;border-bottom:1px solid #ccc;text-align:left}td img{width:100%;display:block}td:first-child{width:12%}</style>
<h1>RichView against Highcharts and ECharts</h1>
<p class="meta">RichView build ${esc(manifest.richview.commit)} (${esc((manifest.richview.styles ?? [{ style: manifest.richview.style }]).map((x) => x.style).join(' and '))}), Highcharts ${esc(manifest.libraries.highcharts.version)}, ECharts ${esc(manifest.libraries.echarts.version)}. ${c.types} types: ${c.both} in both libraries, ${c.highchartsOnly} Highcharts only, ${c.echartsOnly} ECharts only, ${c.neither} in neither. Desktop captures at 720x440; open a type for both widths.</p>
<table><tr><th>Type</th>${columns(manifest.types[0] ?? {}).map(([, n]) => `<th>${n}</th>`).join('')}</tr>
${rows}</table>`;
}
