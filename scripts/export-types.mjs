// The per-chart-type vector check of the PDF export (FEA-CH-75, FEA-RP-43), apart from the tools
// that read the PDF: given what `pdftotext -bbox-layout` and `pdftohtml -xml` report, where each
// chart's title sits, whether a drawn label is copyable text beside it, and how much of its box a
// raster covers. verify-exports.mjs reads the PDF; tools/rv-gate/test/export-types.test.mjs drives this.

export const norm = text => text.toLowerCase().replace(/[\s­]+/g, '').replace(/[‘’]/g, "'").replace(/[“”]/g, '"');
const key = text => norm(text).slice(0, 30);
const entities = { '&lt;': '<', '&gt;': '>', '&quot;': '"', '&#39;': "'", '&apos;': "'", '&amp;': '&' };
const decode = word => word.replace(/&(?:lt|gt|quot|#39|apos|amp);/g, entity => entities[entity]);

/** Pages of `pdftotext -bbox-layout`: `{width, height, blocks: [{top, bottom, left, right, text}]}`, text normalised. */
export function parseBlocks(xml) {
  return [...xml.matchAll(/<page width="([\d.]+)" height="([\d.]+)">([\s\S]*?)<\/page>/g)].map(([, width, height, inner]) => ({
    width: +width, height: +height,
    blocks: [...inner.matchAll(/<block xMin="([\d.]+)" yMin="([\d.]+)" xMax="([\d.]+)" yMax="([\d.]+)">([\s\S]*?)<\/block>/g)].map(([, left, top, right, bottom, body]) => ({
      left: +left, top: +top, right: +right, bottom: +bottom,
      text: norm(decode([...body.matchAll(/<word[^>]*>([^<]*)<\/word>/g)].map(word => word[1]).join(' '))),
    })),
  }));
}

/** Selectable words from `pdftotext -bbox-layout`, separately from styled text fragments. */
export function parseWords(xml) {
  return [...xml.matchAll(/<page\b[^>]*>([\s\S]*?)<\/page>/g)].map(([, inner]) =>
    [...inner.matchAll(/<word xMin="(-?[\d.]+)" yMin="(-?[\d.]+)" xMax="(-?[\d.]+)" yMax="(-?[\d.]+)">([^<]*)<\/word>/g)]
      .map(([, left, top, right, bottom, text]) => ({ left: +left, top: +top, width: +right - +left, height: +bottom - +top,
        text: decode(text.replace(/&#(?:x([0-9a-f]+)|(\d+));/gi, (_, hex, decimal) => String.fromCodePoint(parseInt(hex ?? decimal, hex ? 16 : 10)))) })));
}

/** Images of `pdftohtml -xml`, in the page's points: `[{page, left, top, width, height}]`. Its page size is scaled by 1.5, so each page's own width gives the factor. */
export function parseImages(xml, pages) {
  return [...xml.matchAll(/<page number="(\d+)"[^>]* width="([\d.]+)">([\s\S]*?)<\/page>/g)].flatMap(([, number, width, inner]) => {
    const scale = Number(width) / pages[number - 1].width;
    return [...inner.matchAll(/<image top="(-?[\d.]+)" left="(-?[\d.]+)" width="([\d.]+)" height="([\d.]+)"/g)]
      .map(([, top, left, w, h]) => ({ page: +number, left: left / scale, top: top / scale, width: w / scale, height: h / scale }));
  });
}

/**
 * Each chart's title block, in document order: the first block from the previous chart's title on
 * that starts with the title's first 30 characters. A repeated title takes the next occurrence.
 * Returns `{page, block}` (page from 1) or null for a chart whose title is not in the PDF text.
 */
export function locateTitles(pages, titles) {
  let page = 0, index = 0;
  return titles.map(title => {
    const wanted = key(title);
    for (let p = page, b = index; p < pages.length; p++, b = 0) {
      const at = pages[p].blocks.findIndex((block, i) => i >= b && wanted && block.text.startsWith(wanted));
      if (at >= 0) { page = p; index = at + 1; return { page: p + 1, block: pages[p].blocks[at] }; }
    }
    return null;
  });
}

/**
 * The share of `box` that `image` covers; both `{left, top, width, height}`. Positions rather than
 * sizes, because a page can hold an authored poster beside a chart, and only a raster over the
 * chart is a rasterised chart.
 */
export function overlapShare(box, image) {
  const across = Math.min(box.left + box.width, image.left + image.width) - Math.max(box.left, image.left);
  const down = Math.min(box.top + box.height, image.top + image.height) - Math.max(box.top, image.top);
  return across > 0 && down > 0 && box.width * box.height > 0 ? (across * down) / (box.width * box.height) : 0;
}

/**
 * The labels a chart draws that can be looked for in the PDF text: its canvas text items with a
 * letter first (a digit shows up anywhere on a page), else the rest of three or more characters,
 * else any. Text that the figure's own HTML also holds (title, subtitle, legend) is dropped, since
 * that HTML reaches the PDF as text whatever the chart does.
 */
export function drawnLabels(items, htmlText) {
  const html = norm(htmlText);
  const unique = [...new Set(items.map(norm).filter(text => text && !html.includes(text)))];
  const letters = unique.filter(text => /\p{L}/u.test(text) && text.length >= 2);
  return letters.length ? letters : unique.filter(text => text.length >= 3);
}

/** Whether any of `labels` is in the text of `page`'s blocks that start at or below `top` and above `bottom`, outside the figure's own HTML text. */
export function labelOnPage(page, { top, bottom }, labels, htmlText) {
  const html = norm(htmlText);
  const text = page.blocks.filter(block => block.top >= top - 2 && block.top < bottom && !(block.text.length >= 4 && html.includes(block.text))).map(block => block.text).join('');
  return labels.find(label => text.includes(label)) ?? null;
}

/**
 * The fallback for text set along a curve, whose glyphs sit on a rotated path, so layout analysis
 * splits a word into pieces ("Sal", "es") while the content stream still holds it in order:
 * a label that follows the chart's title in the page's content-stream text (`pdftotext -raw`).
 */
export function labelInStream(rawPage, title, labels) {
  const text = norm(rawPage);
  const at = text.indexOf(norm(title));
  const after = at < 0 ? '' : text.slice(at + norm(title).length);
  return labels.find(label => after.includes(label)) ?? null;
}

/** The original bounds-only view of paintedShapes, in page-space points. */
export function paintedPaths(stream, pageHeight) {
  return paintedShapes(stream, pageHeight)
    .filter(shape => shape.kind === 'path')
    .map(({ left, top, width, height }) => ({ left, top, width, height }));
}

/**
 * Painted PDF paths with page-space controls, paint operators and original path operands/CTMs.
 * `s`/`b` close the current subpath; fill operators retain their implicit fill closure semantics.
 * Each paint retains its paint-time CTM, saved paint history and active clip paths.
 * Text/state operators are not paths. External, unknown and inline-image drawing is reported as
 * `kind: 'unsupported'`, so a geometry proof must resolve or reject it rather than count it.
 */
export function paintedShapes(stream, pageHeight, { markedContent = false } = {}) {
  const marked = [];
  const tokens = [];
  const lexer = /\(|<<|>>|<[0-9a-fA-F\s]*>|\[|\]|\/[^\s/<>[\]()%]*|%[^\r\n]*|[+-]?(?:\d+\.?\d*|\.\d+)|[A-Za-z][A-Za-z0-9]*\*?|['"]/g;
  for (let match; (match = lexer.exec(stream));) {
    if (match[0] !== '(') { tokens.push(match[0]); continue; }
    // PDF literal strings may nest balanced parentheses; their bytes are never path operators.
    let depth = 1, end = lexer.lastIndex;
    while (depth && end < stream.length) {
      const char = stream[end++];
      if (char === '\\') end++;
      else if (char === '(') depth++;
      else if (char === ')') depth--;
    }
    if (depth) throw new Error('Unterminated PDF literal string');
    tokens.push(stream.slice(match.index, end));
    lexer.lastIndex = end;
  }
  const identity = [1, 0, 0, 1, 0, 0];
  const times = ([a, b, c, d, e, f], [g, h, i, j, k, l]) => [a * g + b * i, a * h + b * j, c * g + d * i, c * h + d * j, e * g + f * i + k, e * h + f * j + l];
  const other = new Set('w J j M d ri i gs CS cs SC SCN sc scn G g RG rg K k W W* BT ET Tf Tc Tw Tz TL Tr Ts Td TD Tm T* Tj TJ \' " d0 d1 MP DP BMC BDC EMC BX EX'.split(' '));
  let ctm = identity, paintState = [], clips = [], pendingClip;
  let operands = [], points = [], commands = [], source = [], current, start;
  const paintSetters = new Set('w J j M d ri i gs CS cs SC SCN sc scn G g RG rg K k'.split(' '));
  const saved = [], shapes = [];
  // Keep the original point accumulation and bounds arithmetic unchanged for existing callers.
  const point = (x, y) => {
    const p = [ctm[0] * x + ctm[2] * y + ctm[4], ctm[1] * x + ctm[3] * y + ctm[5]];
    points.push(p);
    return [p[0], pageHeight - p[1]];
  };
  const record = (op, args) => source.push({ op, args, matrix: [...ctm] });
  const close = () => {
    if (commands.length && commands.at(-1).c !== 'z') commands.push({ c: 'z' });
    current = start;
  };
  const clip = () => {
    if (pendingClip) clips = [...clips, { rule: pendingClip, commands }];
    pendingClip = undefined;
  };
  const discard = () => { points = []; commands = []; source = []; current = undefined; start = undefined; };
  const paint = op => {
    if (/^(?:s|b|b\*)$/.test(op)) close();
    clip();
    if (points.length) {
      const xs = points.map(p => p[0]), ys = points.map(p => pageHeight - p[1]);
      const left = Math.min(...xs), top = Math.min(...ys);
      shapes.push({ ...(markedContent ? { mcids: marked.filter(Number.isInteger) } : {}), kind: 'path', op, commands, source, matrix: [...ctm], paintState, clips, left, top, width: Math.max(...xs) - left, height: Math.max(...ys) - top });
    }
    discard();
  };
  for (const token of tokens) {
    if (token.startsWith('%')) continue;
    if (/^[+-]?(?:\d|\.)/.test(token) || /^[/([<\]]/.test(token) || token === '>>') { operands.push(token); continue; }
    const n = operands.map(Number);
    if (['cm', 'm', 'l', 'c', 'v', 'y', 're'].includes(token) && !n.every(Number.isFinite)) throw new Error(`Non-finite PDF ${token} operands`);
    if (['l', 'c', 'v', 'y'].includes(token) && !current) throw new Error(`PDF ${token} has no current point`);
    if (token === 'BDC') { const at = operands.indexOf('/MCID'); marked.push(at < 0 ? null : Number(operands[at + 1])); }
    else if (token === 'BMC') marked.push(null);
    else if (token === 'EMC') marked.pop();
    else if (token === 'q') saved.push({ ctm, paintState, clips });
    else if (token === 'Q') ({ ctm, paintState, clips } = saved.pop() ?? { ctm: identity, paintState: [], clips: [] });
    else if (token === 'cm' && n.length >= 6) ctm = times(n.slice(-6), ctm);
    else if ((token === 'm' || token === 'l') && n.length >= 2) {
      const args = n.slice(-2), p = point(...args);
      commands.push({ c: token, p }); record(token, args); current = p;
      if (token === 'm') start = p;
    } else if (token === 'c' && n.length >= 6) {
      const args = n.slice(-6), c1 = point(args[0], args[1]), c2 = point(args[2], args[3]), p = point(args[4], args[5]);
      commands.push({ c: 'c', c1, c2, p }); record(token, args); current = p;
    } else if ((token === 'v' || token === 'y') && n.length >= 4) {
      const args = n.slice(-4), control = point(args[0], args[1]), p = point(args[2], args[3]);
      commands.push({ c: 'c', c1: token === 'v' ? current : control, c2: token === 'v' ? control : p, p });
      record(token, args); current = p;
    } else if (token === 're' && n.length >= 4) {
      const args = n.slice(-4), [x, y, w, h] = args;
      const a = point(x, y), b = point(x + w, y), c = point(x + w, y + h), d = point(x, y + h);
      commands.push({ c: 'm', p: a }, { c: 'l', p: b }, { c: 'l', p: c }, { c: 'l', p: d }, { c: 'z' });
      record(token, args); current = a; start = a;
    } else if (token === 'h') { close(); record(token, []); }
    else if (/^(?:f|F|f\*|S|s|B|B\*|b|b\*)$/.test(token)) paint(token);
    else if (token === 'W' || token === 'W*') pendingClip = token;
    else if (token === 'n') { clip(); discard(); }
    else if (paintSetters.has(token)) paintState = [...paintState, { op: token, args: [...operands] }];
    else if (!other.has(token)) {
      shapes.push({ kind: 'unsupported', op: token, args: [...operands], matrix: [...ctm], paintState, clips });
      // Inline image data is binary, not another sequence of content operators.
      if (token === 'BI') break;
    }
    operands = [];
  }
  return shapes;
}

/**
 * Each page's decoded content streams, from `qpdf --qdf --object-streams=disable` output read as
 * latin1: `string[]`, one per page. Chrome writes the page's drawing as plain operators in one or
 * more content streams, which `--qdf` leaves uncompressed.
 */
export function pageStreams(qdf) {
  const starts = new Map([...qdf.matchAll(/^(\d+) 0 obj$/gm)].map(match => [Number(match[1]), match.index + match[0].length]));
  const body = id => {
    const at = starts.get(id);
    const from = at === undefined ? -1 : qdf.indexOf('\nstream\n', at);
    const end = at === undefined ? -1 : qdf.indexOf('\nendobj', at);
    if (from < 0 || (end >= 0 && from > end)) return '';
    return qdf.slice(from + 8, qdf.indexOf('\nendstream', from));
  };
  // QDF writes objects in reference order; a page reached through an outline can come first.
  const pages = [...qdf.matchAll(/^%% Page (\d+)\n%% Original object ID: [^\n]*\n(\d+) 0 obj\n([\s\S]*?)\nendobj/gm)]
    .sort((a, b) => Number(a[1]) - Number(b[1]));
  if (pages.some((page, i) => Number(page[1]) !== i + 1)) throw new Error('PDF page stream order has a missing or duplicate page');
  return pages.map(([, , , dict]) => {
    const contents = /\/Contents\s*(\[[^\]]*\]|\d+ 0 R)/.exec(dict)?.[1] ?? '';
    return [...contents.matchAll(/(\d+) 0 R/g)].map(ref => body(Number(ref[1]))).join('\n');
  });
}

/**
 * How many painted paths sit inside the chart's box and are not a backdrop: a path whose own box covers
 * more than half of the chart's box is a page or figure background, not drawing. This is what a chart with
 * no copyable label (a waffle of cells, a Venn of circles) shows as vector.
 */
export function vectorPaths(paths, box) {
  const inside = path => path.left >= box.left - 2 && path.top >= box.top - 2 && path.left + path.width <= box.left + box.width + 2 && path.top + path.height <= box.top + box.height + 2;
  return paths.filter(path => inside(path) && path.width * path.height <= 0.5 * box.width * box.height).length;
}

/** Paths a chart without a drawn label must show in its box for check (b), so a lone frame or rule does not pass it. */
export const MIN_VECTOR_PATHS = 3;
