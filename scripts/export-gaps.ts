import { paintedShapes } from './export-types.ts';

type Frame = { left: number; right: number; top: number; bottom: number };
type Block = Frame & { text: string };
type Figure = { title: string; source?: string; subtitle?: string; method?: string; state?: string; legend?: string[]; frame?: Frame };
type Unit = { top: number; end: number; cut: boolean; owned?: Set<Block> };
type Opening = { opens_section: boolean; height: number } & ({ lead: true; ends_section: boolean | null } | { lead?: false });
type QpdfDocument = {
  pages: { object: string }[];
  qpdf: [unknown, Record<string, { value?: Record<string, unknown> }>];
};

// The pagination gap rule of the PDF check (FEA-RP-43), apart from the PDF: given the text blocks
// of a page as `pdftotext -bbox-layout` reports them, what its first block is and whether a gap
// above it is accepted. verify-exports.ts reads the PDF; tests/export-gaps.test.mjs drives this.

export const norm = (text: string) => text.toLowerCase().replace(/[\s­]+/g, '');
const plain = (text: string) => text.replace(/[‘’]/g, "'").replace(/[“”]/g, '"');
const key = (text: string) => plain(norm(text)).slice(0, 30);

/**
 * What opens a page. `blocks` are `{top, bottom, text}` inside the page margins, sorted by top;
 * `figures` are `{title, source}` and `sectionStarts` the first words of every section.
 *
 * - `opens_section`: the first block starts a section.
 * - `height`: the unit that must fit: a heading with the block after it, or a figure with its note.
 * - `lead`: the first block is a figure. Then `ends_section` says whether the run of figures that
 *   starts there is followed by a section opening (true), by anything else (false), or by nothing
 *   on this page (null: the caller reads the next page). Figures directly after each other are
 *   one run, which floats as a queue, in order (FEA-RP-43).
 */
export function openingOf(blocks: Block[], { pageHeight, margins, sectionStarts, figures }: { pageHeight: number; margins: number[]; sectionStarts: string[]; figures: Figure[] }): Opening | null {
  const [first, next] = blocks;
  if (!first) return null;
  const foot = pageHeight * (1 - margins[1]);
  const opens = (block: { text: string; }) => sectionStarts.some((start: string) => start && block.text.startsWith(key(start)));
  const inFrame = (block: Frame, figure: Figure) => figure.frame && block.left >= figure.frame.left && block.right <= figure.frame.right
    && block.top >= figure.frame.top && block.bottom <= figure.frame.bottom;
  const tagged = new Map<Figure, Unit | null>();
  const taggedUnit = (figure: Figure): Unit | null => {
    if (!figure.frame) return null;
    if (tagged.has(figure)) return tagged.get(figure)!;
    const frame = figure.frame;
    const pending = [figure.title, figure.subtitle, figure.method, figure.state].filter(Boolean).map(text => plain(norm(text!)));
    // A printed legend's names sit between the frame and the caption, on the side the style
    // places them (FEA-CH-67), so going out from the frame they come first. They belong to the
    // figure but are not required: a chart whose plot names its series prints no legend. A block
    // holds one name or several in a row; read, the legend must be whole.
    const names = (figure.legend ?? []).map((text: string) => plain(norm(text))).filter(Boolean);
    const unread = new Set(names.keys());
    const takeNames = (block: Block) => {
      for (let i = 0; i < names.length; i++) {
        let text = '', j = i;
        while (j < names.length && unread.has(j) && text.length < block.text.length) text += names[j++];
        if (j > i && text === block.text) {
          for (let k = i; k < j; k++) unread.delete(k);
          return true;
        }
      }
      return false;
    };
    const withinWidth = (block: { left: number; right: number; }) => block.left >= frame.left && block.right <= frame.right;
    // Consume a field exactly once. Wrapped fragments reconstruct it from the nearest edge;
    // an ambiguous fragment gives no new exception to the legacy reader.
    const take = (fields: string[], block: Block, reverse: boolean) => {
      const exact = fields.findIndex(text => text === block.text);
      const matches = fields.flatMap((text, i) => text && (reverse ? text.endsWith(block.text) : text.startsWith(block.text)) ? [i] : []);
      const index = exact >= 0 ? exact : matches.length === 1 ? matches[0] : -1;
      if (index < 0) return false;
      fields[index] = reverse ? fields[index].slice(0, -block.text.length) : fields[index].slice(block.text.length);
      return true;
    };
    const consume = (block: Block, reverse = false) => {
      if (!block.text || !withinWidth(block)) return false;
      return takeNames(block) || take(pending, block, reverse);
    };
    let from = blocks.findIndex((block: { left: number; right: number; top: number; bottom: number; }) => inFrame(block, figure));
    let to = blocks.findLastIndex((block: { left: number; right: number; top: number; bottom: number; }) => inFrame(block, figure));
    if (from < 0) {
      // No text inside the frame: the caption is the title block nearest to it, so a section
      // heading with the same words elsewhere on the page is never taken for the chart's own.
      const away = (block: { bottom: number; top: number; }) => Math.max(0, frame.top - block.bottom, block.top - frame.bottom);
      from = to = blocks.reduce((best, block, i) => block.text.startsWith(key(figure.title))
        && (best < 0 || away(block) < away(blocks[best])) ? i : best, -1);
      if (from < 0 || !consume(blocks[from])) { tagged.set(figure, null); return null; }
    } else if (blocks.slice(from, to + 1).some((block: { left: number; right: number; top: number; bottom: number; }) => !inFrame(block, figure))) {
      tagged.set(figure, null); return null;
    }
    while (from > 0 && consume(blocks[from - 1], true)) from--;
    while (to + 1 < blocks.length && consume(blocks[to + 1])) to++;
    if (pending.some(Boolean) || (unread.size && unread.size < names.length)) { tagged.set(figure, null); return null; }
    let end = Math.max(frame.bottom, blocks[to].bottom), last = to;
    if (figure.source) {
      let rest = plain(norm(figure.source)), next = to + 1;
      while (rest && next < blocks.length && blocks[next].text && withinWidth(blocks[next]) && rest.startsWith(blocks[next].text)) {
        rest = rest.slice(blocks[next].text.length);
        end = blocks[next++].bottom;
      }
      if (rest) { tagged.set(figure, null); return null; }
      last = next - 1;
    }
    const unit = { top: Math.min(frame.top, blocks[from].top), end, cut: false, owned: new Set(blocks.slice(from, last + 1)) };
    tagged.set(figure, unit);
    return unit;
  };
  const figureAt = (block: Block, requireTagged = false) => figures.find((item: { title: string; }) => {
    if (!item.title) return false;
    const physical = taggedUnit(item);
    return physical ? physical.owned!.has(block) : !requireTagged && block.text.startsWith(key(item.title));
  });
  // The note is a block that starts with the source text; a long one breaks into several blocks,
  // each a piece of that text, so the note ends with the last of them.
  const noteOf = (figure: Figure, block: Block) => {
    const start = figure.source ? blocks.findIndex((other: { top: number; text: string; }) => other.top >= block.top && other.text.startsWith(key(figure.source!))) : -1;
    if (start < 0) return undefined;
    const whole = plain(norm(figure.source!));
    let end = start;
    while (end + 1 < blocks.length && whole.includes(blocks[end + 1].text)) end++;
    return { top: blocks[start].top, bottom: blocks[end].bottom };
  };
  const unitOf = (figure: Figure, block: Block): Unit => {
    const physical = taggedUnit(figure);
    if (physical) return physical;
    const note = noteOf(figure, block);
    if (note) return { top: block.top, end: note.bottom, cut: false };
    return { top: block.top, end: foot, cut: true };
  };
  const opens_section = opens(first);
  const lead = figureAt(first);
  if (lead) {
    const unit = unitOf(lead, first);
    let { end, cut } = unit;
    let after;
    for (;;) {
      after = cut ? undefined : blocks.find((block: { top: number; }) => block.top > end + 1);
      // A verified run cannot borrow a matching caption from a figure elsewhere.
      const another = after && figureAt(after, Boolean(unit.owned));
      if (!another) break;
      ({ end, cut } = unitOf(another, after!));
    }
    return { opens_section, lead: true, ends_section: after ? opens(after) : null, height: +(unit.end - unit.top).toFixed(1) };
  }
  // The heading's neighbour is a figure when it opens with a figure's title; the figure then ends
  // at its source note, or at the foot of the page when the note is not on this page.
  let end = (next ?? first).bottom;
  const figure = next && figureAt(next);
  if (figure) end = unitOf(figure, next).end;
  return { opens_section, height: +(end - first.top).toFixed(1) };
}

/**
 * Whether the gap at the foot of a page is accepted: its next page opens with a section, or with
 * a run of figures that ends its section, and the first unit is taller than the gap. `readPage(n)`
 * returns the opening of the page `n` pages after the next one, for a run that goes on over a page
 * break; it returns null past the last page, where the run ends the report.
 */
export function gapAccepted({ gap, textHeight, next, readPage }: { gap: number; textHeight: number; next: Opening | null; readPage: (offset: number) => Opening | null }) {
  if (!next || !(next.height > gap * textHeight)) return false;
  if (next.opens_section) return true;
  if (!next.lead) return false;
  let ends = next.ends_section;
  for (let ahead = 1; ends === null; ahead++) {
    const page = readPage(ahead);
    if (!page || page.opens_section) return true;
    if (!page.lead) return false;
    ends = page.ends_section;
  }
  return ends;
}

/** Chart-frame rectangles from the tagged PDF itself, matched by its full authored title.
 * qpdf supplies the parsed structure tree and page references; the existing path parser supplies
 * physical page-space rectangles. Missing/ambiguous metadata adds no exception to the old judge.
 */
export function figureFrames<F extends Figure>(document: QpdfDocument, streams: string[], height: number, figures: F[]) {
  const pages = document.pages.map(page => page.object);
  const shapes = new Map<number, ReturnType<typeof paintedShapes>>();
  const result: (F & { frame: Frame })[][] = pages.map(() => []);
  for (const object of Object.values(document.qpdf[1])) {
    const node = object.value;
    if (node?.['/S'] !== '/Figure' || typeof node['/Alt'] !== 'string' || !node['/Alt'].startsWith('u:')) continue;
    const title = plain(norm(node['/Alt'].slice(2)));
    const matches = figures.filter((figure: { title: string; }) => figure.title && title.startsWith(plain(norm(figure.title)) + '.'));
    if (matches.length !== 1) continue;
    const page = pages.indexOf(node['/Pg'] as string);
    if (page < 0) continue;
    const members = Array.isArray(node['/K']) ? node['/K'] : [node['/K']];
    const ids = members.filter(Number.isInteger);
    if (!ids.length) continue;
    if (!shapes.has(page)) shapes.set(page, paintedShapes(streams[page], height, { markedContent: true }));
    const rectangles = shapes.get(page)!.filter((shape): shape is Extract<typeof shape, { kind: 'path' }> => shape.kind === 'path' && shape.mcids!.some((id: number) => ids.includes(id))
      && shape.source.length === 1 && shape.source[0].op === 're' && /^(?:f|F|f\*)$/.test(shape.op)
      && shape.width > 0 && shape.height > 0 && shape.matrix[1] === 0 && shape.matrix[2] === 0);
    if (rectangles.length !== 1) continue;
    const { left, top, width, height: frameHeight } = rectangles[0];
    result[page].push({ ...matches[0], frame: { left, top, right: left + width, bottom: top + frameHeight } });
  }
  return result;
}
